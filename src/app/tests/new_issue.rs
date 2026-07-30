//! The new-issue compose form (`Screen::NewIssue`, `a` on Home/List) and the
//! demo/cache persistence bookkeeping it depends on.

use super::super::*;
use super::support::*;

#[test]
fn confirm_new_issue_form_rejects_an_empty_project() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project.clear();
    app.new_issue.summary = "Something to do".into();
    app.confirm_new_issue_form();
    assert_eq!(app.screen, Screen::NewIssue, "must stay on the form");
    assert!(app.status.contains("project"));
}

#[test]
fn confirm_new_issue_form_rejects_an_empty_summary() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.summary.clear();
    app.confirm_new_issue_form();
    assert_eq!(app.screen, Screen::NewIssue, "must stay on the form");
    assert!(app.status.contains("summary"));
}

#[test]
fn confirm_new_issue_form_rejects_an_empty_issue_type_catalog() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.project_for_types = "DS".into(); // already "in sync" — no resync side effect
    app.new_issue.summary = "Something to do".into();
    app.new_issue.available_types.clear();
    app.confirm_new_issue_form();
    assert_eq!(app.screen, Screen::NewIssue, "must stay on the form");
    assert!(app.status.contains("no issue types"));
}

#[test]
fn confirm_new_issue_form_advances_to_the_description_editor() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.summary = "Something to do".into();
    app.confirm_new_issue_form();
    assert_eq!(app.screen, Screen::Edit);
    assert_eq!(app.edit_target, EditTarget::NewIssue);
    assert_eq!(app.edit_return_screen, Screen::NewIssue);
    assert_eq!(app.edit_key, None, "there's no issue key yet");
}

#[test]
fn esc_from_the_description_editor_returns_to_the_form_with_state_intact() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.project_for_types = "DS".into(); // already "in sync" — no resync side effect
    app.new_issue.summary = "Something to do".into();
    app.new_issue.issue_type_index = 1;
    app.confirm_new_issue_form();
    assert_eq!(app.screen, Screen::Edit);

    app.cancel_edit();
    assert_eq!(
        app.screen,
        Screen::NewIssue,
        "backing out of the description step returns to the form, not Home"
    );
    assert_eq!(app.new_issue.project, "DS");
    assert_eq!(app.new_issue.summary, "Something to do");
    assert_eq!(app.new_issue.issue_type_index, 1);
}

#[test]
fn empty_description_preview_steps_back_to_the_editor_not_a_full_cancel() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.summary = "Something to do".into();
    app.confirm_new_issue_form();

    // Never typed a description — commit the empty buffer.
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);
    assert!(
        app.pending_edit.is_none(),
        "an empty new-issue description should compile to None, not an empty ADF doc"
    );

    app.back_out_of_preview();
    assert_eq!(
        app.screen,
        Screen::Edit,
        "a new issue's description is optional — backing out of an empty preview must not \
         discard the in-progress issue"
    );
    assert_eq!(app.new_issue.summary, "Something to do");
}

#[test]
fn cancel_new_issue_discards_state_and_returns_home() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.summary = "Something to do".into();

    app.cancel_new_issue();
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(app.new_issue.project, "");
    assert_eq!(app.new_issue.summary, "");
}

#[test]
fn demo_create_end_to_end_lands_in_all_issues_and_opens_detail() {
    let mut app = demo_app();
    let before = app.all_issues.len();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.project_for_types = "DS".into(); // already "in sync" — no resync side effect
    app.new_issue.summary = "Fix the flaky login test".into();
    app.new_issue.issue_type_index = 1; // "Bug"
    app.confirm_new_issue_form();
    app.commit_tui_edit(); // no description typed
    app.apply_edit();

    assert_eq!(app.all_issues.len(), before + 1);
    assert_eq!(app.screen, Screen::Detail);
    let detail = app.detail.as_ref().unwrap();
    assert_eq!(detail.summary, "Fix the flaky login test");
    assert_eq!(detail.issue_type, "Bug");
    assert!(detail.key.starts_with("DS-"));
    assert!(
        app.all_issues.iter().any(|i| i.key == detail.key),
        "the new issue must be in all_issues, not just detail_cache"
    );
    assert!(
        app.jax_party_until > app.tick,
        "a successful create should trigger the reactive party moment"
    );
}

