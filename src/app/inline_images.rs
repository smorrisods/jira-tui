//! Resolving and eagerly fetching inline images embedded in an issue's
//! description/acceptance criteria/comments (`images` feature only) — Phase
//! 1 of issue #130's inline-image rendering (comment bodies joined the walk
//! later in the same issue). This module only builds the resolver/cache/
//! fetch machinery; nothing renders these yet (see the `AttachmentPreview`
//! machinery in `app::attachments` for the sibling feature this
//! generalizes, and a later phase for the actual paint code).
//!
//! Unlike the attachment picker's single "preview for whatever's
//! highlighted" slot, every media node a description resolves to is
//! independently and simultaneously relevant, so the cache here is a map
//! (`App::inline_images`) rather than one overwritable slot.

use std::cell::Ref;
use std::collections::{HashMap, VecDeque};

use ratatui::layout::Size;
use ratatui_image::sliced::SlicedProtocol;
use ratatui_image::Resize;
use serde_json::Value;

use crate::adf::{self, InlineMediaRef};
use crate::domain::IssueDetail;

use super::{async_ops, App};

/// Row-count bounds for a rendered inline image (Phase 3 of issue #130) —
/// `sized_inline_image`'s aspect-ratio math is clamped to this band so an
/// image is never so short it's unrecognizable nor so tall it swallows an
/// unreasonable number of scrolled rows, per the plan referenced in this
/// module's own doc comment. Raised from an original 14 once `rows_cols_for`
/// switched to filling the full pane width by default (see its own doc
/// comment) — a wide/tall image at full width can legitimately need more
/// than 14 rows, and `SlicedImage`'s whole reason for existing (Phase 3) is
/// that partial scroll visibility is already handled correctly, so a large
/// reservation isn't the layout hazard a fixed-`StatefulImage` render would
/// have made it.
pub(crate) const MIN_INLINE_IMAGE_ROWS: u16 = 3;
pub(crate) const MAX_INLINE_IMAGE_ROWS: u16 = 45;

/// Identifies one inline image's decoded cache entry (`App::inline_images`).
/// `Attachment(id)` is a resolved `media` node whose `alt` matched an
/// attachment's filename (see `resolve_inline_images`); `External(url)` is a
/// `type: "external"` media node, keyed by its own URL — a stable identity
/// with no attachment/filename lookup involved, since the URL *is* the fetch
/// target. Added in issue #130 phase 4; Phase 1 named the enum anticipating
/// exactly this addition without reshaping any of its own callers.
///
/// `pub`, not `pub(crate)`, purely because `AppEvent` (which carries one)
/// is itself `pub` — Rust requires a public item's field types be at least
/// as visible as the item. Nothing outside this crate actually constructs
/// or inspects one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InlineImageKey {
    Attachment(String),
    External(String),
}

/// Only the first this-many resolvable images — across description,
/// acceptance criteria, *and every comment's body combined* (see
/// `resolve_inline_images`'s own doc comment for the exact document order)
/// — are ever eagerly fetched; the rest still render as their normal
/// `[image: alt]` placeholder (see `adf::render_block`'s `"media"` arm),
/// they just never get a decoded preview. Keeps a pathological
/// description/comment thread from kicking off a pile of concurrent
/// fetches on every detail load.
pub(crate) const MAX_INLINE_IMAGES: usize = 6;

/// Cap for both `App::inline_images` (decoded `DynamicImage`s) and
/// `App::inline_image_protocols` (encoded `SlicedProtocol`s) — Phase 5 of
/// issue #130. Detail alone never approaches this (it fully clears both
/// caches on every navigate, per `App::invalidate_inline_images`, and a
/// single issue resolves at most `MAX_INLINE_IMAGES` = 6 anyway), but quick
/// view's own trigger (`App::refresh_quick_view_inline_images`) deliberately
/// does *not* clear on every selection change — a user scrolling `j`/`k`
/// back and forth over a handful of issues should keep seeing their already
/// -decoded images rather than re-fetching on every pass. Without a cap,
/// unbounded quick-view churn across a long session would grow both maps
/// forever; 24 entries comfortably covers several recently-viewed issues'
/// worth of images (each capped at 6) while staying well short of the
/// multi-issue combinations that would actually matter memory-wise.
pub(super) const INLINE_IMAGE_CACHE_CAP: usize = 24;

