//! Inline description image resolution/cache/fetch (issue #130 phase 1,
//! `images` feature only). Pure `resolve_inline_images` tests need no
//! `App`; the rest mirror `attachments.rs`'s own
//! `attachment_preview_drops_a_stale_response_after_moving_again`/
//! `refresh_detail_invalidates_a_cached_attachment_preview` shape, just
//! against a map cache instead of a single slot.

use serde_json::json;

use crate::domain::{Attachment, IssueDetail};

use super::super::inline_images::resolve_inline_images;
use super::super::*;
use super::support::*;

fn image_attachment(id: &str, filename: &str, mime: &str, thumbnail: Option<&str>) -> Attachment {
    Attachment {
        id: id.into(),
        filename: filename.into(),
        mime_type: mime.into(),
        size: 1024,
        created: "2026-08-25".into(),
        content_url: format!("https://example.atlassian.net/secure/attachment/{id}/{filename}"),
        thumbnail_url: thumbnail.map(String::from),
    }
}

/// A base `IssueDetail` with `description`/`attachments` swapped for
/// whatever a test needs — the other fields are irrelevant to
/// `resolve_inline_images`, so the demo fallback's defaults are fine as-is.
fn detail_with(description: serde_json::Value, attachments: Vec<Attachment>) -> IssueDetail {
    let mut detail = crate::domain::demo_detail("DS-INLINE-TEST");
    detail.description = description;
    detail.attachments = attachments;
    detail.acceptance_criteria = None;
    detail
}

fn media_node(alt: &str) -> serde_json::Value {
    json!({ "type": "media", "attrs": { "id": "x", "type": "file", "alt": alt } })
}

#[test]
fn resolves_a_media_node_whose_alt_matches_an_attachment_filename() {
    let attachment = image_attachment(
        "10001",
        "mockup.png",
        "image/png",
        Some("https://example.atlassian.net/secure/thumbnail/10001/mockup.png"),
    );
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "mediaSingle", "content": [ media_node("mockup.png") ] } ]
    });
    let detail = detail_with(description, vec![attachment]);

    let resolved = resolve_inline_images(&detail);

    assert_eq!(
        resolved,
        vec![(
            InlineImageKey::Attachment("10001".into()),
            "https://example.atlassian.net/secure/thumbnail/10001/mockup.png".into()
        )],
        "thumbnail_url must be preferred over content_url when both are present"
    );
}

#[test]
fn falls_back_to_content_url_when_there_is_no_thumbnail() {
    let attachment = image_attachment("10001", "mockup.png", "image/png", None);
    let content_url = attachment.content_url.clone();
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ media_node("mockup.png") ]
    });
    let detail = detail_with(description, vec![attachment]);

    let resolved = resolve_inline_images(&detail);

    assert_eq!(
        resolved,
        vec![(InlineImageKey::Attachment("10001".into()), content_url)]
    );
}

#[test]
fn no_match_when_alt_does_not_equal_any_attachment_filename() {
    let attachment = image_attachment("10001", "mockup.png", "image/png", None);
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ media_node("unrelated-file.png") ]
    });
    let detail = detail_with(description, vec![attachment]);

    assert!(resolve_inline_images(&detail).is_empty());
}

#[test]
fn no_match_when_the_matched_attachment_is_not_an_image() {
    let attachment = image_attachment("10002", "notes.pdf", "application/pdf", None);
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ media_node("notes.pdf") ]
    });
    let detail = detail_with(description, vec![attachment]);

    assert!(resolve_inline_images(&detail).is_empty());
}

#[test]
fn a_media_groups_children_resolve_both_in_document_order() {
    let first = image_attachment("1", "first.png", "image/png", None);
    let second = image_attachment("2", "second.png", "image/png", None);
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "mediaGroup", "content": [
            media_node("first.png"),
            media_node("second.png"),
        ] } ]
    });
    let detail = detail_with(description, vec![first, second]);

    let resolved = resolve_inline_images(&detail);

    assert_eq!(
        resolved.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
        vec![
            InlineImageKey::Attachment("1".into()),
            InlineImageKey::Attachment("2".into()),
        ]
    );
}

