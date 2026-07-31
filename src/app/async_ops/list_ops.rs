//! `refresh`/`switch_view` dispatch, the one-shot teammate-discovery fetch,
//! and full-detail loading.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::{
    AssignableUser, IssueDetail, IssueSummary, IssueType, Project, Source, Version, ViewKind,
};

use super::super::loader::load_issues_for;
use super::super::{App, Screen};
use super::AppEvent;

/// Spawn `load_issues_for(view, force_demo)` off the render thread for a
/// `refresh`, sending the result back as `AppEvent::Refreshed`.
pub(crate) fn dispatch_refresh(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    view: ViewKind,
    force_demo: bool,
) {
    tokio::spawn(async move {
        let (issues, source, status) = load(view, force_demo).await;
        let _ = tx.send(AppEvent::Refreshed {
            generation,
            issues,
            source,
            status,
        });
    });
}

/// Spawn `load_issues_for(view, force_demo)` off the render thread for a
/// `switch_view`, sending the result back as `AppEvent::ViewSwitched`.
pub(crate) fn dispatch_switch_view(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    view: ViewKind,
    force_demo: bool,
) {
    tokio::spawn(async move {
        let view_for_result = view.clone();
        let (issues, source, status) = load(view, force_demo).await;
        let _ = tx.send(AppEvent::ViewSwitched {
            generation,
            view: view_for_result,
            issues,
            source,
            status,
        });
    });
}

/// `load_issues_for` calls the blocking `ureq`-based Jira client, so it runs
/// on the blocking-task pool rather than a runtime worker thread.
async fn load(view: ViewKind, force_demo: bool) -> (Vec<IssueSummary>, Source, String) {
    tokio::task::spawn_blocking(move || load_issues_for(&view, force_demo))
        .await
        .unwrap_or_else(|_| {
            (
                Vec::new(),
                Source::Demo,
                "internal error: fetch task panicked".into(),
            )
        })
}

/// Spawn a one-shot background fetch of the project's assignable users,
/// sending the result back as `AppEvent::TeammatesDiscovered`. Dispatched
/// once from `App::new` for a genuine live session so the view picker's
/// teammate list is populated without the user having to manually visit
/// All Project Issues first — see `App::merge_teammate_names`, which
/// applies the result without disturbing `all_issues`/`current_view`. Uses
/// `GET /rest/api/3/user/assignable/search` (`jira::assignable_users`)
/// rather than a full issue search: a single lightweight call listing
/// every assignable project member, with no issue payloads to page
/// through — cheap enough to fire unconditionally on every live-session
/// startup rather than needing to be lazy.
pub(crate) fn dispatch_teammate_discovery(tx: UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let users = tokio::task::spawn_blocking(assignable_users_blocking)
            .await
            .unwrap_or_default();
        let _ = tx.send(AppEvent::TeammatesDiscovered { users });
    });
}

/// Mirrors `fetch_detail_blocking`'s "load config, call the live client,
/// fall back on any failure" shape. Returns an empty list (rather than
/// demo data) on failure since there's nothing sensible to show in the
/// view picker for a broken live session — it just stays as-is until the
/// user manually visits a view that reveals teammates another way.
#[allow(unused_variables)]
fn assignable_users_blocking() -> Vec<AssignableUser> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            if let Ok(users) = crate::jira::assignable_users(&cfg, &cfg.project) {
                return users;
            }
        }
    }
    Vec::new()
}

/// Spawn a one-shot background fetch of the current project's versions,
/// sending the result back as `AppEvent::ProjectVersionsLoaded`. Dispatched
/// once from `App::new` for a genuine live session, mirroring
/// `dispatch_teammate_discovery` — so the version picker (`R`) and the
/// release review screen both have data the moment the user opens them,
/// without a dedicated fetch-on-open round-trip.
pub(crate) fn dispatch_project_versions(tx: UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let versions = tokio::task::spawn_blocking(project_versions_blocking)
            .await
            .unwrap_or_default();
        let _ = tx.send(AppEvent::ProjectVersionsLoaded { versions });
    });
}

/// Mirrors `assignable_users_blocking`'s "no credentials/failure means an
/// empty list" shape — there's nothing sensible to show in the picker/
/// release screen for a broken live session either.
#[allow(unused_variables)]
fn project_versions_blocking() -> Vec<Version> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            if let Ok(versions) = crate::jira::list_versions(&cfg, &cfg.project) {
                return versions;
            }
        }
    }
    Vec::new()
}

