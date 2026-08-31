//! The attachment picker (`a`, Detail screen only): listing an issue's
//! attachments and opening the highlighted one in the system browser, or
//! downloading it to disk. Mirrors `app::transitions`' shape — plain
//! `attachments_open`/`attachment_index` fields on `App` rather than a
//! dedicated state struct, since there's nothing here beyond an open flag
//! and a cursor.
//!
//! Also holds the upload flow (`u`, Detail only — see `AttachmentUpload`):
//! browse the filesystem (or `Tab` into typing a path by hand), stat the
//! chosen file, and confirm a preview before the actual multipart POST.
//! Unlike the picker above, this *is* a dedicated enum — there's real
//! per-stage data (the browser's cwd/listing; the typed path; the resolved
//! filename/size/mime once something's been stat'd) rather than just an
//! open flag and a cursor.

use std::path::{Path, PathBuf};

use crate::domain::Source;
use crate::infra;

use super::file_browser::FileBrowserState;
use super::{async_ops, App, Screen};

/// How many run-loop iterations the attachment picker's highlighted row
/// must sit unchanged before its preview is actually fetched (`images`
/// feature only) — restarted on every move, so holding an arrow key
/// through several image attachments only ever dispatches once, for
/// whatever's highlighted when the user stops. Mirrors
/// `search::SEARCH_DEBOUNCE_TICKS`'s own reasoning (an approximate
/// ~360ms debounce, not a precise one, since `app.tick` advances on any
/// run-loop iteration, not on a fixed clock).
#[cfg(feature = "images")]
const ATTACHMENT_PREVIEW_DEBOUNCE_TICKS: u64 = 4;

/// The attachment picker's fetched-and-decoded image preview for the
/// currently-highlighted attachment (`images` feature only) — see
/// `App::refresh_attachment_preview`.
#[cfg(feature = "images")]
pub struct AttachmentPreview {
    /// The attachment id this preview belongs to, so a fetch that lands
    /// after the user has already moved the selection elsewhere can be told
    /// apart from the current one (belt-and-suspenders alongside the
    /// generation check — see `App::apply_attachment_preview_loaded`).
    pub attachment_id: String,
    pub protocol: ratatui_image::protocol::StatefulProtocol,
}

/// The upload flow's stages. `Browse` — an interactive filesystem browser —
/// is the default entry point (`u`); `Input`, a plain path-entry line, is a
/// `Tab`-accessible fallback for typing an exact path by hand (e.g. pasting
/// one from outside the terminal). `Confirm` is the mandatory preview
/// CLAUDE.md's "Preview before any mutating Jira call" requires before
/// `App::confirm_attachment_upload` actually dispatches anything.
#[derive(Clone, Debug, PartialEq)]
pub enum AttachmentUpload {
    Browse {
        browser: FileBrowserState,
    },
    Input {
        path: String,
    },
    Confirm {
        /// The path exactly as typed/selected (before `~`-expansion) — kept
        /// around so `App::back_out_of_attachment_upload_confirm` can
        /// restore `Input` with what the user actually typed/picked, not
        /// the resolved absolute path.
        path: String,
        filename: String,
        size: u64,
        mime: &'static str,
        /// A short text preview of the file's contents (up to ~4KB),
        /// populated only for text-ish files — see
        /// `read_attachment_text_preview`. `None` for anything binary,
        /// unreadable as UTF-8, or simply not detected as text; those keep
        /// today's metadata-only preview.
        content_preview: Option<String>,
    },
}

impl App {
    /// `a` (Detail screen only): open the attachment picker over the
    /// current issue's attachments. A silent no-op off the Detail screen or
    /// with no issue loaded; a no-op with a status flash when the issue has
    /// no attachments — matching `open_transitions`'s own empty-list guard.
    pub fn open_attachments(&mut self) {
        if self.screen != Screen::Detail {
            return;
        }
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        if detail.attachments.is_empty() {
            self.status = "no attachments on this issue".into();
            return;
        }
        self.attachment_index = 0;
        self.attachments_open = true;
        #[cfg(feature = "images")]
        self.refresh_attachment_preview();
    }

