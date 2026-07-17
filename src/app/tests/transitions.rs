//! Workflow transition tests.

use super::super::*;
use super::support::*;

#[test]
fn confirm_transition_updates_status_locally() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    // Pick the "Done" transition (index 3 in the demo model).
    app.open_transitions();
    app.picker_index = 3;
    app.confirm_transition();
    assert!(!app.picker_open);
    assert_eq!(app.detail.as_ref().unwrap().status, "Done");
    // The summary list reflects it too.
    let key = &app.detail.as_ref().unwrap().key;
    assert_eq!(
        app.issues.iter().find(|i| &i.key == key).unwrap().status,
        "Done"
    );
}

#[test]
fn confirm_transition_to_done_triggers_the_jax_party_scene() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_transitions();
    app.picker_index = 3; // "Done", per the demo transitions list.
    app.confirm_transition();
    assert!(
        app.jax_party_until > app.tick,
        "transitioning to Done should trigger a reactive party moment"
    );
}

#[test]
fn confirm_transition_to_a_non_done_status_does_not_trigger_the_jax_party_scene() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_transitions();
    app.picker_index = 1; // "In Progress", per the demo transitions list.
    app.confirm_transition();
    assert_eq!(
        app.jax_party_until, 0,
        "transitioning to a non-Done status should not trigger a party moment"
    );
}

#[tokio::test]
async fn confirm_transition_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    let initial_status = app.detail.as_ref().unwrap().status.clone();
    app.screen = Screen::Detail;
    app.open_transitions();
    // "Done", per the demo transitions list.
    app.picker_index = 3;

    app.confirm_transition();
    assert!(app.loading);
    assert!(!app.picker_open);
    assert_eq!(
        app.detail.as_ref().unwrap().status,
        initial_status,
        "must not apply until the transition resolves"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.detail.as_ref().unwrap().status, "Done");
    assert_eq!(
        app.issues.iter().find(|i| i.key == key).unwrap().status,
        "Done"
    );
}

/// Regression test for a code-review finding on PR #20: without a
/// re-entrancy guard, cancelling out of a pending edit/transition and
/// immediately starting a new one bumps the shared generation counter,
/// silently discarding the first request's result (success or failure)
/// with no user-visible feedback. `open_transitions` now refuses to reopen
/// the picker while a transition is still in flight.
#[tokio::test]
async fn open_transitions_refuses_to_reopen_while_one_is_in_flight() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_transitions();
    app.picker_index = 3; // "Done"
    app.confirm_transition();
    assert!(app.loading);
    assert!(app.transition_pending);
    let generation = app.transition_generation;

    // Reopening the picker while the first transition is still resolving
    // must not be possible — that would let a second `confirm_transition`
    // bump the generation counter out from under the first request.
    app.open_transitions();
    assert!(
        !app.picker_open,
        "the picker must not reopen while a transition is in flight"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert!(!app.transition_pending);
    assert_eq!(app.detail.as_ref().unwrap().status, "Done");
    assert_eq!(app.transition_generation, generation);
}
