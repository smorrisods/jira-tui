//! The swimlane Kanban board: status columns, Epic-grouped lanes, and 2D
//! (lane/column/card) navigation.

use crate::domain::IssueSummary;

use super::{App, Screen};

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
    }

    pub fn board_move_col(&mut self, delta: isize) {
        let cols = self.board_columns();
        if cols.is_empty() {
            return;
        }
        let idx = (self.board_sel.col as isize + delta).clamp(0, cols.len() as isize - 1);
        self.board_sel.col = idx as usize;
        self.board_sel.card = 0;
    }

    pub fn board_move_lane(&mut self, delta: isize) {
        let lanes = self.board_lanes();
        if lanes.is_empty() {
            return;
        }
        let idx = (self.board_sel.lane as isize + delta).clamp(0, lanes.len() as isize - 1);
        self.board_sel.lane = idx as usize;
        self.board_sel.card = 0;
    }

    pub fn board_scroll_by(&mut self, delta: isize) {
        let new = self.board_scroll as isize + delta;
        self.board_scroll = new.max(0) as u16;
    }

    /// Open the currently selected card's issue.
    pub fn board_open(&mut self) {
        let lanes = self.board_lanes();
        let cols = self.board_columns();
        let Some(lane) = lanes.get(self.board_sel.lane) else {
            return;
        };
        let Some(status) = cols.get(self.board_sel.col) else {
            return;
        };
        let cell = self.board_cell(lane, status);
        match cell.get(self.board_sel.card) {
            Some(issue) => {
                let key = issue.key.clone();
                self.open_by_key(&key);
            }
            None => self.status = "no card here".into(),
        }
    }
}
