//! Issue-mutation dispatch: transitions, assignment, description updates,
//! and comments. Each is a `dispatch_*`/`*_blocking` pair with no mutual
//! dependencies, so they move here verbatim.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::{Attachment, Comment};

use super::super::{App, ReleaseBulkKind, Screen};
use super::AppEvent;

/// Spawn a workflow transition off the render thread, sending the result
/// back as `AppEvent::TransitionApplied`.
pub(crate) fn dispatch_transition(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    transition_id: String,
    to: String,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let to_for_result = to.clone();
        let error =
            tokio::task::spawn_blocking(move || apply_transition_blocking(&key, &transition_id))
                .await
                .unwrap_or_else(|_| Some("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::TransitionApplied {
            generation,
            key: key_for_result,
            to: to_for_result,
            error,
        });
    });
}

/// Mirrors the live branch of the old synchronous `confirm_transition`: no
/// credentials/config means "nothing to do live", not an error.
#[allow(unused_variables)]
fn apply_transition_blocking(key: &str, transition_id: &str) -> Option<String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::apply_transition(&cfg, key, transition_id)
                .err()
                .map(|e| e.to_string());
        }
    }
    None
}

/// Spawn an assignee change off the render thread, sending the result back
/// as `AppEvent::AssigneeApplied`. `account_id`/`display_name` are both
/// `None` together to unassign, or both `Some` to assign to a specific
/// teammate — mirrors `dispatch_transition`'s shape.
pub(crate) fn dispatch_assign(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    account_id: Option<String>,
    display_name: Option<String>,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let display_name_for_result = display_name.clone();
        let error =
            tokio::task::spawn_blocking(move || assign_issue_blocking(&key, account_id.as_deref()))
                .await
                .unwrap_or_else(|_| Some("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::AssigneeApplied {
            generation,
            key: key_for_result,
            display_name: if error.is_none() {
                display_name_for_result
            } else {
                None
            },
            error,
        });
    });
}

/// Mirrors `apply_transition_blocking`'s "no credentials means nothing to do
/// live" shape.
#[allow(unused_variables)]
fn assign_issue_blocking(key: &str, account_id: Option<&str>) -> Option<String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::assign_issue(&cfg, key, account_id)
                .err()
                .map(|e| e.to_string());
        }
    }
    None
}

/// Spawn a Fix/Affects Version update off the render thread, sending the
/// result back as `AppEvent::VersionsApplied`. `fix_versions`/
/// `affects_versions` are each `None` when that field is unchanged in the
/// picker (see `App::confirm_version_picker`) — `set_versions_blocking`
/// skips a field entirely rather than sending a needless write for it.
pub(crate) fn dispatch_set_versions(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    fix_versions: Option<Vec<String>>,
    affects_versions: Option<Vec<String>>,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let fix_for_result = fix_versions.clone();
        let affects_for_result = affects_versions.clone();
        let (fix_error, affects_error) = tokio::task::spawn_blocking(move || {
            set_versions_blocking(&key, fix_versions.as_deref(), affects_versions.as_deref())
        })
        .await
        .unwrap_or_else(|_| {
            (
                Some("internal error: task panicked".into()),
                Some("internal error: task panicked".into()),
            )
        });
        let _ = tx.send(AppEvent::VersionsApplied {
            generation,
            key: key_for_result,
            fix_versions: if fix_error.is_none() {
                fix_for_result
            } else {
                None
            },
            fix_error,
            affects_versions: if affects_error.is_none() {
                affects_for_result
            } else {
                None
            },
            affects_error,
        });
    });
}

/// Mirrors `apply_transition_blocking`'s "no credentials means nothing to do
/// live" shape, for both fields independently — a `None` field is skipped
/// entirely rather than sent as an empty write.
#[allow(unused_variables)]
fn set_versions_blocking(
    key: &str,
    fix_versions: Option<&[String]>,
    affects_versions: Option<&[String]>,
) -> (Option<String>, Option<String>) {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            let fix_error = fix_versions.and_then(|v| {
                crate::jira::set_fix_versions(&cfg, key, v)
                    .err()
                    .map(|e| e.to_string())
            });
            let affects_error = affects_versions.and_then(|v| {
                crate::jira::set_affects_versions(&cfg, key, v)
                    .err()
                    .map(|e| e.to_string())
            });
            return (fix_error, affects_error);
        }
    }
    (None, None)
}

/// Spawn a bulk add-to-release or remove-from-release off the render
/// thread, sending the result back as `AppEvent::ReleaseBulkApplied`. See
/// `App::release_remove_selected`/`release_add_to_release` for the two
/// call sites.
pub(crate) fn dispatch_release_bulk(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    version_name: String,
    keys: Vec<String>,
    kind: ReleaseBulkKind,
) {
    tokio::spawn(async move {
        let version_for_result = version_name.clone();
        let results =
            tokio::task::spawn_blocking(move || release_bulk_blocking(&version_name, &keys, kind))
                .await
                .unwrap_or_default();
        let _ = tx.send(AppEvent::ReleaseBulkApplied {
            generation,
            version_name: version_for_result,
            kind,
            results,
        });
    });
}

