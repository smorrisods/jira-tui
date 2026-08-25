//! The attachment picker: open/close/move gating and the demo-download
//! flash-message path.

use super::super::*;
use super::support::*;

#[test]
fn open_attachments_opens_when_the_issue_has_attachments() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    assert!(!app.detail.as_ref().unwrap().attachments.is_empty());
    app.open_attachments();
    assert!(app.attachments_open);
    assert_eq!(app.attachment_index, 0);
}

#[test]
fn open_attachments_is_a_noop_off_the_detail_screen() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.screen = Screen::Home;
    app.open_attachments();
    assert!(
        !app.attachments_open,
        "the picker should only open from the Detail screen"
    );
}

#[test]
fn open_attachments_is_a_noop_with_no_attachments() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    // The "not found" fallback detail has no attachments (see
    // `domain::demo::demo_detail_has_attachments_but_the_not_found_fallback_does_not`).
    app.detail = Some(crate::domain::demo_detail("NOPE-9999"));
    app.screen = Screen::Detail;
    app.open_attachments();
    assert!(
        !app.attachments_open,
        "the picker should not open over an issue with no attachments"
    );
    assert_eq!(app.status, "no attachments on this issue");
}

#[test]
fn close_attachments_closes_the_picker() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_attachments();
    assert!(app.attachments_open);
    app.close_attachments();
    assert!(!app.attachments_open);
}

#[test]
fn attachments_move_clamps_within_bounds() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_attachments();
    let len = app.detail.as_ref().unwrap().attachments.len();
    assert!(len >= 2, "the demo detail should have multiple attachments");

    app.attachments_move(-1);
    assert_eq!(app.attachment_index, 0, "must not go below the first row");

    app.attachments_move(len as isize + 5);
    assert_eq!(
        app.attachment_index,
        len - 1,
        "must clamp to the last row rather than overshoot"
    );
}

#[test]
fn attachments_move_is_a_noop_with_an_empty_list() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.detail = Some(crate::domain::demo_detail("NOPE-9999"));
    assert!(app.detail.as_ref().unwrap().attachments.is_empty());
    app.attachment_index = 0;
    app.attachments_move(1);
    assert_eq!(app.attachment_index, 0);
    app.attachments_move(-1);
    assert_eq!(app.attachment_index, 0);
}

#[test]
fn download_selected_attachment_on_demo_data_flashes_and_does_no_io() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_attachments();
    assert!(matches!(app.source, crate::domain::Source::Demo));

    app.download_selected_attachment();
    assert_eq!(
        app.active_flash(),
        Some("demo data — nothing to download"),
        "a demo/cache session should flash a friendly message instead of attempting I/O"
    );
    assert!(
        !app.loading,
        "no async download should have been dispatched for demo data"
    );
}
