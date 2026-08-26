//! The `AppEvent` enum and `apply_event` dispatcher — a thin, one-arm-per-
//! variant table. Each arm's actual apply logic lives in an `App::apply_*`
//! method co-located with its `dispatch_*` counterpart in `list_ops.rs`,
//! `mutation_ops.rs`, or `setup_ops.rs`, so this file only grows by one
//! match arm — not a whole logic block — per new `AppEvent` variant.

use crate::domain::{
    AssignableUser, Attachment, Comment, IssueDetail, IssueSummary, IssueType, Project, Source,
    Sprint, Version, ViewKind,
};

use super::super::{App, ReleaseBulkKind, Screen};
use super::setup_ops::FieldsFetchResult;

/// Sent back from a spawned fetch once it completes. Carries the
/// `generation` it was dispatched under so a fetch that's been superseded
/// by a newer refresh/switch_view (triggered before this one resolved) can
/// be dropped instead of clobbering fresher state.
pub enum AppEvent {
    Refreshed {
        generation: u64,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    },
    ViewSwitched {
        generation: u64,
        view: ViewKind,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    },
    /// A full-detail fetch resolved. `navigate` distinguishes an explicit
    /// "open" (jump to `Screen::Detail` once loaded) from the quick-view
    /// panel's lazy background load (cache only, stay put). Whether to
    /// navigate is decided at apply-time from `App::detail_pending` rather
    /// than carried on the event itself, so a fetch dispatched as a
    /// cache-only quick-view load that later gets "upgraded" by an explicit
    /// open (before the first one resolves) still navigates once it lands.
    DetailLoaded {
        generation: u64,
        key: String,
        detail: Box<IssueDetail>,
        status: Option<String>,
    },
    /// A workflow transition resolved (or failed) against live Jira.
    TransitionApplied {
        generation: u64,
        key: String,
        to: String,
        error: Option<String>,
    },
    /// A description update resolved. `return_screen` is where the edit
    /// flow should land once applied, matching the synchronous behaviour
    /// this replaces.
    DescriptionUpdated {
        generation: u64,
        key: String,
        adf: serde_json::Value,
        error: Option<String>,
        return_screen: Screen,
    },
    /// A new comment resolved — either the server's copy of the comment
    /// (live) or the locally-composed one (no credentials/offline).
    CommentAdded {
        generation: u64,
        key: String,
        result: Result<Comment, String>,
        return_screen: Screen,
    },
    /// A field-mapping custom-field lookup resolved. `origin` decides how
    /// the result is applied — see `FieldMappingOrigin`. Fields are plain
    /// `(id, name)` pairs rather than `jira::FieldInfo` so this variant (and
    /// `apply_event`) compile under every feature set. The `Option<String>`
    /// alongside them is the field currently mapped in `config.toml` (read
    /// fresh inside the same fetch, since it needs the same `Config` the
    /// fetch itself loads), used to pre-select the catalog.
    FieldsLoaded {
        generation: u64,
        /// Which config-gated custom field this fetch was for — see
        /// `dispatch_field_mapping`'s own doc comment for why the catalog
        /// itself doesn't vary by target, only `result`'s "currently
        /// mapped" half does.
        target: super::super::field_mapping::FieldMappingTarget,
        origin: super::super::field_mapping::FieldMappingOrigin,
        result: FieldsFetchResult,
    },
    /// Onboarding's credential-verification fetch resolved. Whether the
    /// credentials were actually accepted is decided at apply-time from
    /// `source` (a genuine `Source::Live` means success), exactly like the
    /// synchronous `submit_credentials` this replaces used to check.
    CredentialsVerified {
        generation: u64,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    },
    /// A one-shot background fetch of the project's assignable users
    /// resolved, dispatched once at startup for a genuine live session
    /// purely to discover teammates earlier than the user manually
    /// visiting All Project Issues — see `dispatch_teammate_discovery`.
    /// Also seeds `App::assignable_users`, the same list the assignee
    /// picker (`A`) draws from, so opening it doesn't need its own
    /// dedicated fetch. Deliberately carries no `generation`: it never
    /// overwrites `all_issues`/`current_view` (only merges names into
    /// `teammates_seen` and replaces `assignable_users` wholesale), so it
    /// can't be made stale by an unrelated refresh/switch_view and is safe
    /// to apply whenever it lands.
    TeammatesDiscovered { users: Vec<AssignableUser> },
    /// An assignee change (or unassign, when `account_id`/`display_name`
    /// were `None`) resolved against live Jira — see
    /// `App::confirm_assignee`/`dispatch_assign`.
    AssigneeApplied {
        generation: u64,
        key: String,
        display_name: Option<String>,
        error: Option<String>,
    },
    /// The Search screen's live text-search fallback resolved — see
    /// `App::schedule_live_search`/`dispatch_text_search`. Carries its own
    /// `generation` (`App::search_generation`, distinct from the main list's
    /// `generation`) so an unrelated refresh/switch_view can't invalidate an
    /// in-flight search and vice versa. `query` is the exact text this
    /// batch of `issues` answers, so a result that lands after the user has
    /// kept typing can be told apart from one that still matches what's on
    /// screen — see `App::rebuild_search_rows`.
    TextSearched {
        generation: u64,
        query: String,
        issues: Vec<IssueSummary>,
        /// Set on a failed/skipped search (no credentials, network error, a
        /// JQL the server rejected) — surfaced via `App::status` since
        /// there's no fallback data to quietly show in its place.
        error: Option<String>,
    },
    /// A one-shot background fetch of the current project's versions
    /// resolved, dispatched once at startup for a genuine live session —
    /// see `dispatch_project_versions`. Mirrors `TeammatesDiscovered`:
    /// carries no `generation`, since it only replaces `App::project_versions`
    /// wholesale and can't be made stale by an unrelated refresh/switch_view.
    ProjectVersionsLoaded { versions: Vec<Version> },
    /// A one-shot background fetch of every project the authenticated user
    /// can access resolved, dispatched once at startup for a genuine live
    /// session — see `dispatch_accessible_projects`. Mirrors
    /// `ProjectVersionsLoaded`/`TeammatesDiscovered`: carries no
    /// `generation`, since it only replaces `App::accessible_projects`
    /// wholesale and can't be made stale by an unrelated refresh/switch_view.
    AccessibleProjectsLoaded { projects: Vec<Project> },
    /// A Fix/Affects Version update resolved (or failed) against live Jira —
    /// see `App::confirm_version_picker`/`dispatch_set_versions`. Each field
    /// is `None` when it wasn't part of this update (unchanged in the
    /// picker), distinguished from `Some(vec![])` (cleared to empty).
    VersionsApplied {
        generation: u64,
        key: String,
        fix_versions: Option<Vec<String>>,
        fix_error: Option<String>,
        affects_versions: Option<Vec<String>>,
        affects_error: Option<String>,
    },
    /// A one-shot background fetch of the configured board's open sprints
    /// resolved, dispatched once at startup for a genuine live session —
    /// see `dispatch_open_sprints`. Mirrors `ProjectVersionsLoaded`: carries
    /// no `generation`, since it only replaces `App::open_sprints` wholesale
    /// and can't be made stale by an unrelated refresh/switch_view.
    OpenSprintsLoaded { sprints: Vec<Sprint> },
    /// A sprint change (or removal, when `sprint` was `None`) resolved
    /// against live Jira — see `App::confirm_sprint_picker`/`dispatch_set_sprint`.
    SprintApplied {
        generation: u64,
        key: String,
        sprint: Option<Sprint>,
        error: Option<String>,
    },
    /// The release review screen's drill-down fetch resolved — see
    /// `App::open_release_drill`/`dispatch_release_issues`. Carries its own
    /// `release_generation`, distinct from every other counter, so
    /// re-drilling into a different version (or backing out) can't be
    /// clobbered by, or clobber, an unrelated in-flight fetch.
    ReleaseIssuesLoaded {
        generation: u64,
        issues: Vec<IssueSummary>,
        /// Set on a failed/skipped fetch (no credentials, network error) —
        /// surfaced via `App::status` since there's no fallback data to
        /// quietly show in its place.
        error: Option<String>,
    },
    /// A bulk add-to-release or remove-from-release resolved — see
    /// `App::release_remove_selected`/`release_add_to_release` and
    /// `dispatch_release_bulk`. Each issue succeeds or fails independently
    /// (one issue's fetch/write failing shouldn't block the rest), so this
    /// carries a per-key result rather than one pass/fail for the whole
    /// batch.
    ReleaseBulkApplied {
        generation: u64,
        version_name: String,
        kind: ReleaseBulkKind,
        results: Vec<(String, Result<(), String>)>,
    },
    /// The new-issue compose form's issue-type fetch resolved — see
    /// `dispatch_project_issue_types`. Carries its own `generation` (unlike
    /// `ProjectVersionsLoaded`, which is always for the single fixed
    /// `cfg.project` and so can never go stale) since `project` here is
    /// arbitrary user-typed text that can change again before this
    /// resolves — `apply_project_issue_types_loaded` drops a superseded
    /// generation.
    ProjectIssueTypesLoaded {
        generation: u64,
        project: String,
        result: Result<Vec<IssueType>, String>,
    },
    /// A new issue's creation resolved (or failed) — see
    /// `App::apply_new_issue`/`dispatch_create_issue`. Carries the full
    /// compose state, not just the resulting key: unlike Description/
    /// Comment, which patch an *existing* `self.detail`, this has to
    /// *construct* a brand-new issue at apply-time.
    IssueCreated {
        generation: u64,
        issue_type: String,
        summary: String,
        description: Option<serde_json::Value>,
        result: Result<String, String>,
    },
    /// An attachment download to disk resolved (or failed) — see
    /// `App::download_selected_attachment`/`dispatch_attachment_download`.
    /// Carries no `generation`: like `TeammatesDiscovered`, it only ever
    /// surfaces a status flash and never mutates state a stale result could
    /// corrupt, so there's nothing for a counter to guard against. `result`'s
    /// `Ok` payload is the saved file's path, as a display string rather
    /// than a `PathBuf`, since it's only ever shown to the user.
    AttachmentDownloaded {
        key: String,
        filename: String,
        result: Result<String, String>,
    },
    /// An attachment upload resolved (or failed) — see
    /// `App::confirm_attachment_upload`/`dispatch_attachment_upload`.
    /// Carries no `generation`, same reasoning as `AttachmentDownloaded`:
    /// see `apply_attachment_uploaded`'s own doc comment for how a stale
    /// response is guarded instead. `result`'s `Ok` payload is Jira's
    /// response to the upload — normally just the one newly-created
    /// attachment — merged into `App::detail`/`detail_cache`.
    AttachmentUploaded {
        key: String,
        filename: String,
        result: Result<Vec<Attachment>, String>,
    },
}

