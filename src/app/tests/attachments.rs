//! The attachment picker: open/close/move gating and the demo-download
//! flash-message path. Also the upload flow (`u`): path-entry editing,
//! stat-and-preview, and the demo-upload flash-message path.

#[cfg(feature = "images")]
use crate::domain::Attachment;

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

#[test]
fn attachment_upload_input_char_and_backspace_edit_the_typed_path() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_attachment_upload();
    assert_eq!(
        app.attachment_upload,
        Some(AttachmentUpload::Input {
            path: String::new()
        })
    );

    for c in "/tmp/report.pdf".chars() {
        app.attachment_upload_input_char(c);
    }
    assert_eq!(
        app.attachment_upload,
        Some(AttachmentUpload::Input {
            path: "/tmp/report.pdf".into()
        })
    );

    app.attachment_upload_backspace();
    app.attachment_upload_backspace();
    assert_eq!(
        app.attachment_upload,
        Some(AttachmentUpload::Input {
            path: "/tmp/report.p".into()
        })
    );
}

#[test]
fn confirm_attachment_upload_path_with_a_missing_path_stays_in_input_with_a_flash() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_attachment_upload();
    let missing = format!(
        "/tmp/jira-tui-test-does-not-exist-{}-{}",
        std::process::id(),
        line!()
    );
    for c in missing.chars() {
        app.attachment_upload_input_char(c);
    }

    app.confirm_attachment_upload_path();

    assert!(
        matches!(app.attachment_upload, Some(AttachmentUpload::Input { .. })),
        "a nonexistent path must not advance to Confirm"
    );
    assert!(
        app.status.contains(&missing),
        "the status should name the path that failed to stat: {}",
        app.status
    );
}

#[test]
fn confirm_attachment_upload_path_with_a_real_file_advances_to_confirm() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_attachment_upload();

    let dir = std::env::temp_dir().join(format!(
        "jira-tui-upload-test-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notes.txt");
    std::fs::write(&path, b"hello world").unwrap();

    for c in path.to_str().unwrap().chars() {
        app.attachment_upload_input_char(c);
    }
    app.confirm_attachment_upload_path();

    match app.attachment_upload {
        Some(AttachmentUpload::Confirm {
            ref filename,
            size,
            mime,
            ..
        }) => {
            assert_eq!(filename, "notes.txt");
            assert_eq!(size, 11);
            assert_eq!(mime, "text/plain");
        }
        other => panic!("expected Confirm with the stat'd file's details, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn back_out_of_attachment_upload_confirm_returns_to_input_with_the_path_kept() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.attachment_upload = Some(AttachmentUpload::Confirm {
        path: "/tmp/notes.txt".into(),
        filename: "notes.txt".into(),
        size: 11,
        mime: "text/plain",
    });

    app.back_out_of_attachment_upload_confirm();

    assert_eq!(
        app.attachment_upload,
        Some(AttachmentUpload::Input {
            path: "/tmp/notes.txt".into()
        }),
        "esc from Confirm should go back to Input with the typed path preserved, \
         matching the edit-preview screen's own back-out semantics"
    );
}

/// Regression-shaped test for the same "stale async result must not clobber
/// a newer selection" family as `open_transitions_refuses_to_reopen_while_
/// one_is_in_flight` (`transitions.rs`): move the picker's selection twice
/// (bumping `attachment_preview_generation` each time) and confirm a
/// preview that resolves under the *first* move's generation is dropped
/// rather than applied on top of whatever's now highlighted. `#[tokio::test]`
/// only because `open_attachments`/`attachments_move` are eligible to
/// dispatch a real background fetch here (`live_app()` + a detected
/// picker) — that spawned task is never polled (this test never `.await`s),
/// so it never actually touches the network; only `apply_event` (fully
/// synchronous) is under test.
#[cfg(feature = "images")]
#[tokio::test]
async fn attachment_preview_drops_a_stale_response_after_moving_again() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    // A live session's detail load is asynchronous (see `App::open_detail`),
    // so this sets `app.detail` directly rather than waiting on a fetch —
    // same shortcut `transitions.rs`'s `live_app()` tests take.
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_attachments();
    let first_id = app.detail.as_ref().unwrap().attachments[0].id.clone();
    let stale_generation = app.attachment_preview_generation;

    app.attachments_move(1);
    assert_ne!(
        app.attachment_preview_generation, stale_generation,
        "moving the selection must bump the preview generation"
    );

    app.apply_event(AppEvent::AttachmentPreviewLoaded {
        generation: stale_generation,
        attachment_id: first_id,
        image: Some(image::DynamicImage::new_rgb8(1, 1)),
    });

    assert!(
        app.attachment_preview.borrow().is_none(),
        "a preview fetched under a since-superseded generation must not be applied"
    );
}

