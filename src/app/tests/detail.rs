//! Detail-loading tests.

use super::super::*;
use super::support::*;

#[test]
fn demo_detail_unknown_key_is_clearly_labelled_not_found() {
    let detail = crate::domain::demo_detail("DS-99999");
    assert_eq!(detail.key, "DS-99999", "must preserve the requested key");
    assert!(detail.summary.to_lowercase().contains("not found"));
}

#[test]
fn open_by_key_syncs_selection_when_present_in_view() {
    let mut app = demo_app();
    let key = app.issues[2].key.clone();
    app.selected = 0;
    app.open_by_key(&key);
    assert_eq!(app.selected, 2);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}

#[tokio::test]
async fn open_by_key_against_a_live_source_dispatches_and_navigates_once_loaded() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();

    app.open_by_key(&key);
    assert!(app.loading);
    assert_eq!(
        app.screen,
        Screen::Home,
        "must not navigate until the fetch resolves"
    );
    assert!(app.detail.is_none());

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
    assert!(app.detail_cache.contains_key(&key));
}

#[test]
fn refresh_detail_reloads_the_open_issue_without_touching_history() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_by_key("DS-9001"); // build up some link-navigation history
    assert!(app.can_go_back());
    assert!(!app.can_go_forward());

    app.detail_scroll = 7;
    app.refresh_detail();

    // Same issue is still shown, and the back/forward stacks — which a
    // real navigation would touch — are untouched by a refresh.
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    assert!(app.can_go_back());
    assert!(!app.can_go_forward());
    assert!(app.detail_cache.contains_key("DS-9001"));
}

#[test]
fn refresh_detail_does_nothing_with_no_issue_open() {
    let mut app = demo_app();
    app.refresh_detail();
    assert!(app.detail.is_none());
}

#[test]
fn refresh_detail_refreshes_the_focused_quick_view_issue_from_the_list() {
    let mut app = demo_app();
    app.quick_view = true;
    app.list_focus = ListFocus::QuickView;
    app.selected = 0;
    let key = app.issues[0].key.clone();
    app.ensure_quick_view_loaded();
    assert!(app.detail_cache.contains_key(&key));

    app.refresh_detail();
    // Detail screen was never entered, but the quick-view cache entry for
    // the selected issue is refreshed in place.
    assert_eq!(app.screen, Screen::Home);
    assert!(app.detail_cache.contains_key(&key));
}

#[tokio::test]
async fn refresh_detail_against_a_live_source_updates_the_open_issue_once_loaded() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.open_by_key(&key);
    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert_eq!(app.detail.as_ref().unwrap().key, key);

    app.detail_scroll = 5;
    app.refresh_detail();
    assert!(app.loading);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}
