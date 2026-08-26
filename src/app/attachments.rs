//! The attachment picker (`a`, Detail screen only): listing an issue's
//! attachments and opening the highlighted one in the system browser, or
//! downloading it to disk. Mirrors `app::transitions`' shape — plain
//! `attachments_open`/`attachment_index` fields on `App` rather than a
//! dedicated state struct, since there's nothing here beyond an open flag
//! and a cursor.
//!
//! Also holds the upload flow (`u`, Detail only — see `AttachmentUpload`):
//! type a local path, stat it, and confirm a preview before the actual
//! multipart POST. Unlike the picker above, this *is* a dedicated enum —
//! there's real per-stage data (the typed path; the resolved filename/size/
//! mime once it's been stat'd) rather than just an open flag and a cursor.

use std::path::{Path, PathBuf};

use crate::domain::Source;
use crate::infra;

use super::{async_ops, App, Screen};

/// The upload flow's two stages. `Input` is a plain path-entry line;
/// `Confirm` is the mandatory preview CLAUDE.md's "Preview before any
/// mutating Jira call" requires before `App::confirm_attachment_upload`
/// actually dispatches anything.
#[derive(Clone, Debug, PartialEq)]
pub enum AttachmentUpload {
    Input {
        path: String,
    },
    Confirm {
        /// The path exactly as typed (before `~`-expansion) — kept around so
        /// `App::back_out_of_attachment_upload_confirm` can restore `Input`
        /// with what the user actually typed, not the resolved absolute
        /// path.
        path: String,
        filename: String,
        size: u64,
        mime: &'static str,
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
    }

    pub fn close_attachments(&mut self) {
        self.attachments_open = false;
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
        self.attachment_index = idx as usize;
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

    /// `u` (Detail screen only): open the upload flow's path-entry stage.
    /// A silent no-op off the Detail screen or with no issue loaded —
    /// mirrors `open_attachments`'s own guard.
    pub fn open_attachment_upload(&mut self) {
        if self.screen != Screen::Detail || self.detail.is_none() {
            return;
        }
        self.attachment_upload = Some(AttachmentUpload::Input {
            path: String::new(),
        });
    }

    pub fn close_attachment_upload(&mut self) {
        self.attachment_upload = None;
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
        let meta = match std::fs::metadata(&resolved) {
            Ok(m) if m.is_file() => m,
            Ok(_) => {
                self.status = format!("{raw}: not a regular file");
                return;
            }
            Err(e) => {
                self.status = format!("{raw}: {e}");
                return;
            }
        };
        let filename = resolved
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| raw.clone());
        let mime = crate::mime::guess_mime(&filename);
        self.attachment_upload = Some(AttachmentUpload::Confirm {
            path: raw,
            filename,
            size: meta.len(),
            mime,
        });
    }

    /// `Esc` from `Confirm`: back to `Input`, keeping the previously-typed
    /// path so a stray Esc doesn't lose it — same "go back, don't discard"
    /// semantics as `App::back_out_of_preview` for the edit-preview screen
    /// (see `src/ui/preview.rs`'s own doc comment and footer copy). A no-op
    /// if the flow isn't in `Confirm` (or isn't open at all).
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