/// The successful counterpart to the stale-response test above: a response
/// tagged with the *current* generation, for the attachment that's still
/// actually highlighted, is applied.
#[cfg(feature = "images")]
#[tokio::test]
async fn attachment_preview_applies_a_current_response() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    // A live session's detail load is asynchronous (see `App::open_detail`),
    // so this sets `app.detail` directly rather than waiting on a fetch —
    // same shortcut `transitions.rs`'s `live_app()` tests take.
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_attachments();
    let current_id = app.detail.as_ref().unwrap().attachments[0].id.clone();
    let generation = app.attachment_preview_generation;

    app.apply_event(AppEvent::AttachmentPreviewLoaded {
        generation,
        attachment_id: current_id.clone(),
        image: Some(image::DynamicImage::new_rgb8(1, 1)),
    });

    let preview = app.attachment_preview.borrow();
    match preview.as_ref() {
        Some(p) => assert_eq!(p.attachment_id, current_id),
        None => panic!("a current, matching response should populate the preview"),
    }
}

/// Code-review regression test: closing the picker must bump
/// `attachment_preview_generation`, not just clear the cached preview —
/// otherwise a fetch still in flight when the picker closes lands tagged
/// with a generation that's still current (nothing else bumps it between
/// close and the next open/move), passes
/// `apply_attachment_preview_loaded`'s checks, and silently repopulates
/// `attachment_preview` while the picker is closed.
#[cfg(feature = "images")]
#[tokio::test]
async fn closing_the_picker_bumps_the_generation_so_a_late_response_is_dropped() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_attachments();
    let current_id = app.detail.as_ref().unwrap().attachments[0].id.clone();
    let stale_generation = app.attachment_preview_generation;

    app.close_attachments();
    assert_ne!(
        app.attachment_preview_generation, stale_generation,
        "closing the picker must bump the generation, not just clear the cache"
    );

    app.apply_event(AppEvent::AttachmentPreviewLoaded {
        generation: stale_generation,
        attachment_id: current_id,
        image: Some(image::DynamicImage::new_rgb8(1, 1)),
    });

    assert!(
        app.attachment_preview.borrow().is_none(),
        "a response dispatched before close must not repopulate the preview after close"
    );
}

/// Code-review regression test: re-uploading a new version of the
/// currently-cached-preview attachment must invalidate that cache — the
/// old decoded bytes belong to the file that just got replaced.
#[cfg(feature = "images")]
#[tokio::test]
async fn uploading_to_the_current_issue_invalidates_the_cached_preview() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_attachments();
    let current_id = app.detail.as_ref().unwrap().attachments[0].id.clone();
    let generation = app.attachment_preview_generation;
    app.apply_event(AppEvent::AttachmentPreviewLoaded {
        generation,
        attachment_id: current_id.clone(),
        image: Some(image::DynamicImage::new_rgb8(1, 1)),
    });
    assert!(
        app.attachment_preview.borrow().is_some(),
        "setup: the preview must be cached before the upload lands"
    );

    let generation_before_upload = app.attachment_preview_generation;
    app.apply_event(AppEvent::AttachmentUploaded {
        key,
        filename: "mockup.png".into(),
        result: Ok(vec![app.detail.as_ref().unwrap().attachments[0].clone()]),
    });

    assert!(
        app.attachment_preview.borrow().is_none(),
        "a re-upload to the current issue must invalidate its cached preview"
    );
    // A code-review finding: invalidating alone (one generation bump)
    // isn't enough while the picker is already open — a paired
    // `attachments_open`-guarded refresh (a second bump) must also fire,
    // the same shape `refresh_detail_images` uses, or the picker is left
    // showing no preview until the user happens to move the selection.
    assert_eq!(
        app.attachment_preview_generation,
        generation_before_upload + 2,
        "an open picker must get both the invalidate and a re-dispatched refresh"
    );
}

