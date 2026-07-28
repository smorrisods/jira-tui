//! Search / go-to-issue tests.

use super::super::*;
use super::support::*;

#[test]
fn search_finds_matches_by_key_and_summary() {
    let mut app = demo_app();
    app.open_search();
    for c in "accordion".chars() {
        app.search_input_char(c);
    }
    assert!(app.search.rows.iter().any(|r| matches!(
        r,
        SearchRow::Match(idx) if app.all_issues[*idx].summary.to_lowercase().contains("accordion")
    )));
}

#[test]
fn search_key_candidate_detects_issue_keys_only() {
    let mut app = demo_app();
    app.search.query = "DS-2603".to_string();
    assert_eq!(app.search_key_candidate(), Some("DS-2603".to_string()));
    app.search.query = "ds-2603".to_string();
    assert_eq!(app.search_key_candidate(), Some("DS-2603".to_string()));
    app.search.query = "accordion".to_string();
    assert_eq!(app.search_key_candidate(), None);
    app.search.query = "DS-".to_string();
    assert_eq!(app.search_key_candidate(), None);
}

#[test]
fn confirm_search_goto_opens_issue_directly_even_if_unfiltered() {
    let mut app = demo_app();
    app.open_search();
    for c in "DS-2603".chars() {
        app.search_input_char(c);
    }
    // The Goto row should be first.
    assert!(matches!(app.search.rows.first(), Some(SearchRow::Goto(k)) if k == "DS-2603"));
    app.search.selected = 0;
    app.confirm_search();
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-2603");
}

#[test]
fn confirm_search_match_opens_that_issue() {
    let mut app = demo_app();
    let target_key = app.all_issues[1].key.clone();
    app.open_search();
    for c in target_key.chars() {
        app.search_input_char(c);
    }
    // Find the Match row for our target and select it.
    let pos = app
        .search
        .rows
        .iter()
        .position(|r| matches!(r, SearchRow::Match(idx) if app.all_issues[*idx].key == target_key))
        .unwrap();
    app.search.selected = pos;
    app.confirm_search();
    assert_eq!(app.detail.as_ref().unwrap().key, target_key);
}

#[test]
fn close_search_returns_to_prior_screen() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.open_search();
    assert_eq!(app.screen, Screen::Search);
    app.close_search();
    assert_eq!(app.screen, Screen::List);
}

#[test]
fn demo_sessions_never_schedule_a_live_search() {
    let mut app = demo_app();
    app.open_search();
    for c in "widget".chars() {
        app.search_input_char(c);
    }
    assert!(!app.search.live_loading);
    // Advancing the clock alone must not turn on a live search for a demo
    // session — `ensure_search_dispatched` would panic if it tried to
    // `tokio::spawn` outside a runtime, so this also guards that a plain
    // (non-`#[tokio::test]`) test stays safe to call it from.
    app.tick += 100;
    app.ensure_search_dispatched();
    assert!(!app.search.live_loading);
}

#[tokio::test]
async fn a_query_under_the_minimum_length_does_not_schedule_a_live_search() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_search();
    app.search_input_char('a');
    app.tick += 100;
    app.ensure_search_dispatched();
    assert!(
        !app.search.live_loading,
        "a single-character query is too short to fire a live search"
    );
}

#[tokio::test]
async fn a_live_search_dispatches_after_its_debounce_window_and_merges_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.open_search();
    for c in "widget".chars() {
        app.search_input_char(c);
    }
    // Not due yet — still within the debounce window.
    app.ensure_search_dispatched();
    assert!(
        !app.search.live_loading,
        "a live search must not fire before its debounce window elapses"
    );

    app.tick += 100;
    app.ensure_search_dispatched();
    assert!(
        app.search.live_loading,
        "the debounce window has elapsed, so the live search should now be in flight"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.search.live_loading);
    assert_eq!(app.search.live_query.as_deref(), Some("widget"));
}

#[test]
fn stale_live_search_results_are_dropped_without_disturbing_a_newer_in_flight_search() {
    let mut app = demo_app();
    app.open_search();
    app.search.query = "widget".into();
    app.search_generation = 2;
    app.search.live_loading = true;

    // A result tagged with an older generation than the one currently
    // dispatched must be ignored — and must not clear `live_loading`, since
    // the newer (generation 2) search is presumably still in flight.
    app.apply_text_searched(1, "widget".into(), vec![app.all_issues[0].clone()], None);
    assert!(app.search.live_query.is_none());
    assert!(
        app.search.live_loading,
        "a stale result must not clear the loading flag for a still in-flight search"
    );
}

#[test]
fn a_failed_live_search_surfaces_the_error_instead_of_silently_doing_nothing() {
    let mut app = demo_app();
    app.open_search();
    app.search.query = "dropdown".into();
    app.search_generation = 1;

    app.apply_text_searched(1, "dropdown".into(), Vec::new(), Some("boom".into()));
    assert!(
        app.status.contains("boom"),
        "a failed live search must show up in the status line: {}",
        app.status
    );
}

#[test]
fn an_empty_live_search_result_says_so_instead_of_looking_like_nothing_happened() {
    let mut app = demo_app();
    app.open_search();
    app.search.query = "dropdown".into();
    app.search_generation = 1;

    app.apply_text_searched(1, "dropdown".into(), Vec::new(), None);
    assert!(
        app.status.contains("dropdown"),
        "a genuinely empty live search result should say so, not look like it did nothing: {}",
        app.status
    );
}

#[test]
fn live_search_results_are_deduped_against_matches_already_shown_locally() {
    let mut app = demo_app();
    app.open_search();
    let local = app.all_issues[0].clone();
    app.search.query = local.summary.clone();
    app.search_generation = 1;

    // The live fallback re-found the same issue plus one genuinely new one.
    let mut new_issue = local.clone();
    new_issue.key = "DS-9999".into();
    let issues = vec![local.clone(), new_issue.clone()];
    let query = app.search.query.trim().to_lowercase();
    app.apply_text_searched(1, query, issues, None);

    let live_keys: Vec<&str> = app
        .search
        .rows
        .iter()
        .filter_map(|r| match r {
            SearchRow::Live(idx) => Some(app.search.live_results[*idx].key.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        live_keys,
        vec!["DS-9999"],
        "a live result already shown as a local Match must not also appear as a Live row"
    );
}

#[tokio::test]
async fn live_results_are_hidden_once_the_query_has_moved_on() {
    let _guard = crate::test_support::lock_env_async().await;
    // A genuine `Source::Live` session, so the follow-up keystroke below
    // schedules another live search instead of clearing `live_results`
    // outright (see `App::schedule_live_search`) — the point of this test
    // is the `live_query` mismatch check in `rebuild_search_rows`, not the
    // demo/cache short-circuit.
    let mut app = live_app();
    app.open_search();
    let issue = app.all_issues[0].clone();
    app.search.query = "widget".into();
    app.search_generation = 1;
    app.apply_text_searched(1, "widget".into(), vec![issue], None);
    assert!(app
        .search
        .rows
        .iter()
        .any(|r| matches!(r, SearchRow::Live(_))));

    // The user kept typing past what those results answer for — they must
    // no longer show, even though `live_results` itself hasn't changed yet.
    app.search_input_char('!');
    assert!(
        !app.search
            .rows
            .iter()
            .any(|r| matches!(r, SearchRow::Live(_))),
        "stale live results must disappear once the query no longer matches live_query"
    );
}
