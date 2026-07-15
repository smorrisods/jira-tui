//! Application state, event handling, and the data-loading glue.
//!
//! `App` is one struct whose behaviour is split across focused submodules —
//! sorting/filtering, the quick-view panel, search, the swimlane board,
//! transitions, editing, onboarding, and mouse handling — each an `impl App`
//! block in its own file. This module holds the struct definition and its
//! constructor; `loader` carries the top-level data loader (and the cache
//! it sits in front of), and `query` carries the small cross-cutting state
//! helpers (selection, window title, toasts, at-a-glance counts).

use std::cell::Cell;
use std::collections::HashMap;

use ratatui::layout::Rect;

use crate::config::{self, Settings};
use crate::domain::{AssignableUser, IssueDetail, IssueSummary, Source, ViewKind};
use crate::git::GitContext;

mod assign;
mod async_ops;
mod board;
mod comments;
mod detail;
mod edit;
mod field_mapping;
mod history;
mod links;
mod loader;
mod mouse;
mod onboarding;
mod query;
mod quick_view;
mod search;
mod sort_filter;
mod transitions;
mod tree;
mod view_switch;

#[cfg(test)]
mod tests;

pub use assign::{AssigneePickerState, AssigneeRow};
pub use async_ops::AppEvent;
pub use board::BoardSelection;
pub use edit::{EditTarget, EditorState};
pub use field_mapping::{FieldMappingOutcome, FieldMappingState};
pub use mouse::{ListFocus, MouseState};
pub use onboarding::{Field, OnboardingState, WelcomePhase};
pub use search::{SearchRow, SearchState};
pub use sort_filter::SortKey;
pub use tree::ListViewMode;

use loader::load_issues;

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
    /// When `all_issues`/`source` were last loaded for the current view —
    /// drives the header's sync pill (SPEC.md §2). `None` only briefly,
    /// before the constructor's own initial load stamps it.
    pub last_synced: Option<std::time::Instant>,
    pub git: GitContext,
    pub tick: u64,
    pub status: String,
    pub show_help: bool,
    pub should_quit: bool,

    // Sort + filter.
    pub sort_key: SortKey,
    pub sort_asc: bool,
    pub filter_status: Option<String>,
    /// Flat sort order, or a parent/child tree nesting an issue's children
    /// (Epic → stories, story → sub-tasks) beneath it — see `app::tree`.
    pub list_view_mode: ListViewMode,

    // Quick-view panel + a cache of opened issue details.
    pub quick_view: bool,
    pub quick_view_scroll: u16,
    pub list_focus: ListFocus,
    pub detail_cache: HashMap<String, IssueDetail>,

    // In-body link navigation (issue-key/URL mentions in the Detail screen
    // and quick-view panel): `{`/`}` cycle `link_index`, `Enter` opens the
    // highlighted one. The link list itself is recomputed on demand from
    // whichever detail is shown (see `app::links::active_links`) rather
    // than cached, so it can never go stale.
    pub link_index: usize,

    // Detail navigation history (browser-style back/forward), built as the
    // user follows in-body issue links from one Detail view to another —
    // see `app::history`. Opening an issue fresh from the list/search
    // starts a new history; `←`/`→` only step through it while it's
    // non-empty, otherwise falling back to their prior meaning (exit
    // Detail / open the selected issue).
    pub(crate) detail_back: Vec<String>,
    pub(crate) detail_forward: Vec<String>,

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
    /// Set on Ctrl+Z to ask the run loop to suspend the process to the
    /// shell (`SIGTSTP`) and restore the TUI on resume; see
    /// `crate::suspend` in the binary.
    pub request_suspend: bool,
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

    /// The screen `a` was pressed from, so backing out of About (see #38)
    /// restores it instead of always landing on Home.
    pub about_return_screen: Screen,

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
    /// Every distinct assignee (excluding "me") seen across *any* view
    /// loaded so far this session, accumulated in `recompute_view` rather
    /// than derived fresh from `all_issues` each time — otherwise switching
    /// to a teammate's view (which narrows `all_issues` down to just their
    /// issues) would make every other teammate vanish from the picker until
    /// All Project Issues was reloaded. See `known_teammates`.
    pub(crate) teammates_seen: std::collections::BTreeSet<String>,

    // Async data loading (refresh / switch_view against live Jira). See
    // `async_ops` — demo/cache-only sessions still resolve synchronously
    // (there's nothing worth showing a spinner for), only a real fetch
    // dispatches onto the runtime.
    /// Whether a refresh/view-switch fetch is currently in flight.
    pub loading: bool,
    /// Bumped on every dispatched fetch; a completed fetch whose generation
    /// no longer matches the current one has been superseded by a newer
    /// request and is discarded instead of clobbering fresher state.
    pub(crate) generation: u64,
    pub(crate) events_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    /// Drained by the run loop each iteration and applied via `apply_event`.
    pub events_rx: tokio::sync::mpsc::UnboundedReceiver<AppEvent>,

    // Async detail load / transition apply / edit apply — same generation +
    // channel pattern as refresh/switch_view above, one counter per
    // operation kind so an in-flight detail fetch can't be invalidated by an
    // unrelated transition or edit completing (and vice versa). See
    // `async_ops` for the dispatch/apply plumbing.
    /// The key of the detail fetch currently in flight, and whether it
    /// should navigate to `Screen::Detail` once it resolves — a
    /// cache-only quick-view load (`false`) can be "upgraded" to an
    /// explicit open (`true`) if the user opens the same issue before the
    /// first request resolves, without dispatching a duplicate fetch. See
    /// `App::dispatch_detail_fetch`.
    pub(crate) detail_pending: Option<(String, bool)>,
    pub(crate) detail_generation: u64,
    /// Whether a workflow transition is currently in flight. `open_transitions`
    /// refuses to reopen the picker while this is set, so at most one
    /// transition can be dispatched at a time — this keeps
    /// `transition_generation` from ever going stale mid-flight instead of
    /// silently dropping an overlapping request's result.
    pub(crate) transition_pending: bool,
    pub(crate) transition_generation: u64,
    /// Whether an assignee change is currently in flight. Mirrors
    /// `transition_pending`: `open_assignee_picker` refuses to reopen while
    /// this is set, so `assignee_generation` can never go stale mid-flight.
    pub(crate) assignee_pending: bool,
    pub(crate) assignee_generation: u64,
    /// Whether the assignee picker (`A`) is currently open.
    pub assignee_picker_open: bool,
    pub assignee_picker: AssigneePickerState,
    /// Every assignable project member, as fetched by
    /// `async_ops::dispatch_teammate_discovery` for a live session (empty
    /// for demo/cache sessions, which fall back to
    /// `domain::demo_assignable_users()` instead — see
    /// `App::assignable_users_source`).
    pub(crate) assignable_users: Vec<AssignableUser>,
    /// Whether a description update or comment post is currently in
    /// flight. `begin_tui_edit`/`begin_external_edit`/`begin_comment`
    /// refuse to start a new edit session while this is set, for the same
    /// reason as `transition_pending` above.
    pub(crate) edit_pending: bool,
    pub(crate) edit_generation: u64,
    /// Whether a field-mapping custom-field lookup is currently in flight —
    /// guards against a duplicate `F`-key press re-dispatching while one is
    /// already resolving.
    pub(crate) field_mapping_pending: bool,
    pub(crate) field_mapping_generation: u64,
    /// Whether onboarding's credential-verification fetch is currently in
    /// flight — guards against re-submitting the setup form (e.g. a double
    /// Enter press) while one is already resolving.
    pub(crate) onboarding_pending: bool,
    pub(crate) onboarding_generation: u64,
}