    pub fn close_attachments(&mut self) {
        self.attachments_open = false;
        // Free the decoded image (if any) and bump the generation — a
        // code-review finding: without the bump, a preview fetch still
        // in flight when the picker closes would later land tagged with
        // a generation that's still current (nothing else bumps it
        // between close and the next open/move) and pass
        // `apply_attachment_preview_loaded`'s checks, silently
        // repopulating `attachment_preview` while the picker is closed.
        // Also cancels any still-debounced move
        // (`ensure_attachment_preview_dispatched` is itself guarded on
        // `attachments_open`, so this isn't load-bearing for correctness,
        // just avoids a pointless dispatch for a picker that's no longer
        // showing).
        #[cfg(feature = "images")]
        {
            *self.attachment_preview.get_mut() = None;
            self.attachment_preview_generation += 1;
            self.attachment_preview_pending = false;
        }
    }

    /// Move the highlighted row by `delta`, clamped within bounds — same
    /// shape as `App::picker_move`.
    pub fn attachments_move(&mut self, delta: isize) {
        let len = self
            .detail
            .as_ref()
            .map(|d| d.attachments.len())
            .unwrap_or(0);
        if len == 0 {
            return;
        }
        let mut idx = self.attachment_index as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        let idx = idx as usize;
        // A clamped move that lands back on the same row (e.g. holding Down
        // at the last attachment) isn't a real selection change — refreshing
        // the preview anyway would flicker the already-shown image and
        // re-dispatch a redundant fetch/decode on every repeated keypress.
        #[cfg(feature = "images")]
        let changed = idx != self.attachment_index;
        self.attachment_index = idx;
        #[cfg(feature = "images")]
        if changed {
            // Debounced, not immediate (a code-review finding) — holding an
            // arrow key through several image attachments would otherwise
            // fire one uncancelled network fetch + image decode per row.
            // See `App::ensure_attachment_preview_dispatched`, which
            // actually fires this once the highlighted row's sat still for
            // `ATTACHMENT_PREVIEW_DEBOUNCE_TICKS`.
            self.attachment_preview_pending = true;
            self.attachment_preview_dispatch_at_tick =
                self.tick + ATTACHMENT_PREVIEW_DEBOUNCE_TICKS;
        }
    }

    /// Called every run-loop iteration (mirrors `App::ensure_search_dispatched`):
    /// if a debounced picker move is due, dispatch its preview fetch. A
    /// no-op almost every tick — only fires once the highlighted row's sat
    /// unchanged for `ATTACHMENT_PREVIEW_DEBOUNCE_TICKS`, and only while the
    /// picker is still actually open (closing cancels the pending dispatch
    /// outright, via `App::close_attachments`, rather than letting this
    /// still fire a fetch for a picker no longer showing).
    #[cfg(feature = "images")]
    pub fn ensure_attachment_preview_dispatched(&mut self) {
        if !self.attachment_preview_pending || !self.attachments_open {
            return;
        }
        if self.tick < self.attachment_preview_dispatch_at_tick {
            return;
        }
        self.attachment_preview_pending = false;
        // If the highlighted row's preview is already cached, *or* already
        // in flight — e.g. the user moved away and back to the same row
        // within the debounce window, before or after its first fetch
        // landed — there's nothing new to fetch; refreshing anyway would
        // just flicker the already-shown image (cached case) or dispatch a
        // second, redundant concurrent fetch/decode for the exact same
        // attachment (in-flight case, a code-review finding) — either way
        // exactly the waste the debounce itself exists to avoid.
        let already_handled = self
            .detail
            .as_ref()
            .and_then(|d| d.attachments.get(self.attachment_index))
            .is_some_and(|a| {
                self.attachment_preview
                    .borrow()
                    .as_ref()
                    .is_some_and(|p| p.attachment_id == a.id)
                    || self.attachment_preview_inflight_id.as_deref() == Some(a.id.as_str())
            });
        if already_handled {
            return;
        }
        self.refresh_attachment_preview();
    }

