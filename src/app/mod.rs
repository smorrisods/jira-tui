//! Application state, event handling, and the data-loading glue.

use std::cell::Cell;
use std::collections::HashMap;

use ratatui::layout::Rect;

use crate::config::{self, Settings};
use crate::domain::{demo_detail, demo_issues, IssueDetail, IssueSummary, Source};
use crate::git::GitContext;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Welcome,
    Home,
    List,
    Detail,
    Preview,
    Edit,
    Search,
    Board,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Updated,
    Priority,
    Status,
    Key,
}

impl SortKey {
    pub fn label(&self) -> &'static str {
        match self {
            SortKey::Updated => "updated",
            SortKey::Priority => "priority",
            SortKey::Status => "status",
            SortKey::Key => "key",
        }
    }
    fn next(&self) -> SortKey {
        match self {
            SortKey::Updated => SortKey::Priority,
            SortKey::Priority => SortKey::Status,
            SortKey::Status => SortKey::Key,
            SortKey::Key => SortKey::Updated,
        }
    }
}

/// Which panel arrow keys/PageUp/PageDown affect when the quick-view panel is
/// open; toggled with `Tab`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ListFocus {
    #[default]
    List,
    QuickView,
}

/// A row in the Search screen: either a direct "go to issue key" action or a
/// match against the current work list (index into `all_issues`).
#[derive(Clone, Debug)]
pub enum SearchRow {
    Goto(String),
    Match(usize),
}

/// Cursor position within the board: which swimlane, status column, and card
/// (top-to-bottom) within that lane/column cell.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoardSelection {
    pub lane: usize,
    pub col: usize,
    pub card: usize,
}

pub struct App {
    /// Full server-side list; `issues` is the filtered + sorted view of this.
    pub all_issues: Vec<IssueSummary>,
    pub issues: Vec<IssueSummary>,
    pub selected: usize,
    pub screen: Screen,
    pub detail: Option<IssueDetail>,
    pub detail_scroll: u16,
    pub source: Source,
    pub git: GitContext,
    pub tick: u64,
    pub status: String,
    pub show_help: bool,
    pub should_quit: bool,

    // Sort + filter.
    pub sort_key: SortKey,
    pub sort_asc: bool,
    pub filter_status: Option<String>,

    // Quick-view panel + a cache of opened issue details.
    pub quick_view: bool,
    pub quick_view_scroll: u16,
    pub list_focus: ListFocus,
    pub detail_cache: HashMap<String, IssueDetail>,

    // Search / go-to-issue.
    pub search_query: String,
    pub search_rows: Vec<SearchRow>,
    pub search_selected: usize,
    /// Screen to return to when Search is cancelled.
    pub search_return_to: Screen,

    // Swimlane board.
    pub board_sel: BoardSelection,
    pub board_scroll: u16,

    /// Ambient Jax companion (pure entertainment 🦦).
    pub show_jax: bool,

    // In-TUI editor.
    pub editor: EditorState,

    /// Transient toast message; shown while `tick < flash_until`.
    pub flash_msg: String,
    pub flash_until: u64,

    // Mouse mode + drag selection.
    pub mouse_enabled: bool,
    pub selecting: bool,
    pub sel_start_y: u16,
    pub sel_end_y: u16,
    /// Row range (inclusive, screen coords) whose text should be copied.
    pub pending_copy: Option<(u16, u16)>,

    // Draw geometry recorded during render, for mapping mouse coordinates.
    pub list_area: Cell<Rect>,
    pub list_start: Cell<usize>,
    pub detail_area: Cell<Rect>,
    pub quick_view_area: Cell<Rect>,

    // Onboarding welcome + credential setup.
    pub welcome_phase: WelcomePhase,
    pub field_site: String,
    pub field_email: String,
    pub field_token: String,
    pub focus: Field,
    pub setup_msg: String,

    // Transition picker + round-trip edit.
    pub picker_open: bool,
    pub picker_index: usize,
    pub pending_edit: Option<serde_json::Value>,
    /// Set by a key handler to ask the run loop to launch `$EDITOR`.
    pub request_edit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WelcomePhase {
    Intro,
    Setup,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Site,
    Email,
    Token,
}

/// A minimal multi-line text editor for in-TUI description editing.
#[derive(Clone, Debug, Default)]
pub struct EditorState {
    pub lines: Vec<String>,
    pub cx: usize,
    pub cy: usize,
    pub scroll: u16,
}

impl EditorState {
    pub fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        EditorState {
            lines,
            cx: 0,
            cy: 0,
            scroll: 0,
        }
    }

    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    fn line_len(&self, y: usize) -> usize {
        self.lines.get(y).map(|l| l.chars().count()).unwrap_or(0)
    }

    pub fn insert_char(&mut self, c: char) {
        let line = &mut self.lines[self.cy];
        let byte = line
            .char_indices()
            .nth(self.cx)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        line.insert(byte, c);
        self.cx += 1;
    }

