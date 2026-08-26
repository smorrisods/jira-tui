//! The inline quick-view panel: showing and lazily loading the selected
//! issue's full detail without leaving the list.

use crate::domain::{IssueDetail, Source};

use super::App;

impl App {
    /// The cached detail for the currently selected issue, if any (quick view).
    pub fn quick_view_detail(&self) -> Option<&IssueDetail> {
        let key = &self.selected_issue()?.key;
        self.detail_cache.get(key)
    }

    /// Fetch and cache the selected issue's detail for the quick-view panel if
    /// it isn't already cached. Cheap no-op once cached (or once a fetch for
    /// this key is already in flight); call each frame while quick view is
    /// open so panels populate without a full "open" action.
    pub fn ensure_quick_view_loaded(&mut self) {
        if !self.quick_view {
            return;
        }
        let Some(key) = self.selected_issue().map(|i| i.key.clone()) else {
            return;
        };
        if self.detail_cache.contains_key(&key) {
            // Already cached — still worth a cheap re-check for any inline
            // image that never landed: `apply_inline_image_loaded` drops a
            // response whose generation was bumped out from under it by an
            // intervening Detail navigate, and a plain failed/undecodable
            // fetch is never automatically retried otherwise, since this was
            // the only site that ever dispatched one for this issue. This
            // runs every frame while quick view is open (see this method's
            // own doc comment), and `refresh_quick_view_inline_images` is a
            // no-op for anything already resolved or still in flight, so
            // re-checking here costs nothing beyond a cache/pending lookup.
            #[cfg(feature = "images")]
            self.refresh_quick_view_inline_images();
            return;
        }
        if !matches!(self.source, Source::Live { .. }) {
            let detail = self.load_detail(&key);
            self.detail_cache.insert(key, detail);
            // A demo/cache session never actually dispatches an inline-image
            // fetch (`attachments::images_eligible` gates on `Source::Live`),
            // so this is a no-op there — but calling it unconditionally
            // keeps this one call site the single "detail just landed for
            // quick view" trigger, mirroring `open_by_key`'s non-live branch.
            #[cfg(feature = "images")]
            self.refresh_quick_view_inline_images();
            return;
        }
        self.dispatch_detail_fetch(key, false);
    }

    pub fn quick_view_scroll_by(&mut self, delta: isize) {
        let new = self.quick_view_scroll as isize + delta;
        self.quick_view_scroll = new.max(0) as u16;
    }
}
