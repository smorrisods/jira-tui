//! Tests for view switching, onboarding, field-mapping, the async_ops
//! core dispatch (refresh/switch_view/teammate discovery), and Detail's
//! in-body-link navigation history.

use super::super::*;
use super::support::*;

#[test]
fn credential_form_edits_focused_field() {
    let mut app = demo_app();
    app.onboarding.focus = Field::Email;
    app.input_char('a');
    app.input_char('b');
    assert_eq!(app.onboarding.field_email, "ab");
    app.input_backspace();
    assert_eq!(app.onboarding.field_email, "a");
    app.focus_next();
    assert_eq!(app.onboarding.focus, Field::Token);
    app.focus_prev();
    assert_eq!(app.onboarding.focus, Field::Email);
}

#[test]
fn submit_with_empty_fields_reports_and_does_not_panic() {
    let mut app = demo_app();
    app.onboarding.field_site.clear();
    app.onboarding.field_email.clear();
    app.onboarding.field_token.clear();
    app.submit_credentials();
    assert!(!app.onboarding.setup_msg.is_empty());
}

#[test]
fn open_view_picker_lists_my_work_all_project_and_teammates() {
    let mut app = demo_app();
    app.open_view_picker();
    assert!(app.view_picker_open);
    assert_eq!(app.view_picker_options[0], ViewKind::MyWork);
    assert_eq!(app.view_picker_options[1], ViewKind::AllProject);
    // Teammates distinct from the demo "current user" should show up,
    // deduped and sorted; the demo current user itself must not appear as a
    // redundant pseudo-teammate (it's already covered by "My Work").
    let teammates: Vec<&ViewKind> = app.view_picker_options[2..].iter().collect();
    assert!(teammates.contains(&&ViewKind::Teammate("priya.nair".into())));
    assert!(teammates.contains(&&ViewKind::Teammate("alex.chen".into())));
    assert!(!teammates.contains(&&ViewKind::Teammate(
        crate::domain::DEMO_CURRENT_USER.into()
    )));
}

#[test]
fn view_picker_move_clamps_to_bounds() {
    let mut app = demo_app();
    app.open_view_picker();
    let len = app.view_picker_options.len();
    app.view_picker_move(-10);
    assert_eq!(app.view_picker_index, 0);
    app.view_picker_move(1000);
    assert_eq!(app.view_picker_index, len - 1);
}

#[test]
fn confirm_view_switch_to_teammate_filters_by_assignee() {
    let mut app = demo_app();
    app.open_view_picker();
    let idx = app
        .view_picker_options
        .iter()
        .position(|v| *v == ViewKind::Teammate("priya.nair".into()))
        .expect("priya.nair should be a demo teammate");
    app.view_picker_index = idx;
    app.confirm_view_switch();

    assert!(!app.view_picker_open);
    assert_eq!(app.current_view, ViewKind::Teammate("priya.nair".into()));
    assert!(!app.all_issues.is_empty());
    assert!(app
        .all_issues
        .iter()
        .all(|i| i.assignee.as_deref() == Some("priya.nair")));
}

#[test]
fn switch_view_to_all_project_then_back_to_my_work_round_trips() {
    let mut app = demo_app();
    let my_work_count = app.all_issues.len();

    app.switch_view(ViewKind::AllProject);
    assert_eq!(app.current_view, ViewKind::AllProject);
    assert!(!app.all_issues.is_empty());

    app.switch_view(ViewKind::MyWork);
    assert_eq!(app.current_view, ViewKind::MyWork);
    assert_eq!(app.all_issues.len(), my_work_count);
}

#[test]
fn refresh_preserves_the_current_view() {
    let mut app = demo_app();
    app.switch_view(ViewKind::Teammate("alex.chen".into()));
    app.refresh();
    assert_eq!(app.current_view, ViewKind::Teammate("alex.chen".into()));
    assert!(app
        .all_issues
        .iter()
        .all(|i| i.assignee.as_deref() == Some("alex.chen")));
}

