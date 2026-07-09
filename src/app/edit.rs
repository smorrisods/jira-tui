//! Description editing: the built-in multi-line Markdown editor and the
//! ADF round-trip (compile → preview → apply). Also handles adding new
//! comments, which reuse the same editor/preview flow with a different
//! apply action.

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
}

/// A minimal multi-line text editor for in-TUI description editing.
#[derive(Clone, Debug, Default)]
pub struct EditorState {
    pub lines: Vec<String>,
    pub cx: usize,
    pub cy: usize,
    pub scroll: u16,
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
        }
    }

    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    fn line_len(&self, y: usize) -> usize {
        self.lines.get(y).map(|l| l.chars().count()).unwrap_or(0)
    }

    pub fn insert_char(&mut self, c: char) {
        let line = &mut self.lines[self.cy];
        let byte = line
            .char_indices()
            .nth(self.cx)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        line.insert(byte, c);
        self.cx += 1;
    }

    pub fn newline(&mut self) {
        let line = self.lines[self.cy].clone();
        let byte = line
            .char_indices()
            .nth(self.cx)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
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
        self.edit_target = EditTarget::Comment;
        self.edit_key = Some(key);
        self.edit_return_screen = self.screen;
        self.screen = Screen::Edit;
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

    /// Compile edited Markdown to ADF and show it for confirmation.
    pub fn finish_edit(&mut self, markdown: &str) {
        let adf = crate::adf::compile(markdown);
        self.pending_edit = Some(adf);
        self.detail_scroll = 0;
        self.screen = Screen::Preview;
    }

    pub fn cancel_edit(&mut self) {
        self.pending_edit = None;
        let return_screen = self.edit_return_screen;
        self.reset_edit_target();
        self.screen = return_screen;
    }

    /// Clear the edit-target state at the end of a compose session (apply or
    /// cancel) so it can never leak into an unrelated later edit — most
    /// importantly the external `$EDITOR` round-trip, which doesn't call
    /// `begin_tui_edit`/`begin_comment` and so can't re-prime a fresh target
    /// itself.
    fn reset_edit_target(&mut self) {
        self.edit_target = EditTarget::default();
        self.edit_key = None;
        self.edit_return_screen = Screen::Detail;
    }

    /// Apply the previewed edit — either the description update or a new
    /// comment — live if possible, always locally.
    pub fn apply_edit(&mut self) {
        match self.edit_target {
            EditTarget::Description => self.apply_description_edit(),
            EditTarget::Comment => self.apply_comment(),
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

    /// Display name to attribute a locally-composed comment to before any
    /// live response comes back (or in demo/cache mode, where there is none).
    fn current_user_display(&self) -> String {
        match &self.source {
            Source::Live { user, .. } | Source::Cache { user } => user.clone(),
            Source::Demo => "you".into(),
        }
    }
}
