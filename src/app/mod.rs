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
use crate::domain::{demo_issues, IssueDetail, IssueSummary, Source, ViewKind};
use crate::git::GitContext;

mod board;
mod comments;
mod detail;
mod edit;
mod field_mapping;
mod mouse;
mod onboarding;
mod quick_view;
mod search;
mod sort_filter;
mod transitions;
mod view_switch;

#[cfg(test)]
mod tests;

pub use board::BoardSelection;
pub use edit::{EditTarget, EditorState};
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
    /// Whether `Screen::Edit`/`Screen::Preview` are composing a description
    /// edit or a new comment; both share the same compose → preview → apply
    /// flow, only the apply action and footer text differ.
    pub edit_target: EditTarget,
    /// The issue key the current edit/comment applies to. Needed for
    /// comments composed from quick-view, where there's no `self.detail`.
    pub edit_key: Option<String>,
    /// The screen to return to on cancel/apply — Detail when editing from
    /// the full detail screen, List/Home when composing a comment from
    /// quick-view.
    pub edit_return_screen: Screen,

    // Field-mapping discovery (custom field IDs are instance-specific).
    pub field_mapping: FieldMappingState,

    // View switcher: My Work / All Project Issues / a teammate's work.
    /// Which JQL-backed view `all_issues` currently holds.
    pub current_view: ViewKind,
    pub view_picker_open: bool,
    pub view_picker_index: usize,
    /// Computed when the picker opens: My Work, All Project Issues, then one
    /// entry per teammate seen in the currently loaded issues.
    pub view_picker_options: Vec<ViewKind>,
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
            edit_target: EditTarget::default(),
            edit_key: None,
            edit_return_screen: Screen::Detail,
            field_mapping: FieldMappingState::default(),
            current_view: ViewKind::default(),
            view_picker_open: false,
            view_picker_index: 0,
            view_picker_options: Vec::new(),
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
        let kind = self.current_view.clone();
        let (issues, source, status) = load_issues_for(&kind, force_demo);
        self.all_issues = issues;
        self.source = source;
        self.status = format!("↻ {status}");
        self.recompute_view();
    }
}

/// Open the on-disk cache for the current site, running the one-time
/// legacy `my-work.json` import along the way — unless caching is
/// disabled (`cache_enabled` setting / `--no-cache` / `JIRA_NO_CACHE`), or
/// the cache can't be opened at all (treated the same as "no cache
/// available", exactly like a missing/corrupt `my-work.json` always was).
#[cfg(feature = "live")]
fn open_cache_for_site(cfg: &crate::jira::Config) -> Option<(crate::cache::Cache, i64)> {
    if !crate::config::Settings::load().cache_enabled {
        return None;
    }
    let mut cache = crate::cache::Cache::open().ok()?;
    let site_id = cache.site_id(&cfg.base_url).ok()?;
    cache.migrate_legacy_json(site_id, crate::jira::MY_WORK_JQL);
    Some((cache, site_id))
}

/// The issues to show offline for a given view. `MyWork`/`AllProject` both
/// show the full baked-in demo set (the demo dataset stands in for "the
/// whole project" — there's no offline notion of a distinct "my" subset);
/// `Teammate` filters it down to that teammate's assigned issues, so the
/// view picker is meaningfully explorable with zero credentials.
fn demo_view_for(view: &ViewKind) -> Vec<IssueSummary> {
    match view {
        ViewKind::MyWork | ViewKind::AllProject => demo_issues(),
        ViewKind::Teammate(name) => demo_issues()
            .into_iter()
            .filter(|i| i.assignee.as_deref() == Some(name.as_str()))
            .collect(),
    }
}

fn load_issues(force_demo: bool) -> (Vec<IssueSummary>, Source, String) {
    load_issues_for(&ViewKind::MyWork, force_demo)
}

/// Fetch (or fall back to demo/cached data for) whichever view is active.
/// `MyWork` is the only view with a durable on-disk cache entry for now
/// (see `ViewKind::cache_kind`); `AllProject`/`Teammate` are session-only.
fn load_issues_for(view: &ViewKind, force_demo: bool) -> (Vec<IssueSummary>, Source, String) {
    if !force_demo {
        #[cfg(feature = "live")]
        {
            if let Some(cfg) = crate::jira::Config::load() {
                let user = crate::jira::whoami(&cfg).unwrap_or_else(|_| "me".into());
                let mut cache = open_cache_for_site(&cfg);
                let jql = crate::jira::jql_for(view, &cfg.project);
                match crate::jira::search_issues(&cfg, &jql) {
                    Ok(issues) if !issues.is_empty() => {
                        let host = cfg.site_host();
                        let n = issues.len();
                        if let (Some(kind), Some((cache, site_id))) =
                            (view.cache_kind(), &mut cache)
                        {
                            let _ = cache.save_view(*site_id, kind, &view.label(), &jql, &issues);
                        }
                        // search_issues now pages until Jira reports
                        // `isLast`, but still stops at SEARCH_RESULTS_CAP so
                        // a very large project can't page forever — flag it
                        // when that cap was actually hit.
                        let status = if n >= crate::jira::SEARCH_RESULTS_CAP {
                            format!("Loaded {n} issues from Jira (capped at {n}; more may exist)")
                        } else {
                            format!("Loaded {n} issues from Jira")
                        };
                        return (issues, Source::Live { site: host, user }, status);
                    }
                    Ok(_) => {
                        return (
                            demo_view_for(view),
                            Source::Demo,
                            format!("No issues found for {} — showing sample data", view.label()),
                        );
                    }
                    Err(e) => {
                        // Prefer the last cached list over sample data offline.
                        let cached = view.cache_kind().and_then(|kind| {
                            cache
                                .as_ref()
                                .and_then(|(cache, site_id)| cache.load_view(*site_id, kind).ok())
                                .flatten()
                        });
                        if let Some(cached) = cached {
                            let n = cached.len();
                            return (
                                cached,
                                Source::Cache { user },
                                format!("Jira unreachable ({e}) — showing {n} cached issues"),
                            );
                        }
                        return (
                            demo_view_for(view),
                            Source::Demo,
                            format!("Jira unreachable ({e}) — showing sample data"),
                        );
                    }
                }
            }
        }
    }
    (
        demo_view_for(view),
        Source::Demo,
        "Offline demo — set JIRA_EMAIL + token for live mode".into(),
    )
}