/// Spawn a one-shot background fetch of every project the authenticated
/// user can access, sending the result back as
/// `AppEvent::AccessibleProjectsLoaded`. Dispatched once from `App::new`
/// for a genuine live session, mirroring `dispatch_project_versions` — so
/// the new-issue form's project picker (`app::project_picker`) has data the
/// moment it's opened, without a dedicated fetch-on-open round-trip.
pub(crate) fn dispatch_accessible_projects(tx: UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let projects = tokio::task::spawn_blocking(accessible_projects_blocking)
            .await
            .unwrap_or_default();
        let _ = tx.send(AppEvent::AccessibleProjectsLoaded { projects });
    });
}

/// Mirrors `project_versions_blocking`'s "no credentials/failure means an
/// empty list" shape — the project picker falls back to nothing selectable
/// rather than anything misleading for a broken live session.
#[allow(unused_variables)]
fn accessible_projects_blocking() -> Vec<Project> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            if let Ok(projects) = crate::jira::list_projects(&cfg) {
                return projects;
            }
        }
    }
    Vec::new()
}

/// Spawn a one-shot background fetch of `project`'s creatable issue types,
/// sending the result back as `AppEvent::ProjectIssueTypesLoaded` — used by
/// the new-issue compose form (`app::new_issue`), both when it first opens
/// and again whenever the user edits the project field. Unlike
/// `dispatch_project_versions`/`dispatch_teammate_discovery` (always for the
/// single fixed `cfg.project`), `project` here is arbitrary user-typed text
/// that can change again before this resolves — `generation` (bumped on
/// every dispatch, mirroring every other async op in this codebase) is what
/// lets `apply_project_issue_types_loaded` drop a superseded result, rather
/// than a project-string comparison (which can't tell "stale" apart from
/// "resolved in order, but the user has since typed something else").
pub(crate) fn dispatch_project_issue_types(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    project: String,
) {
    tokio::spawn(async move {
        let project_for_result = project.clone();
        let result = tokio::task::spawn_blocking(move || project_issue_types_blocking(&project))
            .await
            .unwrap_or_else(|_| Err("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::ProjectIssueTypesLoaded {
            generation,
            project: project_for_result,
            result,
        });
    });
}

/// Unlike `project_versions_blocking`'s "no credentials/failure means an
/// empty list" shape, a genuine HTTP/decode failure here is surfaced as
/// `Err` rather than silently folded into an empty catalog — an empty
/// project-with-no-real-types and "the request itself failed" would
/// otherwise both show the same "no issue types available" message, with no
/// way to tell a typo'd project key apart from an expired token or a
/// network blip. "No live config loaded" is still treated as "nothing to
/// show" (`Ok(Vec::new())`), not an error: it isn't expected to happen for
/// a genuine `Source::Live` session (the only source this is ever dispatched
/// for), so there's nothing actionable to tell the user.
#[allow(unused_variables)]
fn project_issue_types_blocking(project: &str) -> Result<Vec<IssueType>, String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::list_create_issue_types(&cfg, project).map_err(|e| e.to_string());
        }
    }
    Ok(Vec::new())
}

/// Spawn a full-detail fetch off the render thread, sending the result back
/// as `AppEvent::DetailLoaded`. Only dispatched when `App::load_detail`
/// would otherwise make a real live-network call (see `detail.rs`) — demo
/// and cache sessions resolve inline.
pub(crate) fn dispatch_detail_fetch(tx: UnboundedSender<AppEvent>, generation: u64, key: String) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let (detail, status) = tokio::task::spawn_blocking(move || fetch_detail_blocking(&key))
            .await
            .unwrap_or_else(|_| {
                (
                    crate::domain::demo_detail(&key_for_result),
                    Some("internal error: fetch task panicked".into()),
                )
            });
        let _ = tx.send(AppEvent::DetailLoaded {
            generation,
            key: key_for_result,
            detail: Box::new(detail),
            status,
        });
    });
}

/// Mirrors the live branch of `App::load_detail`, minus the `&mut self`
/// access a background task can't have. Falls back to the offline demo
/// detail on any failure, exactly like the synchronous version.
#[allow(unused_variables)]
fn fetch_detail_blocking(key: &str) -> (IssueDetail, Option<String>) {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            match crate::jira::fetch_detail(&cfg, key) {
                Ok(d) => return (d, Some(format!("Loaded {key}"))),
                Err(e) => {
                    return (
                        crate::domain::demo_detail(key),
                        Some(format!("Live fetch failed ({e}); showing sample")),
                    )
                }
            }
        }
    }
    (crate::domain::demo_detail(key), None)
}

