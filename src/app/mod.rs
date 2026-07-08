//! Application state, event handling, and the data-loading glue.
//!
//! `App` is one struct whose behaviour is split across focused submodules —
//! sorting/filtering, the quick-view panel, search, the swimlane board,
//! transitions, editing, onboarding, and mouse handling — each an `impl App`
//! block in its own file. This module holds the struct definition, its
//! constructor, small cross-cutting helpers, and the top-level data loader.

use std::cell::Cell;
use std::collections::HashMap;

use ratatui::layout::Rect;

use crate::config::{self, Settings};
use crate::domain::{demo_issues, IssueDetail, IssueSummary, Source};
use crate::git::GitContext;

mod board;
mod detail;
mod edit;
mod field_mapping;
mod mouse;
mod onboarding;
mod quick_view;
mod search;
mod sort_filter;
mod transitions;

#[cfg(test)]
mod tests;

pub use board::BoardSelection;
pub use edit::EditorState;
pub use field_mapping::{FieldMappingOutcome, FieldMappingState};
pub use mouse::{ListFocus, MouseState};
pub use onboarding::{Field, OnboardingState, WelcomePhase};
pub use search::{SearchRow, SearchState};
pub use sort_filter::SortKey;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    Welcome,
    #[default]
    Home,
    List,
    Detail,
    Preview,
    Edit,
    Search,
    Board,
    About,
    FieldMapping,
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
    pub search: SearchState,

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
    pub mouse: MouseState,

    // Draw geometry recorded during render, for mapping mouse coordinates.
    pub list_area: Cell<Rect>,
    pub list_start: Cell<usize>,
    pub detail_area: Cell<Rect>,
    pub quick_view_area: Cell<Rect>,
    /// The board's inner rendering area, recorded during render so keyboard
    /// navigation (which has no access to layout at input time) can compute
    /// how many rows are visible and auto-scroll the selection into view.
    pub board_area: Cell<Rect>,

    // Onboarding welcome + credential setup.
    pub onboarding: OnboardingState,

    // Transition picker + round-trip edit.
    pub picker_open: bool,
    pub picker_index: usize,
    pub pending_edit: Option<serde_json::Value>,
    /// Set by a key handler to ask the run loop to launch `$EDITOR`.
    pub request_edit: bool,

    // Field-mapping discovery (custom field IDs are instance-specific).
    pub field_mapping: FieldMappingState,
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
            search: SearchState::default(),
            board_sel: BoardSelection::default(),
            board_scroll: 0,
            show_jax: false,
            editor: EditorState::default(),
            flash_msg: String::new(),
            flash_until: 0,
            mouse: MouseState {
                enabled: settings.mouse,
                ..MouseState::default()
            },
            list_area: Cell::new(Rect::default()),
            list_start: Cell::new(0),
            detail_area: Cell::new(Rect::default()),
            quick_view_area: Cell::new(Rect::default()),
            board_area: Cell::new(Rect::default()),
            onboarding: OnboardingState::default(),
            picker_open: false,
            picker_index: 0,
            pending_edit: None,
            request_edit: false,
            field_mapping: FieldMappingState::default(),
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
            // Demo data has no real Jira site behind it; use an obviously
            // fake placeholder host rather than a real organization's Jira.
            _ => "demo.atlassian.net".to_string(),
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
