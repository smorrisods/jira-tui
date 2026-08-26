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

use std::cell::Ref;

use ratatui::layout::Size;
use ratatui_image::sliced::SlicedProtocol;
use ratatui_image::Resize;
use serde_json::Value;

use crate::adf::{self, InlineMediaRef};
use crate::domain::IssueDetail;

use super::{async_ops, App};

/// Row-count bounds for a rendered inline image (Phase 3 of issue #130) —
/// `sized_inline_image`'s aspect-ratio math is clamped to this band so an
/// image is never so short it's unrecognizable nor so tall it swallows the
/// whole scrollable pane, per the plan referenced in this module's own doc
/// comment.
pub(crate) const MIN_INLINE_IMAGE_ROWS: u16 = 3;
pub(crate) const MAX_INLINE_IMAGE_ROWS: u16 = 14;

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
    ///
    /// Also clears `inline_image_protocols` (the *encoded* `SlicedProtocol`
    /// cache — see its own field doc comment): that cache is keyed purely by
    /// `InlineMediaRef` (a media node's `alt` text), with no issue/generation
    /// component and no link back to `inline_images`' generation-guarded
    /// entries. `sliced_inline_image_protocol`'s only staleness check is
    /// whether the cached protocol's target size still matches — so leaving
    /// a stale entry behind here means a different issue's inline image that
    /// happens to share the same `alt` (e.g. a common filename like
    /// "screenshot.png") and lands on the same target size would render the
    /// *previous* issue's picture, without ever re-checking
    /// `inline_images`/`self.detail` at all.
    pub(crate) fn invalidate_inline_images(&mut self) {
        self.inline_images.get_mut().clear();
        self.inline_image_protocols.get_mut().clear();
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

    /// Build a `MediaSizing` for a Detail-screen document rendered at
    /// `width` columns and hand it to `f` — Phase 3 of issue #130's single
    /// entry point for `ui::detail`/`app::comments`/`app::links`, all of
    /// which need `render::wide_detail`/`narrow_detail` to agree on exactly
    /// the same readiness so a scroll-to-comment/link-cycle offset computed
    /// via one of those callers always lands where the other actually
    /// painted the image. `Disabled` whenever no terminal image capability
    /// was ever detected (`image_picker` is `None`) — the CPS shape (a
    /// callback rather than returning `MediaSizing<'a>` directly) exists
    /// because `MediaSizing::Ready` borrows a `&'a dyn Fn`, and that closure
    /// has to live in *this* function's stack frame, not the caller's; `R`
    /// stays a plain owned value (a `WideDetail`/`NarrowDetail`/link list),
    /// so nothing about the callback's lifetime leaks into what `f` returns.
    ///
    /// A non-`images` build never compiles this method at all — see the
    /// same-named stand-in in `app::mod` (always `Disabled`) that every
    /// caller finds instead, so no call site needs its own `#[cfg]`.
    pub(crate) fn with_detail_media_sizing<R>(
        &self,
        width: u16,
        f: impl FnOnce(&adf::MediaSizing) -> R,
    ) -> R {
        if self.image_picker.is_none() {
            return f(&adf::MediaSizing::Disabled);
        }
        let sizing_fn = |media: &InlineMediaRef| self.sized_inline_image(media, width);
        f(&adf::MediaSizing::Ready(&sizing_fn))
    }

    /// `MediaSizing::Ready`'s callback body: alt -> attachment -> decoded
    /// image (via `decoded_inline_image`, mirroring `resolve_inline_images`'
    /// own matching), then a `(rows, cols)` pair preserving the image's
    /// aspect ratio within `pane_width` columns and the 3-14 row band. Only
    /// ever reached with `image_picker` known `Some` (see
    /// `with_detail_media_sizing`), but re-checked here anyway rather than
    /// threading the `Picker` through as a parameter — this is the only
    /// caller, and `Option`-chaining past a second `None` costs nothing.
    fn sized_inline_image(&self, media: &InlineMediaRef, pane_width: u16) -> Option<(u16, u16)> {
        let picker = self.image_picker.as_ref()?;
        let img = self.decoded_inline_image(&media.alt)?;
        Some(rows_cols_for(&img, picker.font_size(), pane_width))
    }

    /// Look up whichever decoded inline image (if any) corresponds to
    /// `alt` — mirrors `resolve_inline_images`'s own alt -> attachment
    /// matching, then keys into the `inline_images` cache the same way
    /// `refresh_inline_images` populated it. Returns a `Ref` (rather than a
    /// bare `&DynamicImage`) since the cache lives behind a `RefCell` and a
    /// borrow guard can't be shortened to `&self`'s lifetime without one.
    fn decoded_inline_image(&self, alt: &str) -> Option<Ref<'_, image::DynamicImage>> {
        let attachment = self
            .detail
            .as_ref()?
            .attachments
            .iter()
            .find(|a| a.filename == alt)?;
        let key = InlineImageKey::Attachment(attachment.id.clone());
        Ref::filter_map(self.inline_images.borrow(), |cache| cache.get(&key)).ok()
    }

    /// Get-or-build the cached `SlicedProtocol` for one `ImagePlacement`
    /// (`ui::detail`'s paint pass, Phase 3 of issue #130), rebuilding it
    /// from the still-cached `DynamicImage` whenever the cached protocol's
    /// own size no longer matches `placement`'s current `(cols, rows)` —
    /// covers both a first-ever render (nothing cached yet) and a terminal
    /// resize that changed the pane width since the protocol was last
    /// built. `None` if there's no picker, or the backing decoded image was
    /// evicted from `inline_images` since sizing was computed (a same-frame
    /// race that shouldn't normally happen, guarded rather than panicking).
    pub(crate) fn sliced_inline_image_protocol(
        &self,
        placement: &adf::ImagePlacement,
    ) -> Option<Ref<'_, SlicedProtocol>> {
        let picker = self.image_picker.as_ref()?;
        let target = Size::new(placement.cols, placement.rows);
        let up_to_date = self
            .inline_image_protocols
            .borrow()
            .get(&placement.media)
            .is_some_and(|p| p.size() == target);
        if !up_to_date {
            let img = self.decoded_inline_image(&placement.media.alt)?.clone();
            let protocol =
                SlicedProtocol::new_with_resize(picker, img, target, Resize::Fit(None)).ok()?;
            self.inline_image_protocols
                .borrow_mut()
                .insert(placement.media.clone(), protocol);
        }
        Ref::filter_map(self.inline_image_protocols.borrow(), |m| {
            m.get(&placement.media)
        })
        .ok()
    }
}