/// Spawn the Search screen's live text-search fallback off the render
/// thread, sending the result back as `AppEvent::TextSearched`. Only
/// dispatched for a genuine live session, once the query's been idle long
/// enough — see `App::schedule_live_search`/`App::ensure_search_dispatched`.
pub(crate) fn dispatch_text_search(tx: UnboundedSender<AppEvent>, generation: u64, query: String) {
    tokio::spawn(async move {
        let query_for_result = query.clone();
        let (issues, error) = tokio::task::spawn_blocking(move || text_search_blocking(&query))
            .await
            .unwrap_or_else(|_| {
                (
                    Vec::new(),
                    Some("internal error: search task panicked".into()),
                )
            });
        let _ = tx.send(AppEvent::TextSearched {
            generation,
            query: query_for_result,
            issues,
            error,
        });
    });
}

/// Mirrors `fetch_detail_blocking`'s "load config, call the live client,
/// carry back an explanation on failure" shape — unlike
/// `assignable_users_blocking`, a failed live search is worth surfacing:
/// there's no fallback data to quietly show instead, so silently swallowing
/// the error would just look like the search did nothing.
#[allow(unused_variables)]
fn text_search_blocking(query: &str) -> (Vec<IssueSummary>, Option<String>) {
    #[cfg(feature = "live")]
    {
        let Some(cfg) = crate::jira::Config::load() else {
            return (
                Vec::new(),
                Some("live search skipped: no credentials configured".into()),
            );
        };
        match crate::jira::search_by_text(&cfg, query) {
            Ok(issues) => (issues, None),
            Err(e) => (Vec::new(), Some(format!("live search failed: {e}"))),
        }
    }
    #[cfg(not(feature = "live"))]
    (Vec::new(), None)
}

/// Spawn the release review screen's drill-down fetch off the render
/// thread, sending the result back as `AppEvent::ReleaseIssuesLoaded`. Only
/// dispatched for a genuine live session — demo/cache sessions resolve
/// `App::open_release_drill` synchronously via `domain::demo_issues_for_version`.
pub(crate) fn dispatch_release_issues(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    version_name: String,
) {
    tokio::spawn(async move {
        let (issues, error) =
            tokio::task::spawn_blocking(move || release_issues_blocking(&version_name))
                .await
                .unwrap_or_else(|_| (Vec::new(), Some("internal error: task panicked".into())));
        let _ = tx.send(AppEvent::ReleaseIssuesLoaded {
            generation,
            issues,
            error,
        });
    });
}

/// Mirrors `text_search_blocking`'s "no fallback data, so a failure is
/// worth surfacing" shape.
#[allow(unused_variables)]
fn release_issues_blocking(version_name: &str) -> (Vec<IssueSummary>, Option<String>) {
    #[cfg(feature = "live")]
    {
        let Some(cfg) = crate::jira::Config::load() else {
            return (
                Vec::new(),
                Some("release issues skipped: no credentials configured".into()),
            );
        };
        let jql = crate::jira::jql_for_version(&cfg.project, version_name);
        match crate::jira::search_issues(&cfg, &jql) {
            Ok(issues) => (issues, None),
            Err(e) => (
                Vec::new(),
                Some(format!("release issues fetch failed: {e}")),
            ),
        }
    }
    #[cfg(not(feature = "live"))]
    (Vec::new(), None)
}

impl App {
    /// Applies `AppEvent::Refreshed` — see `dispatch_refresh` above.
    pub(super) fn apply_refreshed(
        &mut self,
        generation: u64,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    ) {
        if generation != self.generation {
            return;
        }
        self.loading = false;
        self.record_synced(issues, source);
        self.status = format!("↻ {status}");
        self.recompute_view();
    }

    /// Applies `AppEvent::ViewSwitched` — see `dispatch_switch_view` above.
    pub(super) fn apply_view_switched(
        &mut self,
        generation: u64,
        view: ViewKind,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    ) {
        if generation != self.generation {
            return;
        }
        self.loading = false;
        self.record_synced(issues, source);
        let label = view.label();
        self.current_view = view;
        self.status = format!("↻ {status}");
        self.selected = 0;
        self.recompute_view();
        self.flash(format!("viewing: {label}"));
    }

    /// Applies `AppEvent::DetailLoaded` — see `dispatch_detail_fetch` above.
    pub(super) fn apply_detail_loaded(
        &mut self,
        generation: u64,
        key: String,
        detail: Box<IssueDetail>,
        status: Option<String>,
    ) {
        if generation != self.detail_generation {
            return;
        }
        self.loading = false;
        // The escalated navigate intent lives on `detail_pending`, not the
        // event — a fetch dispatched as a cache-only quick-view load can be
        // "upgraded" by an explicit open before it resolves (see
        // `dispatch_detail_fetch`).
        let navigate = self
            .detail_pending
            .take()
            .map(|(_, navigate)| navigate)
            .unwrap_or(false);
        if let Some(status) = status {
            self.status = status;
        }
        self.detail_cache.insert(key.clone(), (*detail).clone());
        if navigate {
            self.detail_scroll = 0;
            self.detail = Some(*detail);
            self.screen = Screen::Detail;
            if let Some(pos) = self.issues.iter().position(|i| i.key == key) {
                self.selected = pos;
            }
        }
    }

