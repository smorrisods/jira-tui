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

/// Non-`images`-build stand-in for `App::toggle_editor_image_view` (see
/// `app::mod`'s own `#[cfg(not(feature = "images"))]` impl): this build can
/// never actually decode/paint an inline image, so the toggle key is a
/// no-op on `editor_image_view` itself — but not a *silent* one, it tells
/// the user why via the same status-flash mechanism every other
/// build-unavailable action uses. Run this file under
/// `cargo test --no-default-features` to exercise the non-`images` build of
/// the method (under a `default`/`--all-features` build, `images` is
/// compiled in and this same call instead flips the flag and may dispatch a
/// fetch — see `app::tests::inline_images`'s own toggle tests for that
/// build).
#[cfg(not(feature = "images"))]
#[test]
fn toggling_editor_image_view_without_the_images_feature_flashes_instead_of_toggling() {
    let mut app = demo_app();
    assert!(!app.editor_image_view);

    app.toggle_editor_image_view();

    assert!(
        !app.editor_image_view,
        "a non-`images` build can never actually turn image rendering on"
    );
    assert_eq!(
        app.active_flash(),
        Some("image rendering isn't available in this build")
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

#[test]
fn insert_str_splits_on_embedded_newlines_instead_of_inserting_them_literally() {
    let mut ed = EditorState::from_text("");
    ed.insert_str("line one\nline two\nline three");
    assert_eq!(
        ed.lines,
        vec![
            "line one".to_string(),
            "line two".to_string(),
            "line three".to_string(),
        ]
    );
    assert_eq!(
        (ed.cy, ed.cx),
        (2, "line three".chars().count()),
        "cursor should land at the end of the inserted text"
    );
}

#[test]
fn insert_str_splits_mid_line_at_the_cursor() {
    let mut ed = EditorState::from_text("hello world");
    ed.cx = 5; // just after "hello"
    ed.insert_str(" there\nfriend");
    assert_eq!(
        ed.lines,
        vec!["hello there".to_string(), "friend world".to_string()]
    );
    assert_eq!((ed.cy, ed.cx), (1, "friend".chars().count()));
}

#[test]
fn insert_str_with_no_newlines_behaves_like_a_run_of_insert_char() {
    let mut ed = EditorState::from_text("ac");
    ed.cx = 1;
    ed.insert_str("b");
    assert_eq!(ed.lines, vec!["abc".to_string()]);
    assert_eq!((ed.cy, ed.cx), (0, 2));
}

#[test]
fn insert_str_of_an_empty_string_is_a_noop() {
    let mut ed = EditorState::from_text("abc");
    ed.cx = 1;
    ed.insert_str("");
    assert_eq!(ed.lines, vec!["abc".to_string()]);
    assert_eq!((ed.cy, ed.cx), (0, 1));
}

#[test]
fn begin_external_comment_primes_the_target_without_opening_the_tui_editor() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let key = app.detail.as_ref().unwrap().key.clone();

    let started = app.begin_external_comment();
    assert!(started);
    assert_eq!(app.edit_target, EditTarget::Comment);
    assert_eq!(app.edit_key, Some(key));
    assert_eq!(app.edit_return_screen, Screen::Detail);
    // Unlike `begin_comment`, this doesn't open the in-TUI editor screen —
    // the external `$EDITOR` round-trip owns the actual composing.
    assert_eq!(app.screen, Screen::Detail);
}

#[test]
fn begin_external_comment_refuses_without_a_selected_issue() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.quick_view = false;
    let started = app.begin_external_comment();
    assert!(!started);
    assert_eq!(app.status, "no issue selected");
}

#[test]
fn editor_home_end_jump_to_line_start_and_end() {
    let mut ed = EditorState::from_text("hello\nworld");
    ed.cy = 0;
    ed.cx = 3;
    ed.line_start();
    assert_eq!((ed.cy, ed.cx), (0, 0));
    ed.line_end();
    assert_eq!((ed.cy, ed.cx), (0, 5));

    // End on an already-at-end cursor, and Home on an already-at-start
    // cursor, are no-ops rather than moving to an adjacent line.
    ed.line_end();
    assert_eq!((ed.cy, ed.cx), (0, 5));
    ed.line_start();
    ed.line_start();
    assert_eq!((ed.cy, ed.cx), (0, 0));
}

