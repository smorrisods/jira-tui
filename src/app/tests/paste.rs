//! `App::handle_paste`: routing a pasted/dropped path to the
//! attachment-upload `Input` field, or auto-opening the upload flow
//! straight into `Confirm` when it lands on a bare Detail screen.

use super::super::*;
use super::support::*;

#[test]
fn paste_while_input_is_open_sets_the_normalized_path() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    // `open_attachment_upload` now defaults to `Browse` (see
    // `app::attachments`'s module doc) — go straight to `Input` the same
    // way the other `Input`-stage tests in `app::tests::attachments` do,
    // rather than routing through the `Browse` default first.
    app.attachment_upload = Some(AttachmentUpload::Input {
        path: String::new(),
    });

    app.handle_paste("\"C:\\Users\\scott\\notes.txt\"".to_string());

    assert_eq!(
        app.attachment_upload,
        Some(AttachmentUpload::Input {
            path: "/mnt/c/Users/scott/notes.txt".into()
        }),
        "the pasted Windows-style path should land normalized in the Input field"
    );
}

#[test]
fn paste_on_a_bare_detail_screen_auto_opens_confirm_for_a_real_file() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    assert!(app.attachment_upload.is_none());

    let dir = std::env::temp_dir().join(format!(
        "jira-tui-paste-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dropped.txt");
    std::fs::write(&path, b"hello from a drag-and-drop").unwrap();

    app.handle_paste(path.to_str().unwrap().to_string());

    match app.attachment_upload {
        Some(AttachmentUpload::Confirm { ref filename, .. }) => {
            assert_eq!(filename, "dropped.txt");
        }
        other => panic!("expected the paste to auto-open Confirm, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn paste_on_a_bare_detail_screen_is_a_noop_for_a_nonexistent_path() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();

    app.handle_paste("/tmp/jira-tui-paste-test-does-not-exist".to_string());

    assert!(
        app.attachment_upload.is_none(),
        "a pasted path that doesn't resolve to a real file must not open the upload flow"
    );
}

#[test]
fn paste_off_the_detail_screen_is_a_noop() {
    let mut app = demo_app();
    app.screen = Screen::Home;

    app.handle_paste("/tmp/whatever.txt".to_string());

    assert!(app.attachment_upload.is_none());
}

#[test]
fn paste_while_edit_screen_is_active_lands_as_exact_text_not_per_character_artifacts() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_comment();
    assert_eq!(app.screen, Screen::Edit);

    app.handle_paste("first line\nsecond line\nthird line".to_string());

    assert_eq!(
        app.editor.lines,
        vec![
            "first line".to_string(),
            "second line".to_string(),
            "third line".to_string(),
        ],
        "a paste while the editor is active should land as the exact pasted lines, \
         not per-character artifacts"
    );
}

#[test]
fn paste_while_edit_screen_is_active_does_not_normalize_the_text_as_a_path() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_comment();

    app.handle_paste("\"C:\\Users\\scott\\notes.txt\"".to_string());

    assert_eq!(
        app.editor.to_text(),
        "\"C:\\Users\\scott\\notes.txt\"",
        "pasting into the editor is generic text, not a file path — it must not be \
         run through path normalization"
    );
}

#[test]
fn paste_of_a_directory_while_browse_is_open_descends_into_it() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_attachment_upload();

    let dir = std::env::temp_dir().join(format!(
        "jira-tui-paste-browse-dir-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("inside.txt"), b"x").unwrap();

    app.handle_paste(dir.to_str().unwrap().to_string());

    match app.attachment_upload {
        Some(AttachmentUpload::Browse { ref browser }) => {
            assert_eq!(browser.cwd, dir);
            let names: Vec<&str> = browser.entries.iter().map(|e| e.name.as_str()).collect();
            assert!(
                names.contains(&"inside.txt"),
                "browser should now be listing the pasted directory, got {names:?}"
            );
        }
        ref other => {
            panic!("expected the browser to descend into the pasted directory, got {other:?}")
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn paste_of_a_file_while_browse_is_open_finalizes_straight_to_confirm() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_attachment_upload();

    let dir = std::env::temp_dir().join(format!(
        "jira-tui-paste-browse-file-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dropped.txt");
    std::fs::write(&path, b"hello").unwrap();

    app.handle_paste(path.to_str().unwrap().to_string());

    match app.attachment_upload {
        Some(AttachmentUpload::Confirm { ref filename, .. }) => {
            assert_eq!(filename, "dropped.txt");
        }
        ref other => panic!("expected the paste to finalize straight to Confirm, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn paste_of_multiple_lines_flashes_a_note_and_uses_the_first() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    // See the `Input`-stage note above — go straight to `Input` rather than
    // through `open_attachment_upload`'s `Browse` default.
    app.attachment_upload = Some(AttachmentUpload::Input {
        path: String::new(),
    });

    app.handle_paste("/home/scott/a.txt\n/home/scott/b.txt".to_string());

    assert_eq!(
        app.attachment_upload,
        Some(AttachmentUpload::Input {
            path: "/home/scott/a.txt".into()
        })
    );
    assert_eq!(
        app.active_flash(),
        Some("multiple files pasted — using the first")
    );
}