#[test]
fn reopening_a_locally_created_issue_does_not_show_the_not_found_placeholder() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.summary = "Fix the flaky login test".into();
    app.confirm_new_issue_form();
    app.commit_tui_edit();
    app.apply_edit();
    let key = app.detail.as_ref().unwrap().key.clone();

    app.screen = Screen::Home;
    app.detail = None;
    app.open_by_key(&key);

    let detail = app.detail.as_ref().unwrap();
    assert_eq!(detail.summary, "Fix the flaky login test");
    assert_ne!(
        detail.summary, "Not found in demo data",
        "must not fall through to demo_detail's generic placeholder"
    );
}

#[test]
fn a_locally_created_issue_survives_a_refresh() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.summary = "Fix the flaky login test".into();
    app.confirm_new_issue_form();
    app.commit_tui_edit();
    app.apply_edit();
    let key = app.detail.as_ref().unwrap().key.clone();

    // A plain demo session's refresh() resolves synchronously — no
    // tokio runtime needed.
    app.refresh();

    assert!(
        app.all_issues.iter().any(|i| i.key == key),
        "a manual refresh right after creating an issue must not make it vanish"
    );
}

#[tokio::test]
async fn open_new_issue_prefills_empty_project_and_the_demo_issue_type_catalog_offline() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app();
    app.open_new_issue();

    assert_eq!(app.screen, Screen::NewIssue);
    assert_eq!(
        app.new_issue.project, "",
        "no config present in the isolated test env, so nothing to prefill"
    );
    assert_eq!(
        app.new_issue.available_types,
        crate::domain::demo_issue_types()
    );
    assert!(!app.new_issue.types_loading);
    assert_eq!(app.new_issue.issue_type_index, 0);
}

#[tokio::test]
async fn changing_the_project_field_and_leaving_it_dispatches_a_fresh_issue_type_fetch() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_new_issue();
    let first_fetch = next_event(&mut app).await;
    app.apply_event(first_fetch);
    assert!(!app.new_issue.types_loading);

    app.new_issue.focus = NewIssueField::Project;
    app.new_issue.project = "OTHER".into();
    app.new_issue_next_field();
    assert!(
        app.new_issue.types_loading,
        "leaving the Project field after editing it should trigger a refetch"
    );
    assert_eq!(app.new_issue.focus, NewIssueField::IssueType);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.new_issue.types_loading);
    assert_eq!(app.new_issue.project_for_types, "OTHER");
}

#[tokio::test]
async fn a_stale_generation_issue_type_fetch_is_dropped() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_new_issue();
    let generation = app.new_issue_types_generation;

    // A newer fetch (e.g. from tabbing off the Project field again) bumps
    // the generation before the original one resolves.
    app.new_issue_types_generation += 1;
    app.apply_event(AppEvent::ProjectIssueTypesLoaded {
        generation,
        project: "DS".into(),
        result: Ok(crate::domain::demo_issue_types()),
    });
    assert!(
        app.new_issue.types_loading,
        "a stale-generation result must be dropped, leaving types_loading as the newer \
         dispatch set it"
    );
    assert!(app.new_issue.available_types.is_empty());
}

#[tokio::test]
async fn an_issue_type_fetch_resolving_after_the_form_closed_is_dropped() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_new_issue();
    let generation = app.new_issue_types_generation;

    // The user backed out of the form entirely before the fetch resolved.
    app.cancel_new_issue();
    app.apply_event(AppEvent::ProjectIssueTypesLoaded {
        generation,
        project: "DS".into(),
        result: Ok(crate::domain::demo_issue_types()),
    });
    assert_eq!(
        app.new_issue,
        NewIssueState::default(),
        "a result landing after the form closed must not repopulate its state"
    );
}

#[tokio::test]
async fn subtask_issue_types_are_filtered_out_of_the_picker() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_new_issue();
    let generation = app.new_issue_types_generation;

    app.apply_event(AppEvent::ProjectIssueTypesLoaded {
        generation,
        project: "".into(),
        result: Ok(vec![
            crate::domain::IssueType {
                id: "1".into(),
                name: "Task".into(),
                subtask: false,
            },
            crate::domain::IssueType {
                id: "2".into(),
                name: "Sub-task".into(),
                subtask: true,
            },
        ]),
    });
    assert!(
        app.new_issue
            .available_types
            .iter()
            .all(|t| t.name != "Sub-task"),
        "a subtask type can never succeed here (no parent-key field is collected), so it \
         must not be offered"
    );
}

