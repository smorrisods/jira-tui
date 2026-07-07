//! The inline quick-view panel: showing and lazily loading the selected
//! issue's full detail without leaving the list.

use crate::domain::IssueDetail;

use super::App;

impl App {
    /// The cached detail for the currently selected issue, if any (quick view).
    pub fn quick_view_detail(&self) -> Option<&IssueDetail> {
        let key = &self.selected_issue()?.key;
        self.detail_cache.get(key)
    }

    /// Fetch and cache the selected issue's detail for the quick-view panel if
    /// it isn't already cached. Cheap no-op once cached; call each frame while
    /// quick view is open so panels populate without a full "open" action.
    pub fn ensure_quick_view_loaded(&mut self) {
        if !self.quick_view {
            return;
        }
        let Some(key) = self.selected_issue().map(|i| i.key.clone()) else {
            return;
        };
        if self.detail_cache.contains_key(&key) {
            return;
        }
        let detail = self.load_detail(&key);
        self.detail_cache.insert(key, detail);
    }

    pub fn quick_view_scroll_by(&mut self, delta: isize) {
        let new = self.quick_view_scroll as isize + delta;
        self.quick_view_scroll = new.max(0) as u16;
    }
}
