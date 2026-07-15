//! The `AppEvent` enum and `apply_event` dispatcher — a thin, one-arm-per-
//! variant table. Each arm's actual apply logic lives in an `App::apply_*`
//! method co-located with its `dispatch_*` counterpart in `list_ops.rs`,
//! `mutation_ops.rs`, or `setup_ops.rs`, so this file only grows by one
//! match arm — not a whole logic block — per new `AppEvent` variant.

use crate::domain::{AssignableUser, Comment, IssueDetail, IssueSummary, Source, ViewKind};

use super::super::{App, Screen};
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
                origin,
                result,
            } => self.apply_fields_loaded(generation, origin, result),
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
        }
    }
}