#[tokio::test]
async fn an_issue_type_fetch_failure_is_surfaced_distinctly_from_a_genuinely_empty_catalog() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_new_issue();
    let generation = app.new_issue_types_generation;

    app.apply_event(AppEvent::ProjectIssueTypesLoaded {
        generation,
        project: "DS".into(),
        result: Err("Jira request failed: 401 Unauthorized".into()),
    });
    assert!(
        !app.new_issue.types_loading,
        "a failed fetch must still clear the loading flag"
    );
    assert!(
        app.status.contains("fetch failed"),
        "a genuine failure must read differently from \"no issue types found\", which \
         previously silently absorbed every error (network, auth, decode) into the same \
         confusing message — got: {:?}",
        app.status
    );
}

#[tokio::test]
async fn live_create_dispatches_and_applies_via_the_no_credentials_fallback() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_new_issue();
    let fetch = next_event(&mut app).await;
    app.apply_event(fetch);
    // No real credentials in this isolated test env, so the issue-type
    // fetch itself resolved to an empty catalog (see `project_issue_types_blocking`'s
    // no-config fallback) — inject one so the form's own validation doesn't
    // block advancing, which is orthogonal to what this test is exercising
    // (the create dispatch/apply plumbing).
    app.new_issue.available_types = crate::domain::demo_issue_types();
    app.new_issue.project = "DS".into();
    app.new_issue.project_for_types = "DS".into(); // already "in sync" — no resync side effect
    app.new_issue.summary = "Fix the flaky login test".into();
    app.confirm_new_issue_form();
    assert_eq!(app.screen, Screen::Edit);
    app.commit_tui_edit();
    app.apply_edit();
    assert!(app.loading);
    assert!(app.edit_pending);
    assert_eq!(
        app.screen,
        Screen::Preview,
        "must stay put until the create resolves"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    // `apply_issue_created`'s success path clears `edit_pending` and then
    // calls `open_by_key`, which — for a genuine `Source::Live` session —
    // dispatches a *second*, independent detail re-fetch (`app.loading`
    // flips back to `true` for that, unrelated to the create itself; a real
    // Live session always has real credentials by construction, so that
    // second fetch would succeed for real rather than hitting the
    // no-credentials fallback this contrived test harness triggers here).
    // What this test verifies is the create dispatch/apply plumbing itself:
    assert!(!app.edit_pending);
    assert_eq!(
        app.new_issue,
        NewIssueState::default(),
        "the compose form's state must be cleared on success"
    );
    let key = app
        .all_issues
        .iter()
        .find(|i| i.summary == "Fix the flaky login test")
        .map(|i| i.key.clone())
        .expect("the created issue must be in all_issues");
    assert!(
        key.starts_with("DS-"),
        "no live credentials in the isolated test env, so this must be the local-key fallback"
    );
}

#[tokio::test]
async fn apply_edit_refuses_to_dispatch_a_second_create_while_the_first_is_in_flight() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_new_issue();
    let fetch = next_event(&mut app).await;
    app.apply_event(fetch);
    app.new_issue.available_types = crate::domain::demo_issue_types();
    app.new_issue.project = "DS".into();
    app.new_issue.project_for_types = "DS".into();
    app.new_issue.summary = "Fix the flaky login test".into();
    app.confirm_new_issue_form();
    app.commit_tui_edit();
    app.apply_edit();
    assert!(app.edit_pending);
    let generation = app.edit_generation;

    // Reachable via ordinary navigation, not just key-repeat: Esc on
    // Preview (`back_out_of_preview`) returns to `Screen::Edit` without
    // discarding anything for a new issue's (optional) description, so the
    // user can re-confirm while the first submission is still resolving.
    app.back_out_of_preview();
    assert_eq!(app.screen, Screen::Edit);
    app.commit_tui_edit();
    app.apply_edit();

    assert_eq!(
        app.edit_generation, generation,
        "a second confirm while a create is still in flight must not dispatch another one"
    );
    assert!(app.status.contains("in progress"));

    // Draining the original (only) dispatch's result must still work.
    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.edit_pending);
}