/// A capacity-bounded cache that evicts the oldest-*inserted* entry once
/// full, rather than true LRU (Phase 5 of issue #130 — see
/// `INLINE_IMAGE_CACHE_CAP`'s doc comment for which two caches use this).
/// Insertion order, not access order: both caches are read through borrowed
/// `Ref`s at paint time (`App::decoded_inline_image`/
/// `App::sliced_inline_image_protocol`), which only ever need `&self` — bumping
/// recency on every read would mean either taking `&mut self` on the
/// render-thread's hot path just to reorder a queue, or adding `Cell`-based
/// access-order bookkeeping purely to make eviction marginally smarter. For
/// a cache whose job is "bound quick-view's churn without defeating
/// retention across a few recently-visited issues," not "squeeze out every
/// last cache hit," FIFO is the simpler, still-correct choice: a working set
/// that fits within the cap is fully retained either way, and eviction only
/// ever discards the least-recently-*fetched* entry, which in practice
/// tracks "least recently viewed" closely enough.
pub struct BoundedCache<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    cap: usize,
}

impl<K: Eq + std::hash::Hash + Clone, V> BoundedCache<K, V> {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Insert `value` under `key`, evicting the oldest-inserted entry (if
    /// any) once the cache grows past `cap`. Overwriting an already-present
    /// key updates the value in place without touching its position in the
    /// eviction order — it's not "new" for eviction purposes.
    pub fn insert(&mut self, key: K, value: V) {
        if !self.map.contains_key(&key) {
            self.order.push_back(key.clone());
            while self.order.len() > self.cap {
                if let Some(oldest) = self.order.pop_front() {
                    self.map.remove(&oldest);
                }
            }
        }
        self.map.insert(key, value);
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

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
    /// cache — see its own field doc comment): even though it's now keyed by
    /// the same `InlineImageKey` `inline_images` is (no more alt-text
    /// collision risk between different attachments), it still has no
    /// issue/generation component of its own, so a same-id attachment
    /// surviving a refresh with genuinely different image bytes would
    /// otherwise still return the stale encoded protocol under that same
    /// key.
    pub(crate) fn invalidate_inline_images(&mut self) {
        self.inline_images.get_mut().clear();
        self.inline_image_protocols.get_mut().clear();
        // Also drop any keys `refresh_inline_images`/
        // `refresh_quick_view_inline_images` believe are still in flight —
        // a fetch dispatched under the generation this bump just retired
        // will still land eventually (see `apply_inline_image_loaded`), but
        // its response will be dropped for the stale generation, so nothing
        // will ever clear its `pending` entry otherwise. Leaving it behind
        // would permanently block a future re-resolution of the same key
        // under the new generation, since `refresh_*` skips anything
        // already marked pending.
        self.inline_images_pending.clear();
        // Same staleness reasoning as `inline_images_pending` above, but for
        // the redirect-probe's own uuid -> key resolutions: a same-uuid
        // media node surviving a refresh could resolve to a genuinely
        // different attachment now, and a generation bump alone wouldn't
        // evict a stale mapping sitting under that same uuid.
        self.inline_image_uuid_matches.clear();
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
        if !super::attachments::images_eligible(self.image_picker.as_ref(), &self.source) {
            return;
        }
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let generation = self.inline_image_generation;
        let (resolved, candidates) = resolve_inline_images_with_candidates(detail);
        let attachments = detail.attachments.clone();
        for (key, url) in resolved {
            if self.inline_images.borrow().contains_key(&key)
                || self.inline_images_pending.contains(&key)
            {
                continue;
            }
            self.inline_images_pending.insert(key.clone());
            let tx = self.events_tx.clone();
            async_ops::dispatch_inline_image(tx, generation, key, url);
        }
        let has_image_attachment = attachments.iter().any(|a| a.image_preview_url().is_some());
        if std::env::var_os("JIRA_TUI_DEBUG_MEDIA").is_some() {
            eprintln!(
                "[jira-tui] refresh_inline_images: {} candidate(s) {candidates:?}, \
                 has_image_attachment={has_image_attachment}",
                candidates.len()
            );
        }
        if !candidates.is_empty() && has_image_attachment {
            let tx = self.events_tx.clone();
            async_ops::dispatch_uuid_resolve(tx, generation, candidates, attachments);
        }
    }

    /// The quick-view panel's counterpart to `refresh_inline_images` (Phase
    /// 5 of issue #130) — resolves and eagerly fetches images for whichever
    /// issue is *currently* selected, reading `self.quick_view_detail()`
    /// fresh each call rather than trusting a caller-supplied key, so a
    /// selection change between dispatch and this call landing (e.g. a
    /// stale-by-the-time-it-runs async completion) always targets whatever
    /// is actually on screen right now.
    ///
    /// Deliberately does **not** call `invalidate_inline_images` first,
    /// unlike every Detail "a detail just landed" site — that's the crux of
    /// this phase's design difference from Detail. Detail only "lands" a
    /// handful of times per session, so a full clear-and-refetch on every
    /// navigate is cheap and simple. Quick view can churn across a dozen
    /// issues a minute as the user holds `j`/`k`; clearing the whole shared
    /// `inline_images`/`inline_image_protocols` cache on every single
    /// selection change would mean scrolling back to an already-visited
    /// issue always re-fetches its images from scratch, defeating caching
    /// entirely. Instead this leans on two things already in place: the
    /// per-key `contains_key`/`inline_images_pending` dedup below (so
    /// revisiting a still-cached or still-in-flight issue is a no-op, not a
    /// duplicate fetch), and `INLINE_IMAGE_CACHE_CAP`'s bounded eviction (so
    /// the shared cache can't grow without limit as the user churns through
    /// many issues over a long session). The one accepted trade-off: a
    /// Detail-side navigate still fully clears this cache (out of this
    /// phase's scope to change), so returning to quick view on a
    /// previously-cached issue right after visiting Detail on something
    /// else will still re-fetch — a rarer path than plain list scrolling,
    /// and not worth entangling Detail's own invalidation semantics with
    /// quick view's to avoid.
    pub(crate) fn refresh_quick_view_inline_images(&mut self) {
        if !self.quick_view {
            return;
        }
        if !super::attachments::images_eligible(self.image_picker.as_ref(), &self.source) {
            return;
        }
        let Some(issue_key) = self.selected_issue().map(|i| i.key.clone()) else {
            return;
        };
        let Some(detail) = self.detail_cache.get(&issue_key) else {
            return;
        };
        let generation = self.inline_image_generation;
        let (resolved, candidates) = resolve_inline_images_with_candidates(detail);
        let attachments = detail.attachments.clone();
        for (key, url) in resolved {
            if self.inline_images.borrow().contains_key(&key)
                || self.inline_images_pending.contains(&key)
            {
                continue;
            }
            self.inline_images_pending.insert(key.clone());
            let tx = self.events_tx.clone();
            async_ops::dispatch_inline_image(tx, generation, key, url);
        }
        let has_image_attachment = attachments.iter().any(|a| a.image_preview_url().is_some());
        if std::env::var_os("JIRA_TUI_DEBUG_MEDIA").is_some() {
            eprintln!(
                "[jira-tui] refresh_inline_images: {} candidate(s) {candidates:?}, \
                 has_image_attachment={has_image_attachment}",
                candidates.len()
            );
        }
        if !candidates.is_empty() && has_image_attachment {
            let tx = self.events_tx.clone();
            async_ops::dispatch_uuid_resolve(tx, generation, candidates, attachments);
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
        self.with_media_sizing_for(self.detail.as_ref(), width, MAX_INLINE_IMAGE_ROWS, f)
    }

    /// The quick-view panel's counterpart to `with_detail_media_sizing`
    /// (Phase 5 of issue #130) — same CPS shape and same purpose (agreement
    /// between whatever sizing `render::quick_view_wide`/`quick_view_narrow`
    /// used and whatever `ui::quick_view`'s paint pass, `app::links`'
    /// click/cycle mapping, etc. use), just sourced from
    /// `self.quick_view_detail()` (the currently-selected issue's cached
    /// detail) instead of `self.detail`. Also clamps the row band tighter
    /// than Detail's: quick view is typically only ~40-50% of the body
    /// height (`quick_view_max_image_rows`), so Detail's flat 3-14 row band
    /// can be taller than the whole panel.
    pub(crate) fn with_quick_view_media_sizing<R>(
        &self,
        width: u16,
        f: impl FnOnce(&adf::MediaSizing) -> R,
    ) -> R {
        self.with_media_sizing_for(
            self.quick_view_detail(),
            width,
            self.quick_view_max_image_rows(),
            f,
        )
    }

    /// Shared body of `with_detail_media_sizing`/`with_quick_view_media_sizing`
    /// — `detail` is whichever `IssueDetail` the caller's document was built
    /// from (Detail's `self.detail`, or quick view's `self.quick_view_detail()`),
    /// and `max_rows` is the ceiling `rows_cols_for` clamps against (see its
    /// own doc comment).
    fn with_media_sizing_for<R>(
        &self,
        detail: Option<&IssueDetail>,
        width: u16,
        max_rows: u16,
        f: impl FnOnce(&adf::MediaSizing) -> R,
    ) -> R {
        if self.image_picker.is_none() {
            return f(&adf::MediaSizing::Disabled);
        }
        let sizing_fn =
            |media: &InlineMediaRef| self.sized_inline_image(detail, media, width, max_rows);
        f(&adf::MediaSizing::Ready(&sizing_fn))
    }

    /// Quick view's row-budget ceiling for `rows_cols_for` (Phase 5 of issue
    /// #130) — the plan's row-budget note: quick view's pane
    /// (`quick_view_area`, set at paint time by `ui::quick_view::draw_quick_view`)
    /// is often only 10-20 rows total, well short of Detail's own scrollable
    /// column, so clamping to the same flat `MAX_INLINE_IMAGE_ROWS` (14)
    /// could let a single image occupy nearly the whole panel. Reserves 4
    /// rows for whatever always surrounds the description (chips/kv lines in
    /// the narrow layout, the trailing "… ↓ N more lines" fade row in either
    /// layout once content overflows) before clamping into the same
    /// `MIN_INLINE_IMAGE_ROWS..=MAX_INLINE_IMAGE_ROWS` band Detail uses —
    /// never *wider* than Detail's own ceiling, only ever tighter. A pane
    /// that's somehow shorter than the reserve still gets at least
    /// `MIN_INLINE_IMAGE_ROWS`; the paint pass's own off-screen skip
    /// (`ui::detail::paint_inline_images`, reused by quick view) is what
    /// actually keeps that from overflowing the panel, the same way it
    /// already does for Detail.
    fn quick_view_max_image_rows(&self) -> u16 {
        self.quick_view_area
            .get()
            .height
            .saturating_sub(4)
            .clamp(MIN_INLINE_IMAGE_ROWS, MAX_INLINE_IMAGE_ROWS)
    }

    /// `MediaSizing::Ready`'s callback body: alt -> attachment -> decoded
    /// image (via `decoded_inline_image`, mirroring `resolve_inline_images`'
    /// own matching), then a `(rows, cols)` pair preserving the image's
    /// aspect ratio within `pane_width` columns and clamped to `max_rows`.
    /// Only ever reached with `image_picker` known `Some` (see
    /// `with_media_sizing_for`), but re-checked here anyway rather than
    /// threading the `Picker` through as a parameter — this is the only
    /// caller, and `Option`-chaining past a second `None` costs nothing.
    fn sized_inline_image(
        &self,
        detail: Option<&IssueDetail>,
        media: &InlineMediaRef,
        pane_width: u16,
        max_rows: u16,
    ) -> Option<(u16, u16)> {
        let picker = self.image_picker.as_ref()?;
        let img = self.decoded_inline_image(detail, media)?;
        Some(rows_cols_for(
            &img,
            picker.font_size(),
            pane_width,
            max_rows,
        ))
    }

    /// Resolve `media` (an ADF node reference from `render_with_media`'s
    /// recursion) into the `InlineImageKey` it was fetched under — an
    /// external node (`media.url` is `Some`) is keyed by that URL directly,
    /// mirroring how `resolve_inline_images_with_candidates` resolves one;
    /// an attachment-backed node (`media.url` is `None`) tries its `alt`
    /// against `detail.attachments` first (the common case), then falls
    /// back to looking its own `media.id` uuid up in
    /// `self.inline_image_uuid_matches` (issue #130's DS-1880 follow-up —
    /// whatever the redirect-probe fallback has matched by uuid so far).
    /// `detail` is whichever issue's document `media` actually came from
    /// (see `with_media_sizing_for`'s doc comment) — this no longer reads
    /// `self.detail` directly so the same lookup serves both Detail and
    /// quick view.
    fn inline_image_key_for(
        &self,
        detail: Option<&IssueDetail>,
        media: &InlineMediaRef,
    ) -> Option<InlineImageKey> {
        if let Some(url) = media.url.as_ref() {
            return Some(InlineImageKey::External(url.clone()));
        }
        if !media.alt.is_empty() {
            if let Some(attachment) =
                detail.and_then(|d| d.attachments.iter().find(|a| a.filename == media.alt))
            {
                return Some(InlineImageKey::Attachment(attachment.id.clone()));
            }
        }
        let id = media.id.as_ref()?;
        self.inline_image_uuid_matches.get(id).cloned()
    }

    /// Look up whichever decoded inline image (if any) corresponds to
    /// `media` — resolves the same key `refresh_inline_images`/
    /// `refresh_quick_view_inline_images` fetched under
    /// (`inline_image_key_for`), then keys into the `inline_images` cache.
    /// Returns a `Ref` (rather than a bare `&DynamicImage`) since the cache
    /// lives behind a `RefCell` and a borrow guard can't be shortened to
    /// `&self`'s lifetime without one.
    fn decoded_inline_image(
        &self,
        detail: Option<&IssueDetail>,
        media: &InlineMediaRef,
    ) -> Option<Ref<'_, image::DynamicImage>> {
        let key = self.inline_image_key_for(detail, media)?;
        Ref::filter_map(self.inline_images.borrow(), |cache| cache.get(&key)).ok()
    }

    /// Get-or-build the cached `SlicedProtocol` for one `ImagePlacement`
    /// (`ui::detail`'s paint pass, Phase 3 of issue #130; reused as-is by
    /// `ui::quick_view`'s own paint pass in Phase 5), rebuilding it from the
    /// still-cached `DynamicImage` whenever the cached protocol's own size
    /// no longer matches `placement`'s current `(cols, rows)` — covers both
    /// a first-ever render (nothing cached yet) and a terminal resize that
    /// changed the pane width since the protocol was last built. `detail` is
    /// whichever issue's document `placement` came from (Detail's
    /// `self.detail`, or quick view's `self.quick_view_detail()`) — needed
    /// by `decoded_inline_image`'s attachment-id lookup on a cache miss.
    /// `None` if there's no picker, or the backing decoded image was evicted
    /// from `inline_images` since sizing was computed (a same-frame race
    /// that shouldn't normally happen, guarded rather than panicking).
    pub(crate) fn sliced_inline_image_protocol(
        &self,
        detail: Option<&IssueDetail>,
        placement: &adf::ImagePlacement,
    ) -> Option<Ref<'_, SlicedProtocol>> {
        let picker = self.image_picker.as_ref()?;
        // Keyed by the resolved `InlineImageKey` (attachment id / external
        // URL) rather than `placement.media` (an `InlineMediaRef` — just the
        // node's `alt` text) — two different issues' media nodes can easily
        // share the same `alt` (e.g. a common filename like
        // "screenshot.png"), and quick view deliberately never clears this
        // cache on a plain selection change (see `refresh_quick_view_inline_images`'s
        // own doc comment), so an `alt`-only key could return a *different*
        // attachment's already-cached protocol whenever the two happened to
        // land on the same target size. Resolving through the same
        // `InlineImageKey` `inline_images` itself is keyed by rules this out
        // structurally instead of relying on `invalidate_inline_images` to
        // have run recently enough.
        let key = self.inline_image_key_for(detail, &placement.media)?;
        let target = Size::new(placement.cols, placement.rows);
        let up_to_date = self
            .inline_image_protocols
            .borrow()
            .get(&key)
            .is_some_and(|p| p.size() == target);
        if !up_to_date {
            let img = self.decoded_inline_image(detail, &placement.media)?.clone();
            // `Resize::Fit` leaves an image at its native size whenever that
            // native size already fits inside `target` — for a small source
            // (particularly a Jira-generated thumbnail, well below a
            // typical panel's pixel width), that means it never actually
            // fills the reserved space at all. `Scale` always resizes to
            // exactly `target`, growing as well as shrinking, which is what
            // "size the image to the panel width" (the reason `target`
            // itself is now computed to fill `pane_width`, see
            // `rows_cols_for`) actually requires.
            let protocol =
                SlicedProtocol::new_with_resize(picker, img, target, Resize::Scale(None)).ok()?;
            self.inline_image_protocols
                .borrow_mut()
                .insert(key.clone(), protocol);
        }
        Ref::filter_map(self.inline_image_protocols.borrow(), |m| m.get(&key)).ok()
    }
}

/// The aspect-ratio math behind `App::sized_inline_image`: fills the *full*
/// `pane_width` by default (not the image's own native pixel width) — the
/// point of an inline preview is to actually be visible at a useful size,
/// not to sit at whatever size a Jira-generated thumbnail's native
/// resolution happens to natively occupy in cells (`Resize::Scale`, not
/// `Fit`, is what actually stretches the source to this target — see
/// `App::sliced_inline_image_protocol`). `rows` is then derived to match
/// the image's aspect ratio at that width. If that first-pass `rows` falls
/// outside `MIN_INLINE_IMAGE_ROWS..=max_rows`, it's clamped and `cols` is
/// re-derived from the *clamped* row count instead — so a very tall or very
/// short image gets shrunk from full width rather than rendering squashed
/// or stretched at a mismatched aspect ratio. `max_rows` is
/// `MAX_INLINE_IMAGE_ROWS` for Detail, or `App::quick_view_max_image_rows`'s
/// tighter ceiling for quick view (Phase 5 of issue #130); always `>=
/// MIN_INLINE_IMAGE_ROWS` at the call site so the final `.clamp` below never
/// panics on an inverted range.
fn rows_cols_for(
    img: &image::DynamicImage,
    font: ratatui_image::FontSize,
    pane_width: u16,
    max_rows: u16,
) -> (u16, u16) {
    let px_w = img.width().max(1) as f64;
    let px_h = img.height().max(1) as f64;
    let font_w = font.width.max(1) as f64;
    let font_h = font.height.max(1) as f64;
    let max_rows = max_rows.max(MIN_INLINE_IMAGE_ROWS);

    let cols = pane_width.max(1) as f64;
    let rows = ((px_h * cols * font_w) / (px_w * font_h)).round().max(1.0);
    let rows = (rows as u16).clamp(MIN_INLINE_IMAGE_ROWS, max_rows);

    let cols = ((rows as f64 * font_h * px_w) / (px_h * font_w))
        .round()
        .max(1.0);
    let cols = (cols as u16).min(pane_width.max(1));

    (rows, cols)
}

/// Walk `detail`'s description, acceptance criteria, and every comment's
/// body (all raw ADF documents — see `IssueDetail`'s own field docs) for
/// `media` nodes and resolve each one to a fetchable `(InlineImageKey,
/// url)` pair, capped at the first `MAX_INLINE_IMAGES` matches in document
/// order *across description + acceptance criteria + comments combined* —
/// an external node found early in the document still counts against the
/// same cap as an attachment-backed one found later, and a comment's own
/// images count against that same shared budget rather than getting a cap
/// of their own, so a description mixing both kinds plus a chatty comment
/// thread never resolves more than `MAX_INLINE_IMAGES` total. Comments are
/// walked in `detail.comments`' own order — oldest first (see that field's
/// own doc comment), matching how the activity feed already renders them —
/// after description and acceptance criteria, so an image already showing
/// in the description always wins a fetch slot over one buried in a later
/// comment. Anything past the cap, or that never resolved, leaves the
/// source's own `[image: alt]`/`[image: url]` placeholder (see
/// `adf::render_block`) showing instead — not an error, just nothing for
/// the caller to fetch.
///
/// Two independent resolution paths per node:
/// - `type: "external"`: resolves directly from `attrs.url` — no
///   filename/attachment matching needed, the URL *is* the fetch target
///   (issue #130 phase 4; fetched credential-free via
///   `jira::get_bytes_public`, never through the authenticated attachment
///   pipeline, so a hostile external URL can never see the user's Jira
///   auth header).
/// - otherwise: matches the node's `alt` text against
///   `detail.attachments[].filename` (case-sensitive, exact match only —
///   Jira's own editor sets a media node's `alt` to the original filename
///   it was embedded under; externally confirmed this session, not yet
///   verified against this codebase's own live Jira, but a wrong match just
///   leaves the placeholder showing, never a wrong image, so the bet is
///   safe even if imperfect), then filters to matches whose attachment
///   `mime_type` starts with `"image/"`.
///
/// Also returns the `attrs.id` uuids of `type: "file"` media nodes it
/// couldn't resolve via `alt` matching — candidates for
/// `dispatch_uuid_resolve`'s redirect-probe fallback, kicked off by
/// `App::refresh_inline_images`/
/// `App::refresh_quick_view_inline_images` whenever this list comes back
/// non-empty (see either fn's own doc comment for why alt matching alone
/// isn't always enough — a media node embedded without Jira's editor
/// stamping `alt` to the original filename, confirmed to happen in practice
/// on a real instance, has nothing for the alt path to match against at
/// all).
///
/// Doing both in one walk, rather than a second function re-deriving "is
/// this node already resolved" from scratch, avoids the exact kind of
/// duplicated-matching-logic drift a code review already caught once this
/// session (see `App::refresh_detail_images`'s own doc comment) — there's
/// only one place that decides what counts as "resolved via alt" here.
///
/// `candidates` shares `resolved`'s `MAX_INLINE_IMAGES` budget rather than
/// getting its own — the combined total, once the uuid probe resolves
/// however many of these candidates it can, must still respect the same
/// cap the alt-only path already enforced, so this bounds the *candidate
/// list itself* at whatever's left over rather than letting the uuid
/// fallback uncap it.
pub(crate) fn resolve_inline_images_with_candidates(
    detail: &IssueDetail,
) -> (Vec<(InlineImageKey, String)>, Vec<String>) {
    let mut nodes: Vec<&Value> = Vec::new();
    find_media_nodes(&detail.description, &mut nodes);
    if let Some(criteria) = detail.acceptance_criteria.as_ref() {
        find_media_nodes(criteria, &mut nodes);
    }
    for comment in &detail.comments {
        find_media_nodes(&comment.body, &mut nodes);
    }

    let mut resolved = Vec::new();
    let mut candidates = Vec::new();
    for node in nodes {
        if resolved.len() + candidates.len() >= MAX_INLINE_IMAGES {
            break;
        }
        let attrs = node.get("attrs");
        let is_external =
            attrs.and_then(|a| a.get("type")).and_then(|t| t.as_str()) == Some("external");
        if is_external {
            if let Some(url) = attrs.and_then(|a| a.get("url")).and_then(|u| u.as_str()) {
                resolved.push((InlineImageKey::External(url.to_string()), url.to_string()));
            }
            continue;
        }
        let alt = attrs
            .and_then(|a| a.get("alt"))
            .and_then(|a| a.as_str())
            .filter(|a| !a.is_empty());
        if let Some(alt) = alt {
            if let Some(attachment) = detail.attachments.iter().find(|a| a.filename == alt) {
                if let Some(url) = attachment.image_preview_url() {
                    resolved.push((InlineImageKey::Attachment(attachment.id.clone()), url));
                    continue;
                }
            }
        }
        // No alt, or alt present but it didn't match any image attachment —
        // a candidate for the uuid-probe fallback, if the node carries its
        // own Media Services id to look up (it always should for a
        // `type: "file"` node, but defensively skip rather than panic if
        // Jira ever omits it).
        if let Some(uuid) = attrs.and_then(|a| a.get("id")).and_then(|i| i.as_str()) {
            candidates.push(uuid.to_string());
        }
    }
    (resolved, candidates)
}

/// Minimal ADF tree walk scoped to finding `media` nodes, in document order.
/// Unlike `adf::render_block`'s own traversal (which handles every node type
/// it knows how to style/render), this only needs to find `media` nodes
/// wherever they appear — nested under `mediaSingle`/`mediaGroup` — so it
/// generically descends into most nodes' `content` array rather than
/// special-casing each container type. `media` nodes are themselves leaves
/// (no `content` of their own), so the walk stops there rather than
/// recursing further.
///
/// Two container types are deliberately *not* descended into at all, because
/// `adf::render_block` can never actually paint a media node found there —
/// eagerly fetching one would just waste a slot in `MAX_INLINE_IMAGES` on an
/// image nothing ever displays:
/// - `table`: `render_table`'s cell content goes through `cell_content_spans`
///   (text-only — see that function and `collect_text`), never `render_block`
///   itself, so a media node inside a cell is silently dropped at render
///   time no matter what.
/// - `bulletList`/`orderedList`/`taskList`: `render_list_item` only routes a
///   list item's immediate child back through `render_block` when that child
///   is *itself* a further-nested list; every other child of every item, at
///   every nesting depth — including a `mediaSingle`/`media` node, whether
///   directly inside the item or inside a nested list's own item — instead
///   goes through the text-only `inline_spans`, which silently drops
///   anything that isn't text/hardBreak/emoji/mention. Since a `media` node
///   is never itself a list type, it can never be the one child that escapes
///   that routing, no matter how deep the nesting — so skipping the entire
///   list subtree here (rather than only its non-list children) still
///   matches `render_block`'s actual reach exactly. (`taskList`'s own render
///   arm is even flatter: it always uses `inline_spans` on an item's content
///   with no nested-list exception at all.)
fn find_media_nodes<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    let Some(obj) = value.as_object() else {
        return;
    };
    match obj.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "media" => out.push(value),
        "table" | "bulletList" | "orderedList" | "taskList" => {}
        _ => {
            if let Some(content) = obj.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    find_media_nodes(child, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The core eviction contract `App::inline_images`/`inline_image_protocols`
    /// lean on (Phase 5 of issue #130): once `cap` distinct keys have been
    /// inserted, the *oldest-inserted* one is evicted to make room for a new
    /// one — proven directly against `BoundedCache` rather than through a
    /// full `App`/rendering round trip, since the eviction policy itself is
    /// pure bookkeeping with no dependency on `App` at all.
    #[test]
    fn inserting_past_capacity_evicts_the_oldest_inserted_key() {
        let mut cache: BoundedCache<u32, &'static str> = BoundedCache::new(2);
        cache.insert(1, "a");
        cache.insert(2, "b");
        assert_eq!(cache.len(), 2);

        cache.insert(3, "c");

        assert_eq!(
            cache.len(),
            2,
            "cache must stay at its cap, not grow past it"
        );
        assert!(
            !cache.contains_key(&1),
            "the oldest-inserted key must be evicted"
        );
        assert!(cache.contains_key(&2));
        assert!(cache.contains_key(&3));
    }

    /// Overwriting an already-present key updates its value in place without
    /// pushing a second entry onto the eviction queue for it — not doing so
    /// would let repeatedly re-inserting the same key (e.g. a `SlicedProtocol`
    /// rebuilt on every resize) silently inflate the queue past `cap`, and
    /// evict some unrelated key early even though the cache never actually
    /// held more than `cap` distinct keys at once. Since this is FIFO by
    /// *original* insertion order rather than true LRU (see `BoundedCache`'s
    /// own doc comment for why), the update itself doesn't protect key 1 from
    /// eviction either — it's still the first-ever-inserted key, so it's
    /// still the one evicted once a third distinct key arrives.
    #[test]
    fn overwriting_an_existing_key_updates_its_value_without_inflating_the_eviction_queue() {
        let mut cache: BoundedCache<u32, &'static str> = BoundedCache::new(2);
        cache.insert(1, "a");
        cache.insert(2, "b");

        cache.insert(1, "a-updated");
        assert_eq!(cache.len(), 2, "overwriting a key must not grow the cache");
        assert_eq!(cache.get(&1), Some(&"a-updated"));

        cache.insert(3, "c");

        assert!(
            !cache.contains_key(&1),
            "key 1 was still the first ever inserted, so it's still the one evicted"
        );
        assert!(cache.contains_key(&2));
        assert!(cache.contains_key(&3));
    }

    #[test]
    fn clear_empties_both_the_map_and_the_eviction_order() {
        let mut cache: BoundedCache<u32, &'static str> = BoundedCache::new(2);
        cache.insert(1, "a");
        cache.insert(2, "b");

        cache.clear();

        assert!(cache.is_empty());
        // Re-filling past the original cap after a clear must evict again
        // from a clean slate, not carry over any leftover order bookkeeping.
        cache.insert(3, "c");
        cache.insert(4, "d");
        cache.insert(5, "e");
        assert_eq!(cache.len(), 2);
        assert!(!cache.contains_key(&3));
    }
}