    /// Recompute the attachment picker's image preview for whichever
    /// attachment is now highlighted (`images` feature only) — called after
    /// opening the picker or moving the selection, and (guarded on
    /// `attachments_open`) after `App::invalidate_attachment_preview` too,
    /// so a detail refresh while the picker is already open re-dispatches a
    /// fetch instead of leaving the picker blank until the user moves the
    /// selection. Always clears the previous preview first and bumps
    /// `attachment_preview_generation`, so every ineligible case (no
    /// detected terminal capability, a demo/cache session with nothing real
    /// to fetch, a non-image attachment) falls through to "no preview"
    /// without any special-casing at the call sites — `ui::attachment_picker`
    /// already renders its normal placeholder path whenever there's nothing
    /// here.
    ///
    /// Also clears `attachment_preview_pending` (a code-review finding): a
    /// caller reaching this directly (`App::open_attachments`, or a manual
    /// refresh via `refresh_detail_images`) fully supersedes whatever move
    /// `App::attachments_move` may still have debounced — leaving the flag
    /// set would let `App::ensure_attachment_preview_dispatched` fire a
    /// second, redundant fetch once that debounce window elapses, exactly
    /// the duplicate-dispatch waste the debounce itself exists to avoid.
    #[cfg(feature = "images")]
    pub(crate) fn refresh_attachment_preview(&mut self) {
        self.attachment_preview_pending = false;
        self.attachment_preview_inflight_id = None;
        *self.attachment_preview.get_mut() = None;
        self.attachment_preview_generation += 1;
        let generation = self.attachment_preview_generation;

        let Some(attachment) = self
            .detail
            .as_ref()
            .and_then(|d| d.attachments.get(self.attachment_index))
        else {
            return;
        };
        let Some(url) =
            attachment_preview_url(self.image_picker.as_ref(), &self.source, attachment)
        else {
            return;
        };
        let id = attachment.id.clone();
        self.attachment_preview_inflight_id = Some(id.clone());
        let tx = self.events_tx.clone();
        async_ops::dispatch_attachment_preview(tx, generation, id, url);
    }

    /// Drops any cached/in-flight attachment preview (`images` feature
    /// only) — called wherever `self.detail` gets replaced wholesale by a
    /// manual refresh (`App::refresh_detail`, `apply_detail_loaded`'s
    /// navigate branch) rather than a fresh picker open/move. Bumping the
    /// generation alone would drop a same-attachment-id response too (the
    /// image itself may have changed); clearing the cached preview here
    /// means the picker falls back to its placeholder rather than risking a
    /// stale image surviving a refresh. Every caller pairs this with a
    /// `self.attachments_open`-guarded `refresh_attachment_preview()` right
    /// after, so a refresh that lands while the picker is already open
    /// re-dispatches a fetch for whatever's still highlighted instead of
    /// leaving the picker permanently blank until the user happens to move
    /// the selection.
    #[cfg(feature = "images")]
    pub(crate) fn invalidate_attachment_preview(&mut self) {
        *self.attachment_preview.get_mut() = None;
        self.attachment_preview_generation += 1;
        let len = self
            .detail
            .as_ref()
            .map(|d| d.attachments.len())
            .unwrap_or(0);
        if self.attachment_index >= len {
            self.attachment_index = len.saturating_sub(1);
        }
    }

    /// `Enter`/`o` on the picker: open the highlighted attachment's
    /// `content_url` in the system browser — same
    /// success/failure-flash convention as `App::open_highlighted_link`'s
    /// URL branch.
    pub fn open_selected_attachment(&mut self) {
        let Some(url) = self
            .detail
            .as_ref()
            .and_then(|d| d.attachments.get(self.attachment_index))
            .map(|a| a.content_url.clone())
        else {
            return;
        };
        if infra::open_url(&url).is_ok() {
            self.flash(format!("↗ opened {url}"));
        } else {
            self.status = format!("couldn't open {url}");
        }
    }

    /// `d` on the picker: download the highlighted attachment to the
    /// current working directory. Demo/cache sessions have nothing real to
    /// fetch — this flashes a friendly message and does no I/O at all,
    /// mirroring how other mutating actions gate on `Source::Live` (see
    /// e.g. `App::confirm_transition`).
    pub fn download_selected_attachment(&mut self) {
        let Some(attachment) = self
            .detail
            .as_ref()
            .and_then(|d| d.attachments.get(self.attachment_index))
            .cloned()
        else {
            return;
        };
        if !matches!(self.source, Source::Live { .. }) {
            self.flash("demo data — nothing to download");
            return;
        }
        // `Source::Live` is only ever constructed under the `live` feature
        // (see `domain::Source`'s doc comment), so this is unreachable in a
        // `--no-default-features` build — no separate `#[cfg(feature =
        // "live")]` needed here; `dispatch_attachment_download` itself
        // compiles unconditionally and gates the actual network call
        // internally, mirroring `App::confirm_transition`/
        // `dispatch_transition`.
        let key = self
            .detail
            .as_ref()
            .map(|d| d.key.clone())
            .unwrap_or_default();
        self.status = format!("↻ downloading {}…", attachment.filename);
        self.loading = true;
        let tx = self.events_tx.clone();
        async_ops::dispatch_attachment_download(
            tx,
            key,
            attachment.filename,
            attachment.content_url,
        );
    }