/// For each key: fetch its current `fixVersions` (needed so add/remove only
/// touches `version_name`, preserving any other release the issue already
/// targets — Jira has no add/remove endpoint, only "replace the whole
/// array"), edit it, and write the result back. One issue's failure doesn't
/// stop the rest — each gets its own `Result` in the returned `Vec`.
#[allow(unused_variables)]
fn release_bulk_blocking(
    version_name: &str,
    keys: &[String],
    kind: ReleaseBulkKind,
) -> Vec<(String, Result<(), String>)> {
    #[cfg(feature = "live")]
    {
        let Some(cfg) = crate::jira::Config::load() else {
            return keys
                .iter()
                .map(|k| (k.clone(), Err("no credentials configured".to_string())))
                .collect();
        };
        keys.iter()
            .map(|key| {
                let outcome = (|| {
                    let detail = crate::jira::fetch_detail(&cfg, key).map_err(|e| e.to_string())?;
                    let mut versions = detail.fix_versions;
                    match kind {
                        ReleaseBulkKind::Add => {
                            if !versions.iter().any(|v| v == version_name) {
                                versions.push(version_name.to_string());
                            }
                        }
                        ReleaseBulkKind::Remove => versions.retain(|v| v != version_name),
                    }
                    crate::jira::set_fix_versions(&cfg, key, &versions).map_err(|e| e.to_string())
                })();
                (key.clone(), outcome)
            })
            .collect()
    }
    #[cfg(not(feature = "live"))]
    Vec::new()
}

/// Spawn a description update off the render thread, sending the result
/// back as `AppEvent::DescriptionUpdated`.
pub(crate) fn dispatch_update_description(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    adf: serde_json::Value,
    return_screen: Screen,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let adf_for_result = adf.clone();
        let error = tokio::task::spawn_blocking(move || update_description_blocking(&key, &adf))
            .await
            .unwrap_or_else(|_| Some("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::DescriptionUpdated {
            generation,
            key: key_for_result,
            adf: adf_for_result,
            error,
            return_screen,
        });
    });
}

#[allow(unused_variables)]
fn update_description_blocking(key: &str, adf: &serde_json::Value) -> Option<String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::update_description(&cfg, key, adf)
                .err()
                .map(|e| e.to_string());
        }
    }
    None
}

/// Spawn a new-comment post off the render thread, sending the result back
/// as `AppEvent::CommentAdded`. `local_author`/`local_id` seed the
/// locally-composed fallback comment used when there's no live client to
/// post to (mirrors the old synchronous behaviour, which always built this
/// optimistic comment before possibly overwriting it with the server's
/// copy).
pub(crate) fn dispatch_add_comment(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    adf: serde_json::Value,
    local_author: String,
    local_id: String,
    return_screen: Screen,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let result = tokio::task::spawn_blocking(move || {
            add_comment_blocking(&key, &adf, &local_author, &local_id)
        })
        .await
        .unwrap_or_else(|_| Err("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::CommentAdded {
            generation,
            key: key_for_result,
            result,
            return_screen,
        });
    });
}

#[allow(unused_variables)]
fn add_comment_blocking(
    key: &str,
    adf: &serde_json::Value,
    local_author: &str,
    local_id: &str,
) -> Result<Comment, String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::add_comment(&cfg, key, adf).map_err(|e| e.to_string());
        }
    }
    Ok(Comment {
        id: local_id.to_string(),
        author: local_author.to_string(),
        created: "just now".into(),
        body: adf.clone(),
    })
}

/// Spawn a new-issue creation off the render thread, sending the result back
/// as `AppEvent::IssueCreated`. `local_key` is the key `create_issue_blocking`
/// falls back to if there's no live config to actually create against — the
/// same "second safety net" shape as `add_comment_blocking`'s optimistic
/// local comment, precomputed by the caller (`App::apply_new_issue`, via
/// `next_local_key`) since only it knows the session's local-key counter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_create_issue(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    project: String,
    issue_type: String,
    summary: String,
    description: Option<serde_json::Value>,
    local_key: String,
) {
    tokio::spawn(async move {
        let issue_type_for_result = issue_type.clone();
        let summary_for_result = summary.clone();
        let description_for_result = description.clone();
        let result = tokio::task::spawn_blocking(move || {
            create_issue_blocking(
                &project,
                &issue_type,
                &summary,
                description.as_ref(),
                &local_key,
            )
        })
        .await
        .unwrap_or_else(|_| Err("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::IssueCreated {
            generation,
            issue_type: issue_type_for_result,
            summary: summary_for_result,
            description: description_for_result,
            result,
        });
    });
}

#[allow(unused_variables)]
fn create_issue_blocking(
    project: &str,
    issue_type: &str,
    summary: &str,
    description: Option<&serde_json::Value>,
    local_key: &str,
) -> Result<String, String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::create_issue(&cfg, project, summary, issue_type, description)
                .map_err(|e| e.to_string());
        }
    }
    Ok(local_key.to_string())
}

/// Spawn an attachment download off the render thread, sending the result
/// back as `AppEvent::AttachmentDownloaded`. Only ever dispatched for a
/// genuine `Source::Live` session (see `App::download_selected_attachment`),
/// but — like every other `dispatch_*` here — compiles unconditionally,
/// gating the actual network call inside the blocking half.
pub(crate) fn dispatch_attachment_download(
    tx: UnboundedSender<AppEvent>,
    key: String,
    filename: String,
    content_url: String,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let filename_for_result = filename.clone();
        let result = tokio::task::spawn_blocking(move || {
            download_attachment_blocking(&filename, &content_url)
        })
        .await
        .unwrap_or_else(|_| Err("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::AttachmentDownloaded {
            key: key_for_result,
            filename: filename_for_result,
            result,
        });
    });
}

