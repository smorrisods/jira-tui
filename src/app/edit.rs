//! Description editing: the built-in multi-line Markdown editor and the
//! ADF round-trip (compile → preview → apply).

#[cfg(feature = "live")]
use crate::domain::Source;

use super::{App, Screen};

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
        if let Some(md) = self.description_markdown() {
            self.editor = EditorState::from_text(&md);
            self.screen = Screen::Edit;
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
        self.screen = Screen::Detail;
    }

    /// Apply the previewed description (live if possible, always locally).
    pub fn apply_edit(&mut self) {
        let Some(adf) = self.pending_edit.take() else {
            self.screen = Screen::Detail;
            return;
        };
        let Some(key) = self.detail.as_ref().map(|d| d.key.clone()) else {
            self.screen = Screen::Detail;
            return;
        };

        #[cfg(feature = "live")]
        {
            if let Source::Live { .. } = self.source {
                if let Some(cfg) = crate::jira::Config::load() {
                    if let Err(e) = crate::jira::update_description(&cfg, &key, &adf) {
                        self.status = format!("update failed: {e}");
                        self.screen = Screen::Detail;
                        return;
                    }
                }
            }
        }

        if let Some(d) = self.detail.as_mut() {
            d.description = adf;
        }
        self.status = format!("updated {key} description");
        self.flash("✓ description updated");
        self.screen = Screen::Detail;
    }
}
