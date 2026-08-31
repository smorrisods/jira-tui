//! Description editing: the built-in multi-line Markdown editor and the
//! ADF round-trip (compile → preview → apply). Also handles adding new
//! comments, which reuse the same editor/preview flow with a different
//! apply action, and the in-TUI editor's own image-embed pipeline (`Ctrl+V`
//! clipboard capture, or a dropped/pasted image path via `app::paste`):
//! stage → confirm → upload-and-embed, mirroring the dedicated attachment
//! upload flow's own stage → confirm → upload shape (`app::attachments`)
//! just scoped to the editor buffer instead.

use std::path::PathBuf;

use crate::domain::{Comment, Source};

use super::{async_ops, App, Screen};

/// What the in-TUI editor / preview screen is currently editing. Both share
/// the same Markdown-compose → ADF-preview → confirm flow; only the apply
/// action and footer wording differ.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum EditTarget {
    #[default]
    Description,
    /// Composing a new comment for the issue keyed by `App::edit_key`.
    Comment,
    /// Composing a brand-new issue's description — project/type/summary
    /// live in `App::new_issue`, not on `App::edit_key` (there's no issue
    /// key yet). See `app::new_issue`.
    NewIssue,
}

/// A minimal multi-line text editor for in-TUI description editing.
#[derive(Clone, Debug, Default)]
pub struct EditorState {
    pub lines: Vec<String>,
    pub cx: usize,
    pub cy: usize,
    pub scroll: u16,
    /// The text this session was seeded with (`from_text`'s `text`
    /// argument) — `is_dirty` compares the current buffer against this to
    /// tell "nothing's actually been typed/changed yet" from "the buffer
    /// happens to be non-empty" (a freshly opened description edit is
    /// non-empty by definition, since it's preloaded with the existing
    /// description).
    seed: String,
}

impl EditorState {
    pub fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        EditorState {
            lines,
            cx: 0,
            cy: 0,
            scroll: 0,
            seed: text.to_string(),
        }
    }

    /// Like `from_text`, but the buffer starts out already `is_dirty()` —
    /// for loading content that already represents real, unsaved work
    /// (`App::back_out_of_preview`'s reload) rather than a pristine
    /// starting point. `text` must be non-empty, or `is_dirty()` won't
    /// actually come out `true`.
    pub fn from_text_dirty(text: &str) -> Self {
        let mut state = Self::from_text(text);
        state.seed.clear();
        state
    }

    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Whether the buffer has changed since this session was seeded —
    /// what "is there an edit worth protecting from an accidental discard"
    /// actually means, as opposed to merely "is the buffer non-empty" (see
    /// the `seed` field doc).
    pub fn is_dirty(&self) -> bool {
        self.to_text() != self.seed
    }

    fn line_len(&self, y: usize) -> usize {
        self.lines.get(y).map(|l| l.chars().count()).unwrap_or(0)
    }

    pub fn insert_char(&mut self, c: char) {
        let byte = self.cursor_byte_index();
        self.lines[self.cy].insert(byte, c);
        self.cx += 1;
    }

    /// Bulk-insert `s` at the cursor — the paste counterpart to
    /// `insert_char`, used by `App::handle_paste`'s `Screen::Edit` arm so a
    /// whole pasted string lands in one go rather than one call per
    /// crossterm key event. Splits `s` on `\n` and applies each segment
    /// with the same `insert_char`/`newline()` semantics those per-key
    /// paths already use — looping `insert_char` over the raw string
    /// (including embedded newlines) would insert literal `\n` characters
    /// into a line instead of actually splitting it. Leaves the cursor
    /// positioned at the end of the inserted text.
    pub fn insert_str(&mut self, s: &str) {
        let mut segments = s.split('\n');
        if let Some(first) = segments.next() {
            for c in first.chars() {
                self.insert_char(c);
            }
        }
        for segment in segments {
            self.newline();
            for c in segment.chars() {
                self.insert_char(c);
            }
        }
    }

    pub fn newline(&mut self) {
        let byte = self.cursor_byte_index();
        let line = self.lines[self.cy].clone();
        let (left, right) = line.split_at(byte);
        self.lines[self.cy] = left.to_string();
        self.lines.insert(self.cy + 1, right.to_string());
        self.cy += 1;
        self.cx = 0;
    }

    pub fn backspace(&mut self) {
        if self.cx > 0 {
            let line = &mut self.lines[self.cy];
            let byte = line
                .char_indices()
                .nth(self.cx - 1)
                .map(|(i, _)| i)
                .unwrap();
            line.remove(byte);
            self.cx -= 1;
        } else if self.cy > 0 {
            let removed = self.lines.remove(self.cy);
            self.cy -= 1;
            self.cx = self.line_len(self.cy);
            self.lines[self.cy].push_str(&removed);
        }
    }

    pub fn left(&mut self) {
        if self.cx > 0 {
            self.cx -= 1;
        } else if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.line_len(self.cy);
        }
    }

    pub fn right(&mut self) {
        if self.cx < self.line_len(self.cy) {
            self.cx += 1;
        } else if self.cy + 1 < self.lines.len() {
            self.cy += 1;
            self.cx = 0;
        }
    }

    pub fn up(&mut self) {
        if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.cx.min(self.line_len(self.cy));
        }
    }

    pub fn down(&mut self) {
        if self.cy + 1 < self.lines.len() {
            self.cy += 1;
            self.cx = self.cx.min(self.line_len(self.cy));
        }
    }

    /// Move the cursor to the start of the current (logical) line.
    pub fn line_start(&mut self) {
        self.cx = 0;
    }

    /// Move the cursor to the end of the current (logical) line.
    pub fn line_end(&mut self) {
        self.cx = self.line_len(self.cy);
    }

    /// The byte offset of the cursor's `cx` (a char index) within its
    /// current line — the same lookup `insert_char`/`newline` do inline,
    /// exposed for callers (spell-suggest) that need to compare `cx`
    /// against a byte-range span from `spellcheck`.
    pub fn cursor_byte_index(&self) -> usize {
        let line = &self.lines[self.cy];
        line.char_indices()
            .nth(self.cx)
            .map(|(i, _)| i)
            .unwrap_or(line.len())
    }

    /// Replaces the byte range `start..end` of `line` with `replacement`,
    /// and moves the cursor to just after the replacement. Used to apply a
    /// spelling suggestion in place.
    pub fn replace_range(&mut self, line: usize, start: usize, end: usize, replacement: &str) {
        let target = &mut self.lines[line];
        let char_start = target[..start].chars().count();
        target.replace_range(start..end, replacement);
        self.cy = line;
        self.cx = char_start + replacement.chars().count();
    }
}

