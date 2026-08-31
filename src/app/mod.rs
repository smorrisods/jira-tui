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
#[cfg(feature = "images")]
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(feature = "images")]
use std::collections::HashSet;

use ratatui::layout::Rect;

#[cfg(not(feature = "images"))]
use crate::adf;
use crate::config::{self, Settings};
use crate::domain::{
    AssignableUser, IssueDetail, IssueSummary, Project, Source, Sprint, Version, ViewKind,
};
use crate::git::GitContext;

mod assign;
mod async_ops;
mod attachments;
mod board;
mod comments;
mod detail;
mod edit;
mod field_mapping;
mod file_browser;
mod history;
#[cfg(feature = "images")]
mod inline_images;
mod links;
mod loader;
mod mouse;
mod new_issue;
mod onboarding;
mod palette;
mod paste;
mod priority;
mod project_picker;
mod query;
mod quick_view;
mod release;
mod search;
mod sort_filter;
mod spell_suggest;
mod sprint;
mod transitions;
mod tree;
mod versions;
mod view_switch;

#[cfg(test)]
mod tests;

pub use assign::{AssigneePickerState, AssigneeRow};
pub use async_ops::AppEvent;
#[cfg(feature = "images")]
pub use attachments::AttachmentPreview;
pub use attachments::AttachmentUpload;
pub use board::BoardSelection;
pub use detail::RailPanel;
pub use edit::{EditTarget, EditorState};
pub use field_mapping::{FieldMappingOutcome, FieldMappingState, FieldMappingTarget};
pub use file_browser::{FileBrowserState, FileEntry};
pub(crate) use history::{NavEntry, NavHistory};
#[cfg(feature = "images")]
pub use inline_images::{BoundedCache, InlineImageKey};
pub use mouse::{ListFocus, MouseState, SelectionSpan};
pub(crate) use new_issue::LocallyCreatedIssue;
pub use new_issue::{NewIssueField, NewIssueState};
pub use onboarding::{Field, OnboardingState, WelcomePhase};
pub use palette::{PaletteAction, PaletteState};
pub(crate) use palette::{PaletteGroup, PaletteRow};
pub use priority::PriorityPickerState;
pub use project_picker::ProjectPickerState;
pub use release::{ReleaseBulkKind, ReleaseListMode, ReleaseState};
pub use search::{SearchPurpose, SearchRow, SearchState};
pub use sort_filter::SortKey;
pub use spell_suggest::SpellSuggestState;
pub use sprint::{SprintPickerState, SprintRow};
pub use tree::ListViewMode;
pub(crate) use tree::TreeRow;
pub use versions::{VersionField, VersionPickerState};

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
    Release,
    About,
    FieldMapping,
    NewIssue,
}