#[test]
fn editor_home_end_operate_on_the_current_line_only() {
    let mut ed = EditorState::from_text("ab\nlonger line");
    ed.cy = 1;
    ed.cx = 3;
    ed.line_end();
    assert_eq!((ed.cy, ed.cx), (1, "longer line".chars().count()));
    ed.line_start();
    assert_eq!((ed.cy, ed.cx), (1, 0));
}

#[test]
fn editor_cursor_byte_index_accounts_for_multibyte_chars() {
    let mut ed = EditorState::from_text("héllo world");
    // "é" is 2 bytes in UTF-8, so cx=3 (h,é,l) is byte index 4, not 3.
    ed.cx = 3;
    assert_eq!(ed.cursor_byte_index(), 4);
}

#[test]
fn editor_replace_range_swaps_text_and_repositions_the_cursor() {
    let mut ed = EditorState::from_text("This is a mispeled word.");
    let start = ed.lines[0].find("mispeled").unwrap();
    let end = start + "mispeled".len();
    ed.replace_range(0, start, end, "misspelled");
    assert_eq!(ed.lines[0], "This is a misspelled word.");
    assert_eq!(ed.cy, 0);
    assert_eq!(
        ed.cx,
        "This is a misspelled".chars().count(),
        "cursor should land right after the replacement"
    );
}

#[test]
fn editor_replace_range_handles_a_shorter_replacement() {
    let mut ed = EditorState::from_text("aaa bbbbb ccc");
    let start = ed.lines[0].find("bbbbb").unwrap();
    let end = start + "bbbbb".len();
    ed.replace_range(0, start, end, "b");
    assert_eq!(ed.lines[0], "aaa b ccc");
    assert_eq!(ed.cx, "aaa b".chars().count());
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

/// `Ctrl+V`'s handler (`App::paste_clipboard_image`) must never panic and
/// must always leave a status message behind, whatever
/// `infra::clipboard_image::capture_clipboard_image` reports — CI (and most
/// dev sandboxes) has none of the external clipboard tools installed, so
/// this exercises the "no tool found" branch specifically, matching the
/// requirement that an unsupported environment resolves cleanly rather than
/// silently no-opping.
#[test]
fn paste_clipboard_image_flashes_a_status_instead_of_panicking() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.status.clear();

    app.paste_clipboard_image();

    // A successful capture now feeds the upload-and-embed pipeline
    // (`App::begin_image_embed`) instead of flashing a placeholder path —
    // that's a visible confirm prompt of its own (see `ui::editor`), not a
    // silent no-op, even though it leaves `status`/the flash untouched. Every
    // other outcome (no tool installed, an empty clipboard, a read failure)
    // still leaves a status/flash behind exactly as before.
    assert!(
        !app.status.is_empty() || app.active_flash().is_some() || app.pending_image_embed.is_some(),
        "must leave a status line, a flash, or a pending image-embed confirmation \
         behind — never silently no-op"
    );
}

/// `Esc` on a pending image-embed confirmation (`App::decline_image_embed`)
/// falls back to inserting the staged path as plain text — the same place
/// in the buffer a non-image pasted path already lands, so declining an
/// embed never just silently loses the reference.
#[test]
fn decline_image_embed_inserts_the_path_as_plain_text() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_comment();
    let path = std::path::PathBuf::from("/tmp/shot.png");
    app.begin_image_embed(path.clone());
    assert_eq!(app.pending_image_embed, Some(path.clone()));

    app.decline_image_embed();

    assert!(
        app.pending_image_embed.is_none(),
        "declining should close the pending confirmation"
    );
    assert_eq!(
        app.editor.to_text(),
        path.display().to_string(),
        "the raw path should land in the buffer, matching how a non-image \
         pasted path already lands via insert_str"
    );
}