/// Issue #130 phase 4: an external media node resolves to `External(url)`
/// directly from its own `url` attribute — no attachment/filename matching
/// involved at all, unlike the attachment-backed path. A matching `alt` here
/// is incidental, not what makes this resolve.
#[test]
fn an_external_media_node_resolves_to_its_own_url() {
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "media", "attrs": {
            "id": "x", "type": "external", "alt": "mockup.png",
            "url": "https://third-party.example.com/mockup.png"
        } } ]
    });
    let detail = detail_with(description, vec![]);

    let resolved = resolve_inline_images(&detail);

    assert_eq!(
        resolved,
        vec![(
            InlineImageKey::External("https://third-party.example.com/mockup.png".into()),
            "https://third-party.example.com/mockup.png".into()
        )]
    );
}

/// An external media node with no `alt` at all still resolves — its `url`
/// alone is enough identity, unlike an attachment-backed node which needs a
/// non-empty `alt` to have anything to match against.
#[test]
fn an_external_media_node_with_no_alt_still_resolves() {
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "media", "attrs": {
            "id": "x", "type": "external",
            "url": "https://third-party.example.com/pic.png"
        } } ]
    });
    let detail = detail_with(description, vec![]);

    assert_eq!(
        resolve_inline_images(&detail),
        vec![(
            InlineImageKey::External("https://third-party.example.com/pic.png".into()),
            "https://third-party.example.com/pic.png".into()
        )]
    );
}

/// The `MAX_INLINE_IMAGES` cap is shared across both kinds combined, in
/// document order — an external node early in the document still counts
/// against the same budget as attachment-backed ones found later, rather
/// than each kind getting its own separate cap.
#[test]
fn the_cap_is_shared_across_attachment_and_external_nodes_in_document_order() {
    let attachments: Vec<Attachment> = (1..=5)
        .map(|n| image_attachment(&n.to_string(), &format!("img{n}.png"), "image/png", None))
        .collect();
    let mut content = vec![media_node("img1.png"), media_node("img2.png")];
    content.push(json!({ "type": "media", "attrs": {
        "id": "ext", "type": "external", "url": "https://third-party.example.com/a.png"
    } }));
    content.extend(
        (3..=5)
            .map(|n| media_node(&format!("img{n}.png")))
            .collect::<Vec<_>>(),
    );
    content.push(json!({ "type": "media", "attrs": {
        "id": "ext2", "type": "external", "url": "https://third-party.example.com/b.png"
    } }));
    let description = json!({ "type": "doc", "version": 1, "content": content });
    let detail = detail_with(description, attachments);

    let resolved = resolve_inline_images(&detail);

    assert_eq!(
        resolved.len(),
        super::super::inline_images::MAX_INLINE_IMAGES
    );
    assert_eq!(
        resolved.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
        vec![
            InlineImageKey::Attachment("1".into()),
            InlineImageKey::Attachment("2".into()),
            InlineImageKey::External("https://third-party.example.com/a.png".into()),
            InlineImageKey::Attachment("3".into()),
            InlineImageKey::Attachment("4".into()),
            InlineImageKey::Attachment("5".into()),
        ],
        "the 7th match (the second external node) must not resolve — the cap is shared, not per-kind"
    );
}

#[test]
fn more_than_the_cap_only_resolves_the_first_matches_in_document_order() {
    let attachments: Vec<Attachment> = (1..=8)
        .map(|n| image_attachment(&n.to_string(), &format!("img{n}.png"), "image/png", None))
        .collect();
    let content: Vec<serde_json::Value> = (1..=8)
        .map(|n| media_node(&format!("img{n}.png")))
        .collect();
    let description = json!({ "type": "doc", "version": 1, "content": content });
    let detail = detail_with(description, attachments);

    let resolved = resolve_inline_images(&detail);

    assert_eq!(
        resolved.len(),
        super::super::inline_images::MAX_INLINE_IMAGES
    );
    assert_eq!(
        resolved.last().unwrap().0,
        InlineImageKey::Attachment("6".into()),
        "only the first 6 in document order should resolve"
    );
}

#[test]
fn acceptance_criteria_media_nodes_are_also_resolved() {
    let attachment = image_attachment("10001", "ac.png", "image/png", None);
    let content_url = attachment.content_url.clone();
    let mut detail = detail_with(
        json!({ "type": "doc", "version": 1, "content": [] }),
        vec![attachment],
    );
    detail.acceptance_criteria = Some(json!({
        "type": "doc", "version": 1,
        "content": [ media_node("ac.png") ]
    }));

    let resolved = resolve_inline_images(&detail);

    assert_eq!(
        resolved,
        vec![(InlineImageKey::Attachment("10001".into()), content_url)]
    );
}

