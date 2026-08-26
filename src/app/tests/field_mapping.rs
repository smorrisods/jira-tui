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
        target: super::super::field_mapping::FieldMappingTarget::AcceptanceCriteria,
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
        target: super::super::field_mapping::FieldMappingTarget::AcceptanceCriteria,
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

// The field-mapping screen was originally hardcoded to Acceptance Criteria
// alone; generalized once Sprint needed the exact same "search this site's
// custom fields, remember the choice" flow. These cover the generalization
// itself — `Tab` cycling, per-target persistence, and onboarding's own
// target override — not the parts already covered above (which exercise
// AcceptanceCriteria, the default target, so needed no changes at all).

#[test]
fn field_mapping_target_cycles_and_wraps() {
    use super::super::field_mapping::FieldMappingTarget;

    assert_eq!(
        FieldMappingTarget::AcceptanceCriteria.next(),
        FieldMappingTarget::Sprint
    );
    assert_eq!(
        FieldMappingTarget::Sprint.next(),
        FieldMappingTarget::AcceptanceCriteria,
        "cycling must wrap back to the first target, not dead-end"
    );
}

#[test]
fn cycle_field_mapping_target_is_a_noop_before_the_catalog_has_loaded() {
    let mut app = demo_app();
    let target_before = app.field_mapping.target;

    app.cycle_field_mapping_target();

    assert_eq!(
        app.field_mapping.target, target_before,
        "nothing to re-derive selection/current_mapping against before a catalog exists"
    );
}

#[test]
fn cycle_field_mapping_target_reselects_and_relabels_for_the_new_target() {
    use super::super::field_mapping::FieldMappingTarget;

    let _guard = crate::test_support::lock_env();
    let base = std::env::temp_dir().join(format!(
        "jira-tui-test-field-mapping-cycle-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::env::set_var("XDG_CONFIG_HOME", base.join("config"));
    std::env::remove_var("JIRA_ACCEPTANCE_CRITERIA_FIELD");
    std::env::remove_var("JIRA_SPRINT_FIELD");
    crate::config::save_field_mapping("acceptance_criteria_field", Some("customfield_10001"))
        .unwrap();
    crate::config::save_field_mapping("sprint_field", Some("customfield_10020")).unwrap();

    let mut app = demo_app();
    app.field_mapping.catalog = vec![
        (
            String::new(),
            "— none — don't track Acceptance Criteria".into(),
        ),
        ("customfield_10001".into(), "Acceptance Criteria".into()),
        ("customfield_10020".into(), "Sprint".into()),
    ];
    app.field_mapping.target = FieldMappingTarget::AcceptanceCriteria;
    app.field_mapping.current_mapping = Some("customfield_10001".into());
    app.field_mapping.selected = 1;

    app.cycle_field_mapping_target();

    assert_eq!(app.field_mapping.target, FieldMappingTarget::Sprint);
    assert_eq!(
        app.field_mapping.current_mapping.as_deref(),
        Some("customfield_10020"),
        "re-derived from config.toml's sprint_field, not left over from Acceptance Criteria"
    );
    assert_eq!(
        app.field_mapping.selected, 2,
        "the row matching Sprint's own mapping must be pre-selected"
    );
    assert!(
        app.field_mapping.catalog[0].1.contains("Sprint"),
        "the leading sentinel's label must be relabelled for the new target, got: {}",
        app.field_mapping.catalog[0].1
    );

    std::env::remove_var("XDG_CONFIG_HOME");
}

#[test]
fn confirm_field_mapping_saves_to_the_current_targets_own_config_key() {
    use super::super::field_mapping::FieldMappingTarget;

    let _guard = crate::test_support::lock_env();
    let base = std::env::temp_dir().join(format!(
        "jira-tui-test-field-mapping-confirm-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::env::set_var("XDG_CONFIG_HOME", base.join("config"));
    std::env::remove_var("JIRA_ACCEPTANCE_CRITERIA_FIELD");
    std::env::remove_var("JIRA_SPRINT_FIELD");

    let mut app = demo_app();
    app.field_mapping.target = FieldMappingTarget::Sprint;
    app.field_mapping.catalog = vec![
        (String::new(), "— none —".into()),
        ("customfield_10020".into(), "Sprint".into()),
    ];
    app.field_mapping.selected = 1;

    app.confirm_field_mapping();

    let kv = crate::config::read_kv();
    assert_eq!(
        kv.get("sprint_field").map(String::as_str),
        Some("customfield_10020"),
        "Sprint's own config key must be written"
    );
    assert_eq!(
        kv.get("acceptance_criteria_field"),
        None,
        "mapping Sprint must never touch acceptance_criteria_field"
    );
    assert!(app.status.contains("Mapped Sprint"));

    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("JIRA_SPRINT_FIELD");
}

#[tokio::test]
async fn onboarding_field_mapping_always_targets_acceptance_criteria() {
    let _guard = crate::test_support::lock_env_async().await;
    use super::super::field_mapping::FieldMappingTarget;

    let mut app = live_app();
    // Simulate the screen having been left on Sprint from a previous `F`
    // visit — onboarding's handoff must not inherit that.
    app.field_mapping.target = FieldMappingTarget::Sprint;

    app.open_field_mapping_for_onboarding("live · demo.atlassian.net · me".into());

    assert_eq!(
        app.field_mapping.target,
        FieldMappingTarget::AcceptanceCriteria,
        "onboarding's credential-verification handoff is specifically about \
         Acceptance Criteria, regardless of whatever F was last showing"
    );

    // Drain the dispatched fetch so this test doesn't leak a pending task.
    let event = next_event(&mut app).await;
    app.apply_event(event);
}
