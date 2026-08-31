//! Inline description image resolution/cache/fetch (issue #130 phase 1,
//! `images` feature only). Pure `resolve_inline_images_with_candidates` tests need no
//! `App`; the rest mirror `attachments.rs`'s own
//! `attachment_preview_drops_a_stale_response_after_moving_again`/
//! `refresh_detail_invalidates_a_cached_attachment_preview` shape, just
//! against a map cache instead of a single slot.

use serde_json::json;

use crate::adf;
use crate::domain::{Attachment, Comment, IssueDetail};

use super::super::inline_images::{resolve_inline_images_with_candidates, whole_line_media_url};
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
/// `resolve_inline_images_with_candidates`, so the demo fallback's defaults are fine as-is.
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
    let content_url = attachment.content_url.clone();
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "mediaSingle", "content": [ media_node("mockup.png") ] } ]
    });
    let detail = detail_with(description, vec![attachment]);

    let resolved = resolve_inline_images_with_candidates(&detail).0;

    assert_eq!(
        resolved,
        vec![(InlineImageKey::Attachment("10001".into()), content_url)],
        "content_url must be preferred over thumbnail_url — an inline preview should show the \
         real image, not Jira's deliberately-small thumbnail (see Attachment::image_preview_url)"
    );
}

#[test]
fn resolves_to_content_url_when_there_is_no_thumbnail_either() {
    let attachment = image_attachment("10001", "mockup.png", "image/png", None);
    let content_url = attachment.content_url.clone();
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ media_node("mockup.png") ]
    });
    let detail = detail_with(description, vec![attachment]);

    let resolved = resolve_inline_images_with_candidates(&detail).0;

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

    assert!(resolve_inline_images_with_candidates(&detail).0.is_empty());
}

#[test]
fn no_match_when_the_matched_attachment_is_not_an_image() {
    let attachment = image_attachment("10002", "notes.pdf", "application/pdf", None);
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ media_node("notes.pdf") ]
    });
    let detail = detail_with(description, vec![attachment]);

    assert!(resolve_inline_images_with_candidates(&detail).0.is_empty());
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

    let resolved = resolve_inline_images_with_candidates(&detail).0;

    assert_eq!(
        resolved.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
        vec![
            InlineImageKey::Attachment("1".into()),
            InlineImageKey::Attachment("2".into()),
        ]
    );
}

/// Code-review regression test: `adf::render_table`'s cell content is
/// text-only (`cell_content_spans`/`collect_text` never touch a `media`
/// node), so a media node nested in a table cell can never actually be
/// painted — eagerly resolving/fetching it here would only waste a slot in
/// `MAX_INLINE_IMAGES` on an image that never renders, and could starve out
/// a genuinely visible image found later in the same document.
#[test]
fn a_media_node_inside_a_table_cell_is_never_resolved() {
    let attachment = image_attachment("10001", "mockup.png", "image/png", None);
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "table", "content": [
            { "type": "tableRow", "content": [
                { "type": "tableCell", "content": [
                    { "type": "mediaSingle", "content": [ media_node("mockup.png") ] }
                ] }
            ] }
        ] } ]
    });
    let detail = detail_with(description, vec![attachment]);

    assert!(
        resolve_inline_images_with_candidates(&detail).0.is_empty(),
        "a table-cell-nested media node must never be resolved — render_table can't paint it"
    );
}

/// Code-review regression test: a list item only routes a *nested* list back
/// through `adf::render_block` (`render_list_item`); every other child —
/// including a `mediaSingle`/`media` node directly inside the item — goes
/// through the text-only `inline_spans`, so it can never render either. Same
/// waste-of-fetch-budget concern as the table-cell case above.
#[test]
fn a_media_node_directly_inside_a_list_item_is_never_resolved() {
    let attachment = image_attachment("10001", "mockup.png", "image/png", None);
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "bulletList", "content": [
            { "type": "listItem", "content": [
                { "type": "mediaSingle", "content": [ media_node("mockup.png") ] }
            ] }
        ] } ]
    });
    let detail = detail_with(description, vec![attachment]);

    assert!(
        resolve_inline_images_with_candidates(&detail).0.is_empty(),
        "a media node directly inside a list item must never be resolved — \
         render_list_item routes it through the text-only inline_spans, not render_block"
    );
}

