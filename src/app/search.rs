//! The Search / go-to-issue screen: filtering the work list and jumping
//! directly to an issue by key.

use super::{App, Screen};

/// A row in the Search screen: either a direct "go to issue key" action or a
/// match against the current work list (index into `all_issues`).
#[derive(Clone, Debug)]
pub enum SearchRow {
    Goto(String),
    Match(usize),
}

/// The Search / go-to-issue screen's state.
#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub query: String,
    pub rows: Vec<SearchRow>,
    pub selected: usize,
    /// Screen to return to when Search is cancelled.
    pub return_to: Screen,
}

impl App {
    /// Open the Search screen, remembering where to return on cancel.
    pub fn open_search(&mut self) {
        self.search.return_to = self.screen;
        self.search.query.clear();
        self.recompute_search();
        self.screen = Screen::Search;
    }

    pub fn close_search(&mut self) {
        self.screen = self.search.return_to;
    }

    pub fn search_input_char(&mut self, c: char) {
        self.search.query.push(c);
        self.recompute_search();
    }

    pub fn search_backspace(&mut self) {
        self.search.query.pop();
        self.recompute_search();
    }

    /// If the query looks like an issue key (`LETTERS-DIGITS`), return it
    /// normalised to uppercase — this powers the "go to issue" shortcut.
    pub fn search_key_candidate(&self) -> Option<String> {
        let q = self.search.query.trim();
        if q.is_empty() {
            return None;
        }
        let (letters, rest) = q.split_once('-')?;
        if !letters.is_empty()
            && letters.chars().all(|c| c.is_ascii_alphabetic())
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_digit())
        {
            Some(format!("{}-{}", letters.to_uppercase(), rest))
        } else {
            None
        }
    }

    fn recompute_search(&mut self) {
        let mut rows = Vec::new();
        if let Some(key) = self.search_key_candidate() {
            rows.push(SearchRow::Goto(key));
        }
        let q = self.search.query.trim().to_lowercase();
        for (idx, issue) in self.all_issues.iter().enumerate() {
            if q.is_empty()
                || issue.key.to_lowercase().contains(&q)
                || issue.summary.to_lowercase().contains(&q)
            {
                rows.push(SearchRow::Match(idx));
            }
        }
        self.search.rows = rows;
        self.search.selected = 0;
    }

    pub fn search_move(&mut self, delta: isize) {
        if self.search.rows.is_empty() {
            return;
        }
        let len = self.search.rows.len() as isize;
        let mut idx = self.search.selected as isize + delta;
        idx = idx.clamp(0, len - 1);
        self.search.selected = idx as usize;
    }

    /// Open whatever is highlighted in the Search screen: a direct "go to
    /// issue" jump, or the selected match from the work list.
    pub fn confirm_search(&mut self) {
        let Some(row) = self.search.rows.get(self.search.selected).cloned() else {
            return;
        };
        match row {
            SearchRow::Goto(key) => self.open_by_key(&key),
            SearchRow::Match(idx) => {
                if let Some(issue) = self.all_issues.get(idx) {
                    let key = issue.key.clone();
                    self.open_by_key(&key);
                }
            }
        }
    }
}
