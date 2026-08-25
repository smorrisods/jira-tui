//! The attachment picker (`a`, Detail screen only): listing an issue's
//! attachments and opening the highlighted one in the system browser, or
//! downloading it to disk. Mirrors `app::transitions`' shape — plain
//! `attachments_open`/`attachment_index` fields on `App` rather than a
//! dedicated state struct, since there's nothing here beyond an open flag
//! and a cursor.

use std::path::{Path, PathBuf};

use crate::domain::Source;
use crate::infra;

use super::{async_ops, App, Screen};

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
