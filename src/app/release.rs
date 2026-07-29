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

use std::collections::HashSet;

use crate::domain::{IssueSummary, Source, Version};

use super::{async_ops, App, Screen};

/// Which bulk mutation a `dispatch_release_bulk_*` call/`AppEvent::ReleaseBulkApplied`
/// is carrying out — both share the same "fetch current fix_versions, edit
/// the one field, write the whole array back" blocking implementation
/// (`release_bulk_blocking`), differing only in whether the target version
/// is added to or removed from that array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseBulkKind {
    Add,
    Remove,
}

/// The release review screen's state. `drilled` distinguishes the two
/// modes: `None` is the version list (`cursor` indexes `versions`), `Some`
/// is a specific version's issue list (`issue_cursor` indexes `issues`,
/// which is always kept sorted by status so the flat cursor and the
/// grouped-by-status render agree on order — see `App::release_status_groups`).
/// `selected` is drill-mode-only: issue keys checked via `Space`, acted on
/// in bulk by `release_remove_selected`.
#[derive(Clone, Debug, Default)]
pub struct ReleaseState {
    pub cursor: usize,
    pub versions: Vec<Version>,
    pub drilled: Option<Version>,
    pub issues: Vec<IssueSummary>,
    pub issues_loading: bool,
    pub issue_cursor: usize,
    pub selected: HashSet<String>,
    /// Whether a bulk add/remove is currently in flight — guards against
    /// re-triggering one while another is still resolving, mirroring
    /// `version_pending`.
    pub bulk_pending: bool,
}

