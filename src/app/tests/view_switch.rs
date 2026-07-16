//! View-switching (My Work / All Project Issues / teammate) and the
//! generic refresh/superseded-fetch dispatch tests.

use super::super::*;
use super::support::*;

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
fn cycle_view_steps_forward_through_view_options_in_order() {
    let mut app = demo_app();
    let options = app.view_options();
    assert_eq!(app.current_view, options[0], "starts on My Work");
    app.cycle_view(1);
    assert_eq!(app.current_view, options[1]);
}

#[test]
fn cycle_view_wraps_at_either_end() {
    let mut app = demo_app();
    let options = app.view_options();
    let last = options.last().unwrap().clone();

    // Backward from the first view wraps to the last.
    app.cycle_view(-1);
    assert_eq!(app.current_view, last);

    // Forward from the last view wraps back to the first.
    app.cycle_view(1);
    assert_eq!(app.current_view, options[0]);
}

#[test]
fn cycle_view_loads_data_exactly_like_the_picker_does() {
    // Same data path as confirm_view_switch — cycling to a teammate view
    // should filter all_issues by that teammate's assignee, same as
    // `confirm_view_switch_to_teammate_filters_by_assignee` above.
    let mut app = demo_app();
    let options = app.view_options();
    let priya_idx = options
        .iter()
        .position(|v| *v == ViewKind::Teammate("priya.nair".into()))
        .expect("priya.nair should be a demo teammate");
    for _ in 0..priya_idx {
        app.cycle_view(1);
    }
    assert_eq!(app.current_view, ViewKind::Teammate("priya.nair".into()));
    assert!(!app.all_issues.is_empty());
    assert!(app
        .all_issues
        .iter()
        .all(|i| i.assignee.as_deref() == Some("priya.nair")));
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
