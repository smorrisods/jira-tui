//! Quick-view panel tests.

use super::super::*;
use super::support::*;

#[test]
fn quick_view_uses_cached_detail_after_open() {
    let mut app = demo_app();
    app.selected = 0;
    assert!(app.quick_view_detail().is_none());
    app.open_detail();
    // Returning to the list, the opened issue is cached for quick view.
    assert!(app.detail_cache.contains_key(&app.issues[0].key));
}

#[tokio::test]
async fn ensure_quick_view_loaded_against_a_live_source_does_not_duplicate_in_flight_fetches() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.quick_view = true;
    app.selected = 0;
    let key = app.issues[0].key.clone();

    app.ensure_quick_view_loaded();
    assert!(app.loading);
    let first_generation = app.detail_generation;

    // Called again before the first fetch resolves, exactly like the run
    // loop polling every tick — must not dispatch a second fetch for the
    // same key.
    app.ensure_quick_view_loaded();
    assert_eq!(app.detail_generation, first_generation);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(
        app.screen,
        Screen::Home,
        "quick-view load must not navigate"
    );
    assert!(app.detail_cache.contains_key(&key));
}

/// Regression test for a code-review finding on PR #20: a cache-only
/// quick-view load already in flight for a key must be "upgraded" to
/// navigate once an explicit open comes in for the same key, rather than
/// the open being silently swallowed by the in-flight dedup check.
#[tokio::test]
async fn an_explicit_open_escalates_an_in_flight_quick_view_load_to_navigate() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.quick_view = true;
    app.selected = 0;
    let key = app.issues[0].key.clone();

    // The quick-view panel's per-tick poll dispatches a cache-only load.
    app.ensure_quick_view_loaded();
    assert!(app.loading);
    let generation = app.detail_generation;

    // The user explicitly opens the same issue before that load resolves —
    // must not dispatch a second fetch, but must remember to navigate.
    app.open_by_key(&key);
    assert_eq!(
        app.detail_generation, generation,
        "must not dispatch a duplicate fetch for the same key"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(
        app.screen,
        Screen::Detail,
        "the escalated open must still navigate once the shared fetch resolves"
    );
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}