#[test]
fn known_teammates_persist_after_switching_to_a_narrower_view() {
    let mut app = demo_app();
    // `demo_app()`'s constructor already seeded `teammates_seen` from the
    // demo dataset's shortcut "My Work" (which doesn't filter by
    // assignee); reset it to start from the same blank slate a real
    // session would after its first genuinely-filtered "My Work" load.
    app.teammates_seen.clear();
    app.all_issues = crate::domain::demo_issues()
        .into_iter()
        .filter(|i| i.assignee.as_deref() == Some(crate::domain::DEMO_CURRENT_USER))
        .collect();
    app.recompute_view();
    assert!(app.known_teammates().is_empty());

    // Loading a broader view (e.g. All Project Issues) reveals teammates.
    app.all_issues = crate::domain::demo_issues();
    app.recompute_view();
    let discovered = app.known_teammates();
    assert!(discovered.contains(&"priya.nair".to_string()));
    assert!(discovered.contains(&"alex.chen".to_string()));

    // Switching to a single teammate's work narrows `all_issues` down to
    // just their issues again.
    app.all_issues = crate::domain::demo_issues()
        .into_iter()
        .filter(|i| i.assignee.as_deref() == Some("priya.nair"))
        .collect();
    app.recompute_view();

    // Every teammate discovered so far must still be listed, even though
    // `all_issues` no longer mentions most of them — this is the bug fix:
    // teammate selection (and the picker's contents) must survive
    // navigating past the All Project Issues view, not reset to whatever
    // `all_issues` happens to hold right now.
    assert_eq!(app.known_teammates(), discovered);
}

#[tokio::test]
async fn refresh_against_a_non_demo_source_dispatches_and_clears_loading() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app();
    assert!(!app.loading);

    app.refresh();
    assert!(
        app.loading,
        "refresh should flip on the loading flag immediately"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(
        !app.loading,
        "applying the result should clear the loading flag"
    );
}

#[tokio::test]
async fn switch_view_against_a_non_demo_source_dispatches_and_updates_current_view() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app();

    app.switch_view(ViewKind::AllProject);
    assert!(app.loading);
    // current_view/selected only update once the fetch resolves and is applied.
    assert_eq!(app.current_view, ViewKind::MyWork);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.current_view, ViewKind::AllProject);
}

#[tokio::test]
async fn a_superseded_fetch_result_is_dropped_instead_of_clobbering_newer_state() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app();

    app.refresh();
    let stale_generation = app.generation;
    let stale_event = next_event(&mut app).await;

    // A second refresh starts before the first result is applied, bumping
    // the generation counter past the first request's.
    app.refresh();
    assert_ne!(app.generation, stale_generation);

    app.apply_event(stale_event);
    assert!(
        app.loading,
        "the stale result must not clear loading for the newer, still in-flight request"
    );

    let fresh_event = next_event(&mut app).await;
    app.apply_event(fresh_event);
    assert!(!app.loading);
}

#[tokio::test]
async fn teammate_discovery_merges_without_disturbing_the_active_view() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    // Start from a narrow view with no teammates discovered yet, mirroring
    // a fresh live session that just loaded "My Work".
    app.teammates_seen.clear();
    let before_view = app.current_view.clone();
    let before_keys: Vec<String> = app.all_issues.iter().map(|i| i.key.clone()).collect();

    // `dispatch_teammate_discovery` calls the real `assignable_users`
    // endpoint, which needs a `Config`; `live_app()` deliberately has none
    // configured (see `non_demo_app`), so this only exercises the
    // spawn/spawn_blocking/channel plumbing — the merge logic itself is
    // covered directly below via `merge_teammate_names`.
    super::super::async_ops::dispatch_teammate_discovery(app.events_tx.clone());
    let event = next_event(&mut app).await;
    app.apply_event(event);

    // The background discovery fetch must never touch the active view —
    // only merge names into `teammates_seen`.
    assert_eq!(app.current_view, before_view);
    let after_keys: Vec<String> = app.all_issues.iter().map(|i| i.key.clone()).collect();
    assert_eq!(after_keys, before_keys);
}