    /// `u` (Detail screen only): open the upload flow at its default
    /// `Browse` stage, rooted at the user's home directory (falling back to
    /// `.` if it can't be resolved at all — the same "always open something
    /// rather than refuse the flow" spirit as `FileBrowserState::new`'s own
    /// error handling). A silent no-op off the Detail screen or with no
    /// issue loaded — mirrors `open_attachments`'s own guard.
    pub fn open_attachment_upload(&mut self) {
        if self.screen != Screen::Detail || self.detail.is_none() {
            return;
        }
        let start_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        self.open_attachment_browser_at(start_dir);
    }

    /// Shared by `open_attachment_upload` and the `Tab`-from-`Input` switch
    /// below: build a fresh `FileBrowserState` rooted at `start_dir`,
    /// flashing a status message if the initial listing fails (e.g. the
    /// resolved directory turns out to be unreadable) while still opening
    /// the browser rather than refusing the flow outright.
    fn open_attachment_browser_at(&mut self, start_dir: PathBuf) {
        let (browser, err) = FileBrowserState::new(start_dir);
        if let Some(e) = err {
            self.status = e;
        }
        self.attachment_upload = Some(AttachmentUpload::Browse { browser });
    }

    pub fn close_attachment_upload(&mut self) {
        self.attachment_upload = None;
    }

    /// `Tab`: switch between the `Browse` and `Input` stages — a no-op
    /// unless the flow is currently in one of those two (i.e. not while
    /// confirming, and not when the flow isn't open at all).
    ///
    /// `Browse` → `Input` seeds the typed path with the browser's current
    /// directory (trailing separator included) so switching to typing picks
    /// up from where browsing left off, rather than starting blank.
    /// `Input` → `Browse` resolves whatever's been typed so far to a
    /// starting directory — the path itself if it's already a directory,
    /// its parent if it names a (possibly not-yet-finished) file, or the
    /// home directory if neither resolves — so switching back to browsing
    /// lands somewhere relevant instead of always restarting at home.
    pub fn attachment_upload_toggle_input_mode(&mut self) {
        match self.attachment_upload.take() {
            Some(AttachmentUpload::Browse { browser }) => {
                let path = format!("{}{}", browser.cwd.display(), std::path::MAIN_SEPARATOR);
                self.attachment_upload = Some(AttachmentUpload::Input { path });
            }
            Some(AttachmentUpload::Input { path }) => {
                let start_dir = resolve_browse_start(&path);
                self.open_attachment_browser_at(start_dir);
            }
            other => self.attachment_upload = other,
        }
    }

    /// Move the `Browse` stage's highlighted row — a no-op if the flow
    /// isn't in that stage.
    pub fn attachment_browse_move(&mut self, delta: isize) {
        if let Some(AttachmentUpload::Browse { browser }) = self.attachment_upload.as_mut() {
            browser.move_selection(delta);
        }
    }

    /// Type a character into the `Browse` stage's live filter — a no-op if
    /// the flow isn't in that stage.
    pub fn attachment_browse_filter_char(&mut self, c: char) {
        if let Some(AttachmentUpload::Browse { browser }) = self.attachment_upload.as_mut() {
            browser.filter_push(c);
        }
    }

    /// `Backspace` in `Browse`: edit the filter, or go up a directory —
    /// see `FileBrowserState`'s own doc comment. Flashes a status message
    /// on a directory-read failure while leaving the browser open.
    pub fn attachment_browse_backspace(&mut self) {
        if let Some(AttachmentUpload::Browse { browser }) = self.attachment_upload.as_mut() {
            if let Some(e) = browser.backspace() {
                self.status = e;
            }
        }
    }