impl App {
    /// `w` — open the release review screen at the version list.
    pub fn open_release_screen(&mut self) {
        self.release.versions = self.project_versions_source();
        self.release.cursor = 0;
        self.release.drilled = None;
        self.release.issues.clear();
        self.release.issue_cursor = 0;
        self.release.selected.clear();
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

    /// `pub(crate)` (not just called from `release_confirm`) since a
    /// successful bulk-add from Search (`App::release_add_to_release`)
    /// re-drills the same version afterward to refresh what's shown.
    pub(crate) fn open_release_drill(&mut self, version: Version) {
        self.release.issue_cursor = 0;
        self.release.issues.clear();
        self.release.selected.clear();
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
        self.release.selected.clear();
        self.release_generation += 1; // invalidate any in-flight fetch
        true
    }

    /// `Space` (drill mode only) — toggle the highlighted issue into/out of
    /// `release.selected`, the pending set `release_remove_selected` acts on.
    pub fn release_toggle_selected(&mut self) {
        let Some(issue) = self.release.issues.get(self.release.issue_cursor) else {
            return;
        };
        let key = issue.key.clone();
        if !self.release.selected.remove(&key) {
            self.release.selected.insert(key);
        }
    }

    /// `x` (drill mode only) — remove every selected issue from the drilled
    /// version, or just the highlighted one if nothing's explicitly
    /// selected (so a bare `x` on one issue doesn't require `Space` first).
    /// Demo/cache sessions apply locally inline; a genuine live session
    /// dispatches off the render thread — see `dispatch_release_bulk_remove`.
    pub fn release_remove_selected(&mut self) {
        if self.release.bulk_pending {
            self.status = "a release update is already in progress".into();
            return;
        }
        let Some(version) = self.release.drilled.clone() else {
            return;
        };
        let keys = self.release_bulk_target_keys();
        if keys.is_empty() {
            return;
        }

        if !matches!(self.source, Source::Live { .. }) {
            for key in &keys {
                let mut fix_versions = crate::domain::demo_detail(key).fix_versions;
                fix_versions.retain(|v| v != &version.name);
                self.apply_versions_locally(key, Some(fix_versions), None);
            }
            self.release.issues.retain(|i| !keys.contains(&i.key));
            self.release.selected.clear();
            let len = self.release.issues.len();
            self.release.issue_cursor = self.release.issue_cursor.min(len.saturating_sub(1));
            self.status = format!("removed {} issue(s) from {}", keys.len(), version.name);
            self.flash(format!("✓ removed {} from {}", keys.len(), version.name));
            return;
        }

        self.release_bulk_generation += 1;
        let generation = self.release_bulk_generation;
        self.release.bulk_pending = true;
        self.loading = true;
        self.status = format!("↻ removing {} issue(s) from {}…", keys.len(), version.name);
        let tx = self.events_tx.clone();
        async_ops::dispatch_release_bulk(
            tx,
            generation,
            version.name,
            keys,
            ReleaseBulkKind::Remove,
        );
    }

    /// Add `keys` to `version_name`'s membership — the confirm step of the
    /// Search screen's bulk-add mode (`App::open_search_for_release`).
    /// Demo/cache sessions apply locally inline; a genuine live session
    /// dispatches off the render thread. Either way, if the release drill-
    /// down is still open on this exact version, it's refreshed afterward
    /// so newly-added issues show up without a manual back-and-in.
    pub(crate) fn release_add_to_release(&mut self, version_name: String, keys: Vec<String>) {
        if keys.is_empty() {
            return;
        }
        if !matches!(self.source, Source::Live { .. }) {
            let drilled_here = self.release.drilled.as_ref().map(|v| v.name.as_str())
                == Some(version_name.as_str());
            for key in &keys {
                let mut fix_versions = crate::domain::demo_detail(key).fix_versions;
                if !fix_versions.contains(&version_name) {
                    fix_versions.push(version_name.clone());
                }
                self.apply_versions_locally(key, Some(fix_versions), None);
                // Demo data has no real mutable backing store to re-fetch
                // from (`domain::demo_issues_for_version` always recomputes
                // from the same fixed mapping), so a from-scratch refresh
                // like the live path takes below would silently undo this —
                // splice the newly-added issue into the visible list
                // directly instead, sourced from `domain::demo_issues()`
                // (the same lightweight-summary lookup `all_issues` itself
                // is seeded from).
                if drilled_here && !self.release.issues.iter().any(|i| &i.key == key) {
                    if let Some(issue) = crate::domain::demo_issues()
                        .into_iter()
                        .find(|i| &i.key == key)
                    {
                        self.release.issues.push(issue);
                    }
                }
            }
            if drilled_here {
                self.release.issues.sort_by(|a, b| a.status.cmp(&b.status));
            }
            self.status = format!("added {} issue(s) to {version_name}", keys.len());
            self.flash(format!("✓ added {} to {version_name}", keys.len()));
            return;
        }

        self.release_bulk_generation += 1;
        let generation = self.release_bulk_generation;
        self.release.bulk_pending = true;
        self.loading = true;
        self.status = format!("↻ adding {} issue(s) to {version_name}…", keys.len());
        let tx = self.events_tx.clone();
        async_ops::dispatch_release_bulk(tx, generation, version_name, keys, ReleaseBulkKind::Add);
    }

    /// `release.selected` if non-empty, else just the highlighted issue —
    /// shared by `release_remove_selected` so a bare action key works on a
    /// single issue without requiring `Space` first.
    fn release_bulk_target_keys(&self) -> Vec<String> {
        if !self.release.selected.is_empty() {
            return self.release.selected.iter().cloned().collect();
        }
        self.release
            .issues
            .get(self.release.issue_cursor)
            .map(|i| vec![i.key.clone()])
            .unwrap_or_default()
    }

    /// Apply a single add/remove delta to `fix_versions` wherever `key` is
    /// cached (`self.detail`/`detail_cache`) — the bulk counterpart to
    /// `apply_versions_locally`, which takes a full replacement array
    /// instead of a delta; `apply_release_bulk_applied` only knows "add/
    /// remove this one version", not the issue's other fix versions (that's
    /// exactly why `release_bulk_blocking` fetches the issue's current
    /// array before writing, rather than trusting local state).
    pub(crate) fn apply_versions_locally_for_bulk(
        &mut self,
        key: &str,
        version_name: &str,
        kind: ReleaseBulkKind,
    ) {
        let apply = |fix_versions: &mut Vec<String>| match kind {
            ReleaseBulkKind::Add => {
                if !fix_versions.iter().any(|v| v == version_name) {
                    fix_versions.push(version_name.to_string());
                }
            }
            ReleaseBulkKind::Remove => fix_versions.retain(|v| v != version_name),
        };
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                apply(&mut d.fix_versions);
            }
        }
        if let Some(d) = self.detail_cache.get_mut(key) {
            apply(&mut d.fix_versions);
        }
    }

    /// Re-drill the current version if the drill-down is still showing
    /// `version_name` — shared by `release_add_to_release`'s two paths (this
    /// module's own demo/cache branch, and `apply_release_bulk_applied`'s
    /// live-success branch).
    pub(crate) fn refresh_release_drill_if_showing(&mut self, version_name: &str) {
        if self.release.drilled.as_ref().map(|v| v.name.as_str()) == Some(version_name) {
            let version = self.release.drilled.clone().unwrap();
            self.open_release_drill(version);
        }
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
