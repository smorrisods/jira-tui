//! The sprint picker (`S`): adding, changing, or removing (back to the
//! backlog) the currently viewed issue's sprint. Sprint is single-valued per
//! issue (like assignee), not an array like fixVersions/versions, so this
//! mirrors `assign.rs`'s open/move/confirm shape closely rather than the
//! version picker's multi-select checklist — "Remove from sprint" plays the
//! same role `AssigneeRow::Unassign` does.

use crate::domain::{IssueDetail, Source, Sprint};

use super::{async_ops, App, Screen};

/// One row in the sprint picker: either "Remove from sprint" (back to the
/// backlog) or a specific open sprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SprintRow {
    RemoveFromSprint,
    Sprint(Sprint),
}

/// State for the open sprint picker. Recomputed (via `App::open_sprint_picker`)
/// on open; unlike the assignee picker there's no type-to-filter query —
/// `list_open_sprints` already returns a short, server-filtered
/// (active/future only) list, so a query line would be more chrome than it's
/// worth.
#[derive(Clone, Debug, Default)]
pub struct SprintPickerState {
    /// The issue being (re)assigned. Set when the picker opens; `None`
    /// otherwise (including in `App::default()`/`new()`).
    pub key: Option<String>,
    pub rows: Vec<SprintRow>,
    pub selected: usize,
}

impl App {
    /// Whether Sprint tracking is set up for this session: always on in
    /// demo mode (there's no real per-instance config to consult, and demo
    /// mode's whole point is a fully explorable UI regardless of any host
    /// config on disk), otherwise gated on `sprint_field` actually being
    /// configured (`config.toml`/`JIRA_SPRINT_FIELD`) — mirrors
    /// `acceptance_criteria_field`'s per-instance opt-in. This is what lets
    /// the Detail screen's meta panel (`render::meta_lines`/`facts_pairs`)
    /// tell "Sprint isn't tracked on this instance, don't show a row at
    /// all" apart from "it's tracked, but this issue has no current
    /// sprint" (a `sprint: None` row shown as "none").
    pub fn sprint_field_configured(&self) -> bool {
        if matches!(self.source, Source::Demo) {
            return true;
        }
        std::env::var("JIRA_SPRINT_FIELD")
            .ok()
            .or_else(|| crate::config::read_kv().get("sprint_field").cloned())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    }

    /// Open the sprint picker for the currently viewed/quick-viewed issue.
    /// Refuses to open at all when Sprint isn't configured for this
    /// instance — there'd be nothing meaningful to show or do.
    pub fn open_sprint_picker(&mut self) {
        if !self.sprint_field_configured() {
            self.status =
                "Sprint isn't set up for this Jira instance (set sprint_field in config.toml)"
                    .into();
            return;
        }
        if self.sprint_pending {
            self.status = "a sprint update is already in progress".into();
            return;
        }
        let Some(key) = self.sprint_target_key() else {
            return;
        };
        self.sprint_picker.key = Some(key);
        self.sprint_picker.rows = self.sprint_rows_source();
        self.sprint_picker.selected = 0;
        self.sprint_picker_open = true;
    }

    pub fn close_sprint_picker(&mut self) {
        self.sprint_picker_open = false;
    }

    /// The key of the issue the picker should act on — same screen-gated
    /// resolution as `assign.rs`'s `assignee_target_key`.
    fn sprint_target_key(&self) -> Option<String> {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => {
                self.detail.as_ref().map(|d| d.key.clone())
            }
            _ => self.quick_view_detail().map(|d| d.key.clone()),
        }
    }

    fn sprint_target_detail(&self) -> Option<&IssueDetail> {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => self.detail.as_ref(),
            _ => self.quick_view_detail(),
        }
    }

    /// Rows for the picker: "Remove from sprint" always first, then every
    /// open (active/future) sprint on the configured board — the live-
    /// fetched list cached at startup (`App::open_sprints`, populated by
    /// `dispatch_open_sprints`, mirroring `project_versions_source`) for a
    /// live session, or a small baked-in demo roster otherwise.
    pub(crate) fn sprint_rows_source(&self) -> Vec<SprintRow> {
        let sprints = if matches!(self.source, Source::Live { .. }) {
            self.open_sprints.clone()
        } else {
            crate::domain::demo_open_sprints()
        };
        std::iter::once(SprintRow::RemoveFromSprint)
            .chain(sprints.into_iter().map(SprintRow::Sprint))
            .collect()
    }

    pub fn sprint_picker_move(&mut self, delta: isize) {
        let len = self.sprint_picker.rows.len();
        if len == 0 {
            return;
        }
        let mut idx = self.sprint_picker.selected as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.sprint_picker.selected = idx as usize;
    }

    /// Apply the highlighted row (live if possible, always locally).
    ///
    /// Demo/cache sessions apply the local sprint update inline; a genuine
    /// live session dispatches the sprint change off the render thread and
    /// applies the update once it resolves — see `async_ops::dispatch_set_sprint`.
    pub fn confirm_sprint_picker(&mut self) {
        let Some(key) = self.sprint_picker.key.clone() else {
            self.sprint_picker_open = false;
            return;
        };
        let Some(row) = self
            .sprint_picker
            .rows
            .get(self.sprint_picker.selected)
            .cloned()
        else {
            self.sprint_picker_open = false;
            return;
        };
        // No-op if the highlighted row is already the issue's current
        // sprint (or "remove from sprint" while it already has none) —
        // mirrors `confirm_version_picker`'s "nothing changed" short
        // circuit, avoiding a pointless write.
        let unchanged = self
            .sprint_target_detail()
            .map(|d| match &row {
                SprintRow::RemoveFromSprint => d.sprint.is_none(),
                SprintRow::Sprint(s) => d.sprint.as_ref() == Some(s),
            })
            .unwrap_or(false);
        self.sprint_picker_open = false;
        if unchanged {
            return;
        }

        let (sprint_id, sprint): (Option<String>, Option<Sprint>) = match row {
            SprintRow::RemoveFromSprint => (None, None),
            SprintRow::Sprint(s) => (Some(s.id.clone()), Some(s)),
        };

        if !matches!(self.source, Source::Live { .. }) {
            self.apply_sprint_locally(&key, sprint.clone());
            self.status = match &sprint {
                Some(s) => format!("moved {key} to {}", s.name),
                None => format!("removed {key} from its sprint"),
            };
            self.flash(match &sprint {
                Some(s) => format!("✓ moved to {}", s.name),
                None => "✓ removed from sprint".to_string(),
            });
            return;
        }

        self.sprint_generation += 1;
        let generation = self.sprint_generation;
        self.sprint_pending = true;
        self.loading = true;
        self.status = match &sprint {
            Some(s) => format!("↻ moving {key} to {}…", s.name),
            None => format!("↻ removing {key} from its sprint…"),
        };
        let tx = self.events_tx.clone();
        async_ops::dispatch_set_sprint(tx, generation, key, sprint_id, sprint);
    }

    /// Update `sprint` for `key` everywhere it's cached: the open Detail and
    /// the quick-view detail cache — shared by both the demo/cache-
    /// synchronous path above and `AppEvent::SprintApplied`'s handler.
    /// Unlike `apply_assignee_locally`/`apply_versions_locally`, there's no
    /// `IssueSummary`-level mirror to update: sprint isn't shown in the
    /// list/board, only Detail's meta panel.
    pub(crate) fn apply_sprint_locally(&mut self, key: &str, sprint: Option<Sprint>) {
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.sprint = sprint.clone();
            }
        }
        if let Some(d) = self.detail_cache.get_mut(key) {
            d.sprint = sprint;
        }
    }
}
