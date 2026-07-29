//! The per-issue Fix/Affects Version picker (`R`): a checklist over the
//! project's versions, toggled independently for each field and applied
//! together on confirm. Mirrors `assign.rs`'s open/target-resolution shape,
//! extended to a multi-select checklist (Jira's `fixVersions`/`versions` are
//! both arrays, unlike the single-valued assignee) with `Tab` switching
//! which field is being edited rather than needing two separate keys.

use std::collections::BTreeSet;

use crate::domain::{IssueDetail, Source, Version};

use super::{async_ops, App, Screen};

/// Which array field the picker is currently editing — `Tab` toggles this;
/// each field keeps its own independent pending selection (see
/// `VersionPickerState`) so switching back and forth doesn't lose changes
/// made to the other one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VersionField {
    #[default]
    Fix,
    Affects,
}

/// State for the open version picker. `fix_selected`/`affects_selected` are
/// initialized from the target issue's current values when the picker
/// opens, mutated locally as the user toggles rows, and only actually sent
/// to Jira (only the field(s) that changed) when `confirm_version_picker`
/// runs — the checklist itself doubles as the "preview before mutating"
/// step, rather than a separate confirm screen.
#[derive(Clone, Debug, Default)]
pub struct VersionPickerState {
    /// The issue being edited. Set when the picker opens; `None` otherwise.
    pub key: Option<String>,
    pub field: VersionField,
    pub versions: Vec<Version>,
    pub fix_selected: BTreeSet<String>,
    pub affects_selected: BTreeSet<String>,
    pub cursor: usize,
}

impl App {
    /// Open the version picker for the currently viewed/quick-viewed issue.
    /// Unlike `open_assignee_picker`, this doesn't require `list_focus` to be
    /// on the quick-view panel — opening a modal picker captures all
    /// subsequent input anyway, so there's no ambiguity to guard against.
    pub fn open_version_picker(&mut self) {
        if self.version_pending {
            self.status = "a version update is already in progress".into();
            return;
        }
        let Some(key) = self.version_target_key() else {
            return;
        };
        let Some(detail) = self.version_target_detail() else {
            return;
        };
        let fix_selected: BTreeSet<String> = detail.fix_versions.iter().cloned().collect();
        let affects_selected: BTreeSet<String> = detail.affects_versions.iter().cloned().collect();
        self.version_picker.key = Some(key);
        self.version_picker.field = VersionField::Fix;
        self.version_picker.versions = self.project_versions_source();
        self.version_picker.fix_selected = fix_selected;
        self.version_picker.affects_selected = affects_selected;
        self.version_picker.cursor = 0;
        self.version_picker_open = true;
    }

    pub fn close_version_picker(&mut self) {
        self.version_picker_open = false;
    }