/// Fetches `content_url`'s bytes, sanitizes `filename` to a safe on-disk
/// basename (`attachments::sanitize_attachment_filename` — the API response
/// is untrusted input), de-dupes it against the current working directory
/// (`attachments::dedupe_filename`), and writes the bytes there. Returns the
/// saved path as a display string.
#[allow(unused_variables)]
fn download_attachment_blocking(filename: &str, content_url: &str) -> Result<String, String> {
    #[cfg(feature = "live")]
    {
        let cfg =
            crate::jira::Config::load().ok_or_else(|| "no credentials configured".to_string())?;
        let bytes =
            crate::jira::download_attachment(&cfg, content_url).map_err(|e| e.to_string())?;
        let safe_name = super::super::attachments::sanitize_attachment_filename(filename);
        let dir = std::env::current_dir().map_err(|e| e.to_string())?;
        let path = super::super::attachments::dedupe_filename(&dir, &safe_name);
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    }
    #[cfg(not(feature = "live"))]
    Err("this build has no live support".to_string())
}

/// Spawn an attachment upload off the render thread, sending the result
/// back as `AppEvent::AttachmentUploaded`. Only ever dispatched for a
/// genuine `Source::Live` session (see `App::confirm_attachment_upload`),
/// but — like every other `dispatch_*` here — compiles unconditionally,
/// gating the actual network call inside the blocking half. `path` is
/// already fully resolved (`~`-expanded) by the caller; its bytes are read
/// inside the `spawn_blocking` closure, not on the render thread, so a
/// large attachment's file I/O can't stall rendering any more than the
/// upload request itself can.
pub(crate) fn dispatch_attachment_upload(
    tx: UnboundedSender<AppEvent>,
    key: String,
    path: std::path::PathBuf,
    filename: String,
    mime: &'static str,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let filename_for_result = filename.clone();
        let result = tokio::task::spawn_blocking(move || {
            upload_attachment_blocking(&key, &path, &filename, mime)
        })
        .await
        .unwrap_or_else(|_| Err("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::AttachmentUploaded {
            key: key_for_result,
            filename: filename_for_result,
            result,
        });
    });
}

/// Reads `path`'s bytes and POSTs them to `key`'s attachments endpoint,
/// returning Jira's response (the newly-created attachment(s), see
/// `jira::live::attachments::upload_attachment`'s own doc comment).
#[allow(unused_variables)]
fn upload_attachment_blocking(
    key: &str,
    path: &std::path::Path,
    filename: &str,
    mime: &str,
) -> Result<Vec<Attachment>, String> {
    #[cfg(feature = "live")]
    {
        let cfg =
            crate::jira::Config::load().ok_or_else(|| "no credentials configured".to_string())?;
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        crate::jira::upload_attachment(&cfg, key, filename, mime, &bytes).map_err(|e| e.to_string())
    }
    #[cfg(not(feature = "live"))]
    Err("this build has no live support".to_string())
}

/// Spawn an attachment preview image fetch+decode off the render thread
/// (`images` feature only), sending the result back as
/// `AppEvent::AttachmentPreviewLoaded`. Mirrors `dispatch_attachment_download`'s
/// shape — the network fetch runs inside `spawn_blocking` since the Jira
/// client is synchronous `ureq` — but the CPU-bound `image::load_from_memory`
/// decode step is folded into the same blocking closure too, rather than
/// happening back on the event/render thread. The resulting
/// `StatefulProtocol` is deliberately *not* constructed here: that needs
/// `App::image_picker`, which only exists on the render thread's `App`, so
/// construction happens at apply-time instead (see
/// `App::apply_attachment_preview_loaded`).
#[cfg(feature = "images")]
pub(crate) fn dispatch_attachment_preview(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    attachment_id: String,
    url: String,
) {
    tokio::spawn(async move {
        let id_for_result = attachment_id.clone();
        let image = tokio::task::spawn_blocking(move || fetch_attachment_preview_blocking(&url))
            .await
            .unwrap_or(None);
        let _ = tx.send(AppEvent::AttachmentPreviewLoaded {
            generation,
            attachment_id: id_for_result,
            image,
        });
    });
}

/// Downloads `url`'s bytes (`crate::jira::download_attachment`, which is
/// really just a thin wrapper over the same `get_bytes` helper
/// `download_attachment_blocking` above uses — it happens to already accept
/// any absolute URL, not just a `content_url`, so it works unchanged for a
/// `thumbnail_url` too) and decodes them. Any failure — no credentials, a
/// network error, or bytes that don't decode as a supported image format —
/// collapses to `None` rather than propagating an error: a failed preview
/// fetch is never worth surfacing to the user, it just means the picker
/// falls back to its normal metadata + placeholder rendering. Decoded
/// images are downscaled (`downscale_for_preview`) before being handed
/// back, bounding how much memory even a huge source image can occupy for
/// what's only ever rendered at a handful of terminal cells.
#[cfg(feature = "images")]
#[allow(unused_variables)]
fn fetch_attachment_preview_blocking(url: &str) -> Option<image::DynamicImage> {
    #[cfg(feature = "live")]
    {
        let cfg = crate::jira::Config::load()?;
        let bytes = crate::jira::download_attachment(&cfg, url).ok()?;
        image::load_from_memory(&bytes)
            .ok()
            .map(downscale_for_preview)
    }
    #[cfg(not(feature = "live"))]
    None
}