impl App {
    /// Bump and return the current fetch generation counter. Every
    /// dispatched fetch is tagged with the generation it was started
    /// under; `apply_event` drops results whose generation has since gone
    /// stale.
    pub(crate) fn bump_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// Apply a completed fetch's result, unless it's been superseded by a
    /// newer refresh/switch_view dispatched after it. Each arm just hands
    /// off to the `App::apply_*` method living beside the `dispatch_*` that
    /// produced this event — see the sibling `list_ops`/`mutation_ops`/
    /// `setup_ops` files.
    pub fn apply_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Refreshed {
                generation,
                issues,
                source,
                status,
            } => self.apply_refreshed(generation, issues, source, status),
            AppEvent::ViewSwitched {
                generation,
                view,
                issues,
                source,
                status,
            } => self.apply_view_switched(generation, view, issues, source, status),
            AppEvent::DetailLoaded {
                generation,
                key,
                detail,
                status,
            } => self.apply_detail_loaded(generation, key, detail, status),
            AppEvent::TransitionApplied {
                generation,
                key,
                to,
                error,
            } => self.apply_transition_applied(generation, key, to, error),
            AppEvent::DescriptionUpdated {
                generation,
                key,
                adf,
                error,
                return_screen,
            } => self.apply_description_updated(generation, key, adf, error, return_screen),
            AppEvent::CommentAdded {
                generation,
                key,
                result,
                return_screen,
            } => self.apply_comment_added(generation, key, result, return_screen),
            AppEvent::FieldsLoaded {
                generation,
                target,
                origin,
                result,
            } => self.apply_fields_loaded(generation, target, origin, result),
            AppEvent::CredentialsVerified {
                generation,
                issues,
                source,
                status,
            } => self.apply_credentials_verified(generation, issues, source, status),
            AppEvent::TeammatesDiscovered { users } => self.apply_teammates_discovered(users),
            AppEvent::AssigneeApplied {
                generation,
                key,
                display_name,
                error,
            } => self.apply_assignee_applied(generation, key, display_name, error),
            AppEvent::TextSearched {
                generation,
                query,
                issues,
                error,
            } => self.apply_text_searched(generation, query, issues, error),
            AppEvent::ProjectVersionsLoaded { versions } => {
                self.apply_project_versions_loaded(versions)
            }
            AppEvent::AccessibleProjectsLoaded { projects } => {
                self.apply_accessible_projects_loaded(projects)
            }
            AppEvent::VersionsApplied {
                generation,
                key,
                fix_versions,
                fix_error,
                affects_versions,
                affects_error,
            } => self.apply_versions_applied(
                generation,
                key,
                fix_versions,
                fix_error,
                affects_versions,
                affects_error,
            ),
            AppEvent::OpenSprintsLoaded { sprints } => self.apply_open_sprints_loaded(sprints),
            AppEvent::SprintApplied {
                generation,
                key,
                sprint,
                error,
            } => self.apply_sprint_applied(generation, key, sprint, error),
            AppEvent::ReleaseIssuesLoaded {
                generation,
                issues,
                error,
            } => self.apply_release_issues_loaded(generation, issues, error),
            AppEvent::ReleaseBulkApplied {
                generation,
                version_name,
                kind,
                results,
            } => self.apply_release_bulk_applied(generation, version_name, kind, results),
            AppEvent::ProjectIssueTypesLoaded {
                generation,
                project,
                result,
            } => self.apply_project_issue_types_loaded(generation, project, result),
            AppEvent::IssueCreated {
                generation,
                issue_type,
                summary,
                description,
                result,
            } => self.apply_issue_created(generation, issue_type, summary, description, result),
            AppEvent::AttachmentDownloaded {
                key,
                filename,
                result,
            } => self.apply_attachment_downloaded(key, filename, result),
            AppEvent::AttachmentUploaded {
                key,
                filename,
                result,
            } => self.apply_attachment_uploaded(key, filename, result),
        }
    }
}