/// `App::decline_image_embed` is a no-op when nothing is actually pending —
/// mirrors every other `Option::take`-guarded confirm/cancel method in this
/// codebase (e.g. `App::back_out_of_attachment_upload_confirm`).
#[test]
fn decline_image_embed_is_a_noop_with_nothing_pending() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_comment();
    assert!(app.pending_image_embed.is_none());

    app.decline_image_embed();

    assert!(app.editor.to_text().is_empty());
}

/// Demo/cache sessions can't actually upload anything — `App::confirm_image_embed`
/// must flash the same "not available" message `App::confirm_attachment_upload`
/// uses for the dedicated attachment-upload flow, and must not attempt any
/// I/O (no dispatch, no `loading` flag, nothing inserted into the buffer).
#[test]
fn confirm_image_embed_on_demo_data_flashes_and_does_no_io() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_comment();
    assert!(matches!(app.source, crate::domain::Source::Demo));
    let path = std::path::PathBuf::from("/tmp/shot.png");
    app.begin_image_embed(path);

    app.confirm_image_embed();

    assert_eq!(
        app.active_flash(),
        Some("demo mode — uploading needs live Jira credentials"),
        "a demo/cache session should flash a friendly message instead of attempting I/O"
    );
    assert!(
        app.pending_image_embed.is_none(),
        "the confirmation should close either way, success or not"
    );
    assert!(
        !app.loading,
        "no async upload should have been dispatched for demo data"
    );
    assert!(
        app.editor.to_text().is_empty(),
        "nothing should be inserted into the buffer for a refused demo-mode confirm"
    );
}

