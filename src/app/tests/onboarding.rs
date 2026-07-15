//! Onboarding credential-form and verification-fetch tests.

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