/// Opening an issue whose description resolves to a real image kicks off an
/// eager fetch (`App::refresh_inline_images`, called from
/// `App::open_by_key`'s `locally_created` shortcut — see
/// `App::load_detail`), and a completed `InlineImageLoaded` under the
/// current generation lands in the cache. The real dispatch always resolves
/// to `image: None` in this test environment (no live credentials for
/// `fetch_attachment_preview_blocking` to fetch against), so — mirroring
/// `attachment_preview_applies_a_current_response` — this drains the real
/// dispatch to confirm it targeted the right key/generation, then applies a
/// synthetic completed response to exercise the cache-insert path itself.
#[tokio::test]
async fn opening_an_issue_with_a_resolvable_image_dispatches_a_fetch_that_lands_in_the_cache() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let summary = app.issues[0].clone();
    let key = summary.key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.attachments = vec![image_attachment("10001", "mockup.png", "image/png", None)];
    detail.description = json!({
        "type": "doc", "version": 1,
        "content": [ media_node("mockup.png") ]
    });
    app.locally_created
        .push(LocallyCreatedIssue { summary, detail });

    app.open_by_key(&key);
    let generation = app.inline_image_generation;
    assert!(app.inline_images.borrow().is_empty());

    let event = next_event(&mut app).await;
    match event {
        AppEvent::InlineImageLoaded {
            generation: g,
            key: k,
            ..
        } => {
            assert_eq!(g, generation);
            assert_eq!(k, InlineImageKey::Attachment("10001".into()));
        }
        _ => panic!("expected InlineImageLoaded, got a different event"),
    }

    app.apply_event(AppEvent::InlineImageLoaded {
        generation,
        key: InlineImageKey::Attachment("10001".into()),
        image: Some(image::DynamicImage::new_rgb8(1, 1)),
    });

    let cache = app.inline_images.borrow();
    assert_eq!(cache.len(), 1);
    assert!(cache.contains_key(&InlineImageKey::Attachment("10001".into())));
}

/// Issue #130 phase 4: dispatching a fetch for an `External` key needs no
/// Jira credentials at all — unlike the `Attachment` path (whose blocking
/// fetch starts with `crate::jira::Config::load()?` and returns `None`
/// immediately, with no network attempt, when none are configured; see the
/// test above), the `External` path (`fetch_external_image_blocking`) calls
/// `jira::get_bytes_public(url)` directly, which takes no `Config`
/// parameter at all — there is nothing for it to load. `live_app()` here
/// guarantees zero configured credentials (env vars cleared,
/// `XDG_CONFIG_HOME` pointed at an empty temp dir); the dispatch still runs
/// to completion — a real, deterministic, offline connection-refused
/// failure against a closed loopback port — rather than short-circuiting on
/// a missing `Config`, proving the `External` path is structurally
/// credential-free at the app-level wiring, not just the leaf function.
/// (`get_bytes_public` sending no `Authorization` header on an actual
/// successful request is verified directly, at the wire, by
/// `jira::live::support`'s own
/// `fetch_public_bytes_sends_no_authorization_header` test — `mockito` has
/// no TLS support, so that assertion can't be driven through this crate's
/// `https://`-only gate from an app-level test.)
#[tokio::test]
async fn external_fetch_runs_with_zero_jira_credentials_configured() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let summary = app.issues[0].clone();
    let key = summary.key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.attachments = vec![];
    let external_url = "https://127.0.0.1:1/unreachable.png";
    detail.description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "media", "attrs": {
            "id": "x", "type": "external", "url": external_url
        } } ]
    });
    app.locally_created
        .push(LocallyCreatedIssue { summary, detail });

    app.open_by_key(&key);
    let generation = app.inline_image_generation;

    let event = next_event(&mut app).await;
    match event {
        AppEvent::InlineImageLoaded {
            generation: g,
            key: k,
            image,
        } => {
            assert_eq!(g, generation);
            assert_eq!(k, InlineImageKey::External(external_url.into()));
            assert!(
                image.is_none(),
                "127.0.0.1:1 refuses the connection; a Some would mean this \
                 somehow reached a real server"
            );
        }
        _ => panic!("expected InlineImageLoaded, got a different event"),
    }
}

