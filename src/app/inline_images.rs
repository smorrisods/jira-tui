//! Resolving and eagerly fetching inline images embedded in an issue's
//! description/acceptance criteria (`images` feature only) — Phase 1 of
//! issue #130's inline-image rendering. This module only builds the
//! resolver/cache/fetch machinery; nothing renders these yet (see the
//! `AttachmentPreview` machinery in `app::attachments` for the sibling
//! feature this generalizes, and a later phase for the actual paint code).
//!
//! Unlike the attachment picker's single "preview for whatever's
//! highlighted" slot, every media node a description resolves to is
//! independently and simultaneously relevant, so the cache here is a map
//! (`App::inline_images`) rather than one overwritable slot.

use serde_json::Value;

use crate::domain::IssueDetail;

use super::{async_ops, App};

/// Identifies one inline image's decoded cache entry (`App::inline_images`).
/// Only `Attachment` exists so far — a resolved `media` node whose `alt`
/// matched an attachment's filename (see `resolve_inline_images`) — but the
/// variant is named so an `External(String)` case (keyed by URL, for
/// `type: "external"` media nodes) can be added in a later phase without
/// reshaping any of this phase's callers.
///
/// `pub`, not `pub(crate)`, purely because `AppEvent` (which carries one)
/// is itself `pub` — Rust requires a public item's field types be at least
/// as visible as the item. Nothing outside this crate actually constructs
/// or inspects one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InlineImageKey {
    Attachment(String),
}

/// Only the first this-many resolvable images in document order are ever
/// eagerly fetched — a description with more than this many still renders
/// the rest as their normal `[image: alt]` placeholder (see
/// `adf::render_block`'s `"media"` arm), it just never gets a decoded
/// preview. Keeps a pathological description from kicking off a pile of
/// concurrent fetches on every detail load.
pub(crate) const MAX_INLINE_IMAGES: usize = 6;

impl App {
    /// Drops every cached/in-flight inline image and bumps
    /// `inline_image_generation` — called alongside
    /// `App::invalidate_attachment_preview` at every site where `self.detail`
    /// gets replaced wholesale by a fresh `IssueDetail` fetch/load. Clearing
    /// the whole map (rather than just bumping the generation) matters the
    /// same way it does for the attachment preview: a same-id attachment
    /// surviving a refresh could have a genuinely different image now, and a
    /// generation bump alone wouldn't evict the stale entry sitting under
    /// that same key.
    pub(crate) fn invalidate_inline_images(&mut self) {
        self.inline_images.get_mut().clear();
        self.inline_image_generation += 1;
    }

    /// Resolve every inline image the now-current `self.detail` can show
    /// (`resolve_inline_images`, capped at `MAX_INLINE_IMAGES`) and kick off
    /// an eager fetch for each one not already cached. Called immediately
    /// after `App::invalidate_inline_images` at the same three "a detail
    /// just landed" sites (`App::resolve_detail_sync`, `App::refresh_detail`'s
    /// non-live branch, and `apply_detail_loaded`'s navigate branch) — the
    /// invalidate clears stale state for the previous issue, this kicks off
    /// fresh fetches for the new one.
    ///
    /// A no-op with nothing to fetch: no issue loaded, or
    /// `attachments::images_eligible` says this session/terminal can't
    /// render one anyway (demo/cache session, no detected terminal image
    /// capability) — checked once for the whole batch here rather than per
    /// candidate, since `resolve_inline_images` already filtered to image
    /// mimes during resolution.
    pub(crate) fn refresh_inline_images(&mut self) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        if !super::attachments::images_eligible(self.image_picker.as_ref(), &self.source) {
            return;
        }
        let generation = self.inline_image_generation;
        for (key, url) in resolve_inline_images(detail) {
            if self.inline_images.borrow().contains_key(&key) {
                continue;
            }
            let tx = self.events_tx.clone();
            async_ops::dispatch_inline_image(tx, generation, key, url);
        }
    }
}

/// Walk `detail`'s description and acceptance criteria (both raw ADF
/// documents — see `IssueDetail`'s own field docs) for `media` nodes,
/// matching each one's `alt` text against `detail.attachments[].filename`
/// (case-sensitive, exact match only — Jira's own editor sets a media node's
/// `alt` to the original filename it was embedded under; externally
/// confirmed this session, not yet verified against this codebase's own
/// live Jira, but a wrong match just leaves the placeholder showing, never a
/// wrong image, so the bet is safe even if imperfect). Filters to matches
/// whose attachment `mime_type` starts with `"image/"`, and caps the result
/// at the first `MAX_INLINE_IMAGES` in document order — anything past the
/// cap, or that never matched, or matched a non-image attachment, resolves
/// to no key at all: not an error, just nothing for the caller to fetch, so
/// the description's own `[image: alt]`/`[embedded media]` placeholder (see
/// `adf::render_block`) keeps showing.
///
/// `type: "external"` media nodes are deliberately never resolved here:
/// fetching an arbitrary third-party URL through the same authenticated
/// pipeline used for Jira's own attachments would leak the auth header to
/// that third party. External media gets its own resolution path in a later
/// phase of issue #130.
pub(crate) fn resolve_inline_images(detail: &IssueDetail) -> Vec<(InlineImageKey, String)> {
    let mut nodes: Vec<&Value> = Vec::new();
    find_media_nodes(&detail.description, &mut nodes);
    if let Some(criteria) = detail.acceptance_criteria.as_ref() {
        find_media_nodes(criteria, &mut nodes);
    }

    let mut resolved = Vec::new();
    for node in nodes {
        if resolved.len() >= MAX_INLINE_IMAGES {
            break;
        }
        let attrs = node.get("attrs");
        let is_external =
            attrs.and_then(|a| a.get("type")).and_then(|t| t.as_str()) == Some("external");
        if is_external {
            continue;
        }
        let Some(alt) = attrs
            .and_then(|a| a.get("alt"))
            .and_then(|a| a.as_str())
            .filter(|a| !a.is_empty())
        else {
            continue;
        };
        let Some(attachment) = detail.attachments.iter().find(|a| a.filename == alt) else {
            continue;
        };
        if !attachment.mime_type.starts_with("image/") {
            continue;
        }
        let url = attachment
            .thumbnail_url
            .clone()
            .unwrap_or_else(|| attachment.content_url.clone());
        resolved.push((InlineImageKey::Attachment(attachment.id.clone()), url));
    }
    resolved
}

/// Minimal ADF tree walk scoped to finding `media` nodes, in document order.
/// Unlike `adf::render_block`'s own traversal (which handles every node type
/// it knows how to style/render), this only needs to find `media` nodes
/// wherever they appear — including nested under `mediaSingle`/`mediaGroup`,
/// list items, or table cells — so it generically descends into any node's
/// `content` array rather than special-casing each container type. `media`
/// nodes are themselves leaves (no `content` of their own), so the walk
/// stops there rather than recursing further.
fn find_media_nodes<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    let Some(obj) = value.as_object() else {
        return;
    };
    if obj.get("type").and_then(|t| t.as_str()) == Some("media") {
        out.push(value);
        return;
    }
    if let Some(content) = obj.get("content").and_then(|c| c.as_array()) {
        for child in content {
            find_media_nodes(child, out);
        }
    }
}