    /// The key of the issue the picker should act on — same screen-gated
    /// resolution as `assign.rs`'s `assignee_target_key`.
    fn version_target_key(&self) -> Option<String> {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => {
                self.detail.as_ref().map(|d| d.key.clone())
            }
            _ => self.quick_view_detail().map(|d| d.key.clone()),
        }
    }

    fn version_target_detail(&self) -> Option<&IssueDetail> {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => self.detail.as_ref(),
            _ => self.quick_view_detail(),
        }
    }

    /// Every version defined on the current project: the live-fetched list
    /// cached at startup (`App::project_versions`, populated by
    /// `dispatch_project_versions`) for a live session, or the baked-in demo
    /// roster otherwise. Mirrors `assign.rs`'s `assignable_users_source`.
    pub(crate) fn project_versions_source(&self) -> Vec<Version> {
        if matches!(self.source, Source::Live { .. }) {
            self.project_versions.clone()
        } else {
            crate::domain::demo_versions()
        }
    }

    pub fn version_picker_move(&mut self, delta: isize) {
        let len = self.version_picker.versions.len();
        if len == 0 {
            return;
        }
        let mut idx = self.version_picker.cursor as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.version_picker.cursor = idx as usize;
    }

    /// `Tab` — switch between editing Fix Version(s) and Affects Version(s),
    /// keeping both fields' pending selections intact.
    pub fn version_picker_switch_field(&mut self) {
        self.version_picker.field = match self.version_picker.field {
            VersionField::Fix => VersionField::Affects,
            VersionField::Affects => VersionField::Fix,
        };
    }

    /// `Space`/`Enter` on a row — toggle the highlighted version's
    /// membership in whichever field is currently active.
    pub fn version_picker_toggle(&mut self) {
        let Some(v) = self
            .version_picker
            .versions
            .get(self.version_picker.cursor)
            .cloned()
        else {
            return;
        };
        let set = match self.version_picker.field {
            VersionField::Fix => &mut self.version_picker.fix_selected,
            VersionField::Affects => &mut self.version_picker.affects_selected,
        };
        if !set.remove(&v.name) {
            set.insert(v.name);
        }
    }

    /// Apply whichever field(s) actually changed since the picker opened —
    /// unlike `confirm_assignee`, this can touch up to two fields in one
    /// go, so only the changed one(s) are sent (`None` means "leave alone",
    /// distinguished from `Some(vec![])`, which means "clear it").
    ///
    /// Demo/cache sessions apply locally inline; a genuine live session
    /// dispatches off the render thread — see `async_ops::dispatch_set_versions`.
    pub fn confirm_version_picker(&mut self) {
        let Some(key) = self.version_picker.key.clone() else {
            self.version_picker_open = false;
            return;
        };
        let Some(original) = self.version_target_detail() else {
            self.version_picker_open = false;
            return;
        };
        let original_fix: BTreeSet<String> = original.fix_versions.iter().cloned().collect();
        let original_affects: BTreeSet<String> =
            original.affects_versions.iter().cloned().collect();
        self.version_picker_open = false;

        let fix_changed = self.version_picker.fix_selected != original_fix;
        let affects_changed = self.version_picker.affects_selected != original_affects;
        if !fix_changed && !affects_changed {
            return;
        }
        let fix_versions: Option<Vec<String>> =
            fix_changed.then(|| self.version_picker.fix_selected.iter().cloned().collect());
        let affects_versions: Option<Vec<String>> = affects_changed.then(|| {
            self.version_picker
                .affects_selected
                .iter()
                .cloned()
                .collect()
        });

        if !matches!(self.source, Source::Live { .. }) {
            self.apply_versions_locally(&key, fix_versions.clone(), affects_versions.clone());
            self.status = format!("updated {key} versions");
            self.flash("✓ versions updated");
            return;
        }

        self.version_generation += 1;
        let generation = self.version_generation;
        self.version_pending = true;
        self.loading = true;
        self.status = format!("↻ updating {key} versions…");
        let tx = self.events_tx.clone();
        async_ops::dispatch_set_versions(tx, generation, key, fix_versions, affects_versions);
    }

    /// Update `fix_versions`/`affects_versions` for `key` everywhere it's
    /// cached, wherever `Some` — shared by the demo/cache-synchronous path
    /// above and `AppEvent::VersionsApplied`'s handler. `None` leaves that
    /// field untouched, matching `confirm_version_picker`'s "only the
    /// changed field(s)" contract.
    pub(crate) fn apply_versions_locally(
        &mut self,
        key: &str,
        fix_versions: Option<Vec<String>>,
        affects_versions: Option<Vec<String>>,
    ) {
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                if let Some(fv) = &fix_versions {
                    d.fix_versions = fv.clone();
                }
                if let Some(av) = &affects_versions {
                    d.affects_versions = av.clone();
                }
            }
        }
        if let Some(d) = self.detail_cache.get_mut(key) {
            if let Some(fv) = &fix_versions {
                d.fix_versions = fv.clone();
            }
            if let Some(av) = &affects_versions {
                d.affects_versions = av.clone();
            }
        }
    }
}
