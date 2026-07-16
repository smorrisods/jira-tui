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

/// One row of the tree-mode listing, with everything `issue_row` needs to
/// draw box-drawing guides: whether this row has children (`▾ `), whether
/// it's the last child of its parent (`└─ ` vs `├─ `), and — for every
/// non-root ancestor (depths `1..depth`, ending with the immediate parent) —
/// whether that ancestor still has more siblings coming, which is what
/// determines a `│` continuation mark in that column. Root-level items
/// (depth 0) never contribute a rail themselves — they sit flush-left with
/// no connecting line of their own — so `rails.len() == depth.saturating_sub(1)`.
pub(crate) struct TreeRow {
    pub idx: usize,
    pub depth: usize,
    pub has_children: bool,
    pub is_last: bool,
    pub rails: Vec<bool>,
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

        let (children, roots) = self.build_children_and_roots();
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

    /// Like `tree_rows`, but carrying the extra per-row data `issue_row`
    /// needs to draw tree guides (see `TreeRow`). Flat mode's rows all get
    /// trivial guide data (no children marker, no rails) so callers can
    /// treat both modes through one code path.
    pub(crate) fn tree_rows_detailed(&self) -> Vec<TreeRow> {
        if self.list_view_mode == ListViewMode::Flat || self.issues.is_empty() {
            return (0..self.issues.len())
                .map(|i| TreeRow {
                    idx: i,
                    depth: 0,
                    has_children: false,
                    is_last: true,
                    rails: Vec::new(),
                })
                .collect();
        }

        let (children, roots) = self.build_children_and_roots();

        // The full top-level sequence (roots, then any leftover cycle
        // members) needs to be known before we start emitting rows, so
        // `is_last` is correct for the last actual top-level item rather
        // than just the last root — a dry run marks everything reachable
        // from a root first, purely to discover the leftovers.
        let mut top_level = roots.clone();
        {
            let mut dry_visited = vec![false; self.issues.len()];
            for &root in &roots {
                mark_visited(root, &children, &mut dry_visited);
            }
            // Mirror `tree_rows()`'s own leftover-cycle loop: mark each
            // newly-discovered leftover's whole subtree visited as it's
            // found, so later members of the same leftover subtree aren't
            // each counted as their own separate top-level slot (which would
            // inflate `top_level.len()` and corrupt the true last item's
            // `is_last`).
            for i in 0..self.issues.len() {
                if !dry_visited[i] {
                    top_level.push(i);
                    mark_visited(i, &children, &mut dry_visited);
                }
            }
        }

        let mut order = Vec::with_capacity(self.issues.len());
        let mut visited = vec![false; self.issues.len()];
        let n = top_level.len();
        for (pos, &root) in top_level.iter().enumerate() {
            push_subtree_detailed(
                root,
                0,
                Vec::new(),
                pos == n - 1,
                &children,
                &mut visited,
                &mut order,
            );
        }
        order
    }

