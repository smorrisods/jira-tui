//! Field-mapping lookup tests.

use super::super::*;
use super::support::*;

#[tokio::test]
async fn open_field_mapping_against_a_live_source_dispatches_and_reports_the_error() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();

    // `live_app()` has a `Source::Live` but no credentials configured (the
    // XDG dir is an empty temp dir), so this exercises the real
    // spawn/spawn_blocking/channel plumbing all the way through to the
    // "No live credentials configured." `Err` applied by
    // `AppEvent::FieldsLoaded` — that check is no longer made synchronously
    // (see `field_mapping.rs`'s module docs).
    let outcome = app.open_field_mapping();
    assert_eq!(outcome, FieldMappingOutcome::Pending);
    assert!(app.loading);
    assert!(app.field_mapping_pending);
    assert_eq!(
        app.screen,
        Screen::Home,
        "must not navigate until the fetch resolves"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert!(!app.field_mapping_pending);
    assert_eq!(
        app.screen,
        Screen::Home,
        "the F key path leaves the screen as-is on failure"
    );
    assert!(app.status.contains("Could not fetch fields"));
}

#[tokio::test]
async fn open_field_mapping_refuses_to_reopen_while_one_is_in_flight() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();

    app.open_field_mapping();
    assert!(app.field_mapping_pending);
    let generation = app.field_mapping_generation;

    let outcome = app.open_field_mapping();
    assert_eq!(outcome, FieldMappingOutcome::Pending);
    assert_eq!(
        app.field_mapping_generation, generation,
        "must not dispatch a second lookup while one is already in flight"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.field_mapping_pending);
}

#[tokio::test]
async fn onboarding_field_mapping_falls_back_to_home_with_the_connected_status_on_failure() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.status = "live · demo.atlassian.net · me".into();
    let connected_status = app.status.clone();

    // Mirrors `submit_credentials`'s handoff: only a synchronous
    // `NotAvailable` (source isn't live, checked here it is) forces
    // `Screen::Home` immediately; a `Pending` result waits for the fetch.
    let outcome = app.open_field_mapping_for_onboarding(connected_status.clone());
    assert_eq!(outcome, FieldMappingOutcome::Pending);
    assert!(app.loading);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(
        app.screen,
        Screen::Home,
        "onboarding must fall back to Home on a failed lookup"
    );
    assert_eq!(
        app.status, connected_status,
        "onboarding overwrites the field-mapping status with the connected status on failure"
    );
}

// Coverage gap noticed while splitting app/tests.rs: every existing
// `AppEvent::FieldsLoaded` test dispatches through the real async path, but
// the test fixtures (`live_app`/`onboarding_app`) deliberately have no
// credentials configured, so that path can only ever resolve to the `Err`
// branch — `apply_event`'s two `Ok` branches (empty catalog / populated
// catalog) had no coverage anywhere. Since `apply_event` is a plain
// synchronous fn, these construct the event directly instead of dispatching.

#[test]
fn apply_event_fields_loaded_ok_builds_the_catalog_and_navigates_to_field_mapping() {
    let mut app = demo_app();
    let generation = app.field_mapping_generation;

    app.apply_event(AppEvent::FieldsLoaded {
        generation,
        origin: super::super::field_mapping::FieldMappingOrigin::Direct,
        result: Ok((
            vec![("customfield_10001".into(), "Acceptance Criteria".into())],
            None,
        )),
    });

    assert_eq!(app.screen, Screen::FieldMapping);
    assert_eq!(
        app.field_mapping.catalog.len(),
        2,
        "catalog includes the leading \"none\" sentinel"
    );
    assert!(app.status.contains("Loaded 1 custom field"));
}

#[test]
fn apply_event_fields_loaded_ok_with_no_custom_fields_reports_none_found() {
    let mut app = demo_app();
    let generation = app.field_mapping_generation;

    app.apply_event(AppEvent::FieldsLoaded {
        generation,
        origin: super::super::field_mapping::FieldMappingOrigin::Direct,
        result: Ok((Vec::new(), None)),
    });

    assert_eq!(
        app.screen,
        Screen::Home,
        "an empty catalog with a Direct origin should not navigate to the field-mapping screen"
    );
    assert!(app.status.contains("No custom fields found"));
}