impl App {
    pub fn new(force_demo: bool) -> Self {
        let git = GitContext::detect();
        let (issues, source, status) = load_issues(force_demo);
        let settings = Settings::load();
        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();

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
            last_synced: Some(std::time::Instant::now()),
            git,
            tick: 0,
            status,
            show_help: false,
            should_quit: false,
            sort_key: SortKey::Updated,
            sort_asc: false,
            filter_status: None,
            list_view_mode: ListViewMode::default(),
            quick_view: false,
            quick_view_scroll: 0,
            list_focus: ListFocus::List,
            detail_cache: HashMap::new(),
            link_index: 0,
            detail_back: Vec::new(),
            detail_forward: Vec::new(),
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
            request_suspend: false,
            edit_target: EditTarget::default(),
            edit_key: None,
            edit_return_screen: Screen::Detail,
            about_return_screen: Screen::Home,
            field_mapping: FieldMappingState::default(),
            current_view: ViewKind::default(),
            view_picker_open: false,
            view_picker_index: 0,
            view_picker_options: Vec::new(),
            teammates_seen: std::collections::BTreeSet::new(),
            loading: false,
            generation: 0,
            events_tx,
            events_rx,
            detail_pending: None,
            detail_generation: 0,
            transition_pending: false,
            transition_generation: 0,
            assignee_pending: false,
            assignee_generation: 0,
            assignee_picker_open: false,
            assignee_picker: AssigneePickerState::default(),
            assignable_users: Vec::new(),
            edit_pending: false,
            edit_generation: 0,
            field_mapping_pending: false,
            field_mapping_generation: 0,
            onboarding_pending: false,
            onboarding_generation: 0,
        };
        app.recompute_view();

        // If the current branch maps to a known issue, pre-select it.
        if let Some(key) = app.git.issue_key.clone() {
            if let Some(idx) = app.issues.iter().position(|i| i.key == key) {
                app.selected = idx;
            }
        }

        // Kick off a one-shot background fetch of the project's assignable
        // users purely to discover teammates earlier, rather than waiting
        // for the user to manually switch to All Project Issues — see
        // `async_ops::dispatch_teammate_discovery`. Skipped for demo/cache
        // sessions (no live network worth a background call for). Unlike
        // an earlier version of this that fetched All Project Issues,
        // `assignable_users` is a single lightweight non-issue call, so
        // it's cheap enough to fire unconditionally rather than needing to
        // be lazy or gated on the initial view.
        if matches!(app.source, Source::Live { .. }) {
            async_ops::dispatch_teammate_discovery(app.events_tx.clone());
        }

        app
    }

    /// Record a successful issues/source load. Always stamps `last_synced`
    /// alongside `source`/`all_issues` so the two can't drift apart — two
    /// call sites had hand-assigned `source` without pairing it before this
    /// helper existed (caught by the phase-3 UI-refresh review), so every
    /// load path routes through this instead of assigning the fields
    /// directly.
    pub(crate) fn record_synced(&mut self, issues: Vec<IssueSummary>, source: Source) {
        self.all_issues = issues;
        self.source = source;
        self.last_synced = Some(std::time::Instant::now());
    }
}