    /// Shared by `tree_rows`/`tree_rows_detailed`: maps each issue to its
    /// children (by `epic`/parent key) and collects root issues (no
    /// resolvable parent within the current view).
    fn build_children_and_roots(&self) -> (HashMap<usize, Vec<usize>>, Vec<usize>) {
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
        (children, roots)
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

fn mark_visited(idx: usize, children: &HashMap<usize, Vec<usize>>, visited: &mut [bool]) {
    if visited[idx] {
        return;
    }
    visited[idx] = true;
    if let Some(kids) = children.get(&idx) {
        for &kid in kids {
            mark_visited(kid, children, visited);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_subtree_detailed(
    idx: usize,
    depth: usize,
    rails: Vec<bool>,
    is_last: bool,
    children: &HashMap<usize, Vec<usize>>,
    visited: &mut [bool],
    order: &mut Vec<TreeRow>,
) {
    if visited[idx] {
        return;
    }
    visited[idx] = true;
    let kids = children.get(&idx);
    let has_children = kids.is_some_and(|k| !k.is_empty());
    order.push(TreeRow {
        idx,
        depth,
        has_children,
        is_last,
        rails: rails.clone(),
    });
    if let Some(kids) = kids {
        let n = kids.len();
        let mut child_rails = rails;
        // A root (depth 0) never contributes a rail bit of its own — only
        // non-root ancestors do, since the immediate parent's continuation
        // is already encoded by the child's own connector (`is_last`).
        if depth > 0 {
            child_rails.push(!is_last);
        }
        for (i, &kid) in kids.iter().enumerate() {
            push_subtree_detailed(
                kid,
                depth + 1,
                child_rails.clone(),
                i == n - 1,
                children,
                visited,
                order,
            );
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

    #[test]
    fn detailed_rows_mark_the_last_child_and_has_children() {
        // Epic DS-1 has two stories; only the second is its last child.
        let app = app_with(vec![
            issue("DS-1", None),
            issue("DS-2", Some("DS-1")),
            issue("DS-3", Some("DS-1")),
        ]);
        let rows = app.tree_rows_detailed();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].has_children, "DS-1 has children");
        assert!(!rows[1].has_children, "DS-2 has no children");
        assert!(!rows[1].is_last, "DS-2 is not the last child");
        assert!(rows[2].is_last, "DS-3 is the last child");
    }

    #[test]
    fn detailed_rows_never_carry_a_rail_for_the_root_level() {
        // A depth-1 child's rail set is always empty: the root's own
        // continuation is never relevant (roots sit flush-left with no
        // connecting line), and the immediate parent's continuation is
        // already encoded by the child's own connector (`is_last`), not a
        // rail. DS-1 is not the last root (DS-3 follows) and DS-3 is the
        // last root — neither's children should carry any rail bit.
        let app = app_with(vec![
            issue("DS-1", None),
            issue("DS-2", Some("DS-1")),
            issue("DS-3", None),
            issue("DS-4", Some("DS-3")),
        ]);
        let rows = app.tree_rows_detailed();
        let by_idx: std::collections::HashMap<usize, &TreeRow> =
            rows.iter().map(|r| (r.idx, r)).collect();
        assert_eq!(by_idx[&1].rails, Vec::<bool>::new());
        assert_eq!(by_idx[&3].rails, Vec::<bool>::new());
    }

    #[test]
    fn detailed_rows_propagate_only_non_root_ancestor_rails() {
        // Root1 (not last: Root2 follows) has two children: A (not last: B
        // follows) and B (last). A has an only child X, which has an only
        // child Y. The rail that matters for X/Y is A's own continuation
        // (A is NOT last, so a "│" must run all the way down through A's
        // whole subtree) — Root1's continuation must never appear, since
        // roots don't contribute a rail. This distinguishes "rails track
        // non-root ancestors" from a construction that (wrongly) threads
        // the root's own bit through instead.
        let app = app_with(vec![
            issue("Root1", None),
            issue("A", Some("Root1")),
            issue("B", Some("Root1")),
            issue("X", Some("A")),
            issue("Y", Some("X")),
            issue("Root2", None),
        ]);
        let rows = app.tree_rows_detailed();
        let by_idx: std::collections::HashMap<usize, &TreeRow> =
            rows.iter().map(|r| (r.idx, r)).collect();

        assert_eq!(by_idx[&1].rails, Vec::<bool>::new(), "A: depth 1, no rail");
        assert_eq!(by_idx[&2].rails, Vec::<bool>::new(), "B: depth 1, no rail");
        assert!(!by_idx[&1].is_last, "A is not Root1's last child");
        assert!(by_idx[&2].is_last, "B is Root1's last child");

        // X (depth 2) is under A, which is NOT last — its rail carries A's
        // continuation (true), not Root1's.
        assert_eq!(by_idx[&3].rails, vec![true]);
        assert!(by_idx[&3].is_last, "X is A's only (thus last) child");

        // Y (depth 3) extends X's rails with X's own last-ness (X IS last,
        // so !is_last = false).
        assert_eq!(by_idx[&4].rails, vec![true, false]);
        assert!(by_idx[&4].is_last, "Y is X's only (thus last) child");
    }

    #[test]
    fn detailed_rows_give_the_true_final_top_level_group_is_last() {
        // One real root, followed by a leftover 2-cycle the roots-walk can't
        // reach. The cycle is a SINGLE extra top-level group (its two
        // members resolve each other, so only the entry point — Cycle1 —
        // counts as a new top-level slot), not two — the dry run must mark
        // Cycle2 visited as soon as Cycle1 is discovered, or the phantom
        // extra slot pushes the true last group's `is_last` to false.
        let app = app_with(vec![
            issue("Root", None),
            issue("Cycle1", Some("Cycle2")),
            issue("Cycle2", Some("Cycle1")),
        ]);
        let rows = app.tree_rows_detailed();
        let by_idx: std::collections::HashMap<usize, &TreeRow> =
            rows.iter().map(|r| (r.idx, r)).collect();
        assert!(!by_idx[&0].is_last, "Root: the cycle group still follows");
        assert!(
            by_idx[&1].is_last,
            "Cycle1 is the true final top-level group"
        );
    }

    #[test]
    fn detailed_rows_terminate_and_cover_every_issue_on_a_cycle() {
        let app = app_with(vec![
            issue("DS-1", Some("DS-2")),
            issue("DS-2", Some("DS-1")),
        ]);
        let rows = app.tree_rows_detailed();
        assert_eq!(rows.len(), 2);
        let mut indices: Vec<usize> = rows.iter().map(|r| r.idx).collect();
        indices.sort();
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn flat_mode_detailed_rows_have_no_guide_data() {
        let mut app = app_with(vec![issue("A", None), issue("B", Some("A"))]);
        app.list_view_mode = ListViewMode::Flat;
        let rows = app.tree_rows_detailed();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert_eq!(row.depth, 0);
            assert!(!row.has_children);
            assert!(row.is_last);
            assert!(row.rails.is_empty());
        }
    }
}
