//! Sorting and status-filtering of the work list.

use crate::domain::IssueSummary;

use super::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Updated,
    Priority,
    Status,
    Key,
}

impl SortKey {
    pub fn label(&self) -> &'static str {
        match self {
            SortKey::Updated => "updated",
            SortKey::Priority => "priority",
            SortKey::Status => "status",
            SortKey::Key => "key",
        }
    }

    fn next(&self) -> SortKey {
        match self {
            SortKey::Updated => SortKey::Priority,
            SortKey::Priority => SortKey::Status,
            SortKey::Status => SortKey::Key,
            SortKey::Key => SortKey::Updated,
        }
    }
}

impl App {
    /// Rebuild `issues` from `all_issues` applying the current filter and sort,
    /// preserving the selected issue by key where possible.
    pub fn recompute_view(&mut self) {
        let cur_key = self.selected_issue().map(|i| i.key.clone());

        let mut view: Vec<IssueSummary> = self
            .all_issues
            .iter()
            .filter(|i| {
                self.filter_status
                    .as_ref()
                    .map(|s| &i.status == s)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        let key_num = |k: &str| -> u64 {
            k.rsplit('-')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0)
        };
        view.sort_by(|a, b| {
            let ord = match self.sort_key {
                SortKey::Updated => a.updated.cmp(&b.updated),
                SortKey::Priority => a.priority.rank().cmp(&b.priority.rank()),
                SortKey::Status => a.status.cmp(&b.status),
                SortKey::Key => key_num(&a.key).cmp(&key_num(&b.key)),
            };
            if self.sort_asc {
                ord
            } else {
                ord.reverse()
            }
        });

        self.issues = view;
        self.selected = cur_key
            .and_then(|k| self.issues.iter().position(|i| i.key == k))
            .unwrap_or(0)
            .min(self.issues.len().saturating_sub(1));
    }

    pub fn cycle_sort(&mut self) {
        self.sort_key = self.sort_key.next();
        self.recompute_view();
        self.status = format!(
            "sort: {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        );
    }

    pub fn toggle_sort_dir(&mut self) {
        self.sort_asc = !self.sort_asc;
        self.recompute_view();
        self.status = format!(
            "sort: {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        );
    }

    /// Cycle the status filter through: all → each distinct status → all.
    pub fn cycle_filter(&mut self) {
        let mut statuses: Vec<String> = Vec::new();
        for i in &self.all_issues {
            if !statuses.contains(&i.status) {
                statuses.push(i.status.clone());
            }
        }
        statuses.sort();
        self.filter_status = match &self.filter_status {
            None => statuses.first().cloned(),
            Some(cur) => {
                let idx = statuses.iter().position(|s| s == cur);
                match idx {
                    Some(i) if i + 1 < statuses.len() => Some(statuses[i + 1].clone()),
                    _ => None,
                }
            }
        };
        self.recompute_view();
        self.status = match &self.filter_status {
            Some(s) => format!("filter: {s}"),
            None => "filter: all".into(),
        };
    }

    pub fn sort_label(&self) -> String {
        format!(
            "sort {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        )
    }

    pub fn filter_label(&self) -> Option<String> {
        self.filter_status.as_ref().map(|s| format!("filter {s}"))
    }
}
