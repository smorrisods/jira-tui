//! The priority picker (`P`): changing the currently viewed/quick-viewed
//! issue's priority. Priority is a fixed 5-value enum (unlike assignee/
//! sprint/version, which are live-fetched lists), so there's no "rows
//! source" branch on `Source::Live` at all — the row list is always
//! `Priority::ALL`. Mirrors `sprint.rs`'s single-valued, no-type-to-filter
//! shape, just simpler still (no live/demo row-source split, no "remove"
//! row — a priority is never absent on an issue).

use crate::domain::{IssueDetail, Priority, Source};

use super::{async_ops, App, Screen};

/// State for the open priority picker.
#[derive(Clone, Debug, Default)]
pub struct PriorityPickerState {
    /// The issue being repriorized. Set when the picker opens; `None`
    /// otherwise (including in `App::default()`/`new()`).
    pub key: Option<String>,
    pub selected: usize,
}

impl App {
    /// Open the priority picker for the currently viewed/quick-viewed
    /// issue, preselecting its current priority.
    pub fn open_priority_picker(&mut self) {
        if self.priority_pending {
            self.status = "a priority update is already in progress".into();
            return;
        }
        let Some(key) = self.priority_target_key() else {
            return;
        };
        let current = self.priority_target_detail().map(|d| d.priority.clone());
        self.priority_picker.key = Some(key);
        self.priority_picker.selected = current
            .and_then(|p| Priority::ALL.iter().position(|c| *c == p))
            .unwrap_or(0);
        self.priority_picker_open = true;
    }

    pub fn close_priority_picker(&mut self) {
        self.priority_picker_open = false;
    }

    /// The key of the issue the picker should act on — same screen-gated
    /// resolution as `assign.rs`'s `assignee_target_key`/`sprint.rs`'s
    /// `sprint_target_key`.
    fn priority_target_key(&self) -> Option<String> {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => {
                self.detail.as_ref().map(|d| d.key.clone())
            }
            _ => self.quick_view_detail().map(|d| d.key.clone()),
        }
    }

    fn priority_target_detail(&self) -> Option<&IssueDetail> {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => self.detail.as_ref(),
            _ => self.quick_view_detail(),
        }
    }

    pub fn priority_picker_move(&mut self, delta: isize) {
        let len = Priority::ALL.len() as isize;
        let mut idx = self.priority_picker.selected as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len {
            idx = len - 1;
        }
        self.priority_picker.selected = idx as usize;
    }

    /// Apply the highlighted priority (live if possible, always locally).
    ///
    /// Demo/cache sessions apply the local update inline; a genuine live
    /// session dispatches the change off the render thread and applies it
    /// once it resolves — see `async_ops::dispatch_set_priority`.
    pub fn confirm_priority_picker(&mut self) {
        let Some(key) = self.priority_picker.key.clone() else {
            self.priority_picker_open = false;
            return;
        };
        let Some(priority) = Priority::ALL.get(self.priority_picker.selected).cloned() else {
            self.priority_picker_open = false;
            return;
        };
        // No-op if the highlighted row is already the issue's current
        // priority — mirrors `confirm_sprint_picker`'s "nothing changed"
        // short circuit, avoiding a pointless write.
        let unchanged = self
            .priority_target_detail()
            .map(|d| d.priority == priority)
            .unwrap_or(false);
        self.priority_picker_open = false;
        if unchanged {
            return;
        }

        if !matches!(self.source, Source::Live { .. }) {
            self.apply_priority_locally(&key, priority.clone());
            self.status = format!("set {key} priority to {}", priority.label());
            self.flash(format!("✓ priority set to {}", priority.label()));
            return;
        }

        self.priority_generation += 1;
        let generation = self.priority_generation;
        self.priority_pending = true;
        self.loading = true;
        self.status = format!("↻ setting {key} priority to {}…", priority.label());
        let tx = self.events_tx.clone();
        async_ops::dispatch_set_priority(tx, generation, key, priority);
    }

    /// Update `priority` for `key` everywhere it's cached: the open Detail,
    /// the quick-view detail cache, and the list summary — shared by both
    /// the demo/cache-synchronous path above and `AppEvent::
    /// PriorityApplied`'s handler. Mirrors `apply_assignee_locally`'s scope
    /// (priority, like assignee, is shown in the list/board, unlike
    /// sprint).
    pub(crate) fn apply_priority_locally(&mut self, key: &str, priority: Priority) {
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.priority = priority.clone();
            }
        }
        if let Some(d) = self.detail_cache.get_mut(key) {
            d.priority = priority.clone();
        }
        if let Some(sum) = self.issues.iter_mut().find(|i| i.key == key) {
            sum.priority = priority.clone();
        }
        if let Some(sum) = self.all_issues.iter_mut().find(|i| i.key == key) {
            sum.priority = priority.clone();
        }
    }
}
