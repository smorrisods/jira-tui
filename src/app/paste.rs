//! Handling for `Event::Paste` (bracketed paste, enabled in `main.rs`'s
//! `setup_terminal`) — dispatched from the run loop's event match rather
//! than going through `src/keys/` since it's a single terminal event, not a
//! keypress. Covers the attachment-upload `Input` and `Browse` stages, the
//! "drop a file straight onto a bare Detail screen" auto-attach shortcut,
//! and the in-TUI editor's generic text paste, which is why this dispatches
//! on `(self.screen, &self.attachment_upload)` rather than a pile of
//! independent `if`s.

use super::attachments::AttachmentUpload;
use super::{App, Screen};
use crate::infra;

impl App {
    /// Route a paste's text to whatever's currently focused.
    ///
    /// - The in-TUI editor (`Screen::Edit`, no attachment-upload modal
    ///   open): the raw, un-normalized text goes straight into the editor
    ///   buffer via `EditorState::insert_str` — this is generic prose (e.g.
    ///   a paragraph pasted into a description), not a file path, so none
    ///   of the path normalization below applies.
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
            self.editor.insert_str(&text);
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
}