/// Issue #130 phase 4's `External`-keyed sibling of
/// `fetch_attachment_preview_blocking` above: fetches `url` (an ADF `media`
/// node's `type: "external"` `url`, potentially any third-party host) via
/// `jira::get_bytes_public` instead of the authenticated attachment
/// pipeline — no `Config` is loaded or needed at all, since there's no
/// Jira credential to apply to an arbitrary external host in the first
/// place (see `get_bytes_public`'s own doc comment for the full reasoning,
/// including its `https://`-only restriction). Same downscale step as the
/// attachment path — an external image is, if anything, more likely to be
/// oversized, since nothing about it is shaped by Jira's own attachment
/// handling.
#[cfg(feature = "images")]
#[allow(unused_variables)]
fn fetch_external_image_blocking(url: &str) -> Option<image::DynamicImage> {
    #[cfg(feature = "live")]
    {
        let bytes = crate::jira::get_bytes_public(url).ok()?;
        image::load_from_memory(&bytes)
            .ok()
            .map(downscale_for_preview)
    }
    #[cfg(not(feature = "live"))]
    None
}

/// Row/column-agnostic downscale cap applied to every decoded inline/preview
/// image before it's cached (`fetch_attachment_preview_blocking`/
/// `fetch_external_image_blocking` above) — an oversized `DynamicImage`
/// sitting in `App::inline_images`/`App::attachment_preview` for the life of
/// the session is a real memory cost for something that only ever renders
/// at a handful of terminal rows/cols (see `inline_images::MAX_INLINE_IMAGE_ROWS`),
/// so anything north of ~2 megapixels gets resized down before it's ever
/// stored, not just before it's painted. Applies to the attachment-picker's
/// single preview slot too, not just inline images — both share this same
/// decode step, and bounding memory there is equally worth having, not a
/// side effect to work around.
#[cfg(feature = "images")]
const MAX_PREVIEW_PIXELS: u32 = 2_000_000;

/// Resize `img` down to at most `MAX_PREVIEW_PIXELS` total pixels,
/// preserving aspect ratio, when it's bigger than that — a no-op otherwise.
/// `DynamicImage::resize` (rather than `resize_exact`, which would distort
/// the aspect ratio, or `thumbnail`, whose faster nearest-neighbour-ish
/// filter trades away more quality than this needs to) fits the image
/// within a same-aspect-ratio bounding box computed from the target pixel
/// count, using a `Triangle` (bilinear) filter — a reasonable quality/speed
/// tradeoff for a terminal preview image, not a final-quality asset.
/// `target_w`/`target_h` are floored rather than rounded: since both are
/// scaled down by the same factor, flooring each dimension independently
/// can only shrink their product relative to the exact (unrounded) target,
/// guaranteeing the result never rounds back up past `MAX_PREVIEW_PIXELS` —
/// rounding instead could overshoot the cap by a few dozen pixels.
#[cfg(feature = "images")]
fn downscale_for_preview(img: image::DynamicImage) -> image::DynamicImage {
    let (w, h) = (img.width().max(1), img.height().max(1));
    if (w as u64) * (h as u64) <= MAX_PREVIEW_PIXELS as u64 {
        return img;
    }
    let scale = (MAX_PREVIEW_PIXELS as f64 / (w as f64 * h as f64)).sqrt();
    let target_w = ((w as f64 * scale).floor() as u32).max(1);
    let target_h = ((h as f64 * scale).floor() as u32).max(1);
    img.resize(target_w, target_h, image::imageops::FilterType::Triangle)
}

/// Spawn an eager inline-image fetch+decode off the render thread (`images`
/// feature only), sending the result back as `AppEvent::InlineImageLoaded`.
/// One dispatch per resolved `(key, url)` pair from
/// `App::refresh_inline_images` — the byte-fetch-and-decode step branches on
/// `key`'s variant: `Attachment` reuses the existing authenticated
/// `fetch_attachment_preview_blocking` (it already takes nothing but a URL,
/// so there's no attachment-specific logic to parameterize around);
/// `External` (issue #130 phase 4) instead uses
/// `fetch_external_image_blocking`, which fetches credential-free via
/// `jira::get_bytes_public` rather than the authenticated attachment
/// pipeline. Either way the result comes back tagged with the same
/// `InlineImageKey` it was dispatched under.
#[cfg(feature = "images")]
pub(crate) fn dispatch_inline_image(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: super::super::InlineImageKey,
    url: String,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let image = tokio::task::spawn_blocking(move || match key {
            super::super::InlineImageKey::Attachment(_) => fetch_attachment_preview_blocking(&url),
            super::super::InlineImageKey::External(_) => fetch_external_image_blocking(&url),
        })
        .await
        .unwrap_or(None);
        let _ = tx.send(AppEvent::InlineImageLoaded {
            generation,
            key: key_for_result,
            image,
        });
    });
}