pub struct App {
    /// Full server-side list; `issues` is the filtered + sorted view of this.
    pub all_issues: Vec<IssueSummary>,
    pub issues: Vec<IssueSummary>,
    pub selected: usize,
    pub screen: Screen,
    pub detail: Option<IssueDetail>,
    pub detail_scroll: u16,
    /// Narrow Detail's facts panel folded to one line (`x`, SPEC.md §6).
    /// Reset to `false` only in `App::show_issue`, i.e. only when actually
    /// navigating to an issue (fresh open, or stepping through in-body
    /// links) — deliberately *not* reset by a same-issue `r` refresh
    /// (`apply_detail_loaded`/`refresh_detail`), matching how `link_index`
    /// is already handled, so refreshing doesn't silently unfold a panel
    /// the user just collapsed.
    pub facts_folded: bool,
    /// Keyboard focus among the wide Detail layout's side-rail panels, for
    /// scrolling one that has more content than its allotted height — see
    /// `App::cycle_rail_focus`. `None` (the default) means arrow keys
    /// scroll the main column, same as before this existed. Reset whenever
    /// a new issue is shown (`App::show_issue`/`apply_detail_loaded`).
    pub rail_focus: Option<RailPanel>,
    /// Per-panel scroll offset for the wide Detail layout's side rail,
    /// indexed by `RailPanel::index` — see `rail_focus`. Reset alongside
    /// `detail_scroll` whenever a new issue is shown.
    pub rail_scroll: [u16; 5],
    /// Whether each side-rail panel (same `RailPanel::index` order) has more
    /// content than the height `ui::detail::draw_rail` actually granted it
    /// this frame — recorded during render so `cycle_rail_focus` can skip
    /// panels that don't need scrolling, mirroring `board_area`'s "recorded
    /// during render for keyboard nav" pattern.
    pub rail_overflow: Cell<[bool; 5]>,
    /// Each side-rail panel's actual granted content height (post-border,
    /// same `RailPanel::index` order), recorded alongside `rail_overflow` —
    /// used to clamp `rail_scroll` when auto-scrolling a highlighted link
    /// into view (`App::scroll_rail_panel_by`) rather than jumping the
    /// scroll position on every step even when the target's already
    /// visible.
    pub rail_visible_rows: Cell<[u16; 5]>,
    pub source: Source,
    /// When `all_issues`/`source` were last loaded for the current view —
    /// drives the header's sync pill (SPEC.md §2). `None` only briefly,
    /// before the constructor's own initial load stamps it.
    pub last_synced: Option<std::time::Instant>,
    pub git: GitContext,
    pub tick: u64,
    pub status: String,
    pub show_help: bool,
    /// The "nerd info" diagnostics popup (build version, detected terminal
    /// env vars, detected image graphics capability) — see
    /// `ui::nerd_info`. Palette-only, no dedicated key; closes on any
    /// keypress, same shape as `show_help`.
    pub nerd_info_open: bool,
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

    /// Cross-screen issue navigation history — a forest of visited issues
    /// backing `←`/`→`/`,`/`.` back/forward stepping, the persistent
    /// recent-issues strip, and Home's rail card. See `app::history`.
    pub(crate) nav: NavHistory,
    /// The persistent recent-issues strip's inner rendering area, recorded
    /// during render so click hit-testing can recompute chip offsets from
    /// the same layout the renderer used.
    pub(crate) nav_strip_area: Cell<Rect>,
    /// Home wide layout's "recent" rail card inner area, same purpose as
    /// `nav_strip_area` above.
    pub(crate) home_recent_area: Cell<Rect>,

    // Search / go-to-issue.
    pub search: SearchState,

    // Swimlane board.
    pub board_sel: BoardSelection,
    /// Index of the first visible swimlane (SPEC.md §7) — not a text-row
    /// offset, since a lane's own band is a variable number of multi-row
    /// bordered cards rather than one packed text line. Mouse wheel
    /// (`board_scroll_by`) moves it by whole lanes, one notch per lane.
    pub board_scroll: u16,

    /// Ambient Jax companion (pure entertainment 🦦, SPEC.md §9). Means "the
    /// user has explicitly popped the full floating box out" — not "Jax is
    /// enabled at all" — since the mini footer dock shows ambiently at
    /// narrow widths regardless of this flag (see `ui::jax_companion::jax_mode`).
    pub jax_popped: bool,
    /// Tick deadline for a reactive "party" moment (a successful
    /// transition-to-Done/edit/comment, `App::trigger_jax_party`) to force
    /// Jax's party scene regardless of the normal rotation. Same "forced
    /// state until a tick deadline" shape as `flash`/`flash_until`.
    pub(crate) jax_party_until: u64,
    /// The mini-Jax footer dock's last-rendered area, for click
    /// hit-testing (`App::point_in_jax_mini`) — only meaningful while
    /// `ui::jax_companion::jax_mode` is actually `Mini`.
    pub(crate) jax_mini_area: Cell<Rect>,

    // In-TUI editor.
    pub editor: EditorState,
    /// Whether the spelling-suggestion picker (`F2`, `Screen::Edit` only)
    /// is currently open.
    pub spell_suggest_open: bool,
    pub spell_suggest: SpellSuggestState,

    /// Transient toast message; shown while `tick < flash_until`.
    pub flash_msg: String,
    pub flash_until: u64,

    // Mouse mode + drag selection.
    pub mouse: MouseState,