/// A media node inside a *nested* list (a sub-list within a list item) is
/// just as unreachable as one directly inside a top-level item:
/// `render_list_item` only ever forwards a list item's immediate child back
/// through `render_block` when that child is itself a further-nested list —
/// the media node's own immediate container is never a list type, so it
/// still ends up routed through the text-only `inline_spans` at whatever
/// depth it's found, and must not be resolved either.
#[test]
fn a_media_node_inside_a_nested_list_is_also_never_resolved() {
    let attachment = image_attachment("10001", "mockup.png", "image/png", None);
    let description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "bulletList", "content": [
            { "type": "listItem", "content": [
                { "type": "bulletList", "content": [
                    { "type": "listItem", "content": [
                        { "type": "mediaSingle", "content": [ media_node("mockup.png") ] }
                    ] }
                ] }
            ] }
        ] } ]
    });
    let detail = detail_with(description, vec![attachment]);

    assert!(
        resolve_inline_images_with_candidates(&detail).0.is_empty(),
        "a media node inside a nested list must never be resolved either — its own immediate \
         container is never itself a list type, so render_list_item's nested-list exception \
         never applies to it, at any depth"
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

    let resolved = resolve_inline_images_with_candidates(&detail).0;

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
        resolve_inline_images_with_candidates(&detail).0,
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

    let resolved = resolve_inline_images_with_candidates(&detail).0;

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

    let resolved = resolve_inline_images_with_candidates(&detail).0;

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

    let resolved = resolve_inline_images_with_candidates(&detail).0;

    assert_eq!(
        resolved,
        vec![(InlineImageKey::Attachment("10001".into()), content_url)]
    );
}

fn comment_with(id: &str, body: serde_json::Value) -> Comment {
    Comment {
        id: id.into(),
        author: "someone".into(),
        created: "1h ago".into(),
        body,
    }
}

/// A comment's own body is walked the same way description/acceptance
/// criteria already are — closing the gap this issue's comment-image phase
/// exists to fix.
#[test]
fn comment_media_nodes_are_also_resolved() {
    let attachment = image_attachment("10001", "comment-shot.png", "image/png", None);
    let content_url = attachment.content_url.clone();
    let mut detail = detail_with(
        json!({ "type": "doc", "version": 1, "content": [] }),
        vec![attachment],
    );
    detail.comments = vec![comment_with(
        "1",
        json!({
            "type": "doc", "version": 1,
            "content": [ media_node("comment-shot.png") ]
        }),
    )];

    let resolved = resolve_inline_images_with_candidates(&detail).0;

    assert_eq!(
        resolved,
        vec![(InlineImageKey::Attachment("10001".into()), content_url)]
    );
}

