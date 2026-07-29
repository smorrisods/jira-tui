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