    // Draw geometry recorded during render, for mapping mouse coordinates.
    pub list_area: Cell<Rect>,
    pub list_start: Cell<usize>,
    /// The Detail screen's whole inner area — used only to pick the
    /// Wide/Narrow layout breakpoint (`app::links`/`app::comments`), since
    /// its width is the true terminal width regardless of the wide layout's
    /// rail. Mouse hit-testing uses `detail_main_area` instead (see below).
    pub detail_area: Cell<Rect>,
    /// The Rect `detail_scroll` actually scrolls: the wide layout's main
    /// column (excluding the identity block and the side rail), or the
    /// whole inner area in the narrow layout, where there's only one
    /// column. Kept separate from `detail_area` because the main column is
    /// narrower than the whole screen once the rail is showing, and mouse
    /// hit-testing (`app::mouse::link_at`) needs the exact scrolled Rect,
    /// not the breakpoint-decision width.
    pub detail_main_area: Cell<Rect>,
    /// The wide Detail layout's five side-rail panels' inner areas (post-
    /// border), for mouse hit-testing (`app::mouse::link_at`) — deliberately
    /// non-scrolling (see `ui::detail::draw_rail`'s doc comment), so unlike
    /// `detail_main_area` these need no separate scroll-Rect distinction.
    pub detail_workflow_area: Cell<Rect>,
    pub detail_meta_area: Cell<Rect>,
    pub detail_links_area: Cell<Rect>,
    pub detail_children_area: Cell<Rect>,
    pub detail_attachments_area: Cell<Rect>,
    pub quick_view_area: Cell<Rect>,
    /// The board's inner rendering area, recorded during render so keyboard
    /// navigation (which has no access to layout at input time) can compute
    /// how many rows are visible and auto-scroll the selection into view.
    pub board_area: Cell<Rect>,
    /// The new-issue compose form's three field cards (the whole bordered
    /// card, not just the inner text row — clicking the border or title
    /// focuses the field too), recorded during render for click-to-focus
    /// hit-testing (`App::new_issue_field_at`). Only meaningful while
    /// `screen == Screen::NewIssue`, which that hit-test re-checks, so a
    /// stale `Rect` from a previous visit can't misfire.
    pub new_issue_project_area: Cell<Rect>,
    pub new_issue_type_area: Cell<Rect>,
    pub new_issue_summary_area: Cell<Rect>,
    /// Whether the new-issue form's issue-type dropdown popup is open —
    /// same shape as `version_picker_open`/`assignee_picker_open`: a
    /// top-level flag rather than living on `NewIssueState`, so the modal
    /// key-handling block in `keys::handle_key` can check it before ever
    /// looking at which screen is active.
    pub new_issue_type_picker_open: bool,

    // Onboarding welcome + credential setup.
    pub onboarding: OnboardingState,
    /// The credential-setup form's three field rows, recorded during render
    /// for click-to-focus hit-testing (`App::onboarding_field_at`) — same
    /// pattern as `new_issue_*_area` above, just synthesized from a captured
    /// line index rather than a per-field layout `Rect`, since
    /// `draw_welcome_setup` blits all three fields as one `Paragraph`. Only
    /// meaningful while `screen == Screen::Welcome` and
    /// `onboarding.welcome_phase == WelcomePhase::Setup` — both re-checked
    /// by the hit-test, and also cleared back to `Rect::default()` whenever
    /// `draw_welcome_intro` renders instead, so a stale `Rect` from a
    /// previous visit can't misfire (mirrors `jax_mini_area`'s clearing).
    pub onboarding_site_area: Cell<Rect>,
    pub onboarding_email_area: Cell<Rect>,
    pub onboarding_token_area: Cell<Rect>,

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
    /// Modal: `Screen::Edit`'s Esc asks for confirmation before discarding a
    /// non-empty buffer, rather than dropping it immediately. Swallows the
    /// next keypress — `y`/`Y` confirms the discard, anything else dismisses
    /// the prompt and resumes editing.
    pub confirm_discard: bool,

