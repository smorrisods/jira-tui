//! Release review screen tests.

use super::super::*;
use super::support::*;

#[test]
fn open_release_screen_lists_demo_versions() {
    let mut app = demo_app();
    app.open_release_screen();
    assert_eq!(app.screen, Screen::Release);
    assert!(app.release.drilled.is_none());
    let names: Vec<&str> = app
        .release
        .versions
        .iter()
        .map(|v| v.name.as_str())
        .collect();
    assert!(names.contains(&"v3.4.0"));
    assert!(names.contains(&"v3.5.0"));
}

#[test]
fn release_move_clamps_to_bounds_in_list_mode() {
    let mut app = demo_app();
    app.open_release_screen();
    let len = app.release.versions.len();
    app.release_move(-5);
    assert_eq!(app.release.cursor, 0);
    app.release_move(1000);
    assert_eq!(app.release.cursor, len - 1);
}

#[test]
fn release_confirm_drills_into_the_highlighted_version_in_demo_mode() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();

    assert_eq!(app.release.drilled.as_ref().unwrap().name, "v3.4.0");
    assert!(!app.release.issues.is_empty());
    // Sorted by status, so a run boundary is a real status change.
    let statuses: Vec<&str> = app
        .release
        .issues
        .iter()
        .map(|i| i.status.as_str())
        .collect();
    let mut sorted = statuses.clone();
    sorted.sort();
    assert_eq!(statuses, sorted);
}

#[test]
fn release_confirm_on_a_version_with_no_issues_leaves_the_list_empty() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.6.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();

    assert_eq!(app.release.drilled.as_ref().unwrap().name, "v3.6.0");
    assert!(app.release.issues.is_empty());
}

#[test]
fn release_confirm_on_a_drilled_issue_opens_it() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();
    let key = app.release.issues[0].key.clone();

    app.release.issue_cursor = 0;
    app.release_confirm();

    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}

#[test]
fn release_back_returns_to_the_version_list_then_falls_through() {
    let mut app = demo_app();
    app.open_release_screen();
    app.release_confirm(); // drill into whatever's first
    assert!(app.release.drilled.is_some());

    assert!(app.release_back(), "first back should undrill");
    assert!(app.release.drilled.is_none());

    assert!(
        !app.release_back(),
        "second back has nothing left to undo at the list level"
    );
}

#[test]
fn release_progress_counts_done_issues() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();

    let (done, total) = app.release_progress();
    assert!(total > 0);
    assert_eq!(
        done,
        app.release
            .issues
            .iter()
            .filter(|i| i.status == "Done")
            .count()
    );
}

#[test]
fn release_toggle_selected_checks_and_unchecks_the_highlighted_issue() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();
    let key = app.release.issues[0].key.clone();
    app.release.issue_cursor = 0;

    app.release_toggle_selected();
    assert!(app.release.selected.contains(&key));
    app.release_toggle_selected();
    assert!(!app.release.selected.contains(&key));
}

#[test]
fn release_remove_selected_drops_checked_issues_and_updates_fix_versions_in_demo_mode() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();
    let before = app.release.issues.len();
    assert!(before > 0);
    let key = app.release.issues[0].key.clone();
    // Load this issue's detail into the cache first, so the assertion below
    // can observe `apply_versions_locally`'s effect on it directly.
    app.open_by_key(&key);
    app.screen = Screen::Release;
    app.release.issue_cursor = 0;
    app.release_toggle_selected();

    app.release_remove_selected();

    assert_eq!(app.release.issues.len(), before - 1);
    assert!(!app.release.issues.iter().any(|i| i.key == key));
    assert!(app.release.selected.is_empty());
    assert!(
        !app.detail_cache
            .get(&key)
            .unwrap()
            .fix_versions
            .contains(&"v3.4.0".to_string()),
        "the removed issue's cached detail should no longer list v3.4.0"
    );
}

#[test]
fn release_remove_selected_with_nothing_checked_removes_just_the_highlighted_issue() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();
    let before = app.release.issues.len();
    app.release.issue_cursor = 0;

    app.release_remove_selected();

    assert_eq!(app.release.issues.len(), before - 1);
}

#[test]
fn release_add_to_release_adds_issues_and_refreshes_the_drill_in_demo_mode() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.6.0") // starts with no issues
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();
    assert!(app.release.issues.is_empty());

    app.release_add_to_release("v3.6.0".into(), vec!["DS-2599".into()]);

    assert!(
        app.release.issues.iter().any(|i| i.key == "DS-2599"),
        "the drill-down should refresh to include the newly-added issue"
    );
}

#[tokio::test]
async fn release_bulk_remove_against_a_live_source_dispatches_and_applies() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.project_versions = crate::domain::demo_versions();
    app.open_release_screen();
    app.release.cursor = 0;
    let version_name = app.release.versions[0].name.clone();
    app.release_confirm();
    assert!(app.release.drilled.is_some());

    // Drain the drill-down's own async fetch first.
    let event = next_event(&mut app).await;
    app.apply_event(event);

    app.release.issues = vec![crate::domain::demo_issues()[0].clone()];
    app.release.issue_cursor = 0;
    app.release_remove_selected();
    assert!(app.release.bulk_pending);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.release.bulk_pending);
    let _ = version_name;
}

#[tokio::test]
async fn release_confirm_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.project_versions = crate::domain::demo_versions();
    app.open_release_screen();
    app.release.cursor = 0;

    app.release_confirm();
    assert!(app.release.issues_loading);
    assert!(app.release.drilled.is_some());

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.release.issues_loading);
}