    pub fn newline(&mut self) {
        let line = self.lines[self.cy].clone();
        let byte = line
            .char_indices()
            .nth(self.cx)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        let (left, right) = line.split_at(byte);
        self.lines[self.cy] = left.to_string();
        self.lines.insert(self.cy + 1, right.to_string());
        self.cy += 1;
        self.cx = 0;
    }

    pub fn backspace(&mut self) {
        if self.cx > 0 {
            let line = &mut self.lines[self.cy];
            let byte = line
                .char_indices()
                .nth(self.cx - 1)
                .map(|(i, _)| i)
                .unwrap();
            line.remove(byte);
            self.cx -= 1;
        } else if self.cy > 0 {
            let removed = self.lines.remove(self.cy);
            self.cy -= 1;
            self.cx = self.line_len(self.cy);
            self.lines[self.cy].push_str(&removed);
        }
    }

    pub fn left(&mut self) {
        if self.cx > 0 {
            self.cx -= 1;
        } else if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.line_len(self.cy);
        }
    }

    pub fn right(&mut self) {
        if self.cx < self.line_len(self.cy) {
            self.cx += 1;
        } else if self.cy + 1 < self.lines.len() {
            self.cy += 1;
            self.cx = 0;
        }
    }

    pub fn up(&mut self) {
        if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.cx.min(self.line_len(self.cy));
        }
    }

    pub fn down(&mut self) {
        if self.cy + 1 < self.lines.len() {
            self.cy += 1;
            self.cx = self.cx.min(self.line_len(self.cy));
        }
    }
}

impl App {
    pub fn new(force_demo: bool) -> Self {
        let git = GitContext::detect();
        let (issues, source, status) = load_issues(force_demo);
        let settings = Settings::load();

        let mut app = App {
            all_issues: issues.clone(),
            issues,
            selected: 0,
            screen: if config::is_onboarded() {
                Screen::Home
            } else {
                Screen::Welcome
            },
            detail: None,
            detail_scroll: 0,
            source,
            git,
            tick: 0,
            status,
            show_help: false,
            should_quit: false,
            sort_key: SortKey::Updated,
            sort_asc: false,
            filter_status: None,
            quick_view: false,
            quick_view_scroll: 0,
            list_focus: ListFocus::List,
            detail_cache: HashMap::new(),
            search_query: String::new(),
            search_rows: Vec::new(),
            search_selected: 0,
            search_return_to: Screen::Home,
            board_sel: BoardSelection::default(),
            board_scroll: 0,
            show_jax: false,
            editor: EditorState::default(),
            flash_msg: String::new(),
            flash_until: 0,
            mouse_enabled: settings.mouse,
            selecting: false,
            sel_start_y: 0,
            sel_end_y: 0,
            pending_copy: None,
            list_area: Cell::new(Rect::default()),
            list_start: Cell::new(0),
            detail_area: Cell::new(Rect::default()),
            quick_view_area: Cell::new(Rect::default()),
            welcome_phase: WelcomePhase::Intro,
            field_site: "https://ontariodotca.atlassian.net".to_string(),
            field_email: String::new(),
            field_token: String::new(),
            focus: Field::Site,
            setup_msg: String::new(),
            picker_open: false,
            picker_index: 0,
            pending_edit: None,
            request_edit: false,
        };
        app.recompute_view();

        // If the current branch maps to a known issue, pre-select it.
        if let Some(key) = app.git.issue_key.clone() {
            if let Some(idx) = app.issues.iter().position(|i| i.key == key) {
                app.selected = idx;
            }
        }
        app
    }

    // ── Sort + filter ────────────────────────────────────────────────────────

