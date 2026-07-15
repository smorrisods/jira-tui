//! Shared test fixtures reused across the other files in this module:
//! demo/non-demo/live/onboarding `App` builders and the async event-loop
//! helper `next_event`.

use super::super::*;

pub(super) fn demo_app() -> App {
    let mut app = App::new(true);
    app.screen = Screen::Home;
    app
}

/// Drain a completed fetch off `app.events_rx`, with a generous timeout —
/// these tests have no real network to wait on, only `spawn_blocking`
/// scheduling, so this should resolve almost immediately.
pub(super) async fn next_event(app: &mut App) -> AppEvent {
    tokio::time::timeout(std::time::Duration::from_secs(5), app.events_rx.recv())
        .await
        .expect("fetch task did not complete in time")
        .expect("events_tx dropped unexpectedly")
}

/// A non-demo session (no credentials configured) still exercises the real
/// async dispatch path in `refresh`/`switch_view` — `load_issues_for` falls
/// back to demo data internally, but the point here is the
/// spawn/spawn_blocking/channel plumbing around it, not the fetched data.
/// Points `XDG_CONFIG_HOME` at an empty temp dir and clears the `JIRA_*` env
/// vars so `Config::load()` deterministically finds no credentials — this
/// machine's real `~/.config/jira-tui/config.toml` (if any) must not leak
/// in and trigger an actual network call. See `crate::test_support::lock_env_async`,
/// held for the caller's whole test (including across the `.await` that
/// drains the fetch) so a racing test can't change these back mid-flight.
pub(super) fn non_demo_app() -> App {
    let base = std::env::temp_dir().join(format!(
        "jira-tui-asynctest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::env::set_var("XDG_CONFIG_HOME", &base);
    for var in [
        "JIRA_BASE_URL",
        "JIRA_EMAIL",
        "JIRA_API_TOKEN",
        "JIRA_TOKEN_FILE",
    ] {
        std::env::remove_var(var);
    }
    let mut app = demo_app();
    app.source = crate::domain::Source::Cache { user: "me".into() };
    app
}

/// Like `non_demo_app`, but with a genuine `Source::Live` session — the
/// condition every detail/transition/edit dispatch actually checks (unlike
/// `refresh`/`switch_view`, which also treat `Source::Cache` as needing a
/// live round-trip). Reuses the same `XDG_CONFIG_HOME`/`JIRA_*` isolation so
/// `Config::load()` deterministically finds no credentials and these tests
/// exercise the spawn/spawn_blocking/channel plumbing (falling back to demo
/// detail data) rather than a real network call.
pub(super) fn live_app() -> App {
    let mut app = non_demo_app();
    app.source = crate::domain::Source::Live {
        site: "demo.atlassian.net".into(),
        user: "me".into(),
    };
    app
}

/// A demo app with `XDG_CONFIG_HOME`/`JIRA_*` isolated the same way
/// `non_demo_app` is, but without pre-setting `Source` — `submit_credentials`
/// decides everything itself (it writes fresh credentials, then dispatches
/// a verification fetch). Only meaningful under the `live` feature:
/// `submit_credentials`'s dispatch is itself compiled out (in favour of a
/// distinct "this build has no live support" message) when `live` is off,
/// unlike detail/transition/edit/field_mapping's dispatch, which is always
/// compiled and just checks `Source::Live` at runtime.
#[cfg(feature = "live")]
pub(super) fn onboarding_app() -> App {
    non_demo_app()
}
