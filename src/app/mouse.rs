//! Mouse mode: click-to-select/open, drag-to-copy, and keyboard/quick-view
//! focus tracking.

use crate::render::DetailPane;
use crate::ui::quick_view_columns::{meta_width_for, quick_view_layout_for_width, QuickViewLayout};

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

    /// Whether the point is over the mini-Jax footer dock (SPEC.md §9).
    /// Re-checks the same gates `ui::jax_companion::jax_mode` uses (not
    /// just the recorded `Rect`), so a stale area from a previous
    /// mini-showing frame can't misfire once `jax_popped` flips true (the
    /// full box pops out instead) or the screen changes to one where Jax
    /// is hidden entirely.
    pub fn point_in_jax_mini(&self, x: u16, y: u16) -> bool {
        !self.jax_popped
            && !matches!(self.screen, Screen::Welcome | Screen::Edit | Screen::About)
            && Self::point_in(self.jax_mini_area.get(), x, y)
    }

    /// `J`, or a click on the mini-Jax footer dock in mouse mode: pop the
    /// full Jax box out (or tuck it back away). See `jax_popped`'s doc
    /// comment for exactly what this flag means.
    pub fn toggle_jax(&mut self) {
        self.jax_popped = !self.jax_popped;
        self.status = if self.jax_popped {
            "Jax is here to keep you company 🦦".into()
        } else {
            "Jax went for a nap 😴".into()
        };
    }

    /// Toggle the quick-view panel (`v`, or a middle-click on Home/List —
    /// see `keys::mouse::handle_mouse`). Closing it forces keyboard focus
    /// back to the list, matching `toggle_list_focus`'s own rule, so arrow
    /// keys never end up stuck scrolling a now-hidden panel.
    pub fn toggle_quick_view(&mut self) {
        self.quick_view = !self.quick_view;
        if !self.quick_view {
            self.list_focus = ListFocus::List;
        }
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
    /// quick-view panel. Wrap-aware: `render::line_col_at_row` maps the
    /// clicked screen row back to the logical line/column a `LinkTarget`'s
    /// `line`/`start`/`end` were computed against, so this works on long
    /// wrapped description/comment text too, not just short field lines.
    ///
    /// On the Detail screen's wide layout, both the main column and the
    /// four side-rail panels (workflow/meta/links/children — deliberately
    /// non-scrolling, see `ui::detail::draw_rail`) are clickable. The
    /// returned index is still an index into `active_links()`'s full
    /// cross-pane ordering, so it stays consistent with
    /// `next_link`/`prev_link`/highlighting.
    pub fn link_at(&self, x: u16, y: u16) -> Option<usize> {
        if self.screen == Screen::Detail {
            if let Some(idx) = self.link_at_pane(
                x,
                y,
                DetailPane::Main,
                self.detail_main_area.get(),
                self.detail_scroll as usize,
            ) {
                return Some(idx);
            }
            for (pane, area) in [
                (DetailPane::Workflow, self.detail_workflow_area.get()),
                (DetailPane::Meta, self.detail_meta_area.get()),
                (DetailPane::Links, self.detail_links_area.get()),
                (DetailPane::Children, self.detail_children_area.get()),
            ] {
                if let Some(idx) = self.link_at_pane(x, y, pane, area, 0) {
                    return Some(idx);
                }
            }
            return None;
        }
        if self.point_in_quick_view(x, y) {
            let area = self.quick_view_area.get();
            let col = (x - area.x) as usize;
            // Wide quick view's meta column (to the right) isn't
            // independently scrolled and has no `Rect` of its own recorded
            // for hit-testing yet — restrict matches to the description
            // pane so a click in the meta column can't coincidentally
            // resolve to the wrong link.
            if quick_view_layout_for_width(area.width) == QuickViewLayout::Wide {
                let desc_width = area.width.saturating_sub(meta_width_for(area.width)) as usize;
                if col >= desc_width {
                    return None;
                }
            }
            return self.link_at_pane(
                x,
                y,
                DetailPane::Main,
                area,
                self.quick_view_scroll as usize,
            );
        }
        None
    }

    /// Hit-tests one pane's recorded area: maps the click to a wrapped
    /// row/column via `active_pane_lines(pane)`, then `render::line_col_at_row`
    /// back to a logical line/column, then finds the matching `LinkTarget`
    /// in `active_links()`. `scroll` is a row offset into the pane's own
    /// wrapped content (0 for the non-scrolling rail panels).
    fn link_at_pane(
        &self,
        x: u16,
        y: u16,
        pane: DetailPane,
        area: ratatui::layout::Rect,
        scroll: usize,
    ) -> Option<usize> {
        if !Self::point_in(area, x, y) {
            return None;
        }
        let lines = self.active_pane_lines(pane)?;
        let width = area.width as usize;
        let row = scroll + (y - area.y) as usize;
        let col = (x - area.x) as usize;
        let (line, col) = crate::render::line_col_at_row(&lines, width, row, col)?;
        self.active_links()
            .iter()
            .position(|t| t.pane == pane && t.line == line && col >= t.start && col < t.end)
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
            } else if self.point_in_jax_mini(x, y) {
                self.toggle_jax();
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
