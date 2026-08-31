//! Handling for `Event::Paste` (bracketed paste, enabled in `main.rs`'s
//! `setup_terminal`) — dispatched from the run loop's event match rather
//! than going through `src/keys/` since it's a single terminal event, not a
//! keypress. Covers the attachment-upload `Input` and `Browse` stages, the
//! "drop a file straight onto a bare Detail screen" auto-attach shortcut,
//! and the in-TUI editor's generic text paste (plus its own "drop/paste an
//! image path straight into the embed pipeline" shortcut — see
//! `resolve_pasted_image_path`), which is why this dispatches on
//! `(self.screen, &self.attachment_upload)` rather than a pile of
//! independent `if`s.

use std::path::PathBuf;

use super::attachments::AttachmentUpload;
use super::{App, Screen};
use crate::infra;

impl App {
    /// Route a paste's text to whatever's currently focused.
    ///
    /// - The in-TUI editor (`Screen::Edit`, no attachment-upload modal
    ///   open): see `handle_editor_paste` — either generic text (the common
    ///   case) or an image path that feeds the upload-and-embed pipeline.
    /// - The attachment-upload path field (`Input` stage): normalize the
    ///   pasted text as a dropped file path and set it as the typed path.
    /// - The attachment-upload file browser (`Browse` stage): if the
    ///   normalized text resolves to an existing path, jump the browser
    ///   straight to it — descending into a directory, or finalizing a file
    ///   straight into `Confirm`.
    /// - A bare Detail screen (no modal open): if the normalized text
    ///   resolves to an existing, readable regular file, open the
    ///   attachment-upload flow directly into `Confirm` with it — this is
    ///   what actually delivers "drag a file onto the terminal to attach
    ///   it," with no `u` keypress needed first.
    /// - Anything else: no-op.
    pub fn handle_paste(&mut self, text: String) {
        if let (Screen::Edit, None) = (self.screen, &self.attachment_upload) {
            self.handle_editor_paste(text);
            return;
        }

        if infra::has_multiple_paths(&text) {
            self.flash("multiple files pasted — using the first");
        }
        let normalized = infra::normalize_dropped_path(&text);

        match (self.screen, &self.attachment_upload) {
            (_, Some(AttachmentUpload::Input { .. })) => {
                self.set_attachment_upload_input_path(normalized);
            }
            (_, Some(AttachmentUpload::Browse { .. })) => {
                self.paste_into_attachment_browser(&normalized);
            }
            (Screen::Detail, None) => {
                self.try_auto_attach(&normalized);
            }
            _ => {}
        }
    }

    /// `handle_paste`'s `Screen::Edit` arm (no attachment-upload modal
    /// open): if the pasted/dropped text, once normalized the same way a
    /// dropped attachment path is (`infra::normalize_dropped_path`),
    /// resolves to an existing image file, stage it for the upload-and-embed
    /// pipeline (`App::begin_image_embed`) instead of landing as text.
    /// Everything else — ordinary prose, a path to a non-image file, a path
    /// that doesn't resolve to anything on disk — keeps going straight into
    /// the buffer via `EditorState::insert_str`, exactly as before this
    /// pipeline existed; deliberately *not* run through path normalization
    /// in that fallback case (only the image-detection check above is), so
    /// generic pasted text (e.g. a paragraph pasted into a description)
    /// still lands byte-for-byte, matching this method's own pre-existing
    /// behaviour.
    ///
    /// A pending image-embed confirmation swallows any further paste input
    /// outright — mirroring `confirm_discard`'s "swallow the next keypress"
    /// modal shape (see `keys::handle_key`) — rather than letting a second
    /// paste slip text into the buffer underneath the still-open prompt.
    fn handle_editor_paste(&mut self, text: String) {
        if self.pending_image_embed.is_some() {
            return;
        }
        let normalized = infra::normalize_dropped_path(&text);
        if let Some(path) = resolve_pasted_image_path(&normalized) {
            self.begin_image_embed(path);
            return;
        }
        self.editor.insert_str(&text);
    }
}

/// If `normalized` resolves to an existing regular file whose guessed MIME
/// type looks like an image (`crate::mime::guess_mime`), returns its
/// resolved path — used by `App::handle_editor_paste` to tell "an image file
/// was dropped/pasted" apart from ordinary pasted prose or a non-image file
/// path, either of which must keep landing as plain text. `~`-expanded via
/// `app::attachments::expand_home`, the same way the dedicated
/// attachment-upload flow resolves a typed/pasted path — a dropped/pasted
/// path can carry a literal `~` the same way a typed one can.
fn resolve_pasted_image_path(normalized: &str) -> Option<PathBuf> {
    let resolved = super::attachments::expand_home(normalized);
    let meta = std::fs::metadata(&resolved).ok()?;
    if !meta.is_file() {
        return None;
    }
    let filename = resolved.file_name()?.to_string_lossy();
    crate::mime::guess_mime(&filename)
        .starts_with("image/")
        .then_some(resolved)
}