/// Spawn the redirect-probe uuid fallback off the render thread (`images`
/// feature only) for whatever candidates `resolve_inline_images_with_candidates`
/// couldn't resolve via `alt` matching — see `App::resolve_unmatched_media_by_uuid`,
/// which is this dispatch's only caller. Resolves *identity* only (which
/// candidate uuid belongs to which attachment); each resolved pair still
/// needs its own byte-fetch, which `App::apply_inline_image_uuids_resolved`
/// hands off to the existing `dispatch_inline_image` once this lands.
#[cfg(feature = "images")]
pub(crate) fn dispatch_uuid_resolve(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    candidates: Vec<String>,
    attachments: Vec<Attachment>,
) {
    tokio::spawn(async move {
        let resolved =
            tokio::task::spawn_blocking(move || resolve_uuids_blocking(&candidates, &attachments))
                .await
                .unwrap_or_default();
        let _ = tx.send(AppEvent::InlineImageUuidsResolved {
            generation,
            resolved,
        });
    });
}

/// Blocking body of `dispatch_uuid_resolve`: probes each image-mime
/// attachment's `content_url` redirect (`jira::media_uuid_for` — see that
/// fn's own doc comment for the mechanism, confirmed live on issue #122)
/// to build a `{uuid -> Attachment}` map, then looks up each of
/// `candidates` in it. Only image attachments are probed at all — a
/// candidate media node can never resolve to a non-image attachment (see
/// `resolve_inline_images_with_candidates`'s own alt-matching path, which
/// applies the same restriction), so probing one would just be a wasted
/// request that can never match. `None`/error from any single probe just
/// drops that attachment from the map — one attachment Jira won't redirect
/// for shouldn't block resolving the rest.
///
/// Temporary diagnostic: with `JIRA_TUI_DEBUG_MEDIA` set to anything, every
/// step (candidates in, each attachment's probe outcome, the resulting
/// uuid map, final match count) is traced to stderr — every failure mode
/// here otherwise collapses silently to "no match" by design (a failed
/// preview fetch is never worth surfacing to the user), which makes a
/// live-only mismatch like issue #130's DS-1880 follow-up otherwise
/// unobservable without this.
#[cfg(feature = "images")]
#[allow(unused_variables)]
fn resolve_uuids_blocking(
    candidates: &[String],
    attachments: &[Attachment],
) -> Vec<(super::super::InlineImageKey, String)> {
    #[cfg(feature = "live")]
    {
        let debug = std::env::var_os("JIRA_TUI_DEBUG_MEDIA").is_some();
        if debug {
            eprintln!(
                "[jira-tui] uuid-probe: {} candidate(s) {candidates:?}, {} attachment(s)",
                candidates.len(),
                attachments.len()
            );
        }
        let Some(cfg) = crate::jira::Config::load() else {
            if debug {
                eprintln!("[jira-tui] uuid-probe: no Config loaded, aborting");
            }
            return Vec::new();
        };
        let mut uuid_map: std::collections::HashMap<String, &Attachment> =
            std::collections::HashMap::new();
        for attachment in attachments {
            if !attachment.mime_type.starts_with("image/") {
                continue;
            }
            match crate::jira::media_uuid_for(&cfg, &attachment.content_url) {
                Ok(Some(uuid)) => {
                    if debug {
                        eprintln!(
                            "[jira-tui] uuid-probe: {:?} ({}) -> uuid {uuid}",
                            attachment.filename, attachment.content_url
                        );
                    }
                    uuid_map.insert(uuid, attachment);
                }
                Ok(None) => {
                    if debug {
                        eprintln!(
                            "[jira-tui] uuid-probe: {:?} ({}) -> no redirect (not a 3xx, or no Location header)",
                            attachment.filename, attachment.content_url
                        );
                    }
                }
                Err(e) => {
                    if debug {
                        eprintln!(
                            "[jira-tui] uuid-probe: {:?} ({}) -> error: {e}",
                            attachment.filename, attachment.content_url
                        );
                    }
                }
            }
        }
        if debug {
            eprintln!(
                "[jira-tui] uuid-probe: uuid map has {} entr{}: {:?}",
                uuid_map.len(),
                if uuid_map.len() == 1 { "y" } else { "ies" },
                uuid_map.keys().collect::<Vec<_>>()
            );
        }
        let resolved: Vec<_> = candidates
            .iter()
            .filter_map(|candidate| {
                let attachment = uuid_map.get(candidate)?;
                let url = attachment.image_preview_url()?;
                Some((
                    super::super::InlineImageKey::Attachment(attachment.id.clone()),
                    url,
                ))
            })
            .collect();
        if debug {
            eprintln!(
                "[jira-tui] uuid-probe: matched {}/{} candidate(s)",
                resolved.len(),
                candidates.len()
            );
        }
        resolved
    }
    #[cfg(not(feature = "live"))]
    Vec::new()
}

/// Merge `uploaded` (Jira's response to a successful `upload_attachment`
/// call) into `existing`: replacing any attachment that already shares an
/// id (re-uploading to an existing entry, which Jira allows) and appending
/// everything else. In practice `uploaded` is just the one just-uploaded
/// file, but the API returns an array, so this handles more than one just
/// as well.
fn merge_attachments(existing: &mut Vec<Attachment>, uploaded: &[Attachment]) {
    for a in uploaded {
        if let Some(slot) = existing.iter_mut().find(|e| e.id == a.id) {
            *slot = a.clone();
        } else {
            existing.push(a.clone());
        }
    }
}