impl App {
    /// Open the built-in editor preloaded with the description Markdown.
    pub fn begin_tui_edit(&mut self) {
        if self.edit_pending {
            self.status = "an update is still in progress".into();
            return;
        }
        if let Some(md) = self.description_markdown() {
            self.editor = EditorState::from_text(&md);
            self.begin_description_edit_target();
            self.screen = Screen::Edit;
        }
    }

    /// Prime the edit-target state for a description edit without touching
    /// `self.editor` or `self.screen` — used both by `begin_tui_edit` (the
    /// in-TUI editor) and the external `$EDITOR` round-trip (`E`), which
    /// calls `finish_edit` directly once the editor process exits.
    pub fn begin_description_edit_target(&mut self) {
        self.edit_target = EditTarget::Description;
        self.edit_key = self.detail.as_ref().map(|d| d.key.clone());
        self.edit_return_screen = Screen::Detail;
    }

    /// Prime the edit-target state for the external `$EDITOR` round-trip.
    /// Must run before `request_edit` is set, since `finish_edit` (called
    /// once the editor exits) doesn't know what it's editing on its own —
    /// only `begin_tui_edit`/`begin_comment` normally set that up. Guarding
    /// here (rather than only in `begin_tui_edit`/`begin_comment`) keeps the
    /// `E` round-trip from starting a second edit while a previous one is
    /// still resolving against live Jira; callers should check the return
    /// value before setting `request_edit`.
    pub fn begin_external_edit(&mut self) -> bool {
        if self.edit_pending {
            self.status = "an update is still in progress".into();
            return false;
        }
        self.begin_description_edit_target();
        true
    }

    /// Open the built-in editor to compose a brand-new comment. Works from
    /// both the full detail screen and the quick-view panel (List/Home),
    /// returning to whichever screen it was opened from.
    pub fn begin_comment(&mut self) {
        if self.edit_pending {
            self.status = "an update is still in progress".into();
            return;
        }
        let Some(key) = self.comment_target_key() else {
            self.status = "no issue selected".into();
            return;
        };
        self.editor = EditorState::from_text("");
        self.begin_comment_edit_target(key, self.screen);
        self.screen = Screen::Edit;
    }

