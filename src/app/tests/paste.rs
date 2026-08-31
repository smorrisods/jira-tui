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
    // `u` now opens `Browse` by default — seed `Input` directly, since
    // that's the stage this test actually exercises.
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
fn paste_of_multiple_lines_flashes_a_note_and_uses_the_first() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    // `u` now opens `Browse` by default — seed `Input` directly, since
    // that's the stage this test actually exercises.
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