impl App {
    /// Applies `AppEvent::TransitionApplied` — see `dispatch_transition` above.
    pub(super) fn apply_transition_applied(
        &mut self,
        generation: u64,
        key: String,
        to: String,
        error: Option<String>,
    ) {
        if generation != self.transition_generation {
            return;
        }
        self.loading = false;
        self.transition_pending = false;
        if let Some(e) = error {
            self.status = format!("transition failed: {e}");
            return;
        }
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.status = to.clone();
            }
        }
        if let Some(sum) = self.issues.iter_mut().find(|i| i.key == key) {
            sum.status = to.clone();
        }
        self.status = format!("moved {key} → {to}");
        self.flash(format!("✓ moved to {to}"));
        if to == "Done" {
            self.trigger_jax_party();
        }
    }

    /// Applies `AppEvent::DescriptionUpdated` — see
    /// `dispatch_update_description` above.
    pub(super) fn apply_description_updated(
        &mut self,
        generation: u64,
        key: String,
        adf: serde_json::Value,
        error: Option<String>,
        return_screen: Screen,
    ) {
        if generation != self.edit_generation {
            return;
        }
        self.loading = false;
        self.edit_pending = false;
        self.screen = return_screen;
        if let Some(e) = error {
            self.status = format!("update failed: {e}");
            return;
        }
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.description = adf;
            }
        }
        self.status = format!("updated {key} description");
        self.flash("✓ description updated");
        self.trigger_jax_party();
    }

    /// Applies `AppEvent::CommentAdded` — see `dispatch_add_comment` above.
    pub(super) fn apply_comment_added(
        &mut self,
        generation: u64,
        key: String,
        result: Result<Comment, String>,
        return_screen: Screen,
    ) {
        if generation != self.edit_generation {
            return;
        }
        self.loading = false;
        self.edit_pending = false;
        self.screen = return_screen;
        let comment = match result {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("comment failed: {e}");
                return;
            }
        };
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.comments.push(comment.clone());
            }
        }
        if let Some(cached) = self.detail_cache.get_mut(&key) {
            cached.comments.push(comment);
        }
        self.status = format!("added comment to {key}");
        self.flash("✓ comment added");
        self.trigger_jax_party();
    }

    /// Applies `AppEvent::IssueCreated` — see `dispatch_create_issue` above.
    /// On failure, lands back on `Screen::NewIssue` (not `Screen::Home`) so
    /// the user can fix a bad project/permission error and resubmit without
    /// retyping the summary — the compose form's own state (`self.new_issue`)
    /// hasn't been touched yet at this point, so it's still there to retry
    /// against.
    pub(super) fn apply_issue_created(
        &mut self,
        generation: u64,
        issue_type: String,
        summary: String,
        description: Option<serde_json::Value>,
        result: Result<String, String>,
    ) {
        if generation != self.edit_generation {
            return;
        }
        self.loading = false;
        self.edit_pending = false;
        // Safe to reset now regardless of outcome: `edit_pending` just went
        // false, so `apply_edit`'s re-entrancy guard no longer needs
        // `edit_target`/`edit_return_screen` to still describe this session
        // (unlike while the dispatch was in flight — see `apply_new_issue`).
        // Both branches below set `self.screen` directly rather than reading
        // `edit_return_screen`, so this can't strand either one.
        self.reset_edit_target();
        let key = match result {
            Ok(k) => k,
            Err(e) => {
                self.status = format!("create failed: {e}");
                self.screen = Screen::NewIssue;
                return;
            }
        };
        self.land_new_issue(key.clone(), issue_type, summary, description);
        self.new_issue = super::super::NewIssueState::default();
        self.status = format!("created {key}");
        self.flash(format!("✓ created {key}"));
        self.trigger_jax_party();
        self.open_by_key(&key);
    }

    /// Applies `AppEvent::AssigneeApplied` — see `dispatch_assign` above.
    pub(super) fn apply_assignee_applied(
        &mut self,
        generation: u64,
        key: String,
        display_name: Option<String>,
        error: Option<String>,
    ) {
        if generation != self.assignee_generation {
            // A newer picker interaction (or the picker closing and
            // reopening) superseded this result; drop it silently,
            // mirroring `apply_transition_applied`'s stale-generation guard.
            return;
        }
        self.loading = false;
        self.assignee_pending = false;
        if let Some(e) = error {
            self.status = format!("assign failed: {e}");
            return;
        }
        self.apply_assignee_locally(&key, display_name.as_deref());
        self.status = match &display_name {
            Some(name) => format!("assigned {key} to {name}"),
            None => format!("unassigned {key}"),
        };
        self.flash(match &display_name {
            Some(name) => format!("✓ assigned to {name}"),
            None => "✓ unassigned".to_string(),
        });
    }

    /// Applies `AppEvent::VersionsApplied` — see `dispatch_set_versions`
    /// above. Unlike the other mutation applies, both fields can fail (or
    /// succeed) independently in one round-trip, so each is reported and
    /// applied on its own terms rather than the usual single error branch.
    pub(super) fn apply_versions_applied(
        &mut self,
        generation: u64,
        key: String,
        fix_versions: Option<Vec<String>>,
        fix_error: Option<String>,
        affects_versions: Option<Vec<String>>,
        affects_error: Option<String>,
    ) {
        if generation != self.version_generation {
            return;
        }
        self.loading = false;
        self.version_pending = false;
        self.apply_versions_locally(&key, fix_versions, affects_versions);
        match (&fix_error, &affects_error) {
            (None, None) => {
                self.status = format!("updated {key} versions");
                self.flash("✓ versions updated");
            }
            (Some(e), None) => self.status = format!("fix version update failed: {e}"),
            (None, Some(e)) => self.status = format!("affects version update failed: {e}"),
            (Some(fe), Some(ae)) => self.status = format!("version update failed: {fe}; {ae}"),
        }
    }

    /// Applies `AppEvent::ReleaseBulkApplied` — see `dispatch_release_bulk`
    /// above. Each successful `Remove` drops that issue from
    /// `release.issues`/`release.selected`; a successful `Add` doesn't
    /// touch `release.issues` directly (the issue may not have been in the
    /// drilled list at all) — `refresh_release_drill_if_showing` re-fetches
    /// instead, so the list reflects the real server state rather than a
    /// locally-guessed one.
    pub(super) fn apply_release_bulk_applied(
        &mut self,
        generation: u64,
        version_name: String,
        kind: ReleaseBulkKind,
        results: Vec<(String, Result<(), String>)>,
    ) {
        if generation != self.release_bulk_generation {
            return;
        }
        self.loading = false;
        self.release.bulk_pending = false;

        // Whether the drill-down is still showing the exact version this
        // bulk op was for — the user may have backed out and drilled into a
        // different one while it was in flight. `release_bulk_generation`
        // only distinguishes this op from a *newer* one, not from unrelated
        // navigation in between, so it can't stand in for this check: a
        // late-arriving Remove would otherwise `retain()`/clamp
        // `release.issues` against whatever version is now on screen,
        // possibly dropping an issue that belongs to both versions from the
        // wrong one's list. Mirrors the Add branch's own
        // `refresh_release_drill_if_showing` guard below.
        let still_showing =
            self.release.drilled.as_ref().map(|v| v.name.as_str()) == Some(version_name.as_str());

        let mut failures = 0usize;
        for (key, result) in &results {
            match result {
                Ok(()) => {
                    if still_showing {
                        self.release.selected.remove(key);
                        if kind == ReleaseBulkKind::Remove {
                            self.release.issues.retain(|i| &i.key != key);
                        }
                    }
                    self.apply_versions_locally_for_bulk(key, &version_name, kind);
                }
                Err(_) => failures += 1,
            }
        }
        let succeeded = results.len() - failures;
        self.status = if failures == 0 {
            self.flash(format!("✓ updated {succeeded} issue(s)"));
            format!("updated {succeeded} issue(s) for {version_name}")
        } else {
            format!("updated {succeeded} issue(s), {failures} failed for {version_name}")
        };

        if kind == ReleaseBulkKind::Remove {
            if still_showing {
                let len = self.release.issues.len();
                self.release.issue_cursor = self.release.issue_cursor.min(len.saturating_sub(1));
            }
        } else {
            self.refresh_release_drill_if_showing(&version_name);
        }
    }

    /// Applies `AppEvent::AttachmentDownloaded` — see
    /// `dispatch_attachment_download` above. No generation to check (see
    /// that event variant's own doc comment) — this only ever surfaces a
    /// status flash, never mutates state a stale result could corrupt.
    pub(super) fn apply_attachment_downloaded(
        &mut self,
        key: String,
        filename: String,
        result: Result<String, String>,
    ) {
        self.loading = false;
        match result {
            Ok(path) => {
                self.status = format!("{key}: downloaded {filename} to {path}");
                self.flash(format!("✓ downloaded {filename}"));
            }
            Err(e) => self.status = format!("download failed: {e}"),
        }
    }

    /// Applies `AppEvent::AttachmentUploaded` — see
    /// `dispatch_attachment_upload` above. No generation to check, same as
    /// `apply_attachment_downloaded`: a stale response here can't corrupt
    /// list navigation state, only this one issue's own attachment list,
    /// which is guarded a different way — the `self.detail` merge only
    /// applies if the app is still viewing this issue (mirroring
    /// `apply_transition_applied`'s own `d.key == key` check), while the
    /// `detail_cache` merge is addressed by `key` directly and so needs no
    /// such guard (mirroring `apply_comment_added`'s cache merge).
    pub(super) fn apply_attachment_uploaded(
        &mut self,
        key: String,
        filename: String,
        result: Result<Vec<Attachment>, String>,
    ) {
        self.loading = false;
        let uploaded = match result {
            Ok(a) => a,
            Err(e) => {
                self.status = format!("upload failed: {e}");
                return;
            }
        };
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                merge_attachments(&mut d.attachments, &uploaded);
            }
        }
        if let Some(cached) = self.detail_cache.get_mut(&key) {
            merge_attachments(&mut cached.attachments, &uploaded);
        }
        self.status = format!("{key}: uploaded {filename}");
        self.flash(format!("✓ uploaded {filename}"));
    }

    /// Applies `AppEvent::AttachmentPreviewLoaded` (`images` feature only) —
    /// see `dispatch_attachment_preview`/`App::refresh_attachment_preview`
    /// above. Guarded two ways against a stale response: the usual
    /// generation check (a newer picker move bumped
    /// `attachment_preview_generation` since this fetch was dispatched), and
    /// the highlighted attachment's id still matching the one this preview
    /// was fetched for. The id check is load-bearing, not redundant: a
    /// manual `r` refresh (`App::refresh_detail`/`apply_detail_loaded`)
    /// replaces `self.detail` wholesale and calls
    /// `App::invalidate_attachment_preview` to bump the generation, but
    /// `attachment_index` itself isn't reset — if the refreshed issue's
    /// attachment list changed shape, the same index could now point at a
    /// different attachment than this response was fetched for.
    #[cfg(feature = "images")]
    pub(super) fn apply_attachment_preview_loaded(
        &mut self,
        generation: u64,
        attachment_id: String,
        image: Option<image::DynamicImage>,
    ) {
        if generation != self.attachment_preview_generation {
            return;
        }
        let Some(image) = image else {
            return;
        };
        let still_current = self
            .detail
            .as_ref()
            .and_then(|d| d.attachments.get(self.attachment_index))
            .is_some_and(|a| a.id == attachment_id);
        if !still_current {
            return;
        }
        let Some(picker) = self.image_picker.as_ref() else {
            return;
        };
        let protocol = picker.new_resize_protocol(image);
        *self.attachment_preview.get_mut() = Some(super::super::attachments::AttachmentPreview {
            attachment_id,
            protocol,
        });
    }

    /// Applies `AppEvent::InlineImageLoaded` (`images` feature only) — see
    /// `App::refresh_inline_images`/`dispatch_inline_image` above. Only the
    /// usual generation check guards whether the response is *applied*,
    /// unlike `apply_attachment_preview_loaded`'s extra "still the
    /// highlighted attachment" recheck: there's no single currently-selected
    /// slot this cache tracks, every resolved key is independently valid for
    /// as long as the generation matches, since `App::invalidate_inline_images`
    /// clears the whole map on invalidation rather than one slot getting
    /// overwritten out from under a stale response.
    ///
    /// `key` is freed from `inline_images_pending` unconditionally, before
    /// the generation check — even a since-superseded response means this
    /// particular fetch is no longer in flight, and leaving the key marked
    /// pending forever would permanently block `refresh_inline_images`/
    /// `refresh_quick_view_inline_images` from ever retrying it (Phase 5 of
    /// issue #130's idempotency design — see either function's own doc
    /// comment).
    #[cfg(feature = "images")]
    pub(super) fn apply_inline_image_loaded(
        &mut self,
        generation: u64,
        key: super::super::InlineImageKey,
        image: Option<image::DynamicImage>,
    ) {
        self.inline_images_pending.remove(&key);
        if generation != self.inline_image_generation {
            return;
        }
        let Some(image) = image else {
            return;
        };
        self.inline_images.borrow_mut().insert(key, image);
    }

    /// Applies `AppEvent::InlineImageUuidsResolved` (`images` feature only)
    /// — see `App::resolve_unmatched_media_by_uuid`/`dispatch_uuid_resolve`
    /// above. This event only resolved *identity*, not bytes: for each
    /// `(key, url)` pair not already cached or in flight, this marks it
    /// pending and hands it to the same `dispatch_inline_image` the
    /// alt-matched path already uses, so the actual fetch/decode/cache
    /// pipeline is shared rather than duplicated.
    #[cfg(feature = "images")]
    pub(super) fn apply_inline_image_uuids_resolved(
        &mut self,
        generation: u64,
        resolved: Vec<(super::super::InlineImageKey, String)>,
    ) {
        if generation != self.inline_image_generation {
            return;
        }
        for (key, url) in resolved {
            if self.inline_images.borrow().contains_key(&key)
                || self.inline_images_pending.contains(&key)
            {
                continue;
            }
            self.inline_images_pending.insert(key.clone());
            let tx = self.events_tx.clone();
            dispatch_inline_image(tx, generation, key, url);
        }
    }
}

