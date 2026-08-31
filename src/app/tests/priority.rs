//! Priority picker tests.

use super::super::*;
use super::support::*;

#[test]
fn open_priority_picker_preselects_the_issues_current_priority() {
    let mut app = demo_app();
    app.open_by_key("DS-2722");
    let current = app.detail.as_ref().unwrap().priority.clone();
    app.open_priority_picker();
    assert!(app.priority_picker_open);
    assert_eq!(
        crate::domain::Priority::ALL[app.priority_picker.selected],
        current
    );
}

#[test]
fn priority_picker_move_clamps_to_bounds() {
    let mut app = demo_app();
    app.open_by_key("DS-2722");
    app.open_priority_picker();
    app.priority_picker_move(-5);
    assert_eq!(app.priority_picker.selected, 0);
    app.priority_picker_move(1000);
    assert_eq!(
        app.priority_picker.selected,
        crate::domain::Priority::ALL.len() - 1
    );
}

#[test]
fn confirm_priority_picker_updates_priority_locally_everywhere() {
    let mut app = demo_app();
    let key = app.issues[0].key.clone();
    app.open_by_key(&key);
    app.open_priority_picker();
    // Pick a priority that differs from the issue's current one.
    let current = app.detail.as_ref().unwrap().priority.clone();
    let target_idx = crate::domain::Priority::ALL
        .iter()
        .position(|p| *p != current)
        .unwrap();
    app.priority_picker.selected = target_idx;
    let target = crate::domain::Priority::ALL[target_idx].clone();

    app.confirm_priority_picker();

    assert!(!app.priority_picker_open);
    assert_eq!(app.detail.as_ref().unwrap().priority, target);
    assert_eq!(
        app.issues.iter().find(|i| i.key == key).unwrap().priority,
        target,
        "the list's own copy of the issue should update too"
    );
}

#[test]
fn confirm_priority_picker_with_no_change_is_a_no_op() {
    let mut app = demo_app();
    app.open_by_key("DS-2722");
    app.open_priority_picker();
    let before = app.status.clone();
    // The picker already preselects the current priority.
    app.confirm_priority_picker();

    assert!(!app.priority_picker_open);
    assert_eq!(
        app.status, before,
        "no actual change means nothing to apply, so status shouldn't change"
    );
}

#[tokio::test]
async fn confirm_priority_picker_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.detail.as_mut().unwrap().priority = crate::domain::Priority::Low;
    app.screen = Screen::Detail;
    app.open_priority_picker();
    assert!(app.priority_picker_open);
    // Row 0 is Highest — differs from the seeded Low.
    app.priority_picker.selected = 0;

    app.confirm_priority_picker();
    assert!(app.loading);
    assert!(!app.priority_picker_open);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(
        app.detail.as_ref().unwrap().priority,
        crate::domain::Priority::Highest
    );
}