/// Code-review regression test: the sequence the review actually flagged
/// as reachable — confirming an upload with the picker *closed* (the only
/// way to reach it at all, since `attachments_open` swallows `u`), then
/// opening the picker *while that upload is still in flight* (nothing
/// guards `a` on an in-flight upload), then the response landing. The
/// picker must still end up showing a re-dispatched preview, not a blank
/// one left over from `invalidate_attachment_preview` alone.
#[cfg(feature = "images")]
#[tokio::test]
async fn opening_the_picker_while_an_upload_is_in_flight_still_gets_a_refreshed_preview() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;

    // Picker closed throughout the "confirm upload" step — matches the
    // only reachable way to start one.
    assert!(!app.attachments_open);

    // The picker opens *after* the upload was dispatched but *before* its
    // response lands.
    app.open_attachments();
    assert!(app.attachments_open);
    let generation_before_upload = app.attachment_preview_generation;

    app.apply_event(AppEvent::AttachmentUploaded {
        key,
        filename: "mockup.png".into(),
        result: Ok(vec![app.detail.as_ref().unwrap().attachments[0].clone()]),
    });

    assert_eq!(
        app.attachment_preview_generation,
        generation_before_upload + 2,
        "the now-open picker must get a re-dispatched refresh, not just an invalidate"
    );
}

/// Code-review regression test: uploading a brand-new, unrelated
/// attachment to the same issue must not tear down and re-fetch an
/// already-correct cached preview for whatever's actually highlighted —
/// only a re-upload that touches the highlighted id itself should.
#[cfg(feature = "images")]
#[tokio::test]
async fn uploading_an_unrelated_new_attachment_does_not_disturb_the_cached_preview() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_attachments();
    let highlighted_id = app.detail.as_ref().unwrap().attachments[0].id.clone();
    let generation = app.attachment_preview_generation;
    app.apply_event(AppEvent::AttachmentPreviewLoaded {
        generation,
        attachment_id: highlighted_id,
        image: Some(image::DynamicImage::new_rgb8(1, 1)),
    });
    assert!(
        app.attachment_preview.borrow().is_some(),
        "setup: the highlighted attachment's preview must be cached before the upload lands"
    );
    let generation_before_upload = app.attachment_preview_generation;

    let new_attachment = Attachment {
        id: "99999".into(),
        filename: "brand-new.png".into(),
        mime_type: "image/png".into(),
        size: 100,
        created: "2026-08-28".into(),
        content_url: "https://example.atlassian.net/secure/attachment/99999/brand-new.png".into(),
        thumbnail_url: None,
    };
    app.apply_event(AppEvent::AttachmentUploaded {
        key,
        filename: "brand-new.png".into(),
        result: Ok(vec![new_attachment]),
    });

    assert_eq!(
        app.attachment_preview_generation, generation_before_upload,
        "an unrelated upload must not invalidate an already-correct cached preview"
    );
    assert!(
        app.attachment_preview.borrow().is_some(),
        "the highlighted attachment's preview must remain cached"
    );
}

/// Code-review regression test: an attachment referenced inline in the
/// description (its cached decoded bytes sitting in `App::inline_images`,
/// independent of whatever the attachment picker itself has cached) must
/// also get invalidated on re-upload, not just the picker's own preview —
/// otherwise the description keeps rendering the pre-upload image
/// indefinitely.
#[cfg(feature = "images")]
#[tokio::test]
async fn re_uploading_an_attachment_cached_as_an_inline_image_invalidates_that_too() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    // Cleared so the walk this test cares about isn't affected by the
    // demo fixture's own baked-in comment media node (see
    // `app::tests::inline_images`'s own doc comments on this exact
    // pitfall).
    detail.comments = vec![];
    app.detail = Some(detail);
    app.screen = Screen::Detail;
    let reuploaded_id = app.detail.as_ref().unwrap().attachments[0].id.clone();

    // Seeds the inline-image cache directly rather than driving the real
    // resolve/fetch pipeline — isolates the invalidation behaviour this
    // test actually cares about, mirroring how `app::tests::inline_images`
    // seeds `inline_images` for its own render-lookup tests.
    app.inline_images.borrow_mut().insert(
        InlineImageKey::Attachment(reuploaded_id.clone()),
        image::DynamicImage::new_rgb8(1, 1),
    );
    let inline_generation_before = app.inline_image_generation;

    let reuploaded = app.detail.as_ref().unwrap().attachments[0].clone();
    app.apply_event(AppEvent::AttachmentUploaded {
        key,
        filename: reuploaded.filename.clone(),
        result: Ok(vec![reuploaded]),
    });

    assert!(
        !app.inline_images
            .borrow()
            .contains_key(&InlineImageKey::Attachment(reuploaded_id)),
        "the stale inline-cached image for the re-uploaded id must be dropped"
    );
    assert_ne!(
        app.inline_image_generation, inline_generation_before,
        "invalidating the inline-image cache must bump its generation"
    );
}

