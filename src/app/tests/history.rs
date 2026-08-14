//! `App`-level navigation-history integration tests: `open_by_key`/
//! `follow_link` wired through to `App::go_back`/`go_forward`/`nav_jump`
//! and `App::show_issue`. Pure `NavHistory` tree-logic tests (eviction,
//! ancestor-cycle guards, re-parenting) live alongside `NavHistory` itself
//! in `app::history`.

use super::super::*;
use super::support::*;

#[test]
fn open_by_key_is_always_a_fresh_open_even_from_within_detail() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let first = app.detail.as_ref().unwrap().key.clone();
    assert!(!app.can_go_back());

    // `open_by_key` (list/search/board/release — never an in-body link) no
    // longer infers a link-follow just because `screen == Detail`; it's
    // always a fresh open, regardless of which screen it's called from.
    app.open_by_key("DS-9001");
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    assert!(
        !app.can_go_back(),
        "a fresh open from Detail must not create a parent edge back to {first}"
    );
}

#[test]
fn follow_link_steps_back_and_forward_through_issues_followed_via_links() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let first = app.detail.as_ref().unwrap().key.clone();
    assert!(!app.can_go_back());
    assert!(!app.can_go_forward());

    // Simulate following an in-body link to a second issue, then a third.
    app.follow_link("DS-9001");
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    assert!(app.can_go_back());
    assert!(!app.can_go_forward());

    app.follow_link("DS-9002");
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

/// The core branching scenario worked through with the user: open an
/// issue, follow a link, go back, follow a *different* link. `←` from the
/// new branch must return to the true origin, not the abandoned branch —
/// and the abandoned branch must still be visible/jumpable, not deleted.
#[test]
fn back_from_a_new_branch_returns_to_true_origin_and_keeps_the_abandoned_branch() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let a = app.detail.as_ref().unwrap().key.clone();

    app.follow_link("DS-9001"); // A -> B
    app.go_back(); // back to A
    assert_eq!(app.detail.as_ref().unwrap().key, a);

    app.follow_link("DS-9002"); // A -> C (a new branch)
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9002");

    app.go_back();
    assert_eq!(
        app.detail.as_ref().unwrap().key,
        a,
        "back from the new branch must return to the true origin"
    );
    app.go_forward();
    assert_eq!(
        app.detail.as_ref().unwrap().key,
        "DS-9002",
        "forward should resume the most recently taken branch"
    );

    let recent: Vec<String> = app.nav.entries().into_iter().map(|e| e.key).collect();
    assert!(
        recent.contains(&"DS-9001".to_string()),
        "the abandoned branch must still be present: {recent:?}"
    );
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

#[test]
fn nav_jump_repositions_without_losing_other_branches() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let a = app.detail.as_ref().unwrap().key.clone();
    app.follow_link("DS-9001"); // A -> B
    app.go_back(); // back to A
    app.follow_link("DS-9002"); // A -> C

    app.nav_jump("DS-9001");
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    app.go_back();
    assert_eq!(
        app.detail.as_ref().unwrap().key,
        a,
        "the jumped-to entry's own real parent must be unaffected by the jump"
    );
}

/// The gap this whole redesign fixes: a link clicked from the quick-view
/// panel (not the Detail screen) must still parent the target under the
/// quick-viewed issue, exactly as if it had been clicked from that issue's
/// full Detail view — `open_highlighted_link`'s only call site is
/// `follow_link`, resolved via `active_comment_detail`, not `self.screen`.
#[test]
fn following_a_link_from_quick_view_parents_under_the_quick_viewed_issue() {
    let mut app = demo_app();
    app.selected = 0;
    let quick_viewed_key = app.selected_issue().unwrap().key.clone();

    // Populate `detail_cache` for the quick-viewed issue the way a real
    // quick-view load would, without actually opening full Detail.
    app.open_by_key(&quick_viewed_key);
    app.screen = Screen::Home;
    app.selected = 0;
    assert_eq!(app.selected_issue().unwrap().key, quick_viewed_key);

    app.follow_link("DS-9001");
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    assert!(
        app.can_go_back(),
        "the link's target should be parented under the quick-viewed issue"
    );
    app.go_back();
    assert_eq!(app.detail.as_ref().unwrap().key, quick_viewed_key);
}

#[test]
fn back_count_reflects_true_ancestor_depth() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    assert_eq!(app.back_count(), 0);
    app.follow_link("DS-9001");
    assert_eq!(app.back_count(), 1);
    app.follow_link("DS-9002");
    assert_eq!(app.back_count(), 2);
}

/// Shift+←/→ walk the recent strip's flat *display* order
/// (`app.nav.entries()` — the same order `ui::nav_strip` renders), not the
/// tree — a deliberately different axis from `,`/`.`/plain `←`/`→`. Derives
/// the expected order from `entries()` itself rather than hardcoding an
/// assumption about it, so this test documents (and would catch a
/// regression in) whichever order the strip actually renders.
#[test]
fn step_display_walks_the_flat_strip_order_not_the_tree() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let a = app.detail.as_ref().unwrap().key.clone();
    app.follow_link("DS-9001"); // A -> B
    app.go_back(); // back to A
    app.follow_link("DS-9002"); // A -> C (new branch); current = C

    let expected_order: Vec<String> = app.nav.entries().into_iter().map(|e| e.key).collect();
    assert_eq!(
        expected_order.len(),
        3,
        "all three issues should be in the display order: {expected_order:?}"
    );
    assert_eq!(
        expected_order[0], "DS-9002",
        "the current entry should be first in display order"
    );

    // Walk forward through the whole display order from the current
    // position and confirm it matches `entries()` exactly.
    let mut walked = vec![app.detail.as_ref().unwrap().key.clone()];
    while app.can_step_display_forward() {
        app.step_display_forward();
        walked.push(app.detail.as_ref().unwrap().key.clone());
    }
    assert_eq!(walked, expected_order);
    assert!(!app.can_step_display_forward());

    // And back to the start.
    while app.can_step_display_back() {
        app.step_display_back();
    }
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9002");
    assert!(!app.can_step_display_back());

    // A display-step is a jump, not a tree edge: after stepping onto A via
    // display order, `,`/`←` (the true tree walk) must be completely
    // unaffected by having taken this detour.
    app.step_display_forward(); // C -> A (display order)
    assert_eq!(app.detail.as_ref().unwrap().key, a);
    assert!(
        !app.can_go_back(),
        "A is still a root in the tree — a display jump must not have re-parented it"
    );
}