/// A response tagged with a since-superseded generation (the issue changed
/// again — via a manual `r` refresh here — before the first fetch resolved)
/// must not be applied, mirroring
/// `attachment_preview_drops_a_stale_response_after_moving_again`.
#[tokio::test]
async fn a_stale_generation_response_is_dropped() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let summary = app.issues[0].clone();
    let key = summary.key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.attachments = vec![image_attachment("10001", "mockup.png", "image/png", None)];
    detail.description = json!({
        "type": "doc", "version": 1,
        "content": [ media_node("mockup.png") ]
    });
    app.locally_created
        .push(LocallyCreatedIssue { summary, detail });

    app.open_by_key(&key);
    let stale_generation = app.inline_image_generation;
    let _dispatched = next_event(&mut app).await;

    app.refresh_detail();
    let refreshed = next_event(&mut app).await;
    app.apply_event(refreshed);
    assert_ne!(
        app.inline_image_generation, stale_generation,
        "refreshing the open issue must bump the generation"
    );

    app.apply_event(AppEvent::InlineImageLoaded {
        generation: stale_generation,
        key: InlineImageKey::Attachment("10001".into()),
        image: Some(image::DynamicImage::new_rgb8(1, 1)),
    });

    assert!(
        app.inline_images.borrow().is_empty(),
        "a response tagged with a since-superseded generation must not be applied"
    );
}

/// A demo/cache session never dispatches an inline-image fetch at all, even
/// when the description resolves to a real match — `images_eligible` gates
/// on `Source::Live` before any fetch is even considered, mirroring
/// `attachment_preview_url_is_none_for_demo_or_cache_sessions`.
#[test]
fn a_demo_session_never_dispatches_an_inline_image_fetch() {
    let mut app = demo_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let summary = app.issues[0].clone();
    let key = summary.key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.attachments = vec![image_attachment("10001", "mockup.png", "image/png", None)];
    detail.description = json!({
        "type": "doc", "version": 1,
        "content": [ media_node("mockup.png") ]
    });
    app.locally_created
        .push(LocallyCreatedIssue { summary, detail });

    app.open_by_key(&key);

    assert!(app.inline_images.borrow().is_empty());
    assert!(
        app.events_rx.try_recv().is_err(),
        "no fetch should ever be dispatched for a demo/cache session"
    );
}

/// Refreshing the open issue clears the whole cache and bumps the
/// generation — mirrors `refresh_detail_invalidates_a_cached_attachment_preview`
/// exactly, just against the map cache instead of a single slot.
#[test]
fn refresh_detail_clears_the_inline_image_cache_and_bumps_the_generation() {
    let mut app = demo_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    app.open_detail();
    let stale_generation = app.inline_image_generation;
    app.inline_images.get_mut().insert(
        InlineImageKey::Attachment("10001".into()),
        image::DynamicImage::new_rgb8(1, 1),
    );

    app.refresh_detail();

    assert_ne!(
        app.inline_image_generation, stale_generation,
        "refreshing the open issue must bump the generation"
    );
    assert!(
        app.inline_images.borrow().is_empty(),
        "refreshing the open issue must drop any cached inline images, not just its generation"
    );
}

/// Regression test for a code-review finding: `invalidate_inline_images`
/// used to clear `inline_images` (the decoded-image cache) but leave
/// `inline_image_protocols` (the *encoded* `SlicedProtocol` cache) behind.
/// That cache is keyed only by a media node's `alt` text, with no
/// issue/generation component — so a stale entry left over from a
/// previously-viewed issue whose inline image happened to share the same
/// `alt` (e.g. a common filename like "screenshot.png") would be returned
/// unchanged for the new issue by `App::sliced_inline_image_protocol`'s
/// size-only staleness check, silently rendering the wrong picture. This
/// confirms both caches are cleared together.
#[test]
fn refresh_detail_also_clears_the_stale_sliced_protocol_cache() {
    let mut app = demo_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    app.open_detail();
    let media = crate::adf::InlineMediaRef {
        alt: "shared-filename.png".into(),
        url: None,
    };
    let picker = app.image_picker.as_ref().unwrap();
    let protocol = ratatui_image::sliced::SlicedProtocol::new_with_resize(
        picker,
        image::DynamicImage::new_rgb8(1, 1),
        ratatui::layout::Size::new(10, 4),
        ratatui_image::Resize::Fit(None),
    )
    .unwrap();
    app.inline_image_protocols
        .get_mut()
        .insert(media.clone(), protocol);

    app.refresh_detail();

    assert!(
        app.inline_image_protocols.borrow().is_empty(),
        "a stale encoded protocol from a previous issue must not survive a detail refresh, \
         or a different issue's same-alt image could render the old picture"
    );
}