#[cfg(all(test, feature = "images"))]
mod tests {
    use super::*;

    /// Issue #130 phase 4's memory-bounding step: a decoded image bigger
    /// than `MAX_PREVIEW_PIXELS` gets resized down (preserving aspect ratio)
    /// before it's ever handed back to be cached — constructs a synthetic
    /// oversized image directly (no network/decode involved) and runs it
    /// through the same `downscale_for_preview` both
    /// `fetch_attachment_preview_blocking` and `fetch_external_image_blocking`
    /// call before returning.
    #[test]
    fn downscale_for_preview_shrinks_an_oversized_image_to_the_pixel_cap() {
        // 3000x2000 = 6,000,000 px, well past the ~2,000,000px cap.
        let oversized = image::DynamicImage::new_rgb8(3000, 2000);

        let result = downscale_for_preview(oversized);

        let pixels = result.width() as u64 * result.height() as u64;
        assert!(
            pixels <= MAX_PREVIEW_PIXELS as u64,
            "expected at most {MAX_PREVIEW_PIXELS} px, got {pixels} ({}x{})",
            result.width(),
            result.height()
        );
        // Aspect ratio (3:2) must survive the resize, not just the pixel count.
        let original_ratio = 3000.0 / 2000.0;
        let result_ratio = result.width() as f64 / result.height() as f64;
        assert!(
            (original_ratio - result_ratio).abs() < 0.01,
            "aspect ratio should be preserved: expected {original_ratio}, got {result_ratio}"
        );
    }

    /// An image already at or under the cap is returned unchanged — no
    /// pointless resize (and no possibility of a rounding-driven upscale)
    /// for images that were already a reasonable size.
    #[test]
    fn downscale_for_preview_leaves_a_small_image_untouched() {
        let small = image::DynamicImage::new_rgb8(100, 80);

        let result = downscale_for_preview(small);

        assert_eq!(result.width(), 100);
        assert_eq!(result.height(), 80);
    }
}