#[test]
fn merge_teammate_names_excludes_me_and_accumulates() {
    let mut app = demo_app();
    app.teammates_seen.clear();

    app.merge_teammate_names(&[
        "priya.nair".to_string(),
        "alex.chen".to_string(),
        crate::domain::DEMO_CURRENT_USER.to_string(),
    ]);

    let discovered = app.known_teammates();
    assert!(discovered.contains(&"priya.nair".to_string()));
    assert!(discovered.contains(&"alex.chen".to_string()));
    assert!(!discovered.contains(&crate::domain::DEMO_CURRENT_USER.to_string()));

    // A second, overlapping call accumulates rather than replaces.
    app.merge_teammate_names(&["jordan.blake".to_string()]);
    let discovered = app.known_teammates();
    assert!(discovered.contains(&"priya.nair".to_string()));
    assert!(discovered.contains(&"jordan.blake".to_string()));
}

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

#[cfg(feature = "live")]
#[tokio::test]
async fn submit_credentials_against_an_unreachable_site_dispatches_and_reports_rejection() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = onboarding_app();
    // Port 1 is never a listening service, so this fails fast with a
    // connection error rather than hanging on a real network round-trip or
    // a slow DNS lookup for a bogus hostname.
    app.onboarding.field_site = "http://127.0.0.1:1".into();
    app.onboarding.field_email = "me@example.com".into();
    app.onboarding.field_token = "not-a-real-token".into();

    app.submit_credentials();
    assert!(app.loading);
    assert!(app.onboarding_pending);
    assert_eq!(app.onboarding.setup_msg, "Verifying…");

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert!(!app.onboarding_pending);
    assert!(
        app.onboarding.setup_msg.contains("did not accept"),
        "a rejected/unreachable site should report the credentials as not accepted, got: {}",
        app.onboarding.setup_msg
    );
    assert!(
        !matches!(app.source, crate::domain::Source::Live { .. }),
        "an unreachable/rejected site must not switch to Source::Live"
    );
}

#[cfg(feature = "live")]
#[tokio::test]
async fn submit_credentials_refuses_to_resubmit_while_verifying() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = onboarding_app();
    app.onboarding.field_site = "http://127.0.0.1:1".into();
    app.onboarding.field_email = "me@example.com".into();
    app.onboarding.field_token = "not-a-real-token".into();

    app.submit_credentials();
    assert!(app.onboarding_pending);
    let generation = app.onboarding_generation;

    // A second submission (e.g. a double Enter press) while the first is
    // still resolving must be refused rather than dispatching a duplicate
    // verification fetch under a bumped generation, which would silently
    // drop the first attempt's result.
    app.submit_credentials();
    assert_eq!(
        app.onboarding_generation, generation,
        "must not dispatch a second verification while one is already in flight"
    );
    assert_eq!(app.onboarding.setup_msg, "Already verifying…");

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.onboarding_pending);
}

/// Regression test: a resubmission while a verification is already in
/// flight must be refused *before* any persistence happens, so it can't
/// silently overwrite the on-disk token/settings or the `JIRA_*` env vars
/// out from under the first, still-pending attempt.
#[cfg(feature = "live")]
#[tokio::test]
async fn submit_credentials_does_not_repersist_while_verifying() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = onboarding_app();
    app.onboarding.field_site = "http://127.0.0.1:1".into();
    app.onboarding.field_email = "first@example.com".into();
    app.onboarding.field_token = "first-token".into();

    app.submit_credentials();
    assert!(app.onboarding_pending);

    // Edit the fields to different credentials and resubmit while the
    // first attempt is still resolving.
    app.onboarding.field_email = "second@example.com".into();
    app.onboarding.field_token = "second-token".into();
    app.submit_credentials();

    assert_eq!(
        std::env::var("JIRA_EMAIL").as_deref(),
        Ok("first@example.com"),
        "a refused resubmission must not overwrite the env vars set by the in-flight attempt"
    );
    assert_eq!(
        std::env::var("JIRA_API_TOKEN").as_deref(),
        Ok("first-token")
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.onboarding_pending);
}