    /// `Enter` in `Browse`: descend into the highlighted directory, or
    /// finalize the highlighted file into `Confirm`. A no-op if nothing's
    /// selected (an empty or fully-filtered-out listing). A directory-read
    /// failure while descending flashes a status message and leaves the
    /// browser open on the directory it was already showing.
    pub fn attachment_browse_activate(&mut self) {
        let Some(AttachmentUpload::Browse { browser }) = self.attachment_upload.as_mut() else {
            return;
        };
        let Some(entry) = browser.selected_entry().cloned() else {
            return;
        };
        if entry.is_dir {
            if let Some(e) = browser.descend(entry.path) {
                self.status = e;
            }
            return;
        }
        let display = entry.path.display().to_string();
        self.finalize_attachment_selection(display, entry.path);
    }

    /// Types a character into the `Input` stage's path — a no-op if the
    /// flow isn't in that stage (or isn't open at all).
    pub fn attachment_upload_input_char(&mut self, c: char) {
        if let Some(AttachmentUpload::Input { path }) = self.attachment_upload.as_mut() {
            path.push(c);
        }
    }

    pub fn attachment_upload_backspace(&mut self) {
        if let Some(AttachmentUpload::Input { path }) = self.attachment_upload.as_mut() {
            path.pop();
        }
    }

    /// `Enter` from `Input`: stat the typed path (after `~`-expansion) and
    /// either advance to `Confirm` — the mandatory preview, see
    /// CLAUDE.md's "Preview before any mutating Jira call" — or flash an
    /// error and stay put so the user can fix a typo without retyping the
    /// whole path.
    pub fn confirm_attachment_upload_path(&mut self) {
        let Some(AttachmentUpload::Input { path }) = self.attachment_upload.as_ref() else {
            return;
        };
        let raw = path.clone();
        let resolved = expand_home(&raw);
        self.finalize_attachment_selection(raw, resolved);
    }

    /// Shared by `confirm_attachment_upload_path` (`Input`) and
    /// `attachment_browse_activate` (`Browse`): stat `resolved`, guess its
    /// MIME type, read a short text preview when applicable, and transition
    /// into `Confirm` — or, on failure, flash a status message and leave
    /// `self.attachment_upload` exactly as it was, so each caller's own
    /// stage stays open for the user to try again (fix a typo in `Input`;
    /// pick something else in `Browse`).
    fn finalize_attachment_selection(&mut self, display_path: String, resolved: PathBuf) {
        let meta = match std::fs::metadata(&resolved) {
            Ok(m) if m.is_file() => m,
            Ok(_) => {
                self.status = format!("{display_path}: not a regular file");
                return;
            }
            Err(e) => {
                self.status = format!("{display_path}: {e}");
                return;
            }
        };
        let filename = resolved
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| display_path.clone());
        let mime = crate::mime::guess_mime(&filename);
        let content_preview = read_attachment_text_preview(&resolved, mime, &filename);
        self.attachment_upload = Some(AttachmentUpload::Confirm {
            path: display_path,
            filename,
            size: meta.len(),
            mime,
            content_preview,
        });
    }

    /// `Esc` from `Confirm`: back to `Input`, keeping the previously-typed/
    /// picked path so a stray Esc doesn't lose it — same "go back, don't
    /// discard" semantics as `App::back_out_of_preview` for the edit-preview
    /// screen (see `src/ui/preview.rs`'s own doc comment and footer copy).
    /// Always lands on `Input` (not back on `Browse`) regardless of which
    /// stage the selection came from — `path` alone is enough context to
    /// resume from, and `Input`'s own `Tab` gets back to browsing if wanted.
    /// A no-op if the flow isn't in `Confirm` (or isn't open at all).
    pub fn back_out_of_attachment_upload_confirm(&mut self) {
        if let Some(AttachmentUpload::Confirm { path, .. }) = self.attachment_upload.take() {
            self.attachment_upload = Some(AttachmentUpload::Input { path });
        }
    }

    /// `y`/`Enter` from `Confirm`: dispatch the upload — or, for a demo/
    /// cache session, flash a friendly message and do no I/O at all,
    /// mirroring `download_selected_attachment`'s own `Source::Live` gate.
    /// Either way the flow closes immediately once this fires: like every
    /// other mutation in this app (see `dispatch_transition`/
    /// `dispatch_add_comment`), the eventual success/failure only ever
    /// surfaces as a status flash, not a modal left open waiting on it.
    pub fn confirm_attachment_upload(&mut self) {
        let Some(AttachmentUpload::Confirm {
            path,
            filename,
            mime,
            ..
        }) = self.attachment_upload.take()
        else {
            return;
        };
        let Some(key) = self.detail.as_ref().map(|d| d.key.clone()) else {
            return;
        };
        if !matches!(self.source, Source::Live { .. }) {
            self.flash("demo mode — uploading needs live Jira credentials");
            return;
        }
        // Same "unreachable but compiles unconditionally" shape as
        // `download_selected_attachment` above: `dispatch_attachment_upload`
        // gates the actual network call internally.
        self.status = format!("↻ uploading {filename}…");
        self.loading = true;
        let tx = self.events_tx.clone();
        async_ops::dispatch_attachment_upload(tx, key, expand_home(&path), filename, mime);
    }
}

