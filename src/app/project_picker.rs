//! The new-issue form's project picker: browsing/searching Jira projects by
//! key or name instead of typing the key in blind. Type-to-filter, same
//! shape as `assign.rs`'s assignee picker (a Jira instance can have far more
//! projects than issue types, unlike the small fixed catalog
//! `new_issue_type_picker_open` browses) — opened from the Project field
//! the same way `open_new_issue_type_picker` opens the issue-type dropdown.

use crate::domain::{Project, Source};

use super::App;

/// State for the open project picker. Recomputed (via
/// `App::recompute_project_rows`) on open and after every keystroke that
/// changes `query`, mirroring `AssigneePickerState`.
#[derive(Clone, Debug, Default)]
pub struct ProjectPickerState {
    pub query: String,
    pub rows: Vec<Project>,
    pub selected: usize,
}

impl App {
    /// Enter on the Project field (see the `Screen::NewIssue` key block) —
    /// opens the dropdown/search popup over every accessible project, with
    /// an empty query so it starts out showing everything.
    pub fn open_project_picker(&mut self) {
        self.project_picker.query.clear();
        self.recompute_project_rows();
        self.project_picker_open = true;
    }

    /// Esc — close without changing the Project field.
    pub fn close_project_picker(&mut self) {
        self.project_picker_open = false;
    }

    /// Every project for the current source: the live-fetched list cached
    /// at startup (`App::accessible_projects`, populated by
    /// `async_ops::dispatch_accessible_projects`) for a live session, or the
    /// baked-in demo catalog otherwise. Mirrors `assignable_users_source`.
    fn accessible_projects_source(&self) -> Vec<Project> {
        if matches!(self.source, Source::Live { .. }) {
            self.accessible_projects.clone()
        } else {
            crate::domain::demo_projects()
        }
    }

    /// Rebuild `project_picker.rows` from the current query: every project
    /// whose key or name case-insensitively contains it, sorted by key.
    /// Resets `selected` to 0 whenever the row list changes shape, mirroring
    /// `recompute_assignee_rows`.
    pub(crate) fn recompute_project_rows(&mut self) {
        let query = self.project_picker.query.to_lowercase();
        let mut projects = self.accessible_projects_source();
        projects.sort_by(|a, b| a.key.cmp(&b.key));

        let rows: Vec<Project> = projects
            .into_iter()
            .filter(|p| {
                query.is_empty()
                    || p.key.to_lowercase().contains(&query)
                    || p.name.to_lowercase().contains(&query)
            })
            .collect();

        self.project_picker.rows = rows;
        self.project_picker.selected = 0;
    }

    pub fn project_picker_input_char(&mut self, c: char) {
        self.project_picker.query.push(c);
        self.recompute_project_rows();
    }

    pub fn project_picker_backspace(&mut self) {
        self.project_picker.query.pop();
        self.recompute_project_rows();
    }

    pub fn project_picker_move(&mut self, delta: isize) {
        let len = self.project_picker.rows.len();
        if len == 0 {
            return;
        }
        let mut idx = self.project_picker.selected as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.project_picker.selected = idx as usize;
    }

    /// Enter — commit the highlighted project's key into the form's Project
    /// field and close the popup, then re-run the same issue-type resync
    /// leaving the field via Tab would trigger
    /// (`refresh_new_issue_types_if_project_changed`), since picking a
    /// project from the popup is equivalent to having typed its key and
    /// moved on. A no-op (beyond closing) if the query matched nothing.
    pub fn confirm_project_picker(&mut self) {
        if let Some(project) = self
            .project_picker
            .rows
            .get(self.project_picker.selected)
            .cloned()
        {
            self.new_issue.project = project.key;
        }
        self.project_picker_open = false;
        self.refresh_new_issue_types_if_project_changed();
    }
}
