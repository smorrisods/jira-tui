//! Assignee picker tests.

use super::super::*;
use super::support::*;

#[test]
fn assignee_picker_pins_me_first_then_lists_others_alphabetically() {
    let mut app = demo_app();
    app.open_detail();
    app.open_assignee_picker();
    assert!(app.assignee_picker_open);
    let labels: Vec<String> = app
        .assignee_picker
        .rows
        .iter()
        .map(|r| match r {
            AssigneeRow::Unassign => "Unassign".to_string(),
            AssigneeRow::User(u) => u.display_name.clone(),
        })
        .collect();
    assert_eq!(labels[0], "Unassign");
    assert_eq!(labels[1], crate::domain::DEMO_CURRENT_USER);
    // Everyone after "me" is alphabetical.
    let rest = &labels[2..];
    let mut sorted = rest.to_vec();
    sorted.sort();
    assert_eq!(rest, sorted.as_slice());
}

#[test]
fn assignee_picker_filters_by_typed_query() {
    let mut app = demo_app();
    app.open_detail();
    app.open_assignee_picker();
    app.assignee_picker_input_char('p');
    app.assignee_picker_input_char('r');
    app.assignee_picker_input_char('i');
    let labels: Vec<String> = app
        .assignee_picker
        .rows
        .iter()
        .map(|r| match r {
            AssigneeRow::Unassign => "Unassign".to_string(),
            AssigneeRow::User(u) => u.display_name.clone(),
        })
        .collect();
    assert!(labels.iter().any(|l| l.contains("priya")));
    assert!(!labels.contains(&"Unassign".to_string()));
    assert!(!labels.contains(&crate::domain::DEMO_CURRENT_USER.to_string()));

    app.assignee_picker_backspace();
    app.assignee_picker_backspace();
    app.assignee_picker_backspace();
    assert!(app
        .assignee_picker
        .rows
        .iter()
        .any(|r| matches!(r, AssigneeRow::Unassign)));
}

#[test]
fn confirm_assignee_updates_assignee_locally_in_demo_mode() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let key = app.detail.as_ref().unwrap().key.clone();
    app.open_assignee_picker();
    // Row 1 is "me" (row 0 is Unassign).
    app.assignee_picker.selected = 1;
    app.confirm_assignee();

    assert!(!app.assignee_picker_open);
    assert_eq!(
        app.detail.as_ref().unwrap().assignee.as_deref(),
        Some(crate::domain::DEMO_CURRENT_USER)
    );
    assert_eq!(
        app.issues
            .iter()
            .find(|i| i.key == key)
            .unwrap()
            .assignee
            .as_deref(),
        Some(crate::domain::DEMO_CURRENT_USER)
    );
}

#[test]
fn confirm_assignee_unassign_clears_assignee_locally() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_assignee_picker();
    app.assignee_picker.selected = 0; // Unassign
    app.confirm_assignee();

    assert!(app.detail.as_ref().unwrap().assignee.is_none());
}

#[test]
fn assignee_picker_move_clamps_to_bounds() {
    let mut app = demo_app();
    app.open_detail();
    app.open_assignee_picker();
    let len = app.assignee_picker.rows.len();
    app.assignee_picker_move(-5);
    assert_eq!(app.assignee_picker.selected, 0);
    app.assignee_picker_move(1000);
    assert_eq!(app.assignee_picker.selected, len - 1);
}

#[tokio::test]
async fn confirm_assignee_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    // Simulate the startup teammate-discovery fetch having already
    // populated the assignable-users cache (see
    // `async_ops::dispatch_teammate_discovery`) — `live_app()`'s user is
    // "me", so that's the display name to pin first in the picker.
    app.assignable_users = vec![
        crate::domain::AssignableUser {
            account_id: "acc-me".into(),
            display_name: "me".into(),
        },
        crate::domain::AssignableUser {
            account_id: "acc-other".into(),
            display_name: "Other Person".into(),
        },
    ];
    app.open_assignee_picker();
    app.assignee_picker.selected = 1; // "me", pinned right after Unassign

    app.confirm_assignee();
    assert!(app.loading);
    assert!(!app.assignee_picker_open);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.detail.as_ref().unwrap().assignee.as_deref(), Some("me"));
    assert_eq!(
        app.issues
            .iter()
            .find(|i| i.key == key)
            .unwrap()
            .assignee
            .as_deref(),
        Some("me")
    );
}
