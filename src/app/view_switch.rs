//! The view switcher: picking between "My Work", "All Project Issues", and
//! a teammate's work — all backed by the same generic `search_issues` JQL
//! runner (see `jira::live::jql_for`), swapped in for `App.all_issues`
//! exactly like `refresh()` does today.

use crate::domain::{Source, ViewKind};

use super::{load_issues_for, App};

impl App {
    /// Open the view picker: My Work, All Project Issues, then one entry per
    /// teammate currently visible in `all_issues` — seeded for free from
    /// already-loaded assignees, no extra API call needed just to populate
    /// the list (per the design sketch in issue #6).
    pub fn open_view_picker(&mut self) {
        let mut options = vec![ViewKind::MyWork, ViewKind::AllProject];
        options.extend(self.known_teammates().into_iter().map(ViewKind::Teammate));
        self.view_picker_index = options
            .iter()
            .position(|v| *v == self.current_view)
            .unwrap_or(0);
        self.view_picker_options = options;
        self.view_picker_open = true;
    }

    pub fn close_view_picker(&mut self) {
        self.view_picker_open = false;
    }

    pub fn view_picker_move(&mut self, delta: isize) {
        let len = self.view_picker_options.len();
        if len == 0 {
            return;
        }
        let mut idx = self.view_picker_index as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.view_picker_index = idx as usize;
    }

    /// Teammate display names seen in the currently loaded issues, deduped
    /// and sorted, excluding unassigned issues and (best-effort, by display
    /// name) the current user — "my work" already covers your own issues.
    pub fn known_teammates(&self) -> Vec<String> {
        let me = match &self.source {
            Source::Live { user, .. } | Source::Cache { user } => Some(user.as_str()),
            Source::Demo => None,
        };
        let mut seen = std::collections::BTreeSet::new();
        for issue in &self.all_issues {
            if let Some(name) = &issue.assignee {
                if Some(name.as_str()) != me {
                    seen.insert(name.clone());
                }
            }
        }
        seen.into_iter().collect()
    }

    /// Apply the highlighted entry in the view picker.
    pub fn confirm_view_switch(&mut self) {
        self.view_picker_open = false;
        let Some(kind) = self
            .view_picker_options
            .get(self.view_picker_index)
            .cloned()
        else {
            return;
        };
        self.switch_view(kind);
    }

    /// Load `view` and swap it in as the active issue list.
    pub fn switch_view(&mut self, view: ViewKind) {
        let force_demo = matches!(self.source, Source::Demo);
        let (issues, source, status) = load_issues_for(&view, force_demo);
        self.all_issues = issues;
        self.source = source;
        let label = view.label();
        self.current_view = view;
        self.status = format!("↻ {status}");
        self.selected = 0;
        self.recompute_view();
        self.flash(format!("viewing: {label}"));
    }
}
