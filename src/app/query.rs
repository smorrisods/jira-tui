//! Small cross-cutting `App` query helpers that don't belong to any single
//! concern module: selection, window title, toasts, and the at-a-glance
//! counts Home's rail renders.

use crate::domain::{IssueSummary, Source};

use super::{App, Screen};

impl App {
    pub fn selected_issue(&self) -> Option<&IssueSummary> {
        self.issues.get(self.selected)
    }

    /// `a` (or the command palette's "about"): open the About screen,
    /// remembering where it was opened from (issue #38) so backing out
    /// restores it — but only when not already in About, or a second call
    /// would overwrite that memory with About itself.
    pub fn open_about(&mut self) {
        if self.screen != Screen::About {
            self.about_return_screen = self.screen;
        }
        self.screen = Screen::About;
    }

    /// The list-summary "updated" relative timestamp for `key`, if it's
    /// present in the currently loaded list data — `IssueDetail` itself
    /// carries no `updated` field, so the Detail screen's people & meta /
    /// facts panels borrow it from whichever `IssueSummary` list already
    /// has it, falling back to an em dash for an issue opened outside the
    /// current view (e.g. followed via an in-body link).
    pub(crate) fn issue_updated(&self, key: &str) -> &str {
        self.all_issues
            .iter()
            .chain(self.issues.iter())
            .find(|i| i.key == key)
            .map(|i| i.updated.as_str())
            .unwrap_or("—")
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

    /// Force Jax's party scene for a few seconds (SPEC.md §9: a successful
    /// transition-to-Done/edit/comment) — same "forced state until a tick
    /// deadline" shape as `flash`/`flash_until`, just longer (`flash`'s
    /// ~1.5s toast is `tick + 18`; "a few seconds" here is `tick + 36`).
    /// Only visible if Jax is already showing (mini or full) — this never
    /// forces Jax onto screen by itself.
    pub(crate) fn trigger_jax_party(&mut self) {
        self.jax_party_until = self.tick + 36;
    }

    /// The Jira browse URL for an arbitrary issue key — shared by
    /// `selected_issue_url` and the command palette (`app::palette`, SPEC.md
    /// §8), which needs to build a URL for a Board-selected card that isn't
    /// necessarily `selected_issue()`.
    pub fn issue_url_for(&self, key: &str) -> String {
        let site = match &self.source {
            Source::Live { site, .. } => site.clone(),
            // Demo data has no real Jira site behind it; use an obviously
            // fake placeholder host rather than a real organization's Jira.
            _ => "demo.atlassian.net".to_string(),
        };
        format!("https://{site}/browse/{key}")
    }

    /// The Jira URL for the selected issue, when we know the site.
    pub fn selected_issue_url(&self) -> Option<String> {
        Some(self.issue_url_for(&self.selected_issue()?.key))
    }

    /// `y`: copy the selected issue's key to the clipboard via OSC 52.
    /// The command palette's "copy issue key" calls `copy_key_value`
    /// directly with its own already-resolved key (e.g. a Board-selected
    /// card, which `selected_issue()` doesn't reflect) rather than through
    /// this entry point — see `app::palette`.
    pub fn copy_key(&mut self) {
        let Some(issue) = self.selected_issue() else {
            return;
        };
        let key = issue.key.clone();
        self.copy_key_value(&key);
    }

    pub fn copy_key_value(&mut self, key: &str) {
        let _ = crate::infra::osc52_copy(key);
        self.status = format!("copied {key} to clipboard");
        self.flash(format!("✓ copied {key}"));
    }

    /// `Y`: copy the selected issue's browse URL to the clipboard via OSC
    /// 52. See `copy_key`'s doc comment — the palette calls
    /// `copy_url_for_key` with its own resolved key instead.
    pub fn copy_url(&mut self) {
        let Some(issue) = self.selected_issue() else {
            return;
        };
        let key = issue.key.clone();
        self.copy_url_for_key(&key);
    }

    pub fn copy_url_for_key(&mut self, key: &str) {
        let url = self.issue_url_for(key);
        let _ = crate::infra::osc52_copy(&url);
        self.status = format!("copied {url} to clipboard");
        self.flash("✓ copied issue URL");
    }

    /// The command palette's "open in browser" (SPEC.md §8) — no direct key
    /// reaches this today (only in-body links, via `app::links`); the
    /// palette is the first caller, via `open_in_browser_for_key` with its
    /// own resolved key (see `copy_key`'s doc comment). Reuses the same
    /// `issue_url_for`/`infra::open_url` primitives `copy_url`/
    /// `open_highlighted_link` already use.
    pub fn open_selected_in_browser(&mut self) {
        let Some(issue) = self.selected_issue() else {
            return;
        };
        let key = issue.key.clone();
        self.open_in_browser_for_key(&key);
    }

    pub fn open_in_browser_for_key(&mut self, key: &str) {
        let url = self.issue_url_for(key);
        match crate::infra::open_url(&url) {
            Ok(()) => self.flash(format!("↗ opened {url}")),
            Err(_) => self.status = format!("couldn't open {url}"),
        }
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

    pub fn in_review(&self) -> Vec<&IssueSummary> {
        self.all_issues
            .iter()
            .filter(|i| i.status == "In Review")
            .collect()
    }

    /// Issues marked `Done` and updated within the last 7 days. Recomputed
    /// from `updated_at` each call rather than cached, matching every other
    /// query here — issues with no parseable timestamp (`updated_at: None`)
    /// are excluded rather than assumed recent.
    pub fn done_this_week(&self) -> Vec<&IssueSummary> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
        self.all_issues
            .iter()
            .filter(|i| i.status == "Done" && i.updated_at.is_some_and(|t| t >= cutoff))
            .collect()
    }
}