    // Attachment picker (`a`, Detail only): open a picker over the current
    // issue's attachments, then open the highlighted one in the browser or
    // download it — see `app::attachments`.
    /// Whether the attachment picker is currently open.
    pub attachments_open: bool,
    pub attachment_index: usize,
    /// The upload flow (`u`, Detail only): typing a local path, then a
    /// mandatory preview before the actual multipart POST — see
    /// `App::open_attachment_upload`. `None` when the flow isn't active.
    pub attachment_upload: Option<AttachmentUpload>,
    /// Runtime-detected terminal image capability (`images` feature only) —
    /// see `main::detect_image_picker`, called once at startup strictly
    /// before `crossterm::EventStream` starts polling stdin (the detection
    /// query reads a synchronous response off stdin, which a concurrently
    /// polling event stream would otherwise steal). `None` whenever the
    /// terminal wasn't queried, the query failed, or stdin/stdout isn't a
    /// real tty — every other code path treats that exactly like the
    /// `images` feature being absent: fall back to the `[image: alt]`
    /// placeholder.
    #[cfg(feature = "images")]
    pub image_picker: Option<ratatui_image::picker::Picker>,
    /// The attachment picker's fetched-and-decoded preview for the
    /// currently-highlighted attachment (`images` feature only) — see
    /// `App::refresh_attachment_preview`/`AppEvent::AttachmentPreviewLoaded`.
    /// Wrapped in a `RefCell` (rather than a plain field, like every other
    /// `attachment_*` field above) because rendering it needs a `&mut
    /// StatefulProtocol` (ratatui-image resizes/re-encodes at render time),
    /// while `ui::draw` only ever holds `&App` — the same "interior
    /// mutability so render-time code can still update itself" shape as
    /// this struct's `Cell<Rect>` hit-test fields, just for a type that
    /// isn't `Copy`.
    #[cfg(feature = "images")]
    pub attachment_preview: RefCell<Option<AttachmentPreview>>,
    /// Bumped on every attachment-picker open/move; a completed preview
    /// fetch whose generation no longer matches this has been superseded by
    /// a newer selection and is dropped instead of overwriting a preview
    /// for a different attachment. Mirrors every other `*_generation`
    /// counter on `App`.
    #[cfg(feature = "images")]
    pub(crate) attachment_preview_generation: u64,
    /// A picker move debounced but not yet dispatched (`images` feature
    /// only) — see `App::ensure_attachment_preview_dispatched`. Just a
    /// bool, not the moved-to id/index: `refresh_attachment_preview`
    /// always reads `self.attachment_index` fresh when it actually fires,
    /// so whichever row is highlighted *then* (not whichever triggered
    /// this particular debounce restart) is what gets fetched — the same
    /// "recompute, don't cache" shape as `App::active_links` and friends.
    #[cfg(feature = "images")]
    pub(crate) attachment_preview_pending: bool,
    /// The tick `attachment_preview_pending` becomes eligible to dispatch
    /// at — mirrors `SearchState`'s own `dispatch_at_tick`.
    #[cfg(feature = "images")]
    pub(crate) attachment_preview_dispatch_at_tick: u64,
    /// The attachment id `refresh_attachment_preview`'s current-generation
    /// fetch is outstanding for, if any — a code-review finding on the
    /// debounce itself: `attachment_preview_pending`'s own "already cached"
    /// check only catches a *landed* result, not one still in flight, so
    /// moving away and back to the same still-loading row within the
    /// debounce window used to dispatch a second, redundant fetch for it.
    /// Set right before the actual dispatch in `refresh_attachment_preview`,
    /// cleared once that generation's response lands in
    /// `apply_attachment_preview_loaded` (success or not — either way
    /// nothing's in flight for it anymore).
    #[cfg(feature = "images")]
    pub(crate) attachment_preview_inflight_id: Option<String>,
    /// Fetched-and-decoded inline description images (`images` feature
    /// only), keyed by `InlineImageKey` rather than one single slot like
    /// `attachment_preview` — every media node the description/acceptance
    /// criteria resolves to is independently and simultaneously valid,
    /// there's no single "current selection" the way the attachment picker
    /// has one. See `App::refresh_inline_images`/
    /// `AppEvent::InlineImageLoaded`. `RefCell`-wrapped for the same
    /// "paint-time code needs `&mut` access through `&App`" reason as
    /// `attachment_preview` — a later phase's rendering will construct a
    /// `StatefulProtocol`-adjacent value from each cached `DynamicImage` at
    /// paint time. `pub` (not `pub(crate)`, unlike most of this struct's
    /// internal caches) for the same reason `attachment_preview` is: the
    /// `tests/render.rs` integration suite (a separate crate) needs to seed
    /// a decoded image directly, bypassing the async fetch, to exercise the
    /// Detail screen's inline-image paint pass headlessly.
    /// Bounded (Phase 5 of issue #130 — see `inline_images::BoundedCache`'s
    /// own doc comment for why: Detail's own three "detail landed" sites
    /// still fully clear this on every navigate via
    /// `invalidate_inline_images`, so the bound only ever matters for quick
    /// view's higher-churn, no-clear-on-selection trigger,
    /// `refresh_quick_view_inline_images`).
    #[cfg(feature = "images")]
    pub inline_images: RefCell<BoundedCache<InlineImageKey, image::DynamicImage>>,
    /// Bumped whenever the viewed issue changes (mirrors
    /// `attachment_preview_generation`) — a completed fetch whose generation
    /// no longer matches this belongs to an issue that's no longer showing
    /// and is dropped instead of populating the cache for the wrong issue.
    #[cfg(feature = "images")]
    pub(crate) inline_image_generation: u64,
    /// Keys with a fetch currently in flight (dispatched by
    /// `refresh_inline_images`/`refresh_quick_view_inline_images` but not
    /// yet answered by `apply_inline_image_loaded`) — Phase 5 of issue #130.
    /// Without this, quick view's higher selection-churn rate could
    /// re-dispatch a fetch for the same key on every revisit before the
    /// first one resolves (the cache-membership check alone only catches an
    /// *already-completed* fetch, not one still in flight). Cleared
    /// alongside the cache itself in `invalidate_inline_images`, and per-key
    /// in `apply_inline_image_loaded` once that key's response lands (or
    /// fails to decode), so a later re-resolution of the same key can retry.
    #[cfg(feature = "images")]
    pub(crate) inline_images_pending: HashSet<InlineImageKey>,
    /// Media node uuid -> resolved `InlineImageKey`, for whatever the
    /// redirect-probe fallback (`dispatch_uuid_resolve`) has matched so far
    /// (issue #130's DS-1880 follow-up) — `App::inline_image_key_for` reads
    /// this when a node's `alt` doesn't resolve, so a node with no (or a
    /// mismatched) `alt` can still be keyed into `inline_images` by its own
    /// `attrs.id`. Populated by `apply_inline_image_uuids_resolved`
    /// alongside dispatching the actual byte fetch; cleared alongside every
    /// other inline-image cache in `invalidate_inline_images`.
    #[cfg(feature = "images")]
    pub(crate) inline_image_uuid_matches: HashMap<String, InlineImageKey>,
    /// Encoded `SlicedProtocol`s for the Detail screen's and quick-view
    /// panel's inline description images (`images` feature only, Phase 3 of
    /// issue #130; quick view added in Phase 5) — keyed by `InlineImageKey`
    /// (the same resolved attachment-id/external-URL identity `inline_images`
    /// itself is keyed by, not the node's bare `alt` text), separate from
    /// `inline_images` (the *decoded* `DynamicImage` cache) because encoding
    /// a `SlicedProtocol` is real work (resizing + protocol-specific
    /// encoding) that should only happen once per target size, not on every
    /// frame. `ui::detail`'s (and `ui::quick_view`'s) paint pass rebuilds an
    /// entry from the still-cached `DynamicImage` in `inline_images` whenever
    /// the cached protocol's own `.size()` no longer matches the placement's
    /// current `(cols, rows)` (e.g. a terminal resize changed the pane
    /// width), rather than tracking the target size in a second field
    /// alongside it. Bounded the same way and for the same reason as
    /// `inline_images` (see its own doc comment).
    #[cfg(feature = "images")]
    pub(crate) inline_image_protocols:
        RefCell<BoundedCache<InlineImageKey, ratatui_image::sliced::SlicedProtocol>>,

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
    /// Whether a Fix/Affects Version update is currently in flight. Mirrors
    /// `assignee_pending`: `open_version_picker` refuses to reopen while
    /// this is set, so `version_generation` can never go stale mid-flight.
    pub(crate) version_pending: bool,
    pub(crate) version_generation: u64,
    /// Whether the Fix/Affects Version picker (`R`) is currently open.
    pub version_picker_open: bool,
    pub version_picker: VersionPickerState,
    /// The current project's versions, as fetched by
    /// `async_ops::dispatch_project_versions` for a live session (empty for
    /// demo/cache sessions, which fall back to `domain::demo_versions()`
    /// instead — see `App::project_versions_source`). Also backs the
    /// release review screen's version list.
    pub(crate) project_versions: Vec<Version>,
    /// Whether a sprint change is currently in flight. Mirrors
    /// `version_pending`: `open_sprint_picker` refuses to reopen while this
    /// is set, so `sprint_generation` can never go stale mid-flight.
    pub(crate) sprint_pending: bool,
    pub(crate) sprint_generation: u64,
    /// Whether the sprint picker (`S`) is currently open.
    pub sprint_picker_open: bool,
    pub sprint_picker: SprintPickerState,
    /// Every open (active/future) sprint on the configured board, as fetched
    /// by `async_ops::dispatch_open_sprints` for a live session (empty for
    /// demo/cache sessions, which fall back to `domain::demo_open_sprints()`
    /// instead — see `App::sprint_rows_source`). Empty (rather than an
    /// error) when `sprint_board_id` isn't configured — the picker still
    /// offers "Remove from sprint" either way.
    pub(crate) open_sprints: Vec<Sprint>,
    /// Whether a priority change is currently in flight. Mirrors
    /// `sprint_pending`: `open_priority_picker` refuses to reopen while this
    /// is set, so `priority_generation` can never go stale mid-flight.
    pub(crate) priority_pending: bool,
    pub(crate) priority_generation: u64,
    /// Whether the priority picker (`P`) is currently open.
    pub priority_picker_open: bool,
    pub priority_picker: PriorityPickerState,
    /// The release review screen's state (`w`) — see `app::release`.
    pub release: ReleaseState,
    /// Bumped on every drilled-into version (including re-entering the
    /// version list and drilling into a different one); a completed
    /// `dispatch_release_issues` fetch whose generation no longer matches
    /// this is stale and dropped, mirroring `search_generation`.
    pub(crate) release_generation: u64,
    /// Bumped on every dispatched bulk add/remove (`release_remove_selected`/
    /// `release_add_to_release`); a completed `dispatch_release_bulk` whose
    /// generation no longer matches this is stale and dropped.
    pub(crate) release_bulk_generation: u64,
    /// Whether the command palette (`ctrl-k`, SPEC.md §8) is currently open.
    pub palette_open: bool,
    pub palette: PaletteState,
    /// Every assignable project member, as fetched by
    /// `async_ops::dispatch_teammate_discovery` for a live session (empty
    /// for demo/cache sessions, which fall back to
    /// `domain::demo_assignable_users()` instead — see
    /// `App::assignable_users_source`).
    pub(crate) assignable_users: Vec<AssignableUser>,
    /// Every project the authenticated user can access, as fetched by
    /// `async_ops::dispatch_accessible_projects` for a live session (empty
    /// for demo/cache sessions, which fall back to
    /// `domain::demo_projects()` instead — see
    /// `App::accessible_projects_source`). Backs the new-issue compose
    /// form's project picker (`app::project_picker`).
    pub(crate) accessible_projects: Vec<Project>,
    /// Whether the new-issue form's project dropdown popup is open — same
    /// shape as `new_issue_type_picker_open`.
    pub project_picker_open: bool,
    pub project_picker: ProjectPickerState,
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
    /// Bumped on every dispatched live text search (the Search screen's
    /// beyond-the-loaded-view fallback — see `App::schedule_live_search`).
    /// Its own counter, separate from `generation` above: an unrelated
    /// refresh/switch_view must not invalidate an in-flight search, and vice
    /// versa.
    pub(crate) search_generation: u64,

