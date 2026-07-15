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
