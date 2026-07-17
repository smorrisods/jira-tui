//! The swimlane Kanban board: status columns, Epic-grouped lanes, and 2D
//! (lane/column/card) navigation.

use crate::domain::IssueSummary;
use crate::ui::board_columns::{self, BoardLayout, CARD_HEIGHT};

use super::{App, Screen};

/// A status column's name and how many of a lane's issues sit in it —
/// returned by `App::board_neighbour_counts` for the narrow pager's peek
/// line.
pub(crate) type ColumnCount = (String, usize);

/// Cursor position within the board: which swimlane, status column, and card
/// (top-to-bottom) within that lane/column cell.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoardSelection {
    pub lane: usize,
    pub col: usize,
    pub card: usize,
}

impl App {
    /// Preferred left-to-right column order; anything else present is
    /// appended afterwards in alphabetical order.
    const BOARD_PREFERRED_COLUMNS: [&'static str; 5] =
        ["Backlog", "To Do", "In Progress", "In Review", "Done"];

    pub fn open_board(&mut self) {
        self.screen = Screen::Board;
        self.board_scroll = 0;
        self.board_clamp();
    }

    /// Status columns present in the current (filtered/sorted) view, in
    /// workflow order.
    pub fn board_columns(&self) -> Vec<String> {
        let mut cols: Vec<String> = Vec::new();
        for p in Self::BOARD_PREFERRED_COLUMNS {
            if self.issues.iter().any(|i| i.status == p) {
                cols.push(p.to_string());
            }
        }
        let mut others: Vec<String> = Vec::new();
        for i in &self.issues {
            if !Self::BOARD_PREFERRED_COLUMNS.contains(&i.status.as_str())
                && !others.contains(&i.status)
            {
                others.push(i.status.clone());
            }
        }
        others.sort();
        cols.extend(others);
        if cols.is_empty() {
            cols.push("No status".to_string());
        }
        cols
    }

    /// Swimlanes (grouped by parent/Epic key), in first-seen order, with a
    /// trailing "no epic" lane (`None`) when any issue lacks one.
    pub fn board_lanes(&self) -> Vec<Option<String>> {
        let mut lanes: Vec<Option<String>> = Vec::new();
        let mut has_none = false;
        for i in &self.issues {
            match &i.epic {
                Some(e) => {
                    if !lanes.iter().any(|l| l.as_deref() == Some(e.as_str())) {
                        lanes.push(Some(e.clone()));
                    }
                }
                None => has_none = true,
            }
        }
        if has_none || lanes.is_empty() {
            lanes.push(None);
        }
        lanes
    }

    /// The issues in a given lane/column cell, in current sort order.
    pub fn board_cell(&self, lane: &Option<String>, status: &str) -> Vec<&IssueSummary> {
        self.issues
            .iter()
            .filter(|i| &i.epic == lane && i.status == status)
            .collect()
    }

    /// A human label for a swimlane header, using a cached Epic summary when
    /// we've already loaded it.
    pub fn board_lane_label(&self, lane: &Option<String>) -> String {
        match lane {
            None => "No epic".to_string(),
            Some(key) => match self.detail_cache.get(key) {
                Some(detail) => format!("{key} · {}", detail.summary),
                None => key.clone(),
            },
        }
    }

    fn board_clamp(&mut self) {
        let lanes = self.board_lanes();
        if self.board_sel.lane >= lanes.len() {
            self.board_sel.lane = 0;
        }
        let cols = self.board_columns();
        if self.board_sel.col >= cols.len() {
            self.board_sel.col = 0;
        }
        let len = lanes
            .get(self.board_sel.lane)
            .zip(cols.get(self.board_sel.col))
            .map(|(lane, status)| self.board_cell(lane, status).len())
            .unwrap_or(0);
        if self.board_sel.card >= len {
            self.board_sel.card = len.saturating_sub(1);
        }
    }

    pub fn board_move_card(&mut self, delta: isize) {
        let lanes = self.board_lanes();
        let cols = self.board_columns();
        let Some(lane) = lanes.get(self.board_sel.lane) else {
            return;
        };
        let Some(status) = cols.get(self.board_sel.col) else {
            return;
        };
        let len = self.board_cell(lane, status).len();
        if len == 0 {
            return;
        }
        let idx = (self.board_sel.card as isize + delta).clamp(0, len as isize - 1);
        self.board_sel.card = idx as usize;
        self.board_ensure_visible();
    }

    pub fn board_move_col(&mut self, delta: isize) {
        let cols = self.board_columns();
        if cols.is_empty() {
            return;
        }
        let idx = (self.board_sel.col as isize + delta).clamp(0, cols.len() as isize - 1);
        self.board_sel.col = idx as usize;
        self.board_sel.card = 0;
        self.board_ensure_visible();
    }

    pub fn board_move_lane(&mut self, delta: isize) {
        let lanes = self.board_lanes();
        if lanes.is_empty() {
            return;
        }
        let idx = (self.board_sel.lane as isize + delta).clamp(0, lanes.len() as isize - 1);
        self.board_sel.lane = idx as usize;
        self.board_sel.card = 0;
        self.board_ensure_visible();
    }

    pub fn board_scroll_by(&mut self, delta: isize) {
        let new = self.board_scroll as isize + delta;
        self.board_scroll = new.max(0) as u16;
    }

    /// Partitions swimlanes into "render expanded" vs "fold into the
    /// collapsed summary count", by a caller-supplied predicate — the
    /// mechanism shared by both Board layouts (SPEC.md §7): the wide grid
    /// collapses lanes that are 100% in the terminal column
    /// (`board_wide_lanes`), the narrow pager collapses lanes with nothing
    /// in the currently paged-to column (`board_narrow_lanes`). The
    /// selected lane is always exempted — collapsing it would hide the
    /// current selection with no way to see it, other than the general
    /// "pgdn reveals whatever the selection lands on" rule this enables.
    fn board_visible_lanes(
        &self,
        collapse: impl Fn(&Option<String>) -> bool,
    ) -> (Vec<Option<String>>, usize) {
        let lanes = self.board_lanes();
        let mut visible = Vec::new();
        let mut hidden = 0usize;
        for (i, lane) in lanes.into_iter().enumerate() {
            if i != self.board_sel.lane && collapse(&lane) {
                hidden += 1;
            } else {
                visible.push(lane);
            }
        }
        (visible, hidden)
    }

    /// Wide layout (SPEC.md §7: "Fully-done lanes collapse behind `pgdn`").
    /// A lane is "fully done" when every issue in it has the workflow's
    /// "Done" status — preferring the literal `"Done"` column when present
    /// (`board_columns` appends any non-preferred status alphabetically
    /// *after* "Done", so `cols.last()` is only "Done" when the workflow
    /// has no custom statuses at all) and falling back to the last column
    /// positionally for a workflow that doesn't use that name.
    pub(crate) fn board_wide_lanes(&self) -> (Vec<Option<String>>, usize) {
        let cols = self.board_columns();
        let Some(done) = cols
            .iter()
            .find(|c| c.as_str() == "Done")
            .or_else(|| cols.last())
            .cloned()
        else {
            return (self.board_lanes(), 0);
        };
        self.board_visible_lanes(|lane| {
            let mut issues = self.issues.iter().filter(|i| &i.epic == lane).peekable();
            issues.peek().is_some() && issues.all(|i| i.status == done)
        })
    }

    /// Narrow layout (SPEC.md §7: "Lanes with nothing in the current column
    /// collapse to one ghost line").
    pub(crate) fn board_narrow_lanes(&self, status: &str) -> (Vec<Option<String>>, usize) {
        self.board_visible_lanes(|lane| self.board_cell(lane, status).is_empty())
    }

    /// Narrow pager's per-lane header counts: how many of this lane's
    /// issues are in the currently-paged-to column, out of its total.
    pub(crate) fn board_lane_counts(&self, lane: &Option<String>, status: &str) -> (usize, usize) {
        let here = self.board_cell(lane, status).len();
        let total = self.issues.iter().filter(|i| &i.epic == lane).count();
        (here, total)
    }

    /// Narrow pager's selected-card neighbour-peek: this lane's issue count
    /// in the immediately adjacent columns, `None` at the first/last column.
    pub(crate) fn board_neighbour_counts(
        &self,
        lane: &Option<String>,
    ) -> (Option<ColumnCount>, Option<ColumnCount>) {
        let cols = self.board_columns();
        let col = self.board_sel.col;
        let prev = col
            .checked_sub(1)
            .and_then(|i| cols.get(i))
            .map(|s| (s.clone(), self.board_cell(lane, s).len()));
        let next = cols
            .get(col + 1)
            .map(|s| (s.clone(), self.board_cell(lane, s).len()));
        (prev, next)
    }

    /// The deepest column cell in a lane, for the wide grid — the number of
    /// stacked card slots every column in that lane's row gets, floored at
    /// 1 so an all-empty lane (shouldn't happen per `board_lanes`'s own
    /// invariant, but not relied on here) still reserves a row. Shared by
    /// `board_lane_height` and `ui::board::draw_wide_lane` so the height
    /// reserved for a lane and the grid actually drawn into it can never
    /// disagree — two independent computations of this exact number is
    /// exactly the kind of drift that silently clips or misaligns content.
    pub(crate) fn board_max_rows_wide(&self, lane: &Option<String>) -> usize {
        self.board_columns()
            .iter()
            .map(|s| self.board_cell(lane, s).len())
            .max()
            .unwrap_or(0)
            .max(1)
    }

    /// How many rows a lane's band occupies in a given layout — shared by
    /// the renderer (to decide how many lanes fit) and `board_ensure_visible`
    /// (to scroll the selection into view), so the two can never disagree.
    pub(crate) fn board_lane_height(&self, lane: &Option<String>, layout: BoardLayout) -> u16 {
        let cols = self.board_columns();
        match layout {
            BoardLayout::Wide => 1 + self.board_max_rows_wide(lane) as u16 * CARD_HEIGHT,
            BoardLayout::Narrow => {
                let status = cols.get(self.board_sel.col).cloned().unwrap_or_default();
                let n = self.board_cell(lane, &status).len().max(1);
                let extra = if self.board_lanes().get(self.board_sel.lane) == Some(lane) {
                    1
                } else {
                    0
                };
                1 + n as u16 * CARD_HEIGHT + extra
            }
        }
    }

    /// Scroll the board so the current selection's lane is visible.
    /// `board_scroll` is the index of the first visible lane (not a
    /// text-row offset, now that cards are multi-row bordered blocks rather
    /// than single text rows) — keyboard navigation
    /// (`board_move_card`/`_col`/`_lane`) has no access to the render-time
    /// viewport height, so this reads back the area recorded during the
    /// last `draw_board` call.
    fn board_ensure_visible(&mut self) {
        let area = self.board_area.get();
        if area.height == 0 {
            return;
        }
        // The renderer splits off a 1-row column-header/tab-strip line
        // before handing the rest to its own "how many lanes fit" pass
        // (`fit_lanes` in `ui::board`) — match that exactly, or this can
        // conclude a lane fits when the renderer's smaller budget would
        // drop it, silently scrolling the selection off-screen with no
        // further keypress able to fix it (both budgets must agree on what
        // "fits" means for the same reason `board_lane_height` is shared
        // between renderer and scroll code at all).
        let body_height = area.height.saturating_sub(1);
        if body_height == 0 {
            return;
        }
        let layout = board_columns::board_layout_for_width(area.width);
        let (visible, _) = match layout {
            BoardLayout::Wide => self.board_wide_lanes(),
            BoardLayout::Narrow => {
                let status = self
                    .board_columns()
                    .get(self.board_sel.col)
                    .cloned()
                    .unwrap_or_default();
                self.board_narrow_lanes(&status)
            }
        };
        let all = self.board_lanes();
        let Some(pos) = all
            .get(self.board_sel.lane)
            .and_then(|sel| visible.iter().position(|l| l == sel))
        else {
            return;
        };
        let scroll = self.board_scroll as usize;
        if pos < scroll {
            self.board_scroll = pos as u16;
            return;
        }
        let mut used = 0u16;
        let mut last_fit = scroll;
        for (i, lane) in visible.iter().enumerate().skip(scroll) {
            let h = self.board_lane_height(lane, layout) + 1;
            if used + h > body_height && i > scroll {
                break;
            }
            used += h;
            last_fit = i;
        }
        if pos > last_fit {
            self.board_scroll = pos as u16;
        }
    }

    /// Open the currently selected card's issue.
    pub fn board_open(&mut self) {
        match self.board_selected_issue() {
            Some(issue) => {
                let key = issue.key.clone();
                self.open_by_key(&key);
            }
            None => self.status = "no card here".into(),
        }
    }

    /// The card currently selected on the Board screen, if any — shared by
    /// `board_open` and the command palette's context resolver
    /// (`app::palette`, SPEC.md §8), which needs the same lane/column/cell
    /// lookup Board itself uses to answer "what issue is this palette
    /// acting on" when opened from Board.
    pub(crate) fn board_selected_issue(&self) -> Option<&IssueSummary> {
        let lanes = self.board_lanes();
        let cols = self.board_columns();
        let lane = lanes.get(self.board_sel.lane)?;
        let status = cols.get(self.board_sel.col)?;
        self.board_cell(lane, status)
            .get(self.board_sel.card)
            .copied()
    }
}
