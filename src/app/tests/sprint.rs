//! Sprint picker tests.

use super::super::*;
use super::support::*;

#[test]
fn sprint_field_configured_is_always_true_in_demo_mode() {
    let app = demo_app();
    assert!(app.sprint_field_configured());
}

#[test]
fn open_sprint_picker_refuses_when_sprint_isnt_configured() {
    let _guard = crate::test_support::lock_env();
    std::env::remove_var("JIRA_SPRINT_FIELD");
    let mut app = non_demo_app();
    app.open_by_key("DS-2722");
    let before = app.status.clone();
    app.open_sprint_picker();
    assert!(!app.sprint_picker_open);
    assert_ne!(app.status, before, "refusing to open should still say why");
}

#[test]
fn open_sprint_picker_seeds_remove_row_and_demo_sprints() {
    let mut app = demo_app();
    app.open_by_key("DS-2722"); // carries a demo sprint
    app.open_sprint_picker();
    assert!(app.sprint_picker_open);
    assert_eq!(app.sprint_picker.selected, 0);
    assert_eq!(app.sprint_picker.rows[0], SprintRow::RemoveFromSprint);
    let sprint_rows: Vec<_> = app
        .sprint_picker
        .rows
        .iter()
        .filter(|r| !matches!(r, SprintRow::RemoveFromSprint))
        .collect();
    assert_eq!(sprint_rows.len(), crate::domain::demo_open_sprints().len());
}

#[test]
fn sprint_picker_move_clamps_to_bounds() {
    let mut app = demo_app();
    app.open_by_key("DS-2722");
    app.open_sprint_picker();
    let len = app.sprint_picker.rows.len();
    app.sprint_picker_move(-5);
    assert_eq!(app.sprint_picker.selected, 0);
    app.sprint_picker_move(1000);
    assert_eq!(app.sprint_picker.selected, len - 1);
}

#[test]
fn confirm_sprint_picker_moves_issue_into_the_selected_sprint_locally() {
    let mut app = demo_app();
    app.open_by_key("DS-2599"); // no sprint in demo data
    assert!(app.detail.as_ref().unwrap().sprint.is_none());
    app.open_sprint_picker();
    // Row 1 is the first real sprint (row 0 is "Remove from sprint").
    app.sprint_picker.selected = 1;
    let expected = crate::domain::demo_open_sprints()[0].clone();
    app.confirm_sprint_picker();

    assert!(!app.sprint_picker_open);
    assert_eq!(app.detail.as_ref().unwrap().sprint, Some(expected));
}

#[test]
fn confirm_sprint_picker_remove_clears_the_sprint_locally() {
    let mut app = demo_app();
    app.open_by_key("DS-2722"); // has a demo sprint
    assert!(app.detail.as_ref().unwrap().sprint.is_some());
    app.open_sprint_picker();
    app.sprint_picker.selected = 0; // Remove from sprint
    app.confirm_sprint_picker();

    assert!(app.detail.as_ref().unwrap().sprint.is_none());
}

#[test]
fn confirm_sprint_picker_with_no_change_is_a_no_op() {
    let mut app = demo_app();
    app.open_by_key("DS-2599"); // no sprint
    app.open_sprint_picker();
    let before = app.status.clone();
    app.sprint_picker.selected = 0; // already has no sprint, so "Remove" changes nothing
    app.confirm_sprint_picker();

    assert!(!app.sprint_picker_open);
    assert_eq!(
        app.status, before,
        "no actual change means nothing to apply, so status shouldn't change"
    );
}

#[tokio::test]
async fn confirm_sprint_picker_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    std::env::set_var("JIRA_SPRINT_FIELD", "customfield_10020");
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.detail.as_mut().unwrap().sprint = None;
    app.screen = Screen::Detail;
    app.open_sprints = crate::domain::demo_open_sprints();
    app.open_sprint_picker();
    assert!(app.sprint_picker_open);
    app.sprint_picker.selected = 1; // first real sprint row

    app.confirm_sprint_picker();
    assert!(app.loading);
    assert!(!app.sprint_picker_open);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(
        app.detail.as_ref().unwrap().sprint,
        Some(crate::domain::demo_open_sprints()[0].clone())
    );

    std::env::remove_var("JIRA_SPRINT_FIELD");
}
