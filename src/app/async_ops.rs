//! Async dispatch for `refresh`/`switch_view` against live Jira, and
//! applying the results back onto `App` once they arrive.
//!
//! Demo/cache-only sessions skip all of this and resolve inline (there's no
//! network round-trip worth a spinner for) — see `App::refresh` and
//! `App::switch_view`. A genuine live fetch is offloaded via
//! `tokio::task::spawn_blocking` (the Jira REST client is synchronous
//! `ureq`) and its result flows back over an `mpsc` channel, drained by the
//! run loop each iteration and applied here.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::{IssueSummary, Source, ViewKind};

use super::{load_issues_for, App};

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
    /// newer refresh/switch_view dispatched after it.
    pub fn apply_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Refreshed {
                generation,
                issues,
                source,
                status,
            } => {
                if generation != self.generation {
                    return;
                }
                self.loading = false;
                self.all_issues = issues;
                self.source = source;
                self.status = format!("↻ {status}");
                self.recompute_view();
            }
            AppEvent::ViewSwitched {
                generation,
                view,
                issues,
                source,
                status,
            } => {
                if generation != self.generation {
                    return;
                }
                self.loading = false;
                self.all_issues = issues;
                self.source = source;
                let label = view.label();
                self.current_view = view;
                self.status = format!("↻ {status}");
                self.selected = 0;
                self.recompute_view();
                self.flash(format!("viewing: {label}"));
            }
        }
    }
}

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