    // New-issue compose form (`a` on Home/List) — see `app::new_issue`.
    /// State for the project/issue-type/summary form (`Screen::NewIssue`).
    /// The description step and confirmation preview reuse `edit_target`/
    /// `pending_edit`/`Screen::Edit`/`Screen::Preview` above via
    /// `EditTarget::NewIssue`, rather than a parallel compose flow.
    pub new_issue: NewIssueState,
    /// Issues created while offline (demo/cache), kept for the rest of the
    /// session so a freshly-created issue survives being reopened and a
    /// later manual refresh — see `App::land_new_issue`/`load_detail`/
    /// `record_synced`. Always empty for a genuine `Source::Live` session.
    pub(crate) locally_created: Vec<LocallyCreatedIssue>,
    /// Next numeric suffix for a locally-synthesized issue key
    /// (`App::next_local_key`) — seeded well above any baked-in demo
    /// dataset key so a locally-created issue can never collide with one.
    pub(crate) locally_created_next_id: u64,
    /// Bumped on every dispatched issue-type fetch for the compose form
    /// (`App::open_new_issue`/`refresh_new_issue_types_if_project_changed`);
    /// a completed `ProjectIssueTypesLoaded` whose generation no longer
    /// matches has been superseded by a newer fetch and is dropped, mirroring
    /// every other async op's staleness guard.
    pub(crate) new_issue_types_generation: u64,
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
            facts_folded: false,
            rail_focus: None,
            rail_scroll: [0; 5],
            rail_overflow: Cell::new([false; 5]),
            rail_visible_rows: Cell::new([0; 5]),
            source,
            last_synced: Some(std::time::Instant::now()),
            git,
            tick: 0,
            status,
            show_help: false,
            nerd_info_open: false,
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
            nav: NavHistory::default(),
            nav_strip_area: Cell::new(Rect::default()),
            home_recent_area: Cell::new(Rect::default()),
            search: SearchState::default(),
            board_sel: BoardSelection::default(),
            board_scroll: 0,
            jax_popped: false,
            jax_party_until: 0,
            jax_mini_area: Cell::new(Rect::default()),
            editor: EditorState::default(),
            spell_suggest_open: false,
            spell_suggest: SpellSuggestState::default(),
            flash_msg: String::new(),
            flash_until: 0,
            mouse: MouseState {
                enabled: settings.mouse,
                ..MouseState::default()
            },
            list_area: Cell::new(Rect::default()),
            list_start: Cell::new(0),
            detail_area: Cell::new(Rect::default()),
            detail_main_area: Cell::new(Rect::default()),
            detail_workflow_area: Cell::new(Rect::default()),
            detail_meta_area: Cell::new(Rect::default()),
            detail_links_area: Cell::new(Rect::default()),
            detail_children_area: Cell::new(Rect::default()),
            detail_attachments_area: Cell::new(Rect::default()),
            quick_view_area: Cell::new(Rect::default()),
            board_area: Cell::new(Rect::default()),
            new_issue_project_area: Cell::new(Rect::default()),
            new_issue_type_area: Cell::new(Rect::default()),
            new_issue_summary_area: Cell::new(Rect::default()),
            new_issue_type_picker_open: false,
            onboarding: OnboardingState::default(),
            onboarding_site_area: Cell::new(Rect::default()),
            onboarding_email_area: Cell::new(Rect::default()),
            onboarding_token_area: Cell::new(Rect::default()),
            picker_open: false,
            picker_index: 0,
            pending_edit: None,
            request_edit: false,
            request_suspend: false,
            edit_target: EditTarget::default(),
            edit_key: None,
            edit_return_screen: Screen::Detail,
            confirm_discard: false,
            attachments_open: false,
            attachment_index: 0,
            attachment_upload: None,
            #[cfg(feature = "images")]
            image_picker: None,
            #[cfg(feature = "images")]
            attachment_preview: RefCell::new(None),
            #[cfg(feature = "images")]
            attachment_preview_generation: 0,
            #[cfg(feature = "images")]
            attachment_preview_pending: false,
            #[cfg(feature = "images")]
            attachment_preview_dispatch_at_tick: 0,
            #[cfg(feature = "images")]
            attachment_preview_inflight_id: None,
            #[cfg(feature = "images")]
            inline_images: RefCell::new(BoundedCache::new(inline_images::INLINE_IMAGE_CACHE_CAP)),
            #[cfg(feature = "images")]
            inline_image_generation: 0,
            #[cfg(feature = "images")]
            inline_images_pending: HashSet::new(),
            #[cfg(feature = "images")]
            inline_image_uuid_matches: HashMap::new(),
            #[cfg(feature = "images")]
            inline_image_protocols: RefCell::new(BoundedCache::new(
                inline_images::INLINE_IMAGE_CACHE_CAP,
            )),
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
            version_pending: false,
            version_generation: 0,
            version_picker_open: false,
            version_picker: VersionPickerState::default(),
            project_versions: Vec::new(),
            sprint_pending: false,
            sprint_generation: 0,
            sprint_picker_open: false,
            sprint_picker: SprintPickerState::default(),
            open_sprints: Vec::new(),
            priority_pending: false,
            priority_generation: 0,
            priority_picker_open: false,
            priority_picker: PriorityPickerState::default(),
            release: ReleaseState::default(),
            release_generation: 0,
            release_bulk_generation: 0,
            palette_open: false,
            palette: PaletteState::default(),
            assignable_users: Vec::new(),
            accessible_projects: Vec::new(),
            project_picker_open: false,
            project_picker: ProjectPickerState::default(),
            edit_pending: false,
            edit_generation: 0,
            field_mapping_pending: false,
            field_mapping_generation: 0,
            onboarding_pending: false,
            onboarding_generation: 0,
            search_generation: 0,
            new_issue: NewIssueState::default(),
            locally_created: Vec::new(),
            locally_created_next_id: 9001,
            new_issue_types_generation: 0,
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
            async_ops::dispatch_project_versions(app.events_tx.clone());
            async_ops::dispatch_accessible_projects(app.events_tx.clone());
            // Only meaningful once `sprint_board_id` is configured — see
            // `dispatch_open_sprints`'s own blocking half, which no-ops
            // (empty list) without one rather than needing a synchronous
            // check here.
            async_ops::dispatch_open_sprints(app.events_tx.clone());
        }

        app
    }
}