    /// Prime the edit-target state for a comment without touching
    /// `self.editor` or `self.screen` — the comment-composition counterpart
    /// to `begin_description_edit_target`, used by the external `$EDITOR`
    /// round-trip (`C`), which calls `finish_edit` directly once the editor
    /// process exits.
    fn begin_comment_edit_target(&mut self, key: String, return_screen: Screen) {
        self.edit_target = EditTarget::Comment;
        self.edit_key = Some(key);
        self.edit_return_screen = return_screen;
    }

    /// Prime the edit-target state for a new issue's description, without
    /// touching `self.editor`/`self.screen` (the caller, `App::
    /// confirm_new_issue_form`, seeds the editor and sets the screen itself)
    /// — the new-issue counterpart to `begin_description_edit_target`/
    /// `begin_comment_edit_target`. `edit_key` stays `None`: there's no
    /// issue key yet. `edit_return_screen` is `Screen::NewIssue`, not
    /// `Screen::Detail`, so backing out of the description step (Esc)
    /// returns to the compose form with its project/type/summary intact,
    /// not away from the in-progress issue entirely.
    pub(crate) fn begin_new_issue_description_edit_target(&mut self) {
        self.edit_target = EditTarget::NewIssue;
        self.edit_key = None;
        self.edit_return_screen = Screen::NewIssue;
    }

    /// Prime the edit-target state for composing a comment via the external
    /// `$EDITOR` round-trip. Mirrors `begin_external_edit`'s guard against
    /// starting a second edit while a previous one is still resolving
    /// against live Jira; callers should check the return value before
    /// setting `request_edit`.
    pub fn begin_external_comment(&mut self) -> bool {
        if self.edit_pending {
            self.status = "an update is still in progress".into();
            return false;
        }
        let Some(key) = self.comment_target_key() else {
            self.status = "no issue selected".into();
            return false;
        };
        self.begin_comment_edit_target(key, self.screen);
        true
    }

