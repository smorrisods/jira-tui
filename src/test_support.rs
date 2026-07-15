//! Test-only support shared across unit tests in different modules.
//!
//! Several tests mutate process-global environment variables
//! (`XDG_CONFIG_HOME`, `JIRA_*`) to exercise config/credential loading in
//! isolation. Under `cargo nextest` each test gets its own process, so this
//! doesn't matter — but under plain `cargo test`, unit tests in the same
//! binary run concurrently on threads that share one process's environment,
//! and two tests mutating the same env vars at once can race. This mutex
//! serializes just that env-mutating section, regardless of test runner.
//!
//! It's a `tokio::sync::Mutex` rather than `std::sync::Mutex` so the async
//! tests exercising `App::refresh`/`switch_view`'s dispatch-to-`spawn_blocking`
//! path (which read these env vars from a background task, not just at the
//! `await` call site) can hold the guard across an `.await` without
//! `clippy::await_holding_lock` firing, while sync tests share the same lock
//! via `lock_env`'s `blocking_lock`.

use tokio::sync::{Mutex, MutexGuard};

use crate::domain::{IssueSummary, Priority};

static ENV_LOCK: Mutex<()> = Mutex::const_new(());

/// A minimal `IssueSummary` fixture for tests that only care about a couple
/// of fields — override the rest with struct-update syntax, e.g.
/// `IssueSummary { blocked: true, ..sample_issue("DS-1") }`. Shared so
/// per-module test builders (`app::tree`, `app::tests::list_and_detail`,
/// …) don't each hand-roll the same nine-field literal.
pub(crate) fn sample_issue(key: &str) -> IssueSummary {
    IssueSummary {
        key: key.to_string(),
        summary: format!("summary for {key}"),
        issue_type: "Task".into(),
        status: "To Do".into(),
        priority: Priority::Medium,
        assignee: None,
        blocked: false,
        updated: "now".into(),
        epic: None,
    }
}

/// Acquire the shared env-var lock from synchronous test code.
pub(crate) fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK.blocking_lock()
}

/// Acquire the shared env-var lock from async test code — safe to hold
/// across `.await`, unlike a `std::sync::Mutex` guard.
pub(crate) async fn lock_env_async() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().await
}
