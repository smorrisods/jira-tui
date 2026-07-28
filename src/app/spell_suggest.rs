//! The spelling-suggestion picker (`F2`, built-in editor only): finds the
//! misspelled word under the cursor and offers replacements from
//! `crate::spellcheck`.

use crate::spellcheck;

use super::App;

/// State for the open spelling-suggestion picker.
#[derive(Clone, Debug, Default)]
pub struct SpellSuggestState {
    /// The line and byte range (within that line) of the flagged word this
    /// picker is offering replacements for, so `confirm_spell_suggest` can
    /// apply the chosen one back to the exact spot it was found.
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub suggestions: Vec<String>,
    pub selected: usize,
}

impl App {
    /// Open the spelling-suggestion picker for the misspelled word the
    /// cursor is currently on (or immediately after), if any.
    pub fn open_spell_suggest(&mut self) {
        let line_idx = self.editor.cy;
        let Some(line) = self.editor.lines.get(line_idx).cloned() else {
            return;
        };
        let cursor_byte = self.editor.cursor_byte_index();
        let Some((start, end)) = spellcheck::misspelled_spans(&line)
            .into_iter()
            .find(|(s, e)| cursor_byte >= *s && cursor_byte <= *e)
        else {
            self.status = "no misspelled word here".into();
            return;
        };
        let suggestions = spellcheck::suggestions(&line[start..end]);
        if suggestions.is_empty() {
            self.status = format!("no suggestions for \"{}\"", &line[start..end]);
            return;
        }
        self.spell_suggest = SpellSuggestState {
            line: line_idx,
            start,
            end,
            suggestions,
            selected: 0,
        };
        self.spell_suggest_open = true;
    }

    pub fn close_spell_suggest(&mut self) {
        self.spell_suggest_open = false;
    }

    pub fn spell_suggest_move(&mut self, delta: isize) {
        let len = self.spell_suggest.suggestions.len();
        if len == 0 {
            return;
        }
        let mut idx = self.spell_suggest.selected as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.spell_suggest.selected = idx as usize;
    }

    /// Apply the highlighted suggestion to the buffer and close the picker.
    pub fn confirm_spell_suggest(&mut self) {
        let s = std::mem::take(&mut self.spell_suggest);
        self.spell_suggest_open = false;
        if let Some(replacement) = s.suggestions.get(s.selected) {
            self.editor
                .replace_range(s.line, s.start, s.end, replacement);
        }
    }
}