    /// Applies `AppEvent::TeammatesDiscovered` — see
    /// `dispatch_teammate_discovery` above.
    pub(super) fn apply_teammates_discovered(&mut self, users: Vec<AssignableUser>) {
        let names: Vec<String> = users.iter().map(|u| u.display_name.clone()).collect();
        self.merge_teammate_names(&names);
        self.assignable_users = users;
    }

    /// Applies `AppEvent::ProjectVersionsLoaded` — see
    /// `dispatch_project_versions` above. Also rebuilds `release.versions`
    /// (not just `project_versions`) when the release review screen is
    /// showing the version list — `App::release_refresh` dispatches this
    /// same fetch to reload it, and the two must land together or a manual
    /// refresh would silently do nothing. `rebuild_release_versions` reads
    /// `project_versions` (just updated above) via `project_versions_source`,
    /// so it picks up the fresh list and still respects `list_mode`.
    pub(super) fn apply_project_versions_loaded(&mut self, versions: Vec<Version>) {
        self.project_versions = versions;
        if self.screen == Screen::Release && self.release.drilled.is_none() {
            self.rebuild_release_versions();
        }
    }

    /// Applies `AppEvent::AccessibleProjectsLoaded` — see
    /// `dispatch_accessible_projects` above. Also rebuilds
    /// `project_picker.rows` if the popup happens to already be open (it
    /// starts empty otherwise, since this fetch races the render loop and
    /// there's no other event that would refresh it later).
    pub(super) fn apply_accessible_projects_loaded(&mut self, projects: Vec<Project>) {
        self.accessible_projects = projects;
        if self.project_picker_open {
            self.recompute_project_rows();
        }
    }

    /// Applies `AppEvent::ProjectIssueTypesLoaded` — see
    /// `dispatch_project_issue_types` above. Dropped if `generation` has been
    /// superseded by a newer dispatch (mirrors every other async op's
    /// staleness guard), or if the compose form isn't showing any more — the
    /// user has moved on to composing the description/preview, and applying
    /// a late result there would silently swap `issue_type_index`/
    /// `available_types` out from under an already-made selection that
    /// `apply_new_issue` reads lazily at submit time. Subtask-type entries
    /// are filtered out: this form collects no parent-issue field, so a
    /// subtask is guaranteed to fail regardless of what else is filled in.
    /// On a genuine apply, resets `issue_type_index` to `0` so it can't
    /// point past the end of a shorter new list, and leaves a status hint if
    /// the (filtered) catalog came back empty (nothing valid to create
    /// with) — or, on a genuine fetch failure, a distinct message so it
    /// doesn't read as "this project just has no issue types". A failure
    /// leaves `available_types` untouched (there's nothing better to show)
    /// but deliberately doesn't update `project_for_types`, so the very next
    /// attempt to submit the form (`confirm_new_issue_form`) still reads as
    /// out of sync and retries the fetch automatically.
    pub(super) fn apply_project_issue_types_loaded(
        &mut self,
        generation: u64,
        project: String,
        result: Result<Vec<crate::domain::IssueType>, String>,
    ) {
        if self.screen != Screen::NewIssue || generation != self.new_issue_types_generation {
            return;
        }
        self.new_issue.types_loading = false;
        let types = match result {
            Ok(types) => types,
            Err(e) => {
                self.status = format!("issue type fetch failed: {e}");
                return;
            }
        };
        let types: Vec<_> = types.into_iter().filter(|t| !t.subtask).collect();
        self.new_issue.issue_type_index = 0;
        if types.is_empty() {
            self.status = format!("no issue types found for {project}");
        }
        self.new_issue.available_types = types;
        self.new_issue.project_for_types = project;
    }

    /// Applies `AppEvent::ReleaseIssuesLoaded` — see
    /// `dispatch_release_issues` above. Sorted by status here (not just at
    /// the demo-data call site) so `release_status_groups`' contiguous-run
    /// grouping stays valid regardless of source.
    pub(super) fn apply_release_issues_loaded(
        &mut self,
        generation: u64,
        mut issues: Vec<IssueSummary>,
        error: Option<String>,
    ) {
        if generation != self.release_generation {
            return;
        }
        self.release.issues_loading = false;
        if let Some(e) = error {
            self.status = format!("⚠ {e}");
        }
        issues.sort_by(|a, b| a.status.cmp(&b.status));
        self.release.issues = issues;
    }
}
