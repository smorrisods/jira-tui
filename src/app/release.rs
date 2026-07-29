//! The release review screen (`w`): browsing the project's versions, then
//! drilling into one to see its issues grouped by status with a done/total
//! progress line. Two-level navigation within one `Screen::Release` (list ↔
//! drill), the same "one screen, internal state decides what's drawn" shape
//! `app::board` uses for its own nested lane/column/card cursor, rather than
//! a second `Screen` variant.
//!
//! Fetching a version's issues mirrors the Search screen's live text-search
//! fallback (`app::search`): a live session dispatches a JQL search
//! (`jira::live::search::jql_for_version`) off the render thread; demo/cache
//! sessions resolve synchronously from `domain::demo_issues_for_version`,
//! matching `assign.rs`/`versions.rs`'s "any non-Live source gets the demo
//! data" convention rather than trying to derive it from whatever's already
//! loaded into `all_issues`.

use crate::domain::{IssueSummary, Source, Version};

use super::{async_ops, App, Screen};

/// The release review screen's state. `drilled` distinguishes the two
/// modes: `None` is the version list (`cursor` indexes `versions`), `Some`
/// is a specific version's issue list (`issue_cursor` indexes `issues`,
/// which is always kept sorted by status so the flat cursor and the
/// grouped-by-status render agree on order — see `App::release_status_groups`).
#[derive(Clone, Debug, Default)]
pub struct ReleaseState {
    pub cursor: usize,
    pub versions: Vec<Version>,
    pub drilled: Option<Version>,
    pub issues: Vec<IssueSummary>,
    pub issues_loading: bool,
    pub issue_cursor: usize,
}

impl App {
    /// `w` — open the release review screen at the version list.
    pub fn open_release_screen(&mut self) {
        self.release.versions = self.project_versions_source();
        self.release.cursor = 0;
        self.release.drilled = None;
        self.release.issues.clear();
        self.release.issue_cursor = 0;
        self.screen = Screen::Release;
    }

    /// Move the highlight: through `versions` in list mode, or through
    /// `issues` in drill mode.
    pub fn release_move(&mut self, delta: isize) {
        if self.release.drilled.is_some() {
            let len = self.release.issues.len();
            if len == 0 {
                return;
            }
            let idx = (self.release.issue_cursor as isize + delta).clamp(0, len as isize - 1);
            self.release.issue_cursor = idx as usize;
        } else {
            let len = self.release.versions.len();
            if len == 0 {
                return;
            }
            let idx = (self.release.cursor as isize + delta).clamp(0, len as isize - 1);
            self.release.cursor = idx as usize;
        }
    }

    /// `⏎`/`→` — drill into the highlighted version, or open the
    /// highlighted issue if already drilled in.
    pub fn release_confirm(&mut self) {
        if self.release.drilled.is_some() {
            if let Some(issue) = self.release.issues.get(self.release.issue_cursor) {
                let key = issue.key.clone();
                self.open_by_key(&key);
            }
            return;
        }
        let Some(version) = self.release.versions.get(self.release.cursor).cloned() else {
            return;
        };
        self.open_release_drill(version);
    }

    fn open_release_drill(&mut self, version: Version) {
        self.release.issue_cursor = 0;
        self.release.issues.clear();
        if !matches!(self.source, Source::Live { .. }) {
            let mut issues = crate::domain::demo_issues_for_version(&version.name);
            issues.sort_by(|a, b| a.status.cmp(&b.status));
            self.release.issues = issues;
            self.release.drilled = Some(version);
            return;
        }
        self.release_generation += 1;
        let generation = self.release_generation;
        self.release.issues_loading = true;
        self.release.drilled = Some(version.clone());
        let tx = self.events_tx.clone();
        async_ops::dispatch_release_issues(tx, generation, version.name);
    }

    /// `esc`/`←` while drilled in — back out to the version list rather than
    /// leaving the screen entirely. Returns `false` (so the caller falls
    /// through to the screen's normal back/quit handling) when already at
    /// the version list.
    pub fn release_back(&mut self) -> bool {
        if self.release.drilled.is_none() {
            return false;
        }
        self.release.drilled = None;
        self.release.issues.clear();
        self.release.issues_loading = false;
        self.release_generation += 1; // invalidate any in-flight fetch
        true
    }

    /// The drilled version's issues, grouped into contiguous runs by
    /// status — valid because `self.release.issues` is always kept sorted
    /// by status (both the demo path above and `apply_release_issues_loaded`
    /// sort it), so a run boundary is a real status change, not an artifact
    /// of fetch order.
    pub(crate) fn release_status_groups(&self) -> Vec<(String, Vec<&IssueSummary>)> {
        let mut groups: Vec<(String, Vec<&IssueSummary>)> = Vec::new();
        for issue in &self.release.issues {
            match groups.last_mut() {
                Some((status, items)) if status.as_str() == issue.status.as_str() => {
                    items.push(issue)
                }
                _ => groups.push((issue.status.clone(), vec![issue])),
            }
        }
        groups
    }

    /// `(done, total)` for the drilled release's progress line — "done"
    /// meaning the literal `"Done"` status, same simplification
    /// `board_wide_lanes` falls back to when a workflow's own terminal
    /// status isn't otherwise known (`IssueSummary` doesn't carry Jira's
    /// status *category*, only its display name).
    pub(crate) fn release_progress(&self) -> (usize, usize) {
        let total = self.release.issues.len();
        let done = self
            .release
            .issues
            .iter()
            .filter(|i| i.status == "Done")
            .count();
        (done, total)
    }
}
