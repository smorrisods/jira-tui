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

/// Mouse mode toggle + drag-selection state.
#[derive(Clone, Debug, Default)]
pub struct MouseState {
    pub enabled: bool,
    pub selecting: bool,
    pub sel_start_y: u16,
    pub sel_end_y: u16,
    /// Row range (inclusive, screen coords) whose text should be copied.
    pub pending_copy: Option<(u16, u16)>,
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
    /// `list_start` is a position within `tree_rows()` (which is just
    /// `0..issues.len()` in `Flat` mode), not a raw index into
    /// `self.issues` — see `ui::list::draw_list`. The area's first screen
    /// row (`y == area.y`) is the column header line, not a data row, so it
    /// maps to no issue; every row below it is offset back by one.
    pub fn list_index_at(&self, y: u16) -> Option<usize> {
        let area = self.list_area.get();
        if area.height == 0 || y <= area.y || y >= area.y + area.height {
            return None;
        }
        let pos = self.list_start.get() + (y - area.y - 1) as usize;
        self.tree_rows().get(pos).map(|(idx, _)| *idx)
    }

    /// Resolve a screen coordinate to the index of a navigable link (issue
    /// key/URL) under the cursor, in the full Detail screen or the
    /// quick-view panel. Best-effort: it maps a screen row directly to a
    /// rendered line index via `detail_scroll`/`quick_view_scroll` without
    /// accounting for line-wrapping, so it's most reliable on short field
    /// lines (`parent:`, the links section) rather than long wrapped
    /// description/comment text — those are still reachable via `{`/`}`
    /// keyboard cycling regardless of wrap.
    pub fn link_at(&self, x: u16, y: u16) -> Option<usize> {
        let (area, scroll) = if self.screen == Screen::Detail {
            (self.detail_area.get(), self.detail_scroll)
        } else if self.point_in_quick_view(x, y) {
            (self.quick_view_area.get(), self.quick_view_scroll)
        } else {
            return None;
        };
        if !Self::point_in(area, x, y) {
            return None;
        }
        let line = scroll as usize + (y - area.y) as usize;
        let col = (x - area.x) as usize;
        self.active_links()
            .iter()
            .position(|t| t.line == line && col >= t.start && col < t.end)
    }

    pub fn mouse_down(&mut self, y: u16) {
        if matches!(self.screen, Screen::Home | Screen::List) {
            if let Some(idx) = self.list_index_at(y) {
                self.selected = idx;
            }
        }
        self.mouse.selecting = true;
        self.mouse.sel_start_y = y;
        self.mouse.sel_end_y = y;
    }

    pub fn mouse_drag(&mut self, y: u16) {
        if self.mouse.selecting {
            self.mouse.sel_end_y = y;
        }
    }

    pub fn mouse_up(&mut self, x: u16, y: u16) {
        if !self.mouse.selecting {
            return;
        }
        self.mouse.selecting = false;
        self.mouse.sel_end_y = y;
        if self.mouse.sel_start_y == self.mouse.sel_end_y {
            // A click, not a drag: open the issue under the cursor, or —
            // in the Detail screen/quick-view panel — the link under it.
            if matches!(self.screen, Screen::Home | Screen::List) && self.list_index_at(y).is_some()
            {
                self.open_detail();
            } else if let Some(idx) = self.link_at(x, y) {
                self.link_index = idx;
                self.open_highlighted_link();
            }
        } else {
            let a = self.mouse.sel_start_y.min(self.mouse.sel_end_y);
            let b = self.mouse.sel_start_y.max(self.mouse.sel_end_y);
            self.mouse.pending_copy = Some((a, b));
        }
    }

    /// The inclusive row range currently being drag-selected, for highlighting.
    pub fn selection_range(&self) -> Option<(u16, u16)> {
        self.mouse.selecting.then(|| {
            (
                self.mouse.sel_start_y.min(self.mouse.sel_end_y),
                self.mouse.sel_start_y.max(self.mouse.sel_end_y),
            )
        })
    }
}