/// The full round trip against a mocked live Jira instance: staging a
/// captured/pasted image raises the confirm prompt, confirming it dispatches
/// the real upload-and-embed pipeline (`App::confirm_image_embed` →
/// `dispatch_image_embed`), and applying the result merges the uploaded
/// attachment into `self.detail` (reusing `apply_attachment_uploaded`,
/// exactly as any other upload would) *and* resolves the redirect-probe uuid
/// (`jira::media_uuid_for`) into a real `adf-media://` token inserted at the
/// cursor. Mirrors `upload_attachment`'s and `media_uuid_for`'s own mockito
/// tests in `jira::live::attachments`, just driven through the full
/// App-level dispatch/apply cycle rather than calling the REST functions
/// directly — `mockito::Server::new_async` (not the sync `Server::new`) is
/// required here specifically because this test body already runs inside a
/// tokio runtime (`#[tokio::test]`); the sync constructor builds its own
/// nested runtime internally and panics if called from within one.
#[cfg(feature = "live")]
#[tokio::test]
async fn confirm_image_embed_against_a_live_source_uploads_probes_the_uuid_and_inserts_the_token() {
    let _guard = crate::test_support::lock_env_async().await;
    let base = std::env::temp_dir().join(format!(
        "jira-tui-image-embed-cfg-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::env::set_var("XDG_CONFIG_HOME", &base);

    let mut server = mockito::Server::new_async().await;
    let content_url = format!("{}/secure/attachment/10099/shot.png", server.url());
    let upload_mock = server
        .mock("POST", "/rest/api/3/issue/DS-1/attachments")
        .match_header("x-atlassian-token", "no-check")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(
            r#"[{{
                "id": "10099",
                "filename": "shot.png",
                "size": 4,
                "mimeType": "image/png",
                "created": "2026-08-31T10:00:00.000-0400",
                "content": "{content_url}"
            }}]"#
        ))
        .create();
    let redirect_mock = server
        .mock("GET", "/secure/attachment/10099/shot.png")
        .with_status(303)
        .with_header(
            "location",
            "https://api.media.atlassian.com/file/uuid-123/binary?token=y",
        )
        .create();

    std::env::set_var("JIRA_BASE_URL", server.url());
    std::env::set_var("JIRA_EMAIL", "me@example.com");
    std::env::set_var("JIRA_API_TOKEN", "secret");

    let mut app = demo_app();
    app.source = crate::domain::Source::Live {
        site: "demo.atlassian.net".into(),
        user: "me".into(),
    };
    app.detail = Some(crate::domain::demo_detail("DS-1"));
    app.screen = Screen::Detail;
    app.begin_tui_edit();
    assert_eq!(app.screen, Screen::Edit);

    let dir = std::env::temp_dir().join(format!(
        "jira-tui-image-embed-file-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("shot.png");
    std::fs::write(&path, b"fake").unwrap();

    app.begin_image_embed(path.clone());
    assert_eq!(app.pending_image_embed, Some(path.clone()));

    app.confirm_image_embed();
    assert!(
        app.pending_image_embed.is_none(),
        "confirming should close the pending prompt immediately, before the \
         upload even resolves"
    );
    assert!(app.image_embed_pending);
    assert!(app.loading);

    let event = next_event(&mut app).await;
    app.apply_event(event);

    upload_mock.assert();
    redirect_mock.assert();
    assert!(!app.image_embed_pending);
    assert!(!app.loading);
    assert!(
        app.editor.to_text().contains("adf-media://file/uuid-123"),
        "expected the resolved media token in the buffer, got: {}",
        app.editor.to_text()
    );
    assert!(
        app.detail
            .as_ref()
            .unwrap()
            .attachments
            .iter()
            .any(|a| a.id == "10099"),
        "the uploaded attachment should also be merged into the issue's \
         attachment list, exactly like any other upload"
    );

    for var in ["JIRA_BASE_URL", "JIRA_EMAIL", "JIRA_API_TOKEN"] {
        std::env::remove_var(var);
    }
    std::env::remove_var("XDG_CONFIG_HOME");
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A upload failure (mocked as a 413, mirroring `upload_attachment_surfaces_http_errors`
/// in `jira::live::attachments`) must surface as a status message and leave
/// nothing inserted into the buffer — no partial token, no plain-text
/// fallback either, since there's nothing real to reference.
#[cfg(feature = "live")]
#[tokio::test]
async fn confirm_image_embed_surfaces_an_upload_failure_and_inserts_nothing() {
    let _guard = crate::test_support::lock_env_async().await;
    let base = std::env::temp_dir().join(format!(
        "jira-tui-image-embed-fail-cfg-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::env::set_var("XDG_CONFIG_HOME", &base);

    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/rest/api/3/issue/DS-1/attachments")
        .with_status(413)
        .create();

    std::env::set_var("JIRA_BASE_URL", server.url());
    std::env::set_var("JIRA_EMAIL", "me@example.com");
    std::env::set_var("JIRA_API_TOKEN", "secret");

    let mut app = demo_app();
    app.source = crate::domain::Source::Live {
        site: "demo.atlassian.net".into(),
        user: "me".into(),
    };
    app.detail = Some(crate::domain::demo_detail("DS-1"));
    app.screen = Screen::Detail;
    // `begin_comment` (unlike `begin_tui_edit`) seeds an empty buffer, so
    // "nothing got inserted" is a meaningful assertion below — a fresh
    // description edit is preloaded with the existing description and would
    // never read as empty regardless of what this test does.
    app.begin_comment();

    let dir = std::env::temp_dir().join(format!(
        "jira-tui-image-embed-fail-file-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("shot.png");
    std::fs::write(&path, b"fake").unwrap();

    app.begin_image_embed(path);
    app.confirm_image_embed();

    let event = next_event(&mut app).await;
    app.apply_event(event);

    assert!(!app.image_embed_pending);
    assert!(!app.loading);
    assert!(
        app.status.contains("image upload failed"),
        "expected a failure status, got: {}",
        app.status
    );
    assert!(
        app.editor.to_text().is_empty(),
        "a failed upload has nothing real to reference, so nothing should \
         be inserted"
    );

    for var in ["JIRA_BASE_URL", "JIRA_EMAIL", "JIRA_API_TOKEN"] {
        std::env::remove_var(var);
    }
    std::env::remove_var("XDG_CONFIG_HOME");
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&dir);
}
