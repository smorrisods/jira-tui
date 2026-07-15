//! Detail in-body-link navigation history tests.

use super::super::*;
use super::support::*;

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
