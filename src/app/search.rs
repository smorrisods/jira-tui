//! The Search / go-to-issue screen: filtering the work list and jumping
//! directly to an issue by key, with a live JQL text-search fallback for
//! issues outside the currently loaded view (see `App::schedule_live_search`).

use std::collections::HashSet;

use crate::domain::{IssueSummary, Source};

use super::{async_ops, App, Screen};

/// How many run-loop iterations the query must sit unchanged before a live
/// search fires — restarted on every keystroke, so this is really "fire
/// this long after the user stops typing," not a fixed interval. `app.tick`
/// advances roughly every `main::TICK` (90ms), but also on any other event
/// (mouse motion, an unrelated fetch completing) — so this is an
/// approximate ~360ms debounce, not a precise one.
const SEARCH_DEBOUNCE_TICKS: u64 = 4;

/// Shortest query a live search fires for — long enough to keep an
/// almost-empty query from firing a near-unbounded `text ~ "x*"` search.
const MIN_LIVE_SEARCH_LEN: usize = 2;

/// A row in the Search screen: a direct "go to issue key" action, a match
/// against the current work list (index into `all_issues`), or a match the
/// live text-search fallback found beyond it (index into
/// `SearchState::live_results`).
#[derive(Clone, Debug)]
pub enum SearchRow {
    Goto(String),
    Match(usize),
    Live(usize),
}

/// The Search / go-to-issue screen's state.
#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub query: String,
    pub rows: Vec<SearchRow>,
    pub selected: usize,
    /// Screen to return to when Search is cancelled.
    pub return_to: Screen,
    /// Issues the live text-search fallback found, beyond what's already
    /// loaded into `all_issues`.
    pub live_results: Vec<IssueSummary>,
    /// The (trimmed, lowercased) query `live_results` answers. Compared
    /// against the live query on every rebuild so a result that lands after
    /// the user's kept typing doesn't get shown under text it no longer
    /// matches — the next debounced search overwrites it once it lands.
    pub live_query: Option<String>,
    /// Whether a live search is currently in flight.
    pub live_loading: bool,
    /// A query awaiting its debounce window before it's dispatched — see
    /// `App::ensure_search_dispatched`.
    pending_query: Option<String>,
    /// The tick `pending_query` becomes eligible to dispatch at.
    dispatch_at_tick: u64,
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

    /// Called every run-loop iteration (mirrors `App::ensure_quick_view_loaded`):
    /// if a debounced live search is due, dispatch it and clear the pending
    /// marker. A no-op almost every tick — only fires once the query's sat
    /// unchanged for `SEARCH_DEBOUNCE_TICKS`.
    pub fn ensure_search_dispatched(&mut self) {
        if self.screen != Screen::Search || self.search.pending_query.is_none() {
            return;
        }
        if self.tick < self.search.dispatch_at_tick {
            return;
        }
        let query = self.search.pending_query.take().unwrap();
        self.search.live_loading = true;
        self.search_generation += 1;
        async_ops::dispatch_text_search(self.events_tx.clone(), self.search_generation, query);
    }

    /// Applies `AppEvent::TextSearched` — see `dispatch_text_search`.
    /// Dropped if a newer live search has since been dispatched (without
    /// touching `live_loading` — that newer search is presumably still in
    /// flight and will clear it itself); otherwise merged in via
    /// `rebuild_search_rows`, which itself only shows `live_results` when
    /// `live_query` still matches what's on screen (see
    /// `SearchState::live_query`), so results for text the user has since
    /// typed past never flash into view.
    ///
    /// `status` is a different story: it's shared with every other screen,
    /// so writing to it is only appropriate while this result is still what
    /// the user is looking at — otherwise a search dispatched right before
    /// the user left Search (Esc, or confirming a match) would land later
    /// and stomp whatever status the screen they're on *now* is showing.
    /// Written on every still-relevant outcome, success included, so an
    /// earlier "no matches"/error message doesn't linger once a later
    /// query actually finds something.
    pub(crate) fn apply_text_searched(
        &mut self,
        generation: u64,
        query: String,
        issues: Vec<IssueSummary>,
        error: Option<String>,
    ) {
        if generation != self.search_generation {
            return;
        }
        self.search.live_loading = false;
        let still_relevant =
            self.screen == Screen::Search && self.search.query.trim().to_lowercase() == query;
        if still_relevant {
            self.status = match &error {
                Some(err) => format!("⚠ {err}"),
                None if issues.is_empty() => {
                    format!("live search: no matches for \"{query}\" beyond your current view")
                }
                None => format!(
                    "live search: {} more match(es) for \"{query}\"",
                    issues.len()
                ),
            };
        }
        self.search.live_results = issues;
        self.search.live_query = Some(query);
        self.rebuild_search_rows();
        self.search.selected = self
            .search
            .selected
            .min(self.search.rows.len().saturating_sub(1));
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
        self.schedule_live_search();
        self.rebuild_search_rows();
        self.search.selected = 0;
    }

    /// Decides whether the current query needs a live search, restarting
    /// the debounce window on every call (i.e. every keystroke) so a search
    /// only actually fires once typing pauses. Demo/cache sessions, and
    /// queries under `MIN_LIVE_SEARCH_LEN`, never schedule one — any
    /// previously shown live results are cleared instead, since they no
    /// longer answer the (now shorter or offline) query.
    fn schedule_live_search(&mut self) {
        let q = self.search.query.trim().to_lowercase();
        if !matches!(self.source, Source::Live { .. }) || q.chars().count() < MIN_LIVE_SEARCH_LEN {
            self.search.pending_query = None;
            self.search.live_loading = false;
            self.search.live_results.clear();
            self.search.live_query = None;
            return;
        }
        self.search.pending_query = Some(q);
        self.search.dispatch_at_tick = self.tick + SEARCH_DEBOUNCE_TICKS;
    }

    /// Rebuilds `search.rows` from the current query against both the
    /// locally loaded `all_issues` and (if still fresh — see
    /// `SearchState::live_query` — and the session is still genuinely live)
    /// the live search fallback's results, skipping any live match whose
    /// key is already shown locally or has already been added from
    /// `live_results` itself (a defensive dedup — Jira's paged results
    /// shouldn't repeat a key, but nothing guarantees it).
    fn rebuild_search_rows(&mut self) {
        let mut rows = Vec::new();
        if let Some(key) = self.search_key_candidate() {
            rows.push(SearchRow::Goto(key));
        }
        let q = self.search.query.trim().to_lowercase();
        let mut seen: HashSet<String> = HashSet::new();
        for (idx, issue) in self.all_issues.iter().enumerate() {
            if q.is_empty()
                || issue.key.to_lowercase().contains(&q)
                || issue.summary.to_lowercase().contains(&q)
            {
                rows.push(SearchRow::Match(idx));
                seen.insert(issue.key.to_lowercase());
            }
        }
        let live_available = matches!(self.source, Source::Live { .. });
        if live_available && self.search.live_query.as_deref() == Some(q.as_str()) {
            for (idx, issue) in self.search.live_results.iter().enumerate() {
                if seen.insert(issue.key.to_lowercase()) {
                    rows.push(SearchRow::Live(idx));
                }
            }
        }
        self.search.rows = rows;
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
            SearchRow::Live(idx) => {
                if let Some(issue) = self.search.live_results.get(idx) {
                    let key = issue.key.clone();
                    self.open_by_key(&key);
                }
            }
        }
    }
}