/// The `MAX_INLINE_IMAGES` cap combines *all three* sources — description,
/// acceptance criteria, and comments — into one shared budget, not a
/// separate cap per source: 2 images in the description, 2 in acceptance
/// criteria, and 3 spread one-per-comment across three comments (6 total
/// candidates before the 7th, in the third/chronologically-last comment)
/// only resolves the first `MAX_INLINE_IMAGES` (6) of them, walked in
/// description → acceptance criteria → comments order, oldest comment
/// first (matching `IssueDetail::comments`' own doc comment and how the
/// activity feed already renders them).
#[test]
fn the_cap_combines_description_acceptance_criteria_and_comments() {
    let attachments: Vec<Attachment> = (1..=7)
        .map(|n| image_attachment(&n.to_string(), &format!("img{n}.png"), "image/png", None))
        .collect();
    let mut detail = detail_with(
        json!({
            "type": "doc", "version": 1,
            "content": [ media_node("img1.png"), media_node("img2.png") ]
        }),
        attachments,
    );
    detail.acceptance_criteria = Some(json!({
        "type": "doc", "version": 1,
        "content": [ media_node("img3.png"), media_node("img4.png") ]
    }));
    detail.comments = vec![
        comment_with(
            "1",
            json!({ "type": "doc", "version": 1, "content": [ media_node("img5.png") ] }),
        ),
        comment_with(
            "2",
            json!({ "type": "doc", "version": 1, "content": [ media_node("img6.png") ] }),
        ),
        comment_with(
            "3",
            json!({ "type": "doc", "version": 1, "content": [ media_node("img7.png") ] }),
        ),
    ];

    let resolved = resolve_inline_images_with_candidates(&detail).0;

    assert_eq!(
        resolved.len(),
        super::super::inline_images::MAX_INLINE_IMAGES
    );
    assert_eq!(
        resolved.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
        vec![
            InlineImageKey::Attachment("1".into()),
            InlineImageKey::Attachment("2".into()),
            InlineImageKey::Attachment("3".into()),
            InlineImageKey::Attachment("4".into()),
            InlineImageKey::Attachment("5".into()),
            InlineImageKey::Attachment("6".into()),
        ],
        "img7 (in the third, chronologically-last comment) must not resolve — the cap is shared \
         across description + acceptance criteria + comments combined, not per-source, and \
         comments are only reached after description and acceptance criteria have had first \
         claim on the shared budget"
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
    // The baked-in demo comment carries its own unrelated media node (see
    // `demo::demo_detail`'s comment fixture) — cleared here so it can't
    // also become a uuid-probe candidate (`resolve_inline_images_with_candidates`)
    // and race an extra `InlineImageUuidsResolved` against the single
    // `InlineImageLoaded` this test expects to drain.
    detail.comments = vec![];
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
    // See the same clearing in `opening_an_issue_with_a_resolvable_image_dispatches_a_fetch_that_lands_in_the_cache`
    // above — the baked-in demo comment's own media node would otherwise
    // race an extra `InlineImageUuidsResolved` event against this test's
    // single expected `InlineImageLoaded`.
    detail.comments = vec![];
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
    // See the same clearing in `opening_an_issue_with_a_resolvable_image_dispatches_a_fetch_that_lands_in_the_cache`
    // above — the baked-in demo comment's own media node would otherwise
    // dispatch an extra uuid-probe fetch, and this test's fixed two-`next_event`
    // sequence (drain the inline-image dispatch, then drain the refresh's
    // own response) assumes exactly one dispatch per step.
    detail.comments = vec![];
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

/// A media node with no `alt` (or an `alt` matching nothing) still gets a
/// uuid-probe fetch dispatched, as long as at least one image attachment
/// exists to possibly match against — the redirect-probe fallback
/// (`dispatch_uuid_resolve`) this session added for issue #130's DS-1880
/// follow-up. No live server is reachable here, so the probe itself always
/// comes back empty (`media_uuid_for` errors on the unreachable host and is
/// filtered out — see `resolve_uuids_blocking`), but that still proves the
/// dispatch is reached and its response applied without panicking, rather
/// than silently skipped.
#[tokio::test]
async fn an_unmatched_media_node_dispatches_a_uuid_probe_when_an_image_attachment_exists() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let summary = app.issues[0].clone();
    let key = summary.key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.attachments = vec![image_attachment("10001", "mockup.png", "image/png", None)];
    detail.description = json!({
        "type": "doc", "version": 1,
        "content": [ { "type": "media", "attrs": {
            "id": "some-media-uuid", "type": "file"
        } } ]
    });
    detail.comments = vec![];
    app.locally_created
        .push(LocallyCreatedIssue { summary, detail });

    app.open_by_key(&key);
    let generation = app.inline_image_generation;

    let event = next_event(&mut app).await;
    match event {
        AppEvent::InlineImageUuidsResolved {
            generation: g,
            resolved,
        } => {
            assert_eq!(g, generation);
            assert!(
                resolved.is_empty(),
                "no live server is reachable, so the probe can never actually match"
            );
        }
        _ => panic!("expected InlineImageUuidsResolved, got a different event"),
    }

    // Applying it must not panic even though nothing resolved.
    app.apply_event(AppEvent::InlineImageUuidsResolved {
        generation,
        resolved: vec![],
    });
    assert!(app.inline_images.borrow().is_empty());
}

/// The other half of DS-1880's fix, once the async round trip above has
/// landed: `App::apply_inline_image_uuids_resolved` records `uuid -> key`
/// in `self.inline_image_uuid_matches`, and `App::inline_image_key_for`
/// (reached via `with_detail_media_sizing`'s `Ready` callback) must
/// actually use it — before this, a media node with no `alt` short-circuited
/// straight to `[embedded media]` with no readiness lookup attempted at
/// all, no matter what was cached.
#[test]
fn a_uuid_resolved_media_node_reports_ready_once_the_apply_lands() {
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let mut detail = crate::domain::demo_detail(&app.issues[0].key);
    detail.attachments = vec![image_attachment("10001", "mockup.png", "image/png", None)];
    detail.comments = vec![];
    app.detail = Some(detail);

    // Seeded before `apply_event` so its already-cached check skips
    // dispatching a real byte-fetch (which would need a Tokio runtime this
    // plain `#[test]` doesn't have) — isolates the render-side lookup this
    // test actually cares about from the fetch pipeline, which is already
    // covered by `an_unmatched_media_node_dispatches_a_uuid_probe_when_an_image_attachment_exists`.
    app.inline_images.borrow_mut().insert(
        InlineImageKey::Attachment("10001".into()),
        image::DynamicImage::new_rgb8(10, 10),
    );
    app.apply_event(AppEvent::InlineImageUuidsResolved {
        generation: app.inline_image_generation,
        resolved: vec![(
            "some-media-uuid".into(),
            InlineImageKey::Attachment("10001".into()),
            "https://example.atlassian.net/secure/attachment/10001/mockup.png".into(),
        )],
    });

    let media = adf::InlineMediaRef {
        alt: String::new(),
        url: None,
        id: Some("some-media-uuid".into()),
    };
    let sized = app.with_detail_media_sizing(80, |sizing| match sizing {
        adf::MediaSizing::Ready(ready) => ready(&media),
        adf::MediaSizing::Disabled => None,
    });

    assert!(
        sized.is_some(),
        "a media node with no alt must still resolve via its own uuid once \
         the redirect-probe fallback has matched it"
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
/// `inline_image_protocols` (the *encoded* `SlicedProtocol` cache) behind —
/// and, until keyed by `InlineImageKey` (a further code-review fix), that
/// cache was keyed only by a media node's `alt` text, with no issue
/// component at all, so a stale entry left over from a previously-viewed
/// issue whose inline image happened to share the same `alt` (e.g. a common
/// filename like "screenshot.png") would be returned unchanged for a
/// *different* attachment in the new issue by `App::sliced_inline_image_protocol`'s
/// size-only staleness check, silently rendering the wrong picture. This
/// confirms both caches are cleared together, even now that the key itself
/// (attachment id) already rules out a cross-attachment collision.
#[test]
fn refresh_detail_also_clears_the_stale_sliced_protocol_cache() {
    let mut app = demo_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    app.selected = 0;
    app.open_detail();
    let key = InlineImageKey::Attachment("shared-attachment-id".into());
    let picker = app.image_picker.as_ref().unwrap();
    let protocol = ratatui_image::sliced::SlicedProtocol::new_with_resize(
        picker,
        image::DynamicImage::new_rgb8(1, 1),
        ratatui::layout::Size::new(10, 4),
        ratatui_image::Resize::Fit(None),
    )
    .unwrap();
    app.inline_image_protocols.get_mut().insert(key, protocol);

    app.refresh_detail();

    assert!(
        app.inline_image_protocols.borrow().is_empty(),
        "a stale encoded protocol from a previous issue must not survive a detail refresh, \
         or a different issue's same-alt image could render the old picture"
    );
}

/// `whole_line_media_url` (the in-TUI editor's image view detection —
/// `ui::editor`'s render pass and `App::refresh_editor_inline_images` both
/// rely on it agreeing on exactly which lines are image tokens) recognizes
/// a line that's nothing but one `![alt](adf-media://…)` token, trimming
/// incidental leading/trailing whitespace.
#[test]
fn whole_line_media_url_detects_a_lone_token_line() {
    assert_eq!(
        whole_line_media_url("![mockup.png](adf-media://file/x?alt=mockup.png)"),
        Some("adf-media://file/x?alt=mockup.png")
    );
    assert_eq!(
        whole_line_media_url("  ![mockup.png](adf-media://file/x?alt=mockup.png)  "),
        Some("adf-media://file/x?alt=mockup.png"),
        "incidental surrounding whitespace must not defeat the match"
    );
}

/// A line carrying anything besides the token — other prose, or a plain
/// (non-`adf-media://`) image a human typed — is never treated as a whole
/// -line image; the editor's image view only ever replaces a whole line,
/// never splices an image into running text.
#[test]
fn whole_line_media_url_rejects_anything_that_is_not_purely_the_token() {
    assert_eq!(
        whole_line_media_url("see the screenshot: ![mockup.png](adf-media://file/x)"),
        None
    );
    assert_eq!(
        whole_line_media_url("![a plain image](https://example.com/pic.png)"),
        None,
        "a plain Markdown image a human typed is not one of this crate's own tokens"
    );
    assert_eq!(whole_line_media_url("just some text"), None);
}

/// Switching the in-TUI editor into image view (`App::toggle_editor_image_view`)
/// scans the buffer for whole-line `adf-media://` tokens and dispatches a
/// fetch for each resolvable one — sharing the exact same
/// `inline_images`/dispatch machinery `App::refresh_inline_images` (Detail)
/// already exercises above, just sourced from `self.editor.lines` instead
/// of `detail.description`. Mirrors
/// `opening_an_issue_with_a_resolvable_image_dispatches_a_fetch_that_lands_in_the_cache`.
#[tokio::test]
async fn toggling_editor_image_view_on_dispatches_a_fetch_for_a_resolvable_token() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let summary = app.issues[0].clone();
    let key = summary.key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.attachments = vec![image_attachment("10001", "mockup.png", "image/png", None)];
    // An empty description/no comments, so `open_by_key`'s own
    // `refresh_inline_images` (Detail's description-driven fetch) finds
    // nothing and dispatches nothing — isolating this test to only the
    // editor buffer's own token, so exactly one event lands on the channel.
    detail.description = json!({ "type": "doc", "version": 1, "content": [] });
    detail.acceptance_criteria = None;
    detail.comments = vec![];
    app.locally_created
        .push(LocallyCreatedIssue { summary, detail });

    app.open_by_key(&key);
    app.begin_tui_edit();
    app.editor.lines = vec![
        "Here is a screenshot:".into(),
        "![mockup.png](adf-media://file/x?alt=mockup.png)".into(),
    ];
    let generation = app.inline_image_generation;

    app.toggle_editor_image_view();
    assert!(app.editor_image_view, "the toggle itself must flip on");

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
}

/// Toggling back off needs no refresh of its own (it just stops consulting
/// the cache — see `App::toggle_editor_image_view`'s own doc comment), and
/// toggling on with nothing resolvable in the buffer dispatches nothing.
#[test]
fn toggling_editor_image_view_with_no_resolvable_token_dispatches_nothing() {
    let mut app = live_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let summary = app.issues[0].clone();
    let key = summary.key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.description = json!({ "type": "doc", "version": 1, "content": [] });
    detail.acceptance_criteria = None;
    detail.comments = vec![];
    app.locally_created
        .push(LocallyCreatedIssue { summary, detail });

    app.open_by_key(&key);
    app.begin_tui_edit();
    app.editor.lines = vec!["just some plain Markdown, no tokens here".into()];

    app.toggle_editor_image_view();
    assert!(app.editor_image_view);
    assert!(
        app.events_rx.try_recv().is_err(),
        "nothing resolvable in the buffer should dispatch nothing"
    );

    app.toggle_editor_image_view();
    assert!(!app.editor_image_view, "a second toggle flips back off");
}

/// A demo/cache session's editor never dispatches an inline-image fetch
/// either, mirroring `a_demo_session_never_dispatches_an_inline_image_fetch`
/// — `images_eligible` gates on `Source::Live` before any fetch is even
/// considered, regardless of which screen triggered the scan.
#[test]
fn a_demo_session_editor_toggle_never_dispatches_a_fetch() {
    let mut app = demo_app();
    app.image_picker = Some(ratatui_image::picker::Picker::halfblocks());
    let summary = app.issues[0].clone();
    let key = summary.key.clone();
    let mut detail = crate::domain::demo_detail(&key);
    detail.attachments = vec![image_attachment("10001", "mockup.png", "image/png", None)];
    detail.description = json!({ "type": "doc", "version": 1, "content": [] });
    detail.acceptance_criteria = None;
    detail.comments = vec![];
    app.locally_created
        .push(LocallyCreatedIssue { summary, detail });

    app.open_by_key(&key);
    app.begin_tui_edit();
    app.editor.lines = vec!["![mockup.png](adf-media://file/x?alt=mockup.png)".into()];

    app.toggle_editor_image_view();

    assert!(app.inline_images.borrow().is_empty());
    assert!(
        app.events_rx.try_recv().is_err(),
        "no fetch should ever be dispatched for a demo/cache session"
    );
}