#[tokio::test]
async fn a_locally_created_issue_reopens_synchronously_after_the_session_becomes_live() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app(); // Source::Cache
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.summary = "Fix the flaky login test".into();
    app.confirm_new_issue_form();
    app.commit_tui_edit();
    app.apply_edit();
    let key = app.detail.as_ref().unwrap().key.clone();

    // The session reconnects to a genuine live source (e.g. the network
    // recovered) without the locally-created issue going anywhere.
    app.source = crate::domain::Source::Live {
        site: "demo.atlassian.net".into(),
        user: "me".into(),
    };
    app.screen = Screen::Home;
    app.detail = None;
    app.open_by_key(&key);

    // Resolved synchronously — a locally-fabricated key can never exist
    // server-side, so no live fetch should have been dispatched at all.
    assert_eq!(app.screen, Screen::Detail);
    let detail = app.detail.as_ref().unwrap();
    assert_eq!(detail.summary, "Fix the flaky login test");
}

#[tokio::test]
async fn a_locally_created_issue_does_not_survive_a_refresh_that_resolves_to_a_genuine_live_source()
{
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app(); // Source::Cache
    app.open_new_issue();
    app.new_issue.project = "DS".into();
    app.new_issue.summary = "Fix the flaky login test".into();
    app.confirm_new_issue_form();
    app.commit_tui_edit();
    app.apply_edit();
    let key = app.detail.as_ref().unwrap().key.clone();
    assert!(app.all_issues.iter().any(|i| i.key == key));

    // A refresh resolves to a genuine live source (e.g. the network
    // recovered) — the fabricated local entry must not be folded into what
    // is now a real live issue list.
    app.record_synced(
        Vec::new(),
        crate::domain::Source::Live {
            site: "demo.atlassian.net".into(),
            user: "me".into(),
        },
    );
    assert!(
        !app.all_issues.iter().any(|i| i.key == key),
        "a locally-created issue must not keep reappearing once the session is genuinely live"
    );
}

#[tokio::test]
async fn a_stale_generation_issue_created_event_is_dropped() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_new_issue();
    let fetch = next_event(&mut app).await;
    app.apply_event(fetch);
    app.new_issue.available_types = crate::domain::demo_issue_types();
    app.new_issue.project = "DS".into();
    app.new_issue.project_for_types = "DS".into(); // already "in sync" — no resync side effect
    app.new_issue.summary = "First issue".into();
    app.confirm_new_issue_form();
    app.commit_tui_edit();
    app.apply_edit();
    assert!(app.edit_pending);

    // A second edit session starts (e.g. the user backs out and starts a
    // comment elsewhere) and bumps `edit_generation` before the first
    // create resolves.
    app.edit_generation += 1;

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(
        app.edit_pending,
        "a stale IssueCreated event must be dropped, not clear edit_pending for a newer session"
    );
}

#[test]
fn new_issue_cycle_issue_type_wraps_around_in_both_directions() {
    let mut app = demo_app();
    app.open_new_issue();
    let len = app.new_issue.available_types.len();
    assert!(
        len > 1,
        "the demo issue-type catalog must have more than one entry"
    );

    assert_eq!(app.new_issue.issue_type_index, 0);
    app.new_issue_cycle_issue_type(-1);
    assert_eq!(
        app.new_issue.issue_type_index,
        len - 1,
        "must wrap backwards from 0"
    );
    app.new_issue_cycle_issue_type(1);
    assert_eq!(
        app.new_issue.issue_type_index, 0,
        "must wrap forwards past the end"
    );
}

#[test]
fn new_issue_input_and_backspace_only_affect_the_focused_text_field() {
    let mut app = demo_app();
    app.open_new_issue();
    app.new_issue.project.clear();
    app.new_issue.summary.clear();
    app.new_issue.focus = NewIssueField::Project;

    app.new_issue_input_char('D');
    app.new_issue_input_char('S');
    assert_eq!(app.new_issue.project, "DS");
    assert_eq!(app.new_issue.summary, "");

    app.new_issue.focus = NewIssueField::IssueType;
    app.new_issue_input_char('x');
    assert_eq!(
        app.new_issue.project, "DS",
        "typing while IssueType has focus must not leak into another field"
    );

    app.new_issue.focus = NewIssueField::Summary;
    app.new_issue_input_char('!');
    assert_eq!(app.new_issue.summary, "!");
    app.new_issue_backspace();
    assert_eq!(app.new_issue.summary, "");
}
