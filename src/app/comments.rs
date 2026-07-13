//! Jumping to and stepping between comments in the Detail screen and the
//! quick-view panel: `]`/`[` jump to the comments section / back to the
//! top, `n`/`p` step to the next/previous individual comment.

use crate::domain::IssueDetail;

use super::{App, Screen};

impl App {
    /// The detail currently shown for comment/link purposes: the open issue
    /// on the Detail screen, or the quick-view panel's cached detail
    /// everywhere else (Home/List). Shared with `app::links`.
    pub(crate) fn active_comment_detail(&self) -> Option<&IssueDetail> {
        match self.screen {
            Screen::Detail => self.detail.as_ref(),
            _ => self.quick_view_detail(),
        }
    }

    pub(crate) fn current_scroll(&self) -> u16 {
        match self.screen {
            Screen::Detail => self.detail_scroll,
            _ => self.quick_view_scroll,
        }
    }

    pub(crate) fn set_scroll(&mut self, value: u16) {
        match self.screen {
            Screen::Detail => self.detail_scroll = value,
            _ => self.quick_view_scroll = value,
        }
    }

    /// `]` — jump the scroll position to the start of the comments section.
    pub fn jump_to_comments(&mut self) {
        let Some(detail) = self.active_comment_detail() else {
            return;
        };
        let rendered = crate::render::issue_detail_lines(detail);
        match rendered.comments_header {
            Some(offset) => self.set_scroll(offset as u16),
            None => self.status = "no comments on this issue".into(),
        }
    }

    /// `[` — jump the scroll position back to the top of the panel.
    pub fn jump_to_top(&mut self) {
        self.set_scroll(0);
    }

    /// `n` — step to the next individual comment, clamped at the last one.
    pub fn next_comment(&mut self) {
        self.step_comment(1);
    }

    /// `p` — step to the previous individual comment, clamped at the first.
    pub fn prev_comment(&mut self) {
        self.step_comment(-1);
    }

    fn step_comment(&mut self, dir: isize) {
        let Some(detail) = self.active_comment_detail() else {
            return;
        };
        let rendered = crate::render::issue_detail_lines(detail);
        if rendered.comment_starts.is_empty() {
            self.status = "no comments on this issue".into();
            return;
        }
        let current = self.current_scroll() as usize;
        let target = if dir > 0 {
            rendered
                .comment_starts
                .iter()
                .find(|&&line| line > current)
                .copied()
                .unwrap_or_else(|| *rendered.comment_starts.last().unwrap())
        } else {
            rendered
                .comment_starts
                .iter()
                .rev()
                .find(|&&line| line < current)
                .copied()
                .unwrap_or_else(|| rendered.comment_starts[0])
        };
        self.set_scroll(target as u16);
    }
}
