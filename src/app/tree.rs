//! The parent/child tree view mode for the work list: toggling between the
//! flat sort order and a hierarchy that nests an issue's children (e.g. an
//! Epic's stories, or a story's sub-tasks) directly beneath it.
//!
//! `IssueSummary::epic` is Jira's generic immediate-parent key despite the
//! name (see `jira::live`'s `str_field(&f, &["parent", "key"])`) — it's an
//! Epic key for stories/tasks, but a Story/Task key for sub-tasks — so it's
//! exactly the field this needs, no extra fetch required.

use std::collections::HashMap;

use super::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ListViewMode {
    #[default]
    Flat,
    Tree,
}

impl App {
    pub fn toggle_list_view_mode(&mut self) {
        self.list_view_mode = match self.list_view_mode {
            ListViewMode::Flat => ListViewMode::Tree,
            ListViewMode::Tree => ListViewMode::Flat,
        };
        self.status = match self.list_view_mode {
            ListViewMode::Flat => "view: flat".into(),
            ListViewMode::Tree => "view: parent ↔ child tree".into(),
        };
    }

    /// The rows to display, as `(index into self.issues, depth)` pairs.
    /// In `Flat` mode this is just `self.issues` in its current sort order,
    /// each at depth 0. In `Tree` mode, root issues (no parent, or a parent
    /// outside the current filtered/sorted view) appear first — still in
    /// the current sort order among themselves — immediately followed by
    /// their descendants, recursively, each one depth deeper than its
    /// parent.
    pub fn tree_rows(&self) -> Vec<(usize, usize)> {
        if self.list_view_mode == ListViewMode::Flat || self.issues.is_empty() {
            return (0..self.issues.len()).map(|i| (i, 0)).collect();
        }

        let index_by_key: HashMap<&str, usize> = self
            .issues
            .iter()
            .enumerate()
            .map(|(i, issue)| (issue.key.as_str(), i))
            .collect();

        let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut roots: Vec<usize> = Vec::new();
        for (i, issue) in self.issues.iter().enumerate() {
            match issue
                .epic
                .as_deref()
                .and_then(|k| index_by_key.get(k))
                .copied()
            {
                // An issue can't be its own parent — guard against a
                // malformed/self-referential `epic` treating it as one.
                Some(parent) if parent != i => children.entry(parent).or_default().push(i),
                _ => roots.push(i),
            }
        }

        let mut order = Vec::with_capacity(self.issues.len());
        let mut visited = vec![false; self.issues.len()];
        for root in roots {
            push_subtree(root, 0, &children, &mut visited, &mut order);
        }
        // Anything left over is part of a parent cycle the walk above
        // couldn't reach from a root — surface it anyway, as a top-level
        // row, rather than silently dropping it from the list.
        for i in 0..self.issues.len() {
            if !visited[i] {
                push_subtree(i, 0, &children, &mut visited, &mut order);
            }
        }
        order
    }
}

fn push_subtree(
    idx: usize,
    depth: usize,
    children: &HashMap<usize, Vec<usize>>,
    visited: &mut [bool],
    order: &mut Vec<(usize, usize)>,
) {
    if visited[idx] {
        return;
    }
    visited[idx] = true;
    order.push((idx, depth));
    if let Some(kids) = children.get(&idx) {
        for &kid in kids {
            push_subtree(kid, depth + 1, children, visited, order);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::IssueSummary;

    fn issue(key: &str, epic: Option<&str>) -> IssueSummary {
        IssueSummary {
            epic: epic.map(String::from),
            ..crate::test_support::sample_issue(key)
        }
    }

    fn app_with(issues: Vec<IssueSummary>) -> App {
        let mut app = App::new(true);
        app.all_issues = issues.clone();
        app.issues = issues;
        app.list_view_mode = ListViewMode::Tree;
        app
    }

    #[test]
    fn flat_mode_is_always_identity_order() {
        let app = app_with(vec![issue("A", None), issue("B", Some("A"))]);
        let mut app = app;
        app.list_view_mode = ListViewMode::Flat;
        assert_eq!(app.tree_rows(), vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn nests_children_under_their_parent_in_encounter_order() {
        // Epic DS-1 has two stories, the second of which has a sub-task.
        let app = app_with(vec![
            issue("DS-1", None),
            issue("DS-2", Some("DS-1")),
            issue("DS-3", Some("DS-1")),
            issue("DS-4", Some("DS-3")),
        ]);
        let rows = app.tree_rows();
        assert_eq!(rows, vec![(0, 0), (1, 1), (2, 1), (3, 2)]);
    }

    #[test]
    fn orphaned_children_whose_parent_is_outside_the_view_become_roots() {
        // DS-2's epic (DS-1) isn't present in this filtered view.
        let app = app_with(vec![issue("DS-2", Some("DS-1")), issue("DS-3", None)]);
        let rows = app.tree_rows();
        assert_eq!(rows, vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn a_self_referential_or_cyclic_parent_does_not_infinite_loop() {
        // DS-1 claims DS-2 as its parent and vice versa — a malformed cycle
        // that must still terminate and account for every issue exactly
        // once.
        let app = app_with(vec![
            issue("DS-1", Some("DS-2")),
            issue("DS-2", Some("DS-1")),
        ]);
        let rows = app.tree_rows();
        assert_eq!(rows.len(), 2);
        let mut indices: Vec<usize> = rows.iter().map(|(i, _)| *i).collect();
        indices.sort();
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn move_selection_walks_tree_order_not_raw_array_order() {
        // DS-1 (root), DS-3 (root, but sorted/stored before its child
        // below), DS-2 is DS-1's child — array order is DS-1, DS-3, DS-2,
        // but tree order must visit DS-1's child immediately after DS-1.
        let mut app = app_with(vec![
            issue("DS-1", None),
            issue("DS-3", None),
            issue("DS-2", Some("DS-1")),
        ]);
        app.selected = 0; // DS-1
        app.move_selection(1);
        assert_eq!(app.issues[app.selected].key, "DS-2");
        app.move_selection(1);
        assert_eq!(app.issues[app.selected].key, "DS-3");
    }
}