/// The aspect-ratio math behind `App::sized_inline_image`: given the
/// image's own pixel dimensions and the terminal's cell-to-pixel ratio
/// (`font`), find the largest `cols` up to both `pane_width` and the
/// image's own natural (1:1 pixel) column count, then derive `rows` to
/// match the image's aspect ratio at that width. If that first-pass `rows`
/// falls outside `MIN_INLINE_IMAGE_ROWS..=MAX_INLINE_IMAGE_ROWS`, it's
/// clamped and `cols` is re-derived from the *clamped* row count instead —
/// otherwise a very tall or very short image would render squashed or
/// stretched at the row count it actually gets.
fn rows_cols_for(
    img: &image::DynamicImage,
    font: ratatui_image::FontSize,
    pane_width: u16,
) -> (u16, u16) {
    let px_w = img.width().max(1) as f64;
    let px_h = img.height().max(1) as f64;
    let font_w = font.width.max(1) as f64;
    let font_h = font.height.max(1) as f64;
    let max_cols = (pane_width.max(1)) as f64;

    let natural_cols = (px_w / font_w).ceil().max(1.0);
    let cols = natural_cols.min(max_cols);
    let rows = ((px_h * cols * font_w) / (px_w * font_h)).round().max(1.0);
    let rows = (rows as u16).clamp(MIN_INLINE_IMAGE_ROWS, MAX_INLINE_IMAGE_ROWS);

    let cols = ((rows as f64 * font_h * px_w) / (px_h * font_w))
        .round()
        .max(1.0);
    let cols = (cols as u16).min(pane_width.max(1));

    (rows, cols)
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