/// Code-review regression test: an inline fetch still *in flight* (never
/// landed, so not yet in `inline_images`) for the re-uploaded attachment
/// id must also trigger invalidation — checking only the decoded cache
/// missed this case entirely, letting the in-flight response later land
/// under the un-bumped generation and permanently cache the pre-upload
/// bytes with nothing left to ever invalidate it.
#[cfg(feature = "images")]
#[tokio::test]
async fn re_uploading_an_attachment_with_an_in_flight_inline_fetch_invalidates_that_too() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.comments = vec![];
    app.detail = Some(detail);
    app.screen = Screen::Detail;
    let reuploaded_id = app.detail.as_ref().unwrap().attachments[0].id.clone();

    // Seeds the *pending* (in-flight, not yet decoded) tracker directly,
    // rather than `inline_images` itself — the exact gap the review found.
    app.inline_images_pending
        .insert(InlineImageKey::Attachment(reuploaded_id.clone()));
    let inline_generation_before = app.inline_image_generation;

    let reuploaded = app.detail.as_ref().unwrap().attachments[0].clone();
    app.apply_event(AppEvent::AttachmentUploaded {
        key,
        filename: reuploaded.filename.clone(),
        result: Ok(vec![reuploaded]),
    });

    assert!(
        !app.inline_images_pending
            .contains(&InlineImageKey::Attachment(reuploaded_id)),
        "the stale in-flight tracking for the re-uploaded id must be cleared"
    );
    assert_ne!(
        app.inline_image_generation, inline_generation_before,
        "an in-flight fetch for the re-uploaded id must also trigger invalidation"
    );
}

/// A non-image attachment (the demo PDF) must never be handed off for
/// decoding — `attachment_preview_url` (the pure eligibility gate) already
/// has direct unit tests in `app::attachments`; this end-to-end version
/// confirms the picker itself never ends up with a preview for it, so
/// `ui::attachment_picker` renders its normal metadata + placeholder path.
#[cfg(feature = "images")]
#[tokio::test]
async fn attachment_preview_is_never_set_for_a_non_image_attachment() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    // A live session's detail load is asynchronous (see `App::open_detail`),
    // so this sets `app.detail` directly rather than waiting on a fetch —
    // same shortcut `transitions.rs`'s `live_app()` tests take.
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_attachments();
    let pdf_index = app
        .detail
        .as_ref()
        .unwrap()
        .attachments
        .iter()
        .position(|a| !a.mime_type.starts_with("image/"))
        .expect("the demo detail should include a non-image attachment");
    app.attachments_move(pdf_index as isize);
    assert_eq!(app.attachment_index, pdf_index);

    assert!(
        app.attachment_preview.borrow().is_none(),
        "a non-image attachment must never end up with a decoded preview"
    );
}

#[test]
fn confirm_attachment_upload_on_demo_data_flashes_and_does_no_io() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    assert!(matches!(app.source, crate::domain::Source::Demo));
    app.attachment_upload = Some(AttachmentUpload::Confirm {
        path: "/tmp/notes.txt".into(),
        filename: "notes.txt".into(),
        size: 11,
        mime: "text/plain",
    });

    app.confirm_attachment_upload();

    assert_eq!(
        app.active_flash(),
        Some("demo mode — uploading needs live Jira credentials"),
        "a demo/cache session should flash a friendly message instead of attempting I/O"
    );
    assert!(
        !app.loading,
        "no async upload should have been dispatched for demo data"
    );
    assert_eq!(
        app.attachment_upload, None,
        "the flow should close once the confirm key fires, success or not"
    );
}