#[test]
fn go_back_and_forward_step_through_issues_followed_via_links() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let first = app.detail.as_ref().unwrap().key.clone();

    // Opening fresh from the list shouldn't create any history yet.
    assert!(!app.can_go_back());
    assert!(!app.can_go_forward());

    // Simulate following an in-body link to a second issue, then a third.
    app.open_by_key("DS-9001");
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    assert!(app.can_go_back());
    assert!(!app.can_go_forward());

    app.open_by_key("DS-9002");
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9002");
    assert!(app.can_go_back());

    // Back once: DS-9002 -> DS-9001.
    app.go_back();
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    assert!(app.can_go_back());
    assert!(app.can_go_forward());

    // Back again: DS-9001 -> first.
    app.go_back();
    assert_eq!(app.detail.as_ref().unwrap().key, first);
    assert!(!app.can_go_back());
    assert!(app.can_go_forward());

    // Forward twice retraces DS-9001 then DS-9002.
    app.go_forward();
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    app.go_forward();
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9002");
    assert!(!app.can_go_forward());
}

#[test]
fn following_a_new_link_clears_the_forward_stack() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_by_key("DS-9001");
    app.open_by_key("DS-9002");
    app.go_back();
    assert!(app.can_go_forward());

    // Branching off to a different issue instead of redoing should drop
    // the now-stale forward history (DS-9002), same as a browser.
    app.open_by_key("DS-9003");
    assert!(!app.can_go_forward());
    assert!(app.can_go_back());
}

#[test]
fn opening_fresh_from_the_list_starts_a_new_history() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_by_key("DS-9001");
    assert!(app.can_go_back());

    // Leaving Detail and opening a different issue fresh from the list is a
    // new navigation session, not a continuation of the old one.
    app.screen = Screen::Home;
    app.open_detail();
    assert!(!app.can_go_back());
    assert!(!app.can_go_forward());
}

#[test]
fn go_back_and_go_forward_are_no_ops_with_empty_history() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let key = app.detail.as_ref().unwrap().key.clone();

    app.go_back();
    assert_eq!(app.detail.as_ref().unwrap().key, key);
    app.go_forward();
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}

// Coverage gap noticed while splitting app/tests.rs: every existing
// `AppEvent::CredentialsVerified` test (`submit_credentials_against_an_
// unreachable_site_dispatches_and_reports_rejection` and friends) exercises
// the rejected/non-live branch — the fixtures deliberately have no real
// credentials, so the `Source::Live` success branch (mark_onboarded, the
// field-mapping offer) had no coverage anywhere. Constructs the event
// directly since `apply_event` is a plain synchronous fn.
#[cfg(feature = "live")]
#[tokio::test]
async fn apply_event_credentials_verified_live_success_marks_onboarded_and_loads_issues() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = onboarding_app();
    let generation = app.onboarding_generation;
    let issues = crate::domain::demo_issues();

    app.apply_event(AppEvent::CredentialsVerified {
        generation,
        issues: issues.clone(),
        source: crate::domain::Source::Live {
            site: "demo.atlassian.net".into(),
            user: "me".into(),
        },
        status: "live · demo.atlassian.net · me".into(),
    });

    assert!(!app.onboarding_pending);
    assert!(matches!(app.source, crate::domain::Source::Live { .. }));
    assert_eq!(app.all_issues.len(), issues.len());
    // `open_field_mapping_for_onboarding` sees the now-Live source and
    // dispatches its own background lookup rather than falling back to Home
    // synchronously — which re-flips `loading` back on and sets
    // `field_mapping_pending`. That nested dispatch's own resolution is
    // already covered by
    // `onboarding_field_mapping_falls_back_to_home_with_the_connected_status_on_failure`.
    assert!(
        app.field_mapping_pending,
        "a genuine live source should trigger the onboarding field-mapping offer"
    );
    assert!(
        app.loading,
        "the follow-up field-mapping dispatch re-flips loading back on"
    );
}
