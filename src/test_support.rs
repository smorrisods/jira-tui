//! Test-only support shared across unit tests in different modules.
//!
//! Several tests mutate process-global environment variables
//! (`XDG_CONFIG_HOME`, `JIRA_*`) to exercise config/credential loading in
//! isolation. Under `cargo nextest` each test gets its own process, so this
//! doesn't matter — but under plain `cargo test`, unit tests in the same
//! binary run concurrently on threads that share one process's environment,
//! and two tests mutating the same env vars at once can race. This mutex
//! serializes just that env-mutating section, regardless of test runner.

use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the shared env-var lock for the duration of a test. If a
/// previous test panicked while holding it, recover the guard rather than
/// poisoning every subsequent test.
pub(crate) fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
