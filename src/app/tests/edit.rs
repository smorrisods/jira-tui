//! In-TUI editor / round-trip edit flow tests.

use super::super::*;
use super::support::*;

#[test]
fn edit_flow_previews_then_applies() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let md = app.description_markdown().unwrap();
    assert!(md.contains("Problem"));

    app.finish_edit("## Edited\n\nBrand new body.");
    assert_eq!(app.screen, Screen::Preview);
    assert!(app.pending_edit.is_some());

    app.apply_edit();
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.pending_edit.is_none());
    // The new ADF is now the description.
    let desc = &app.detail.as_ref().unwrap().description;
    let text = crate::adf::to_markdown(desc);
    assert!(text.contains("Edited"));
    assert!(text.contains("Brand new body"));
}

#[test]
fn a_successful_description_edit_triggers_the_jax_party_scene() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.finish_edit("## Edited\n\nBrand new body.");
    app.apply_edit();
    assert!(
        app.jax_party_until > app.tick,
        "a successful description edit should trigger a reactive party moment"
    );
}

#[test]
fn cancel_edit_discards_pending() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.finish_edit("## Nope");
    app.cancel_edit();
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.pending_edit.is_none());
}

#[test]
fn back_out_of_preview_restores_the_buffer_instead_of_discarding_it() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    for c in "Not done yet.".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);

    app.back_out_of_preview();
    assert_eq!(
        app.screen,
        Screen::Edit,
        "backing out of a non-empty preview should return to the editor, not discard it"
    );
    assert!(app.pending_edit.is_none());
    assert!(
        app.editor.to_text().contains("Not done yet."),
        "the typed content must survive the round trip back into the editor"
    );
}

#[test]
fn back_out_of_preview_with_no_content_falls_back_to_a_full_cancel() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_comment();
    // Never typed anything — commit an empty comment buffer.
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);

    app.back_out_of_preview();
    assert_eq!(
        app.screen,
        Screen::Detail,
        "an empty edit has nothing to preserve, so backing out should fully cancel"
    );
    assert!(app.pending_edit.is_none());
}

#[test]
fn editor_is_dirty_reflects_whether_anything_was_typed_not_just_non_empty() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();

    // Regression test: a freshly opened description edit is preloaded
    // with the existing description, so it's non-empty from the start —
    // `is_dirty()` must still be false until something actually changes,
    // otherwise Esc would raise a "discard this edit?" prompt for a
    // session where nothing was ever typed.
    app.begin_tui_edit();
    assert!(
        !app.editor.is_dirty(),
        "a freshly opened, unmodified description edit must not read as dirty"
    );
    app.editor.insert_char('!');
    assert!(app.editor.is_dirty());

    // A fresh comment starts empty, so the same distinction holds there
    // too (this was the one case the old, now-removed `editor_has_content`
    // got right).
    app.begin_comment();
    assert!(!app.editor.is_dirty());
    app.editor.insert_char('!');
    assert!(app.editor.is_dirty());
}

#[test]
fn back_out_of_preview_reload_is_still_dirty_so_a_second_esc_still_confirms() {
    // The content `back_out_of_preview` reloads into the editor already
    // represents real, unsaved work (we only reach that branch for
    // non-empty content) — it must read as dirty immediately, or a second
    // Esc right after backing out of preview would silently cancel the
    // whole edit instead of asking to confirm first.
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    for c in "Not done yet.".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    app.back_out_of_preview();
    assert_eq!(app.screen, Screen::Edit);
    assert!(
        app.editor.is_dirty(),
        "restored preview content must still count as unsaved work"
    );
}

#[test]
fn back_out_of_preview_does_not_leave_a_phantom_trailing_blank_line() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_comment();
    for c in "Not done yet.".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    app.back_out_of_preview();
    assert_eq!(
        app.editor.lines,
        vec!["Not done yet.".to_string()],
        "the restored buffer must not gain an extra empty line from the \
         Markdown round trip's trailing newline"
    );
}

#[test]
fn in_tui_editor_edits_then_commits_to_preview() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    assert_eq!(app.screen, Screen::Edit);
    assert!(!app.editor.lines.is_empty());
    // Type a heading on a fresh first line.
    app.editor.cx = 0;
    app.editor.cy = 0;
    for c in "X ".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);
    assert!(app.pending_edit.is_some());
}

#[test]
fn editor_newline_and_backspace_merge_lines() {
    let mut ed = EditorState::from_text("ab");
    ed.cx = 1;
    ed.newline();
    assert_eq!(ed.lines, vec!["a".to_string(), "b".to_string()]);
    assert_eq!((ed.cy, ed.cx), (1, 0));
    ed.backspace();
    assert_eq!(ed.lines, vec!["ab".to_string()]);
    assert_eq!((ed.cy, ed.cx), (0, 1));
}

#[tokio::test]
async fn apply_description_edit_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.begin_description_edit_target();
    app.finish_edit("## Edited\n\nBrand new body.");
    assert_eq!(app.screen, Screen::Preview);

    app.apply_edit();
    assert!(app.loading);
    assert_eq!(
        app.screen,
        Screen::Preview,
        "must stay put until the update resolves"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.screen, Screen::Detail);
    let text = crate::adf::to_markdown(&app.detail.as_ref().unwrap().description);
    assert!(text.contains("Edited"));
    assert!(text.contains("Brand new body"));
}

/// Same regression as above, for the edit/comment side: starting a new
/// edit session while a previous description update or comment post is
/// still resolving must be refused, not silently allowed to clobber the
/// shared `edit_generation` counter.
#[tokio::test]
async fn begin_tui_edit_refuses_to_start_while_an_edit_is_in_flight() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.begin_description_edit_target();
    app.finish_edit("## First edit\n\nStill in flight.");
    app.apply_edit();
    assert!(app.loading);
    assert!(app.edit_pending);
    let generation = app.edit_generation;

    // Starting a second edit session while the first is still resolving
    // must be refused — it would otherwise take a fresh `pending_edit` and
    // dispatch a second write under a bumped `edit_generation`, discarding
    // whatever the first one eventually returns.
    app.begin_tui_edit();
    assert_eq!(
        app.screen,
        Screen::Preview,
        "must not open a new edit session while one is pending"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert!(!app.edit_pending);
    let text = crate::adf::to_markdown(&app.detail.as_ref().unwrap().description);
    assert!(text.contains("First edit"));
    assert_eq!(app.edit_generation, generation);
}