/// A non-`images`-build stand-in for `inline_images`'s own
/// `App::with_detail_media_sizing` (Phase 3 of issue #130) — always hands
/// the callback `MediaSizing::Disabled`, so `ui::detail`/`app::comments`/
/// `app::links` can call this same method name unconditionally rather than
/// `#[cfg]`-branching at every call site. `adf::MediaSizing` itself isn't
/// feature-gated (Phase 2 already made every mode compile regardless of
/// `images`), only the machinery that ever produces a real `Ready` closure
/// is.
#[cfg(not(feature = "images"))]
impl App {
    pub(crate) fn with_detail_media_sizing<R>(
        &self,
        _width: u16,
        f: impl FnOnce(&adf::MediaSizing) -> R,
    ) -> R {
        f(&adf::MediaSizing::Disabled)
    }

    /// Non-`images`-build stand-in for `inline_images::App::with_quick_view_media_sizing`
    /// (Phase 5 of issue #130) — see `with_detail_media_sizing` above for why
    /// this always hands the callback `MediaSizing::Disabled`.
    pub(crate) fn with_quick_view_media_sizing<R>(
        &self,
        _width: u16,
        f: impl FnOnce(&adf::MediaSizing) -> R,
    ) -> R {
        f(&adf::MediaSizing::Disabled)
    }
}
