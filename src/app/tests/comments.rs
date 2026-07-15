//! Comment composing/paging tests.

use super::super::*;
use super::support::*;

#[test]
fn begin_comment_from_detail_composes_and_apply_appends_it() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let before = app.detail.as_ref().unwrap().comments.len();

    app.begin_comment();
    assert_eq!(app.screen, Screen::Edit);
    assert_eq!(app.edit_target, EditTarget::Comment);
    assert!(app.editor.lines.iter().all(|l| l.is_empty()));

    for c in "Looks good to me.".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);
    assert!(app.pending_edit.is_some());

    app.apply_edit();
    assert_eq!(app.screen, Screen::Detail);
    let comments = &app.detail.as_ref().unwrap().comments;
    assert_eq!(comments.len(), before + 1);
    let newest = comments.last().unwrap();
    assert_eq!(
        crate::adf::to_markdown(&newest.body).trim(),
        "Looks good to me."
    );
}

#[test]
fn begin_comment_from_quick_view_returns_to_list_and_updates_cache() {
    let mut app = demo_app();
    app.selected = 0;
    app.quick_view = true;
    app.ensure_quick_view_loaded();
    assert_eq!(app.screen, Screen::Home);

    app.begin_comment();
    assert_eq!(app.screen, Screen::Edit);
    assert_eq!(app.edit_target, EditTarget::Comment);

    app.editor.insert_char('!');
    app.commit_tui_edit();
    app.apply_edit();

    // Composing from quick-view returns to the screen it was opened from.
    assert_eq!(app.screen, Screen::Home);
    let key = app.issues[0].key.clone();
    let cached = app.detail_cache.get(&key).unwrap();
    assert!(!cached.comments.is_empty());
}

/// Regression test: a comment session's `edit_target`/`edit_key`/
/// `edit_return_screen` must not leak into a later, unrelated external
/// `$EDITOR` description edit on a different issue — otherwise the second
/// issue's edited description would be silently posted as a comment on the
/// first issue instead of updating its own description.
#[test]
fn external_edit_after_comment_session_targets_the_right_issue() {
    let mut app = demo_app();

    // First, compose (and apply) a comment on issue A from quick-view.
    app.selected = 0;
    app.quick_view = true;
    app.ensure_quick_view_loaded();
    let issue_a = app.issues[0].key.clone();
    app.begin_comment();
    assert_eq!(app.edit_target, EditTarget::Comment);
    app.editor.insert_char('!');
    app.commit_tui_edit();
    app.apply_edit();
    assert_eq!(app.screen, Screen::Home);
    let a_comments_after_first_session = app.detail_cache.get(&issue_a).unwrap().comments.len();

    // Now open a different issue B in the full Detail screen and simulate
    // the external $EDITOR round-trip (`E`): prime the target, then finish
    // the edit as `editor_launch` would after the process exits.
    app.selected = 1;
    app.open_detail();
    let issue_b = app.detail.as_ref().unwrap().key.clone();
    assert_ne!(issue_a, issue_b, "test needs two distinct issues");
    let comments_on_b_before = app.detail.as_ref().unwrap().comments.len();

    app.begin_external_edit();
    assert_eq!(app.edit_target, EditTarget::Description);
    app.finish_edit("Updated description for B.");
    assert_eq!(app.screen, Screen::Preview);

    app.apply_edit();

    // B's description was updated, not appended as a comment...
    assert_eq!(app.screen, Screen::Detail);
    let b = app.detail.as_ref().unwrap();
    assert_eq!(b.key, issue_b);
    assert!(crate::adf::to_markdown(&b.description).contains("Updated description for B."));
    assert_eq!(b.comments.len(), comments_on_b_before);

    // ...and A's comments are untouched by this second session.
    let a_cached = app.detail_cache.get(&issue_a).unwrap();
    assert_eq!(a_cached.comments.len(), a_comments_after_first_session);
}

#[test]
fn cancel_comment_discards_pending_and_returns_to_detail() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let before = app.detail.as_ref().unwrap().comments.len();

    app.begin_comment();
    app.editor.insert_char('x');
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);

    app.cancel_edit();
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.pending_edit.is_none());
    assert_eq!(app.detail.as_ref().unwrap().comments.len(), before);
}

#[test]
fn jump_to_comments_and_back_moves_scroll() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    assert!(
        !app.detail.as_ref().unwrap().comments.is_empty(),
        "demo detail should have canned comments"
    );

    app.detail_scroll = 0;
    app.jump_to_comments();
    assert!(app.detail_scroll > 0);

    app.jump_to_top();
    assert_eq!(app.detail_scroll, 0);
}

#[test]
fn next_and_prev_comment_step_through_and_clamp() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let comment_count = app.detail.as_ref().unwrap().comments.len();
    assert!(comment_count >= 2, "test needs at least 2 demo comments");

    app.detail_scroll = 0;
    let mut positions = Vec::new();
    for _ in 0..comment_count {
        app.next_comment();
        positions.push(app.detail_scroll);
    }
    // Each step should move further down than the last.
    assert!(positions.windows(2).all(|w| w[0] < w[1]));

    // Stepping past the last comment clamps at the last position.
    let last = *positions.last().unwrap();
    app.next_comment();
    assert_eq!(app.detail_scroll, last);

    // Stepping back through should retrace, clamping at the first.
    for _ in 0..comment_count {
        app.prev_comment();
    }
    assert_eq!(app.detail_scroll, positions[0]);
    app.prev_comment();
    assert_eq!(app.detail_scroll, positions[0]);
}

#[tokio::test]
async fn apply_comment_against_a_live_source_dispatches_and_appends_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    let before = app.detail.as_ref().unwrap().comments.len();
    app.begin_comment();
    app.finish_edit("A brand new comment.");
    assert_eq!(app.screen, Screen::Preview);

    app.apply_edit();
    assert!(app.loading);
    assert_eq!(
        app.detail.as_ref().unwrap().comments.len(),
        before,
        "must not append until the post resolves"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().comments.len(), before + 1);
}