    /// The issue key comments should be added to, given the current screen:
    /// the open detail issue, or (from the list/quick-view) the selected
    /// issue's cached detail.
    fn comment_target_key(&self) -> Option<String> {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => {
                self.detail.as_ref().map(|d| d.key.clone())
            }
            _ => self.quick_view_detail().map(|d| d.key.clone()),
        }
    }

    /// Compile the editor buffer and move to the confirmation preview.
    pub fn commit_tui_edit(&mut self) {
        let text = self.editor.to_text();
        self.finish_edit(&text);
    }

    /// Markdown for the current issue description, to seed an editor session.
    pub fn description_markdown(&self) -> Option<String> {
        self.detail
            .as_ref()
            .map(|d| crate::adf::to_markdown(&d.description))
    }

    /// Compile edited Markdown to ADF and show it for confirmation. For a
    /// new issue specifically, an empty buffer means "no description at
    /// all" (`pending_edit = None`) rather than a technically-non-empty-but-
    /// visually-empty ADF document — a new issue's description is genuinely
    /// optional, unlike Description/Comment, which always have real content
    /// to compile by the time this runs.
    pub fn finish_edit(&mut self, markdown: &str) {
        self.pending_edit =
            if self.edit_target == EditTarget::NewIssue && markdown.trim().is_empty() {
                None
            } else {
                Some(crate::adf::compile(markdown))
            };
        self.detail_scroll = 0;
        self.screen = Screen::Preview;
    }

    pub fn cancel_edit(&mut self) {
        self.pending_edit = None;
        let return_screen = self.edit_return_screen;
        self.reset_edit_target();
        self.screen = return_screen;
    }

    /// Back out of the preview screen — used by every "back/cancel" key on
    /// `Screen::Preview` (Esc/q/h/Left/Backspace). If there's content worth
    /// keeping, restore it into the in-TUI editor and return to
    /// `Screen::Edit` for further changes instead of discarding it; only a
    /// genuinely empty edit (nothing typed, or everything deleted) falls
    /// through to a full `cancel_edit`. `pending_edit` holds the latest
    /// compiled content regardless of whether this session started in the
    /// in-TUI editor or the external `$EDITOR` round-trip, so this works
    /// the same way for both — the external round-trip's own temp file is
    /// already gone by the time `Screen::Preview` is showing, but its
    /// content lives on here.
    pub fn back_out_of_preview(&mut self) {
        let markdown = self
            .pending_edit
            .as_ref()
            .map(crate::adf::to_markdown)
            .unwrap_or_default();
        // A new issue's description is genuinely optional (see
        // `finish_edit`'s `None`-for-empty handling), so an empty preview
        // must still step back to the description editor to keep composing
        // the rest of the issue — never a full cancel, which would discard
        // the project/type/summary the user already entered.
        if markdown.trim().is_empty() && self.edit_target != EditTarget::NewIssue {
            self.cancel_edit();
            return;
        }
        // `crate::adf::to_markdown` always appends a trailing `\n` if the
        // compiled Markdown doesn't already end with one, which
        // `EditorState::from_text`'s `split('\n')` would otherwise turn
        // into a phantom empty last line the user never typed.
        self.editor = EditorState::from_text_dirty(markdown.trim_end_matches('\n'));
        self.pending_edit = None;
        self.screen = Screen::Edit;
    }

    /// Clear the edit-target state at the end of a compose session (apply or
    /// cancel) so it can never leak into an unrelated later edit — most
    /// importantly the external `$EDITOR` round-trip, which doesn't call
    /// `begin_tui_edit`/`begin_comment` and so can't re-prime a fresh target
    /// itself.
    pub(crate) fn reset_edit_target(&mut self) {
        self.edit_target = EditTarget::default();
        self.edit_key = None;
        self.edit_return_screen = Screen::Detail;
    }

    /// Apply the previewed edit — either the description update or a new
    /// comment — live if possible, always locally.
    pub fn apply_edit(&mut self) {
        // Guards against a duplicate live write: none of the three targets'
        // apply methods can tell "genuinely nothing pending" apart from
        // "already submitted, waiting on the network" on their own (a new
        // issue's description is legitimately optional, so `pending_edit`
        // being `None` can't double as that signal the way it does for
        // Description/Comment). Re-entry is reachable in the ordinary UI,
        // not just via key-repeat: `back_out_of_preview`'s `NewIssue` branch
        // (see above) returns to `Screen::Edit` without discarding anything,
        // so pressing Esc then re-confirming while the first submission is
        // still in flight would otherwise dispatch a second one.
        if self.edit_pending {
            self.status = "an update is still in progress".into();
            return;
        }
        match self.edit_target {
            EditTarget::Description => self.apply_description_edit(),
            EditTarget::Comment => self.apply_comment(),
            EditTarget::NewIssue => self.apply_new_issue(),
        }
    }

    /// Apply the previewed description edit (live if possible, always
    /// locally). Demo/cache sessions apply inline; a live session dispatches
    /// off the render thread and lands on `return_screen` once the update
    /// resolves — see `dispatch_update_description`.
    fn apply_description_edit(&mut self) {
        let return_screen = self.edit_return_screen;
        self.reset_edit_target();
        let Some(adf) = self.pending_edit.take() else {
            self.screen = return_screen;
            return;
        };
        let Some(key) = self.detail.as_ref().map(|d| d.key.clone()) else {
            self.screen = return_screen;
            return;
        };

        if !matches!(self.source, Source::Live { .. }) {
            if let Some(d) = self.detail.as_mut() {
                d.description = adf;
            }
            self.status = format!("updated {key} description");
            self.flash("✓ description updated");
            self.trigger_jax_party();
            self.screen = return_screen;
            return;
        }

        self.edit_generation += 1;
        let generation = self.edit_generation;
        self.edit_pending = true;
        self.loading = true;
        self.status = format!("↻ updating {key}…");
        let tx = self.events_tx.clone();
        async_ops::dispatch_update_description(tx, generation, key, adf, return_screen);
    }

    /// Post the previewed comment (live if possible, always locally). Demo/
    /// cache sessions apply the optimistic local comment inline; a live
    /// session dispatches the post off the render thread and appends
    /// whichever comment comes back — server copy on success, the same
    /// optimistic one on failure/no-credentials — once it resolves. See
    /// `dispatch_add_comment`.
    fn apply_comment(&mut self) {
        let return_screen = self.edit_return_screen;
        let Some(key) = self.edit_key.take() else {
            self.reset_edit_target();
            self.screen = return_screen;
            return;
        };
        self.reset_edit_target();
        let Some(adf) = self.pending_edit.take() else {
            self.screen = return_screen;
            return;
        };

        if !matches!(self.source, Source::Live { .. }) {
            let comment = Comment {
                id: format!("local-{}", self.tick),
                author: self.current_user_display(),
                created: "just now".into(),
                body: adf,
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
            self.screen = return_screen;
            return;
        }

        self.edit_generation += 1;
        let generation = self.edit_generation;
        self.edit_pending = true;
        self.loading = true;
        self.status = format!("↻ adding comment to {key}…");
        let local_author = self.current_user_display();
        let local_id = format!("local-{}", self.tick);
        let tx = self.events_tx.clone();
        async_ops::dispatch_add_comment(
            tx,
            generation,
            key,
            adf,
            local_author,
            local_id,
            return_screen,
        );
    }

    /// `Ctrl+V` (Edit screen only, see `src/keys/mod.rs`): best-effort read
    /// of an image off the OS clipboard into a stable temp file, via
    /// `infra::clipboard_image`. A successful capture feeds straight into
    /// the shared upload-and-embed pipeline (`begin_image_embed`) — the same
    /// one a dropped/pasted image path reaches via `app::paste`'s
    /// `handle_editor_paste` — rather than being applied immediately; see
    /// that module's doc comment for the full capture rationale (why this
    /// needs external tools at all, what's tried on which platform).
    pub fn paste_clipboard_image(&mut self) {
        use crate::infra::clipboard_image::{capture_clipboard_image, ClipboardImageOutcome};
        match capture_clipboard_image() {
            ClipboardImageOutcome::Captured(path) => {
                self.begin_image_embed(path);
            }
            ClipboardImageOutcome::NoToolAvailable(hint) => {
                self.status = hint;
            }
            ClipboardImageOutcome::NoImage => {
                self.status = "clipboard has no image".into();
            }
            ClipboardImageOutcome::Failed(e) => {
                self.status = format!("couldn't read clipboard image: {e}");
            }
        }
    }

    /// Stage `path` (a captured clipboard image, or a dropped/pasted image
    /// file) for the shared upload-and-embed pipeline: raises the inline
    /// confirm prompt `ui::editor` renders over the buffer (CLAUDE.md's
    /// "Preview before any mutating Jira call") rather than uploading
    /// immediately. `keys::handle_key` captures every keypress while this is
    /// showing — `y`/Enter (`confirm_image_embed`) actually dispatches the
    /// upload; `Esc` (`decline_image_embed`) inserts the raw path as plain
    /// text instead, matching what an ordinary (non-image) pasted path
    /// already does via `insert_str`.
    pub(crate) fn begin_image_embed(&mut self, path: PathBuf) {
        self.pending_image_embed = Some(path);
    }

    /// `Esc` while `pending_image_embed` is showing: decline the embed and
    /// fall back to inserting the path as plain text — the same place in
    /// the buffer a non-image pasted path already lands via `insert_str`,
    /// so declining leaves the buffer exactly as an ordinary text paste
    /// would have.
    pub fn decline_image_embed(&mut self) {
        let Some(path) = self.pending_image_embed.take() else {
            return;
        };
        self.editor.insert_str(&path.display().to_string());
    }

    /// `y`/Enter while `pending_image_embed` is showing: upload the staged
    /// image as an attachment on the issue currently being edited and, once
    /// a Media Services UUID can be resolved for it, embed it as a real
    /// inline `adf-media://` token at the cursor — see
    /// `async_ops::dispatch_image_embed`/`App::apply_image_embedded`.
    /// Demo/cache sessions get the same "not available" guard every other
    /// mutating action uses (mirrors `App::confirm_attachment_upload`
    /// exactly: flash and stop, no local fallback insert — matching that the
    /// dedicated attachment-upload flow doesn't insert anything either in
    /// this case, it just closes).
    pub fn confirm_image_embed(&mut self) {
        let Some(path) = self.pending_image_embed.take() else {
            return;
        };
        // A brand-new issue's description (`EditTarget::NewIssue`) has no
        // key yet — there's no issue to attach the upload to, so this falls
        // back to plain text the same way a decline does, rather than
        // silently discarding the image reference the user just staged.
        let Some(key) = self.edit_key.clone() else {
            self.editor.insert_str(&path.display().to_string());
            self.status = "no issue to attach the image to yet — inserted as plain text".into();
            return;
        };
        if !matches!(self.source, Source::Live { .. }) {
            self.flash("demo mode — uploading needs live Jira credentials");
            return;
        }
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".to_string());
        let mime = crate::mime::guess_mime(&filename);
        self.image_embed_generation += 1;
        let generation = self.image_embed_generation;
        self.image_embed_pending = true;
        self.loading = true;
        self.status = format!("↻ uploading {filename}…");
        let tx = self.events_tx.clone();
        async_ops::dispatch_image_embed(tx, generation, key, path, filename, mime);
    }

    /// Display name to attribute a locally-composed comment to before any
    /// live response comes back (or in demo/cache mode, where there is none).
    /// Also the "who am I" `render::wide_detail`/`narrow_detail` use to pick
    /// out which comment cards get the maple own-comment rule.
    pub(crate) fn current_user_display(&self) -> String {
        match &self.source {
            Source::Live { user, .. } | Source::Cache { user } => user.clone(),
            Source::Demo => "you".into(),
        }
    }
}