/// Regression test for a code-review finding: `App::refresh_detail`
/// replaces `self.detail` wholesale without going through
/// `App::open_attachments`/`attachments_move`, so nothing bumped the
/// preview generation before `App::invalidate_attachment_preview` was added
/// to that path. Without it, a preview response fetched before the refresh
/// could still be applied afterward even though it may no longer correspond
/// to the (possibly reshuffled) attachment now at the same index.
#[cfg(feature = "images")]
#[test]
fn refresh_detail_invalidates_a_cached_attachment_preview() {
    let mut app = demo_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    app.open_detail();
    app.open_attachments();
    let stale_generation = app.attachment_preview_generation;
    *app.attachment_preview.get_mut() = Some(AttachmentPreview {
        attachment_id: app.detail.as_ref().unwrap().attachments[0].id.clone(),
        protocol: app
            .image_picker
            .as_ref()
            .unwrap()
            .new_resize_protocol(image::DynamicImage::new_rgb8(1, 1)),
    });

    app.refresh_detail();

    assert_ne!(
        app.attachment_preview_generation, stale_generation,
        "refreshing the open issue must bump the preview generation"
    );
    assert!(
        app.attachment_preview.borrow().is_none(),
        "refreshing the open issue must drop any cached preview, not just its generation"
    );
}

/// Regression test for a code-review finding: `invalidate_attachment_preview`
/// used to have no matching re-fetch, unlike the parallel `invalidate_inline_
/// images`/`refresh_inline_images` pair — so a live refresh landing while the
/// attachment picker was still open (e.g. `r` pressed on bare Detail, then
/// `a` before that fetch resolved) permanently blanked the preview until the
/// user happened to move the selection. `App::refresh_detail`'s live branch
/// must now re-dispatch a preview fetch (a second generation bump) whenever
/// `attachments_open` is still true once the refreshed detail lands.
///
/// Injects a synthetic `DetailLoaded` (mirroring the generation/key
/// `App::refresh_detail` itself just set up) rather than draining
/// `events_rx` — `open_attachments()` above is itself eligible to dispatch a
/// real preview fetch (this demo detail's first attachment is an image, and
/// `live_app()` + a detected picker make it eligible), so the channel may
/// already hold an unrelated event; every other stale/current-response test
/// in this file sidesteps that the same way.
#[cfg(feature = "images")]
#[tokio::test]
async fn refresh_detail_redispatches_the_attachment_preview_while_the_picker_is_still_open() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    // A live session's detail load is asynchronous (see `App::open_detail`),
    // so this sets `app.detail` directly rather than waiting on a fetch —
    // same shortcut `transitions.rs`'s `live_app()` tests take.
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_attachments();
    let generation_after_open = app.attachment_preview_generation;

    app.refresh_detail();
    app.apply_event(AppEvent::DetailLoaded {
        generation: app.detail_generation,
        key,
        detail: Box::new(app.detail.as_ref().unwrap().clone()),
        status: None,
    });

    assert_eq!(
        app.attachment_preview_generation,
        generation_after_open + 2,
        "invalidate bumps the generation once; since the picker is still open, the \
         re-dispatched refresh must bump it a second time rather than leaving the \
         picker's preview permanently blank until the user happens to move the selection"
    );
}

/// The counterpart to the test above: when the picker is *not* open at
/// refresh time, nothing should eagerly dispatch a preview fetch the user
/// isn't looking at — `invalidate_attachment_preview` alone (one generation
/// bump) is correct in that case.
#[cfg(feature = "images")]
#[tokio::test]
async fn refresh_detail_does_not_redispatch_the_attachment_preview_when_the_picker_is_closed() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_attachments();
    app.close_attachments();
    let generation_after_close = app.attachment_preview_generation;

    app.refresh_detail();
    app.apply_event(AppEvent::DetailLoaded {
        generation: app.detail_generation,
        key,
        detail: Box::new(app.detail.as_ref().unwrap().clone()),
        status: None,
    });

    assert_eq!(
        app.attachment_preview_generation,
        generation_after_close + 1,
        "with the picker closed, only invalidate's own generation bump should happen — \
         no fetch should be dispatched for a preview nothing is currently showing"
    );
}
