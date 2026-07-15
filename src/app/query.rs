//! Small cross-cutting `App` query helpers that don't belong to any single
//! concern module: selection, window title, toasts, and the at-a-glance
//! counts Home's rail renders.

use crate::domain::{IssueSummary, Source};

use super::{App, Screen};

impl App {
    pub fn selected_issue(&self) -> Option<&IssueSummary> {
        self.issues.get(self.selected)
    }

    /// The terminal window title for the app's current state: the key and
    /// summary of the issue actually being viewed (full detail, its preview
    /// or edit flow, or the quick-view panel), falling back to a plain
    /// `jira-tui` outside those screens. Pure state → `String`; the run loop
    /// is responsible for actually issuing a `SetTitle` command only when
    /// this changes, so it stays testable without a real terminal.
    pub fn window_title(&self) -> String {
        const BASE: &str = "jira-tui";
        let issue = match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => self
                .detail
                .as_ref()
                .map(|d| (d.key.as_str(), d.summary.as_str())),
            Screen::List if self.quick_view => self
                .selected_issue()
                .map(|i| (i.key.as_str(), i.summary.as_str())),
            _ => None,
        };
        match issue {
            Some((key, summary)) => format!("{key}: {summary} — {BASE}"),
            None => BASE.to_string(),
        }
    }

    /// Show a transient toast for roughly 1.5s (tied to the animation tick).
    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash_msg = msg.into();
        self.flash_until = self.tick + 18;
    }

    /// The active toast message, if one is currently showing.
    pub fn active_flash(&self) -> Option<&str> {
        if self.tick < self.flash_until && !self.flash_msg.is_empty() {
            Some(&self.flash_msg)
        } else {
            None
        }
    }

    /// The Jira URL for the selected issue, when we know the site.
    pub fn selected_issue_url(&self) -> Option<String> {
        let issue = self.selected_issue()?;
        let site = match &self.source {
            Source::Live { site, .. } => site.clone(),
            // Demo data has no real Jira site behind it; use an obviously
            // fake placeholder host rather than a real organization's Jira.
            _ => "demo.atlassian.net".to_string(),
        };
        Some(format!("https://{site}/browse/{}", issue.key))
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.issues.is_empty() {
            return;
        }
        let rows = self.tree_rows();
        let cur_pos = rows
            .iter()
            .position(|(i, _)| *i == self.selected)
            .unwrap_or(0);
        let mut pos = cur_pos as isize + delta;
        pos = pos.clamp(0, rows.len() as isize - 1);
        self.selected = rows[pos as usize].0;
        self.quick_view_scroll = 0;
        self.link_index = 0;
    }

    pub fn assigned_to_me(&self) -> Vec<&IssueSummary> {
        self.all_issues
            .iter()
            .filter(|i| i.assignee.is_some() && i.status != "Done")
            .collect()
    }

    pub fn blocked(&self) -> Vec<&IssueSummary> {
        self.all_issues.iter().filter(|i| i.blocked).collect()
    }
}