/// Whether — and from where — an attachment's image preview should be
/// fetched (`images` feature only): `Some(url)` only when the terminal
/// actually supports image rendering (`picker` was detected at startup),
/// the session is genuinely live (demo/cache attachments' `content_url`/
/// `thumbnail_url` aren't real, so fetching them would just fail), and the
/// attachment's Jira-reported MIME type says it's an image — never attempt
/// to decode a PDF or a zip. Split out from `App::refresh_attachment_preview`
/// as a pure function so this gate is unit-testable on its own, without the
/// generation/dispatch machinery around it.
#[cfg(feature = "images")]
fn attachment_preview_url(
    picker: Option<&ratatui_image::picker::Picker>,
    source: &Source,
    attachment: &crate::domain::Attachment,
) -> Option<String> {
    if !images_eligible(picker, source) {
        return None;
    }
    attachment.image_preview_url()
}

/// Whether image fetching is worth attempting at all (`images` feature
/// only): a detected terminal image-rendering capability (`picker`, from
/// `main::detect_image_picker`) and a genuine live session (demo/cache
/// attachment URLs aren't real, so fetching them would just fail). Split out
/// of `attachment_preview_url` so `app::inline_images::refresh_inline_images`
/// can reuse the exact same coarse gate for its own eager-fetch batch,
/// rather than re-deriving it — it already filters to image mimes during
/// resolution (see `inline_images::resolve_inline_images`), so it only needs
/// this picker/source half of the check, not the per-attachment mime half.
#[cfg(feature = "images")]
pub(crate) fn images_eligible(
    picker: Option<&ratatui_image::picker::Picker>,
    source: &Source,
) -> bool {
    picker.is_some() && matches!(source, Source::Live { .. })
}

/// Expand a leading `~` (or `~/...`) to the user's home directory, via the
/// `dirs` crate already used for XDG paths elsewhere (see `config::mod`).
/// Any other path — already absolute, plain relative, or a bare `~word`
/// that isn't actually a home-relative reference — is left untouched.
fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest.trim_start_matches('/'));
            }
        }
    }
    PathBuf::from(path)
}

