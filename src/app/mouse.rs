//! Mouse mode: click-to-select/open, drag-to-copy, and keyboard/quick-view
//! focus tracking.

use super::{App, Screen};

/// Which panel arrow keys/PageUp/PageDown affect when the quick-view panel is
/// open; toggled with `Tab`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ListFocus {
    #[default]
    List,
    QuickView,
}

impl App {
    /// Whether the given screen coordinate falls within a recorded panel area.
    fn point_in(area: ratatui::layout::Rect, x: u16, y: u16) -> bool {
        area.width > 0
            && area.height > 0
            && x >= area.x
            && x < area.x + area.width
            && y >= area.y
            && y < area.y + area.height
    }

    /// Whether the point is over the quick-view panel (used to route mouse
    /// wheel scrolling to that panel instead of the list).
    pub fn point_in_quick_view(&self, x: u16, y: u16) -> bool {
        self.quick_view && Self::point_in(self.quick_view_area.get(), x, y)
    }

    /// Toggle keyboard focus between the list and the quick-view panel
    /// (`Tab`). A no-op — and forced back to the list — when quick view is
    /// closed, so arrow keys never get stuck scrolling a hidden panel.
    pub fn toggle_list_focus(&mut self) {
        if !self.quick_view {
            self.list_focus = ListFocus::List;
            return;
        }
        self.list_focus = match self.list_focus {
            ListFocus::List => ListFocus::QuickView,
            ListFocus::QuickView => ListFocus::List,
        };
    }

    /// Map a screen row to an issue index within the recorded list area.
    pub fn list_index_at(&self, y: u16) -> Option<usize> {
        let area = self.list_area.get();
        if area.height == 0 || y < area.y || y >= area.y + area.height {
            return None;
        }
        let idx = self.list_start.get() + (y - area.y) as usize;
        (idx < self.issues.len()).then_some(idx)
    }

    pub fn mouse_down(&mut self, y: u16) {
        if matches!(self.screen, Screen::Home | Screen::List) {
            if let Some(idx) = self.list_index_at(y) {
                self.selected = idx;
            }
        }
        self.selecting = true;
        self.sel_start_y = y;
        self.sel_end_y = y;
    }

    pub fn mouse_drag(&mut self, y: u16) {
        if self.selecting {
            self.sel_end_y = y;
        }
    }

    pub fn mouse_up(&mut self, y: u16) {
        if !self.selecting {
            return;
        }
        self.selecting = false;
        self.sel_end_y = y;
        if self.sel_start_y == self.sel_end_y {
            // A click, not a drag: open the issue under the cursor.
            if matches!(self.screen, Screen::Home | Screen::List) && self.list_index_at(y).is_some()
            {
                self.open_detail();
            }
        } else {
            let a = self.sel_start_y.min(self.sel_end_y);
            let b = self.sel_start_y.max(self.sel_end_y);
            self.pending_copy = Some((a, b));
        }
    }

    /// The inclusive row range currently being drag-selected, for highlighting.
    pub fn selection_range(&self) -> Option<(u16, u16)> {
        self.selecting.then(|| {
            (
                self.sel_start_y.min(self.sel_end_y),
                self.sel_start_y.max(self.sel_end_y),
            )
        })
    }
}
