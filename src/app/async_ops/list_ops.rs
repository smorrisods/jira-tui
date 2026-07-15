//! `refresh`/`switch_view` dispatch, the one-shot teammate-discovery fetch,
//! and full-detail loading.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::{AssignableUser, IssueDetail, IssueSummary, Source, ViewKind};

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
        self.all_issues = issues;
        self.source = source;
        self.last_synced = Some(std::time::Instant::now());
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
        self.all_issues = issues;
        self.source = source;
        self.last_synced = Some(std::time::Instant::now());
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
}