/// Resolve a typed (possibly incomplete) path to a directory the file
/// browser can start from — used by `App::attachment_upload_toggle_input_mode`
/// when switching from `Input` back to `Browse`. Prefers the path itself if
/// it's already a directory, falls back to its parent if it names a file (or
/// a file that doesn't exist yet), and falls back to the home directory (or
/// `.`) if neither resolves to anything browsable.
fn resolve_browse_start(typed: &str) -> PathBuf {
    let resolved = expand_home(typed);
    if resolved.is_dir() {
        return resolved;
    }
    if let Some(parent) = resolved.parent() {
        if parent.is_dir() {
            return parent.to_path_buf();
        }
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Extensions treated as text for the Confirm-stage content preview even
/// when their guessed MIME type doesn't start with `text/` (see
/// `crate::mime::guess_mime` — most of these already map to a `text/*`
/// type, but this list is the single source of truth `read_attachment_text_preview`
/// actually consults, so it stays correct even if `guess_mime` changes).
const TEXT_PREVIEW_EXTENSIONS: &[&str] = &["md", "txt", "log", "csv", "json"];

/// How much of a file to read for the Confirm-stage content preview — enough
/// to give a real sense of the content without the preview itself dominating
/// the popup or the read taking any perceptible time.
const TEXT_PREVIEW_MAX_BYTES: u64 = 4096;

/// Populate `AttachmentUpload::Confirm`'s `content_preview`: read up to
/// `TEXT_PREVIEW_MAX_BYTES` of `path` as UTF-8 text, but only when it looks
/// text-ish in the first place (guessed MIME starting with `text/`, or an
/// extension in `TEXT_PREVIEW_EXTENSIONS` — `guess_mime` maps some of those
/// to non-`text/*` types, e.g. `.json` → `application/json`, so the
/// extension check catches those too). Falls back to `None` on any read
/// failure or invalid UTF-8 (e.g. a binary file whose extension happens to
/// look text-like) — a misdetected binary must never surface as garbled
/// text in the preview, so any error here is silently treated the same as
/// "not text at all" rather than surfaced as its own status flash.
fn read_attachment_text_preview(path: &Path, mime: &str, filename: &str) -> Option<String> {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    let looks_like_text =
        mime.starts_with("text/") || TEXT_PREVIEW_EXTENSIONS.contains(&ext.as_str());
    if !looks_like_text {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let truncated = bytes.len() as u64 > TEXT_PREVIEW_MAX_BYTES;
    let cap = (TEXT_PREVIEW_MAX_BYTES as usize).min(bytes.len());
    let slice = &bytes[..cap];
    let mut text = match std::str::from_utf8(slice) {
        Ok(s) => s.to_string(),
        Err(e) => {
            // A byte cap can split a multi-byte character mid-sequence,
            // which surfaces as an error a handful of bytes before the very
            // end of the slice — back off to the last full character rather
            // than treating a merely-truncated (but otherwise valid) text
            // file as binary. Any invalid byte further back than that is a
            // genuine decode failure (a binary file whose extension/MIME
            // happened to look text-like), so it's a `None`, not a partial
            // preview.
            let valid_up_to = e.valid_up_to();
            if truncated && cap - valid_up_to <= 3 {
                std::str::from_utf8(&slice[..valid_up_to]).ok()?.to_string()
            } else {
                return None;
            }
        }
    };
    if truncated {
        text.push_str("\n…");
    }
    Some(text)
}

/// Reduce an attachment's API-reported filename to a safe on-disk basename —
/// just its final path component, discarding any directory traversal a
/// hostile (or merely creative) API response might smuggle in. Falls back to
/// a literal `"attachment"` when that yields nothing at all (an empty
/// string, or a name that's only path separators/`..`). Only actually
/// called from the live-only download path (`async_ops::mutation_ops`), so
/// it's otherwise dead code in a `--no-default-features` build — exercised
/// here regardless, via this file's own unit tests.
#[cfg_attr(not(feature = "live"), allow(dead_code))]
pub(crate) fn sanitize_attachment_filename(raw: &str) -> String {
    Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "attachment".to_string())
}

/// Find a filename that doesn't already exist in `dir`: `filename` itself if
/// free, otherwise `name (2).ext`, `name (3).ext`, ... — the same
/// "don't clobber an existing file" idiom most desktop downloaders use.
/// Same live-only-caller caveat as `sanitize_attachment_filename` above.
#[cfg_attr(not(feature = "live"), allow(dead_code))]
pub(crate) fn dedupe_filename(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().into_owned());
    let mut n = 2;
    loop {
        let name = match &ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "images")]
    fn test_attachment(mime_type: &str, thumbnail_url: Option<&str>) -> crate::domain::Attachment {
        crate::domain::Attachment {
            id: "10001".into(),
            filename: "mockup.png".into(),
            mime_type: mime_type.into(),
            size: 1024,
            created: "2026-08-25".into(),
            content_url: "https://example.atlassian.net/secure/attachment/10001/mockup.png".into(),
            thumbnail_url: thumbnail_url.map(String::from),
        }
    }

    #[test]
    #[cfg(feature = "images")]
    fn attachment_preview_url_is_none_without_a_detected_picker() {
        let attachment = test_attachment("image/png", None);
        let source = Source::Live {
            site: "example".into(),
            user: "me".into(),
        };
        assert_eq!(attachment_preview_url(None, &source, &attachment), None);
    }

    #[test]
    #[cfg(feature = "images")]
    fn attachment_preview_url_is_none_for_demo_or_cache_sessions() {
        let picker = ratatui_image::picker::Picker::halfblocks();
        let attachment = test_attachment("image/png", None);
        assert_eq!(
            attachment_preview_url(Some(&picker), &Source::Demo, &attachment),
            None,
            "demo attachment URLs aren't real — never attempt to fetch them"
        );
        assert_eq!(
            attachment_preview_url(
                Some(&picker),
                &Source::Cache { user: "me".into() },
                &attachment
            ),
            None
        );
    }

    #[test]
    #[cfg(feature = "images")]
    fn attachment_preview_url_is_none_for_a_non_image_mime_type() {
        let picker = ratatui_image::picker::Picker::halfblocks();
        let source = Source::Live {
            site: "example".into(),
            user: "me".into(),
        };
        let pdf = test_attachment("application/pdf", None);
        assert_eq!(
            attachment_preview_url(Some(&picker), &source, &pdf),
            None,
            "a non-image mime type must never be handed off for decoding"
        );
    }

    #[test]
    #[cfg(feature = "images")]
    fn attachment_preview_url_prefers_content_over_the_thumbnail() {
        let picker = ratatui_image::picker::Picker::halfblocks();
        let source = Source::Live {
            site: "example".into(),
            user: "me".into(),
        };
        // content_url is preferred regardless of whether a thumbnail_url is
        // also present — see Attachment::image_preview_url's own doc
        // comment: the whole point of an in-app preview is to actually be
        // visible at a useful size, and Jira's own thumbnail is
        // deliberately too small a source for that once `Resize::Scale`
        // stretches it to fill the reserved area.
        let with_thumb = test_attachment(
            "image/png",
            Some("https://example.atlassian.net/secure/thumbnail/10001/mockup.png"),
        );
        assert_eq!(
            attachment_preview_url(Some(&picker), &source, &with_thumb).as_deref(),
            Some(with_thumb.content_url.as_str())
        );

        let without_thumb = test_attachment("image/png", None);
        assert_eq!(
            attachment_preview_url(Some(&picker), &source, &without_thumb).as_deref(),
            Some(without_thumb.content_url.as_str())
        );
    }

    #[test]
    fn expand_home_resolves_a_bare_tilde_and_tilde_slash() {
        let home = dirs::home_dir().expect("test host must have a resolvable home dir");
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/notes.txt"), home.join("notes.txt"));
    }

    #[test]
    fn expand_home_leaves_other_paths_untouched() {
        assert_eq!(
            expand_home("/tmp/report.pdf"),
            PathBuf::from("/tmp/report.pdf")
        );
        assert_eq!(
            expand_home("relative/path.png"),
            PathBuf::from("relative/path.png")
        );
        // Not a home-relative reference — `~jane` names a *different* user's
        // home on a real Unix shell, which this helper deliberately doesn't
        // attempt to resolve.
        assert_eq!(expand_home("~jane/file"), PathBuf::from("~jane/file"));
    }

    #[test]
    fn sanitize_attachment_filename_strips_directories() {
        assert_eq!(sanitize_attachment_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_attachment_filename("a/b/c.png"), "c.png");
    }

    #[test]
    fn sanitize_attachment_filename_falls_back_when_empty_or_bare_traversal() {
        assert_eq!(sanitize_attachment_filename(""), "attachment");
        assert_eq!(sanitize_attachment_filename("/"), "attachment");
        assert_eq!(sanitize_attachment_filename(".."), "attachment");
    }

    #[test]
    fn dedupe_filename_keeps_the_plain_name_when_free() {
        let dir = std::env::temp_dir();
        let unique = format!("jira-tui-test-{}-{}.txt", std::process::id(), line!());
        let path = dedupe_filename(&dir, &unique);
        assert_eq!(path, dir.join(&unique));
    }

    #[test]
    fn dedupe_filename_appends_a_counter_when_taken() {
        let dir = std::env::temp_dir().join(format!(
            "jira-tui-dedupe-test-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let base = "report.pdf";
        std::fs::write(dir.join(base), b"x").unwrap();
        let path = dedupe_filename(&dir, base);
        assert_eq!(path, dir.join("report (2).pdf"));
        std::fs::write(&path, b"x").unwrap();
        let path2 = dedupe_filename(&dir, base);
        assert_eq!(path2, dir.join("report (3).pdf"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dedupe_filename_handles_extensionless_names() {
        let dir = std::env::temp_dir().join(format!(
            "jira-tui-dedupe-test-noext-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let base = "README";
        std::fs::write(dir.join(base), b"x").unwrap();
        let path = dedupe_filename(&dir, base);
        assert_eq!(path, dir.join("README (2)"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