    /// Rebuild `issues` from `all_issues` applying the current filter and sort,
    /// preserving the selected issue by key where possible.
    pub fn recompute_view(&mut self) {
        let cur_key = self.selected_issue().map(|i| i.key.clone());

        let mut view: Vec<IssueSummary> = self
            .all_issues
            .iter()
            .filter(|i| {
                self.filter_status
                    .as_ref()
                    .map(|s| &i.status == s)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        let key_num = |k: &str| -> u64 {
            k.rsplit('-')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0)
        };
        view.sort_by(|a, b| {
            let ord = match self.sort_key {
                SortKey::Updated => a.updated.cmp(&b.updated),
                SortKey::Priority => a.priority.rank().cmp(&b.priority.rank()),
                SortKey::Status => a.status.cmp(&b.status),
                SortKey::Key => key_num(&a.key).cmp(&key_num(&b.key)),
            };
            if self.sort_asc {
                ord
            } else {
                ord.reverse()
            }
        });

        self.issues = view;
        self.selected = cur_key
            .and_then(|k| self.issues.iter().position(|i| i.key == k))
            .unwrap_or(0)
            .min(self.issues.len().saturating_sub(1));
    }

    pub fn cycle_sort(&mut self) {
        self.sort_key = self.sort_key.next();
        self.recompute_view();
        self.status = format!(
            "sort: {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        );
    }

    pub fn toggle_sort_dir(&mut self) {
        self.sort_asc = !self.sort_asc;
        self.recompute_view();
        self.status = format!(
            "sort: {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        );
    }

    /// Cycle the status filter through: all → each distinct status → all.
    pub fn cycle_filter(&mut self) {
        let mut statuses: Vec<String> = Vec::new();
        for i in &self.all_issues {
            if !statuses.contains(&i.status) {
                statuses.push(i.status.clone());
            }
        }
        statuses.sort();
        self.filter_status = match &self.filter_status {
            None => statuses.first().cloned(),
            Some(cur) => {
                let idx = statuses.iter().position(|s| s == cur);
                match idx {
                    Some(i) if i + 1 < statuses.len() => Some(statuses[i + 1].clone()),
                    _ => None,
                }
            }
        };
        self.recompute_view();
        self.status = match &self.filter_status {
            Some(s) => format!("filter: {s}"),
            None => "filter: all".into(),
        };
    }

    pub fn sort_label(&self) -> String {
        format!(
            "sort {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        )
    }

    pub fn filter_label(&self) -> Option<String> {
        self.filter_status.as_ref().map(|s| format!("filter {s}"))
    }

    pub fn selected_issue(&self) -> Option<&IssueSummary> {
        self.issues.get(self.selected)
    }

    /// Show a transient toast for roughly 1.5s (tied to the animation tick).
    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash_msg = msg.into();
        self.flash_until = self.tick + 18;
    }

    /// The active toast message, if one is currently showing.
    pub fn active_flash(&self) -> Option<&str> {
        if self.tick < self.flash_until && !self.flash_msg.is_empty() {
            Some(&self.flash_msg)
        } else {
            None
        }
    }

    /// The Jira URL for the selected issue, when we know the site.
    pub fn selected_issue_url(&self) -> Option<String> {
        let issue = self.selected_issue()?;
        let site = match &self.source {
            Source::Live { site, .. } => site.clone(),
            _ => "ontariodotca.atlassian.net".to_string(),
        };
        Some(format!("https://{site}/browse/{}", issue.key))
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.issues.is_empty() {
            return;
        }
        let len = self.issues.len() as isize;
        let mut idx = self.selected as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len {
            idx = len - 1;
        }
        self.selected = idx as usize;
        self.quick_view_scroll = 0;
    }

    pub fn assigned_to_me(&self) -> Vec<&IssueSummary> {
        self.all_issues
            .iter()
            .filter(|i| i.assignee.is_some() && i.status != "Done")
            .collect()
    }

    pub fn blocked(&self) -> Vec<&IssueSummary> {
        self.all_issues.iter().filter(|i| i.blocked).collect()
    }

    pub fn open_detail(&mut self) {
        let Some(issue) = self.selected_issue().cloned() else {
            return;
        };
        self.open_by_key(&issue.key);
    }

    /// Load and open an issue by key directly, regardless of whether it's in
    /// the current filtered/sorted view. Used by search results and "go to
    /// issue". If the key is present in the current view, `selected` is
    /// synced so back-navigation lands somewhere sensible.
    pub fn open_by_key(&mut self, key: &str) {
        self.detail_scroll = 0;
        let detail = self.load_detail(key);
        self.detail_cache.insert(key.to_string(), detail.clone());
        self.detail = Some(detail);
        self.screen = Screen::Detail;
        if let Some(pos) = self.issues.iter().position(|i| i.key == key) {
            self.selected = pos;
        }
    }

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

    // ── Search + go to issue ─────────────────────────────────────────────────

    /// Open the Search screen, remembering where to return on cancel.
    pub fn open_search(&mut self) {
        self.search_return_to = self.screen;
        self.search_query.clear();
        self.recompute_search();
        self.screen = Screen::Search;
    }

    pub fn close_search(&mut self) {
        self.screen = self.search_return_to;
    }

    pub fn search_input_char(&mut self, c: char) {
        self.search_query.push(c);
        self.recompute_search();
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.recompute_search();
    }

    /// If the query looks like an issue key (`LETTERS-DIGITS`), return it
    /// normalised to uppercase — this powers the "go to issue" shortcut.
    pub fn search_key_candidate(&self) -> Option<String> {
        let q = self.search_query.trim();
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
        let q = self.search_query.trim().to_lowercase();
        for (idx, issue) in self.all_issues.iter().enumerate() {
            if q.is_empty()
                || issue.key.to_lowercase().contains(&q)
                || issue.summary.to_lowercase().contains(&q)
            {
                rows.push(SearchRow::Match(idx));
            }
        }
        self.search_rows = rows;
        self.search_selected = 0;
    }

    pub fn search_move(&mut self, delta: isize) {
        if self.search_rows.is_empty() {
            return;
        }
        let len = self.search_rows.len() as isize;
        let mut idx = self.search_selected as isize + delta;
        idx = idx.clamp(0, len - 1);
        self.search_selected = idx as usize;
    }

    /// Open whatever is highlighted in the Search screen: a direct "go to
    /// issue" jump, or the selected match from the work list.
    pub fn confirm_search(&mut self) {
        let Some(row) = self.search_rows.get(self.search_selected).cloned() else {
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

    // ── Swimlane board ───────────────────────────────────────────────────────

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

    // ── In-TUI editor ────────────────────────────────────────────────────────

    /// Open the built-in editor preloaded with the description Markdown.
    pub fn begin_tui_edit(&mut self) {
        if let Some(md) = self.description_markdown() {
            self.editor = EditorState::from_text(&md);
            self.screen = Screen::Edit;
        }
    }

    /// Compile the editor buffer and move to the confirmation preview.
    pub fn commit_tui_edit(&mut self) {
        let text = self.editor.to_text();
        self.finish_edit(&text);
    }

    pub fn open_transitions(&mut self) {
        if let Some(detail) = self.detail.as_ref() {
            if detail.transitions.is_empty() {
                self.status = "no transitions available".into();
                return;
            }
            // Pre-select the current status if present.
            self.picker_index = detail
                .transitions
                .iter()
                .position(|t| t.to == detail.status)
                .unwrap_or(0);
            self.picker_open = true;
        }
    }

    pub fn close_picker(&mut self) {
        self.picker_open = false;
    }

    pub fn picker_move(&mut self, delta: isize) {
        let len = self
            .detail
            .as_ref()
            .map(|d| d.transitions.len())
            .unwrap_or(0);
        if len == 0 {
            return;
        }
        let mut idx = self.picker_index as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.picker_index = idx as usize;
    }

    /// Apply the highlighted transition (live if possible, always locally).
    pub fn confirm_transition(&mut self) {
        let Some(detail) = self.detail.as_ref() else {
            self.picker_open = false;
            return;
        };
        let Some(t) = detail.transitions.get(self.picker_index).cloned() else {
            self.picker_open = false;
            return;
        };
        let key = detail.key.clone();
        self.picker_open = false;

        #[cfg(feature = "live")]
        {
            if let Source::Live { .. } = self.source {
                if let Some(cfg) = crate::jira::Config::load() {
                    if let Err(e) = crate::jira::apply_transition(&cfg, &key, &t.id) {
                        self.status = format!("transition failed: {e}");
                        return;
                    }
                }
            }
        }

        if let Some(d) = self.detail.as_mut() {
            d.status = t.to.clone();
        }
        if let Some(sum) = self.issues.iter_mut().find(|i| i.key == key) {
            sum.status = t.to.clone();
        }
        self.status = format!("moved {key} → {}", t.to);
        self.flash(format!("✓ moved to {}", t.to));
    }

    // ── Round-trip edit ──────────────────────────────────────────────────────

    /// Markdown for the current issue description, to seed an editor session.
    pub fn description_markdown(&self) -> Option<String> {
        self.detail
            .as_ref()
            .map(|d| crate::adf::to_markdown(&d.description))
    }

    /// Compile edited Markdown to ADF and show it for confirmation.
    pub fn finish_edit(&mut self, markdown: &str) {
        let adf = crate::adf::compile(markdown);
        self.pending_edit = Some(adf);
        self.detail_scroll = 0;
        self.screen = Screen::Preview;
    }

    pub fn cancel_edit(&mut self) {
        self.pending_edit = None;
        self.screen = Screen::Detail;
    }

    /// Apply the previewed description (live if possible, always locally).
    pub fn apply_edit(&mut self) {
        let Some(adf) = self.pending_edit.take() else {
            self.screen = Screen::Detail;
            return;
        };
        let Some(key) = self.detail.as_ref().map(|d| d.key.clone()) else {
            self.screen = Screen::Detail;
            return;
        };

        #[cfg(feature = "live")]
        {
            if let Source::Live { .. } = self.source {
                if let Some(cfg) = crate::jira::Config::load() {
                    if let Err(e) = crate::jira::update_description(&cfg, &key, &adf) {
                        self.status = format!("update failed: {e}");
                        self.screen = Screen::Detail;
                        return;
                    }
                }
            }
        }

        if let Some(d) = self.detail.as_mut() {
            d.description = adf;
        }
        self.status = format!("updated {key} description");
        self.flash("✓ description updated");
        self.screen = Screen::Detail;
    }

    // ── Onboarding ───────────────────────────────────────────────────────────

    /// Dismiss the welcome screen and remember not to show it again.
    pub fn finish_onboarding(&mut self) {
        config::mark_onboarded();
        self.screen = Screen::Home;
    }

    /// Write the default config file from the welcome screen.
    pub fn write_config_from_welcome(&mut self) {
        match config::write_default_config() {
            Ok((path, true)) => {
                self.status = format!("wrote config to {}", path.display());
            }
            Ok((path, false)) => {
                self.status = format!("config already exists at {}", path.display());
            }
            Err(e) => {
                self.status = format!("could not write config: {e}");
            }
        }
    }

    // ── Credential setup form ────────────────────────────────────────────────

    fn focused_field_mut(&mut self) -> &mut String {
        match self.focus {
            Field::Site => &mut self.field_site,
            Field::Email => &mut self.field_email,
            Field::Token => &mut self.field_token,
        }
    }

    pub fn input_char(&mut self, c: char) {
        self.focused_field_mut().push(c);
    }

    pub fn input_backspace(&mut self) {
        self.focused_field_mut().pop();
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Field::Site => Field::Email,
            Field::Email => Field::Token,
            Field::Token => Field::Site,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Field::Site => Field::Token,
            Field::Email => Field::Site,
            Field::Token => Field::Email,
        };
    }

    /// Validate, verify against Jira, and persist the entered credentials.
    /// On success, switches to live data and finishes onboarding.
    pub fn submit_credentials(&mut self) {
        let site = self.field_site.trim().trim_end_matches('/').to_string();
        let email = self.field_email.trim().to_string();
        let token = self.field_token.trim().to_string();
        if site.is_empty() || email.is_empty() || token.is_empty() {
            self.setup_msg = "Please fill site, email, and token.".into();
            return;
        }

        // Persist first so the standard config path picks them up.
        if let Err(e) = config::save_token(&token) {
            self.setup_msg = format!("Could not save token: {e}");
            return;
        }
        if let Err(e) = config::save_settings(&site, &email, "DS") {
            self.setup_msg = format!("Could not save settings: {e}");
            return;
        }
        std::env::set_var("JIRA_BASE_URL", &site);
        std::env::set_var("JIRA_EMAIL", &email);
        std::env::set_var("JIRA_API_TOKEN", &token);

        #[cfg(feature = "live")]
        {
            self.setup_msg = "Verifying…".into();
            let (issues, source, status) = load_issues(false);
            match source {
                Source::Live { .. } => {
                    self.all_issues = issues;
                    self.source = source;
                    self.status = status;
                    self.recompute_view();
                    config::mark_onboarded();
                    self.screen = Screen::Home;
                }
                _ => {
                    self.setup_msg =
                        "Saved, but Jira did not accept those credentials. Check and retry, or press Esc to continue in demo mode.".into();
                }
            }
        }
        #[cfg(not(feature = "live"))]
        {
            self.setup_msg =
                "Saved. This build has no live support; rebuild with the `live` feature.".into();
        }
    }

    // ── Mouse mode + drag selection ──────────────────────────────────────────

    /// Whether the given screen coordinate falls within a recorded panel area.
    fn point_in(area: Rect, x: u16, y: u16) -> bool {
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

    #[allow(unused_variables)]
    fn load_detail(&mut self, key: &str) -> IssueDetail {
        #[cfg(feature = "live")]
        {
            if let Source::Live { .. } = self.source {
                if let Some(cfg) = crate::jira::Config::load() {
                    match crate::jira::fetch_detail(&cfg, key) {
                        Ok(d) => {
                            self.status = format!("Loaded {key}");
                            return d;
                        }
                        Err(e) => {
                            self.status = format!("Live fetch failed ({e}); showing sample");
                        }
                    }
                }
            }
        }
        demo_detail(key)
    }

    pub fn refresh(&mut self) {
        let force_demo = matches!(self.source, Source::Demo);
        let (issues, source, status) = load_issues(force_demo);
        self.all_issues = issues;
        self.source = source;
        self.status = format!("↻ {status}");
        self.recompute_view();
    }
}

fn load_issues(force_demo: bool) -> (Vec<IssueSummary>, Source, String) {
    if !force_demo {
        #[cfg(feature = "live")]
        {
            if let Some(cfg) = crate::jira::Config::load() {
                let user = crate::jira::whoami(&cfg).unwrap_or_else(|_| "me".into());
                match crate::jira::fetch_my_work(&cfg) {
                    Ok(issues) if !issues.is_empty() => {
                        let host = cfg.site_host();
                        let n = issues.len();
                        crate::config::cache_issues(&issues);
                        return (
                            issues,
                            Source::Live { site: host, user },
                            format!("Loaded {n} issues from Jira"),
                        );
                    }
                    Ok(_) => {
                        return (
                            demo_issues(),
                            Source::Demo,
                            "No live issues found — showing sample data".into(),
                        );
                    }
                    Err(e) => {
                        // Prefer the last cached list over sample data offline.
                        if let Some(cached) = crate::config::load_cached_issues() {
                            let n = cached.len();
                            return (
                                cached,
                                Source::Cache { user },
                                format!("Jira unreachable ({e}) — showing {n} cached issues"),
                            );
                        }
                        return (
                            demo_issues(),
                            Source::Demo,
                            format!("Jira unreachable ({e}) — showing sample data"),
                        );
                    }
                }
            }
        }
    }
    (
        demo_issues(),
        Source::Demo,
        "Offline demo — set JIRA_EMAIL + token for live mode".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn demo_app() -> App {
        let mut app = App::new(true);
        app.screen = Screen::Home;
        app
    }

    #[test]
    fn move_selection_clamps_to_bounds() {
        let mut app = demo_app();
        app.selected = 0;
        app.move_selection(-5);
        assert_eq!(app.selected, 0);
        app.move_selection(1000);
        assert_eq!(app.selected, app.issues.len() - 1);
    }

    #[test]
    fn list_index_at_maps_rows_to_issues() {
        let app = demo_app();
        app.list_area.set(Rect::new(0, 4, 80, 8));
        app.list_start.set(0);
        assert_eq!(app.list_index_at(4), Some(0));
        assert_eq!(app.list_index_at(6), Some(2));
        // Above the list area.
        assert_eq!(app.list_index_at(0), None);
        // Below the populated rows.
        assert_eq!(app.list_index_at(200), None);
    }

    #[test]
    fn click_opens_detail() {
        let mut app = demo_app();
        app.list_area.set(Rect::new(0, 4, 80, 8));
        app.list_start.set(0);
        app.mouse_down(5);
        app.mouse_up(5);
        assert_eq!(app.screen, Screen::Detail);
        assert!(app.detail.is_some());
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn drag_sets_a_pending_copy_range() {
        let mut app = demo_app();
        app.list_area.set(Rect::new(0, 4, 80, 8));
        app.mouse_down(5);
        assert_eq!(app.selection_range(), Some((5, 5)));
        app.mouse_drag(8);
        assert_eq!(app.selection_range(), Some((5, 8)));
        app.mouse_up(8);
        assert_eq!(app.pending_copy, Some((5, 8)));
        assert!(!app.selecting);
        assert_eq!(app.screen, Screen::Home, "a drag must not open detail");
    }

    #[test]
    fn credential_form_edits_focused_field() {
        let mut app = demo_app();
        app.focus = Field::Email;
        app.input_char('a');
        app.input_char('b');
        assert_eq!(app.field_email, "ab");
        app.input_backspace();
        assert_eq!(app.field_email, "a");
        app.focus_next();
        assert_eq!(app.focus, Field::Token);
        app.focus_prev();
        assert_eq!(app.focus, Field::Email);
    }

    #[test]
    fn submit_with_empty_fields_reports_and_does_not_panic() {
        let mut app = demo_app();
        app.field_site.clear();
        app.field_email.clear();
        app.field_token.clear();
        app.submit_credentials();
        assert!(!app.setup_msg.is_empty());
    }

    #[test]
    fn selected_issue_url_is_a_browse_link() {
        let app = demo_app();
        let url = app.selected_issue_url().unwrap();
        assert!(url.contains("/browse/DS-"));
    }

    #[test]
    fn confirm_transition_updates_status_locally() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        // Pick the "Done" transition (index 3 in the demo model).
        app.open_transitions();
        app.picker_index = 3;
        app.confirm_transition();
        assert!(!app.picker_open);
        assert_eq!(app.detail.as_ref().unwrap().status, "Done");
        // The summary list reflects it too.
        let key = &app.detail.as_ref().unwrap().key;
        assert_eq!(
            app.issues.iter().find(|i| &i.key == key).unwrap().status,
            "Done"
        );
    }

    #[test]
    fn edit_flow_previews_then_applies() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        let md = app.description_markdown().unwrap();
        assert!(md.contains("Problem"));

        app.finish_edit("## Edited\n\nBrand new body.");
        assert_eq!(app.screen, Screen::Preview);
        assert!(app.pending_edit.is_some());

        app.apply_edit();
        assert_eq!(app.screen, Screen::Detail);
        assert!(app.pending_edit.is_none());
        // The new ADF is now the description.
        let desc = &app.detail.as_ref().unwrap().description;
        let text = crate::adf::to_markdown(desc);
        assert!(text.contains("Edited"));
        assert!(text.contains("Brand new body"));
    }

    #[test]
    fn cancel_edit_discards_pending() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.finish_edit("## Nope");
        app.cancel_edit();
        assert_eq!(app.screen, Screen::Detail);
        assert!(app.pending_edit.is_none());
    }

    #[test]
    fn filter_narrows_and_clears() {
        let mut app = demo_app();
        let total = app.all_issues.len();
        // Cycle to the first status filter.
        app.cycle_filter();
        assert!(app.filter_status.is_some());
        let filtered = app.filter_status.clone().unwrap();
        assert!(app.issues.iter().all(|i| i.status == filtered));
        assert!(app.issues.len() <= total);
        // Cycle all the way back to "all".
        for _ in 0..20 {
            if app.filter_status.is_none() {
                break;
            }
            app.cycle_filter();
        }
        assert!(app.filter_status.is_none());
        assert_eq!(app.issues.len(), total);
    }

    #[test]
    fn sort_reorders_and_preserves_selection() {
        let mut app = demo_app();
        // Select a known issue, then re-sort; selection should follow the key.
        let key = app.issues[2].key.clone();
        app.selected = 2;
        app.sort_key = SortKey::Key;
        app.sort_asc = true;
        app.recompute_view();
        assert_eq!(app.selected_issue().unwrap().key, key);
        // Ascending by key: keys are non-decreasing.
        let nums: Vec<u64> = app
            .issues
            .iter()
            .map(|i| i.key.rsplit('-').next().unwrap().parse().unwrap())
            .collect();
        assert!(nums.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn quick_view_uses_cached_detail_after_open() {
        let mut app = demo_app();
        app.selected = 0;
        assert!(app.quick_view_detail().is_none());
        app.open_detail();
        // Returning to the list, the opened issue is cached for quick view.
        assert!(app.detail_cache.contains_key(&app.issues[0].key));
    }

    #[test]
    fn in_tui_editor_edits_then_commits_to_preview() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_tui_edit();
        assert_eq!(app.screen, Screen::Edit);
        assert!(!app.editor.lines.is_empty());
        // Type a heading on a fresh first line.
        app.editor.cx = 0;
        app.editor.cy = 0;
        for c in "X ".chars() {
            app.editor.insert_char(c);
        }
        app.commit_tui_edit();
        assert_eq!(app.screen, Screen::Preview);
        assert!(app.pending_edit.is_some());
    }

    #[test]
    fn editor_newline_and_backspace_merge_lines() {
        let mut ed = EditorState::from_text("ab");
        ed.cx = 1;
        ed.newline();
        assert_eq!(ed.lines, vec!["a".to_string(), "b".to_string()]);
        assert_eq!((ed.cy, ed.cx), (1, 0));
        ed.backspace();
        assert_eq!(ed.lines, vec!["ab".to_string()]);
        assert_eq!((ed.cy, ed.cx), (0, 1));
    }

    #[test]
    fn toggle_list_focus_flips_only_when_quick_view_open() {
        let mut app = demo_app();
        // Quick view closed: toggling is a no-op (always forced to List).
        app.toggle_list_focus();
        assert_eq!(app.list_focus, ListFocus::List);

        app.quick_view = true;
        app.toggle_list_focus();
        assert_eq!(app.list_focus, ListFocus::QuickView);
        app.toggle_list_focus();
        assert_eq!(app.list_focus, ListFocus::List);

        // Closing quick view resets focus even if it was on QuickView.
        app.quick_view = true;
        app.list_focus = ListFocus::QuickView;
        app.quick_view = false;
        app.toggle_list_focus();
        assert_eq!(app.list_focus, ListFocus::List);
    }

    #[test]
    fn point_in_quick_view_respects_recorded_area_and_visibility() {
        let mut app = demo_app();
        app.quick_view_area.set(Rect::new(10, 10, 20, 5));
        // Quick view not open: never true, even inside the recorded rect.
        assert!(!app.point_in_quick_view(12, 12));
        app.quick_view = true;
        assert!(app.point_in_quick_view(12, 12));
        assert!(!app.point_in_quick_view(0, 0));
    }

    #[test]
    fn search_finds_matches_by_key_and_summary() {
        let mut app = demo_app();
        app.open_search();
        for c in "accordion".chars() {
            app.search_input_char(c);
        }
        assert!(app.search_rows.iter().any(|r| matches!(
            r,
            SearchRow::Match(idx) if app.all_issues[*idx].summary.to_lowercase().contains("accordion")
        )));
    }

    #[test]
    fn search_key_candidate_detects_issue_keys_only() {
        let mut app = demo_app();
        app.search_query = "DS-2603".to_string();
        assert_eq!(app.search_key_candidate(), Some("DS-2603".to_string()));
        app.search_query = "ds-2603".to_string();
        assert_eq!(app.search_key_candidate(), Some("DS-2603".to_string()));
        app.search_query = "accordion".to_string();
        assert_eq!(app.search_key_candidate(), None);
        app.search_query = "DS-".to_string();
        assert_eq!(app.search_key_candidate(), None);
    }

    #[test]
    fn confirm_search_goto_opens_issue_directly_even_if_unfiltered() {
        let mut app = demo_app();
        app.open_search();
        for c in "DS-2603".chars() {
            app.search_input_char(c);
        }
        // The Goto row should be first.
        assert!(matches!(app.search_rows.first(), Some(SearchRow::Goto(k)) if k == "DS-2603"));
        app.search_selected = 0;
        app.confirm_search();
        assert_eq!(app.screen, Screen::Detail);
        assert_eq!(app.detail.as_ref().unwrap().key, "DS-2603");
    }

    #[test]
    fn confirm_search_match_opens_that_issue() {
        let mut app = demo_app();
        let target_key = app.all_issues[1].key.clone();
        app.open_search();
        for c in target_key.chars() {
            app.search_input_char(c);
        }
        // Find the Match row for our target and select it.
        let pos = app
            .search_rows
            .iter()
            .position(
                |r| matches!(r, SearchRow::Match(idx) if app.all_issues[*idx].key == target_key),
            )
            .unwrap();
        app.search_selected = pos;
        app.confirm_search();
        assert_eq!(app.detail.as_ref().unwrap().key, target_key);
    }

    #[test]
    fn close_search_returns_to_prior_screen() {
        let mut app = demo_app();
        app.screen = Screen::List;
        app.open_search();
        assert_eq!(app.screen, Screen::Search);
        app.close_search();
        assert_eq!(app.screen, Screen::List);
    }

    #[test]
    fn demo_detail_unknown_key_is_clearly_labelled_not_found() {
        let detail = crate::domain::demo_detail("DS-99999");
        assert_eq!(detail.key, "DS-99999", "must preserve the requested key");
        assert!(detail.summary.to_lowercase().contains("not found"));
    }

    #[test]
    fn open_by_key_syncs_selection_when_present_in_view() {
        let mut app = demo_app();
        let key = app.issues[2].key.clone();
        app.selected = 0;
        app.open_by_key(&key);
        assert_eq!(app.selected, 2);
        assert_eq!(app.detail.as_ref().unwrap().key, key);
    }

    #[test]
    fn board_columns_follow_workflow_order() {
        let app = demo_app();
        let cols = app.board_columns();
        // Demo data spans Backlog, To Do, In Progress, In Review, Done.
        let positions: Vec<usize> = ["Backlog", "To Do", "In Progress", "In Review", "Done"]
            .iter()
            .filter_map(|s| cols.iter().position(|c| c == s))
            .collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "columns should follow workflow order, got {cols:?}"
        );
    }

    #[test]
    fn board_lanes_group_by_epic_with_no_epic_bucket() {
        let app = demo_app();
        let lanes = app.board_lanes();
        assert!(lanes.contains(&None), "a 'no epic' lane should exist");
        assert!(
            lanes.iter().any(|l| l.as_deref() == Some("DS-2602")),
            "an epic-grouped lane should exist, got {lanes:?}"
        );
    }

    #[test]
    fn board_cell_only_contains_matching_lane_and_status() {
        let app = demo_app();
        let lane = Some("DS-2602".to_string());
        let cell = app.board_cell(&lane, "To Do");
        assert!(!cell.is_empty());
        assert!(cell.iter().all(|i| i.epic == lane && i.status == "To Do"));
    }

    #[test]
    fn board_navigation_moves_within_bounds() {
        let mut app = demo_app();
        app.open_board();
        // Column navigation clamps at the edges.
        let cols_len = app.board_columns().len();
        for _ in 0..(cols_len + 5) {
            app.board_move_col(1);
        }
        assert_eq!(app.board_sel.col, cols_len - 1);
        for _ in 0..(cols_len + 5) {
            app.board_move_col(-1);
        }
        assert_eq!(app.board_sel.col, 0);

        // Lane navigation clamps too.
        let lanes_len = app.board_lanes().len();
        for _ in 0..(lanes_len + 5) {
            app.board_move_lane(1);
        }
        assert_eq!(app.board_sel.lane, lanes_len - 1);
    }

    #[test]
    fn board_open_loads_the_selected_card() {
        let mut app = demo_app();
        app.open_board();
        // Find a lane/column with at least one card and select it directly.
        let lanes = app.board_lanes();
        let cols = app.board_columns();
        let mut found = false;
        'outer: for (li, lane) in lanes.iter().enumerate() {
            for (ci, status) in cols.iter().enumerate() {
                if !app.board_cell(lane, status).is_empty() {
                    app.board_sel.lane = li;
                    app.board_sel.col = ci;
                    app.board_sel.card = 0;
                    found = true;
                    break 'outer;
                }
            }
        }
        assert!(found, "expected at least one non-empty cell");
        app.board_open();
        assert_eq!(app.screen, Screen::Detail);
        assert!(app.detail.is_some());
    }

    #[test]
    fn board_scroll_by_never_goes_negative() {
        let mut app = demo_app();
        app.board_scroll = 0;
        app.board_scroll_by(-5);
        assert_eq!(app.board_scroll, 0);
        app.board_scroll_by(3);
        assert_eq!(app.board_scroll, 3);
    }

    #[test]
    fn open_board_clamps_stale_selection() {
        let mut app = demo_app();
        app.board_sel = BoardSelection {
            lane: 999,
            col: 999,
            card: 999,
        };
        app.open_board();
        assert_eq!(app.screen, Screen::Board);
        assert!(app.board_sel.lane < app.board_lanes().len());
        assert!(app.board_sel.col < app.board_columns().len());
    }
}
