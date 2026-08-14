//! Cross-screen issue navigation history: a forest of visited issues.
//!
//! Every issue opened fresh (from the list, search, board, a release, or a
//! post-mutation jump) becomes a new root node. Every in-body link followed
//! — from the Detail screen or the quick-view panel alike, see
//! `App::follow_link` — creates a child edge from whichever issue's body
//! the link was actually found in. `←`/`→` (and `,`/`.`) walk real
//! parent/`last_child` links, so "back" always returns to the issue you
//! actually navigated from, not merely the next-most-recently-viewed one;
//! `last_child` tracks the most recently taken branch, so "forward"
//! resumes it even after you've branched elsewhere and come back. Nothing
//! is ever destructively pruned when you branch — an abandoned branch just
//! stops being what `→` reaches, while remaining a node you can jump back
//! to directly (`App::nav_jump`, backing the persistent recent strip and
//! Home's rail card). Only capacity pressure evicts anything
//! (`NavHistory::evict_over_cap`), and only least-recently-visited leaves,
//! never a node still on the current back/forward path.

use std::collections::{HashMap, HashSet};

use super::App;

/// Node count above which the least-recently-visited unprotected leaf gets
/// evicted — see `NavHistory::evict_over_cap`.
pub(crate) const NAV_CAP: usize = 20;

/// Monotonic, never reused — nodes are found by linear scan (bounded by
/// `NAV_CAP`), so there's no arena-index-reuse/ABA concern to guard
/// against.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct NavId(u64);

pub(crate) struct NavNode {
    id: NavId,
    key: String,
    /// `None` for a fresh-open root.
    parent: Option<NavId>,
    /// The most recently taken branch from this node — what `→` resumes.
    last_child: Option<NavId>,
    /// A logical clock stamp (`NavHistory::clock`), not a wall-clock time —
    /// drives both the display projection's ordering and eviction's
    /// least-recently-visited choice.
    last_visited: u64,
}

/// One row of the display projection (the persistent strip + Home's rail
/// card) — see `NavHistory::entries`.
pub(crate) struct NavEntry {
    pub key: String,
    /// The root node's id, as a stable per-lineage colour key.
    pub lineage: u64,
    pub current: bool,
}

#[derive(Default)]
pub(crate) struct NavHistory {
    /// Unordered; every lookup is a linear scan bounded by `NAV_CAP`.
    nodes: Vec<NavNode>,
    current: Option<NavId>,
    next_id: u64,
    clock: u64,
}

impl NavHistory {
    fn find_by_key(&self, key: &str) -> Option<NavId> {
        self.nodes.iter().find(|n| n.key == key).map(|n| n.id)
    }

    fn node(&self, id: NavId) -> &NavNode {
        self.nodes
            .iter()
            .find(|n| n.id == id)
            .expect("NavId always refers to a live node")
    }

    fn node_mut(&mut self, id: NavId) -> &mut NavNode {
        self.nodes
            .iter_mut()
            .find(|n| n.id == id)
            .expect("NavId always refers to a live node")
    }

    fn touch(&mut self, id: NavId) {
        let clock = self.clock;
        self.node_mut(id).last_visited = clock;
    }

    fn insert_root(&mut self, key: &str) -> NavId {
        let id = NavId(self.next_id);
        self.next_id += 1;
        self.nodes.push(NavNode {
            id,
            key: key.to_string(),
            parent: None,
            last_child: None,
            last_visited: self.clock,
        });
        id
    }

    fn insert_child(&mut self, key: &str, parent: NavId) -> NavId {
        let id = NavId(self.next_id);
        self.next_id += 1;
        self.nodes.push(NavNode {
            id,
            key: key.to_string(),
            parent: Some(parent),
            last_child: None,
            last_visited: self.clock,
        });
        id
    }

    /// Whether `candidate` is `node` itself or one of its ancestors — the
    /// re-parenting cycle guard: re-parenting `candidate` under a
    /// descendant of itself would create a loop.
    fn is_ancestor(&self, candidate: NavId, node: NavId) -> bool {
        let mut cur = Some(node);
        while let Some(id) = cur {
            if id == candidate {
                return true;
            }
            cur = self.node(id).parent;
        }
        false
    }

    fn reparent(&mut self, id: NavId, new_parent: NavId) {
        if let Some(old_parent) = self.node(id).parent {
            if self.node(old_parent).last_child == Some(id) {
                self.node_mut(old_parent).last_child = None;
            }
        }
        self.node_mut(id).parent = Some(new_parent);
    }

    fn root_of(&self, id: NavId) -> NavId {
        let mut cur = id;
        while let Some(parent) = self.node(cur).parent {
            cur = parent;
        }
        cur
    }

    /// A fresh navigation: opening from the list, search, board, a
    /// release, or a post-mutation jump — anything that isn't a click on
    /// an in-body link. A key already present keeps its existing
    /// parent/lineage (the cursor just jumps there) rather than being
    /// severed and re-rooted, so history is never lost just because you
    /// happened to open the same issue again from somewhere else.
    pub fn visit_fresh(&mut self, key: &str) {
        self.clock += 1;
        let id = self
            .find_by_key(key)
            .unwrap_or_else(|| self.insert_root(key));
        self.touch(id);
        self.current = Some(id);
        self.evict_over_cap();
    }

    /// An in-body link followed from `parent_key`'s content (resolved by
    /// the caller via `App::active_comment_detail` — the same issue
    /// whether you're reading it in Detail or the quick-view panel, see
    /// `App::follow_link`). If `parent_key` has no node yet (e.g. it was
    /// only ever quick-viewed, never actually navigated to), it's
    /// implicitly visited fresh first.
    pub fn visit_link(&mut self, parent_key: &str, target_key: &str) {
        self.clock += 1;
        let parent_id = self
            .find_by_key(parent_key)
            .unwrap_or_else(|| self.insert_root(parent_key));

        match self.find_by_key(target_key) {
            // Linking to an issue that's an ancestor of the page you're
            // reading (or to the page itself) would cycle the tree if
            // re-parented — degrade to a genuinely plain jump instead, the
            // same as `jump()`: reposition only, no `touch` (which would
            // otherwise reorder `entries()`'s display order for no reason,
            // contradicting "plain jump").
            Some(id) if self.is_ancestor(id, parent_id) => {
                self.current = Some(id);
            }
            Some(id) => {
                self.reparent(id, parent_id);
                self.node_mut(parent_id).last_child = Some(id);
                self.touch(id);
                self.current = Some(id);
            }
            None => {
                let id = self.insert_child(target_key, parent_id);
                self.node_mut(parent_id).last_child = Some(id);
                self.touch(id);
                self.current = Some(id);
            }
        }

        self.evict_over_cap();
    }

    /// A direct jump — clicking an entry in the recent strip or Home's
    /// rail card, or a Shift+`←`/`→` flat display-order step
    /// (`App::step_display`). Repositions the cursor without touching any
    /// parent/`last_child` edges (so `,`/`←` from here still walks to the
    /// clicked node's real origin) *and* without touching `last_visited`
    /// — a jump isn't a real navigation event, so it must not reorder
    /// `entries()`'s display order either. This matters beyond just
    /// tidiness: `step_display` walks that same order by index while
    /// repeatedly jumping, so if a jump bumped recency, every step would
    /// reshuffle the list out from under the walk and it would never
    /// reach the end (confirmed by an infinite-loop test failure before
    /// this was fixed). Returns `false` (no-op) if `key` isn't a known
    /// node.
    pub fn jump(&mut self, key: &str) -> bool {
        let Some(id) = self.find_by_key(key) else {
            return false;
        };
        self.current = Some(id);
        true
    }

    /// Steps to the current node's real parent. Deliberately does *not*
    /// touch `parent.last_child` — when `current` was reached via a real
    /// edge (`visit_link`/`step_forward`), `last_child` already points at
    /// it by construction, so there's nothing to do; when `current` was
    /// instead reached via `jump` (e.g. clicking an abandoned branch in
    /// the strip), overwriting `last_child` here would silently demote
    /// the tree's real most-recently-taken branch just because the user
    /// glanced at an old one and pressed back — `jump`'s own contract is
    /// that it never touches structure, and stepping away from a jump
    /// must not retroactively violate that.
    pub fn step_back(&mut self) -> Option<String> {
        let current_id = self.current?;
        let parent_id = self.node(current_id).parent?;
        self.clock += 1;
        self.touch(parent_id);
        self.current = Some(parent_id);
        Some(self.node(parent_id).key.clone())
    }

    /// Steps to the current node's most recently taken branch.
    pub fn step_forward(&mut self) -> Option<String> {
        let current_id = self.current?;
        let child_id = self.node(current_id).last_child?;
        self.clock += 1;
        self.touch(child_id);
        self.current = Some(child_id);
        Some(self.node(child_id).key.clone())
    }

    pub fn can_back(&self) -> bool {
        self.current
            .is_some_and(|id| self.node(id).parent.is_some())
    }

    pub fn can_forward(&self) -> bool {
        self.current
            .is_some_and(|id| self.node(id).last_child.is_some())
    }

    /// Real ancestor count from the current node — the header's `← N
    /// back` crumb (`App::back_count`), not a recency count.
    pub fn back_depth(&self) -> usize {
        let mut count = 0;
        let mut cur = self.current.and_then(|id| self.node(id).parent);
        while let Some(id) = cur {
            count += 1;
            cur = self.node(id).parent;
        }
        count
    }

    /// Whether the forest has any visited issues at all — used to hide the
    /// recent strip entirely on a fresh session before anything's been
    /// opened.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The flat display projection backing the recent strip and Home's
    /// rail card: lineages (bands) ordered by their most-recently-visited
    /// node first — switching to a different root jumps its whole band to
    /// the front, so your active lineage is always easiest to find — but
    /// a node's position *within* its own band is fixed by tree structure
    /// (a stable preorder walk: a node, then its children in the order
    /// they were first created), not by recency. Only the `current` flag
    /// moves as you navigate; nothing else in the layout does. This is
    /// deliberate: an MRU-within-band order was tried first and reshuffled
    /// on every navigation, making the strip impossible to build a mental
    /// map of — you couldn't tell whether it had updated at all, or
    /// predict where `→` would land before pressing it.
    pub fn entries(&self) -> Vec<NavEntry> {
        let mut lineage_recency: HashMap<u64, u64> = HashMap::new();
        for node in &self.nodes {
            let root = self.root_of(node.id).0;
            let slot = lineage_recency.entry(root).or_insert(0);
            *slot = (*slot).max(node.last_visited);
        }

        let mut roots: Vec<NavId> = self
            .nodes
            .iter()
            .filter(|n| n.parent.is_none())
            .map(|n| n.id)
            .collect();
        roots.sort_by(|a, b| {
            lineage_recency[&b.0]
                .cmp(&lineage_recency[&a.0])
                .then_with(|| a.0.cmp(&b.0))
        });

        let mut ordered: Vec<NavId> = Vec::with_capacity(self.nodes.len());
        for root in roots {
            self.preorder_from(root, &mut ordered);
        }

        ordered
            .into_iter()
            .map(|id| {
                let n = self.node(id);
                NavEntry {
                    key: n.key.clone(),
                    lineage: self.root_of(id).0,
                    current: self.current == Some(id),
                }
            })
            .collect()
    }

    /// Appends `id` then its children (found by scanning for `parent ==
    /// Some(id)`, ordered by `NavId` — monotonically assigned, so this is
    /// creation order) to `out`, recursively — a stable preorder walk of
    /// one lineage. Bounded by `NAV_CAP`, so plain recursion is fine.
    fn preorder_from(&self, id: NavId, out: &mut Vec<NavId>) {
        out.push(id);
        let mut children: Vec<NavId> = self
            .nodes
            .iter()
            .filter(|n| n.parent == Some(id))
            .map(|n| n.id)
            .collect();
        children.sort_by_key(|c| c.0);
        for child in children {
            self.preorder_from(child, out);
        }
    }

    /// While over `NAV_CAP`, evict the least-recently-visited unprotected
    /// leaf. Protected = the current node plus its ancestor chain (the
    /// back path) and its `last_child` chain (the forward path) — one
    /// path through the tree, so any node off it heads a subtree with no
    /// protected node in it, meaning an unprotected leaf always exists
    /// whenever any unprotected node does; the loop can't stall or strand
    /// the cursor. Leaf-only eviction means a node with children is never
    /// removed, so a lineage's root — and therefore its colour — never
    /// changes underneath it; abandoned branches erode from their tips
    /// inward, oldest-first.
    fn evict_over_cap(&mut self) {
        while self.nodes.len() > NAV_CAP {
            let mut protected: HashSet<u64> = HashSet::new();
            if let Some(cur) = self.current {
                let mut back = Some(cur);
                while let Some(id) = back {
                    protected.insert(id.0);
                    back = self.node(id).parent;
                }
                let mut fwd = self.node(cur).last_child;
                while let Some(id) = fwd {
                    protected.insert(id.0);
                    fwd = self.node(id).last_child;
                }
            }

            let parent_ids: HashSet<u64> = self
                .nodes
                .iter()
                .filter_map(|n| n.parent.map(|p| p.0))
                .collect();

            let victim = self
                .nodes
                .iter()
                .filter(|n| !parent_ids.contains(&n.id.0) && !protected.contains(&n.id.0))
                .min_by_key(|n| n.last_visited)
                .map(|n| n.id);

            match victim {
                Some(id) => {
                    for n in self.nodes.iter_mut() {
                        if n.last_child == Some(id) {
                            n.last_child = None;
                        }
                    }
                    self.nodes.retain(|n| n.id != id);
                }
                // No unprotected leaf — every remaining node is on the
                // active path. Can't happen while nodes.len() > NAV_CAP
                // (a single path can't hold NAV_CAP+1 nodes on its own in
                // any session that also ever called `evict_over_cap` at
                // NAV_CAP), but this guards against a stall regardless.
                None => break,
            }
        }
    }
}

impl App {
    /// Whether `←`/`,` has an issue to step back to.
    pub fn can_go_back(&self) -> bool {
        self.nav.can_back()
    }

    /// Whether `→`/`.` has an issue to step forward to.
    pub fn can_go_forward(&self) -> bool {
        self.nav.can_forward()
    }

    /// `←`/`,` — step back to the issue actually navigated from.
    pub fn go_back(&mut self) {
        if let Some(key) = self.nav.step_back() {
            self.show_issue(&key);
        }
    }

    /// `→`/`.` — step forward into the most recently taken branch.
    pub fn go_forward(&mut self) {
        if let Some(key) = self.nav.step_forward() {
            self.show_issue(&key);
        }
    }

    /// A click on an entry in the recent strip or Home's rail card.
    pub(crate) fn nav_jump(&mut self, key: &str) {
        if self.nav.jump(key) {
            self.show_issue(key);
        }
    }

    /// Real ancestor count from the current issue — the header's `← N
    /// back` crumb.
    pub(crate) fn back_count(&self) -> usize {
        self.nav.back_depth()
    }

    /// The current entry's position in `nav.entries()`'s flat display
    /// order (the same order the recent strip/rail card render — lineage-
    /// banded, most-recent-lineage-first, MRU within a lineage), or `None`
    /// if there's no history yet.
    fn display_index(&self) -> Option<usize> {
        self.nav.entries().iter().position(|e| e.current)
    }

    /// Shift+`←`/`→` — step to the previous/next entry in the recent
    /// strip's flat *display* order, as opposed to `,`/`.`/plain `←`/`→`'s
    /// tree-based parent/`last_child` walk. A jump, not a navigation edge
    /// (same as clicking a chip) — it can land on an entry from a
    /// completely different lineage than the one you're currently in.
    fn step_display(&mut self, delta: isize) {
        let entries = self.nav.entries();
        let Some(idx) = entries.iter().position(|e| e.current) else {
            return;
        };
        let Some(new_idx) = idx.checked_add_signed(delta) else {
            return;
        };
        let Some(entry) = entries.get(new_idx) else {
            return;
        };
        let key = entry.key.clone();
        self.nav_jump(&key);
    }

    /// Whether Shift+`←` has an earlier-in-display-order entry to jump to.
    pub fn can_step_display_back(&self) -> bool {
        self.display_index().is_some_and(|idx| idx > 0)
    }

    /// Whether Shift+`→` has a later-in-display-order entry to jump to.
    pub fn can_step_display_forward(&self) -> bool {
        self.display_index()
            .is_some_and(|idx| idx + 1 < self.nav.entries().len())
    }

    pub fn step_display_back(&mut self) {
        self.step_display(-1);
    }

    pub fn step_display_forward(&mut self) {
        self.step_display(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_opens_are_independent_roots() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        assert!(!nav.can_back());
        nav.visit_fresh("B");
        assert!(
            !nav.can_back(),
            "a second fresh open is its own root, not a child of the first"
        );
    }

    #[test]
    fn visit_link_creates_a_parent_child_edge() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        nav.visit_link("A", "B");
        assert!(nav.can_back());
        assert!(!nav.can_forward());
        assert_eq!(nav.step_back(), Some("A".to_string()));
        assert!(nav.can_forward());
        assert_eq!(nav.step_forward(), Some("B".to_string()));
    }

    /// The scenario worked through directly with the user during design:
    /// open A, link to B, back to A, link to C. `←`/`step_back` from C must
    /// return to A (the true origin), not B (merely the next-most-recent
    /// entry) — and B must not disappear just because a new branch was
    /// taken from A.
    #[test]
    fn back_from_a_new_branch_returns_to_the_true_origin_not_the_abandoned_branch() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        nav.visit_link("A", "B");
        assert_eq!(nav.step_back(), Some("A".to_string()));

        nav.visit_link("A", "C");
        assert_eq!(
            nav.step_back(),
            Some("A".to_string()),
            "back from C should return to its real origin A, not B"
        );
        assert_eq!(
            nav.step_forward(),
            Some("C".to_string()),
            "forward from A should resume the most recently taken branch (C)"
        );

        let keys: Vec<String> = nav.entries().into_iter().map(|e| e.key).collect();
        assert!(
            keys.contains(&"B".to_string()),
            "B must still be present even though it's no longer forward-reachable: {keys:?}"
        );
    }

    /// Regression test: `step_back` used to unconditionally overwrite the
    /// parent's `last_child`, so jumping to an abandoned branch (a strip
    /// click, or Shift+←/→) and then pressing plain `←` would silently
    /// demote the tree's real most-recently-taken branch — `jump`'s own
    /// contract is that it never touches structure, and stepping away
    /// from a jump must not retroactively violate that.
    #[test]
    fn stepping_back_after_a_jump_does_not_corrupt_the_real_last_child() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        nav.visit_link("A", "B");
        nav.step_back(); // current = A
        nav.visit_link("A", "C"); // A.last_child = C (the true most-recent branch)

        assert!(nav.jump("B"), "jump to the abandoned branch B");
        nav.step_back(); // current = A again, but via a jump-then-back path
        assert_eq!(
            nav.step_forward(),
            Some("C".to_string()),
            "forward from A must still resume C — the jump to B and back must not have rewritten A's real last_child"
        );
    }

    /// Regression test: `visit_link`'s ancestor-cycle-guard branch used to
    /// still fall through to a shared `touch()` call despite its own
    /// comment promising "a plain jump" — `jump()` itself never touches
    /// recency, and this branch must match that exactly, or it reorders
    /// `entries()`'s display order for no reason.
    #[test]
    fn linking_to_an_ancestor_does_not_touch_recency_either() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        nav.visit_link("A", "B"); // current = B

        let before: Vec<String> = nav.entries().into_iter().map(|e| e.key).collect();
        nav.visit_link("B", "A"); // degrades to a plain jump back to A
        let after: Vec<String> = nav.entries().into_iter().map(|e| e.key).collect();
        assert_eq!(
            before, after,
            "a degraded-to-jump link-follow must not reorder the display list"
        );
    }

    #[test]
    fn fresh_open_of_an_existing_key_keeps_its_parent() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        nav.visit_link("A", "B");
        nav.step_back(); // current = A

        // Re-opening B "fresh" (e.g. from the list) should jump to it
        // without severing its real origin.
        nav.visit_fresh("B");
        assert!(
            nav.can_back(),
            "B's parent (A) should survive a fresh re-open, not be cleared"
        );
        assert_eq!(nav.step_back(), Some("A".to_string()));
    }

    #[test]
    fn linking_to_an_ancestor_degrades_to_a_plain_jump_instead_of_cycling() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        nav.visit_link("A", "B"); // current = B, B.parent = A

        // Viewing B, click a link back to A — re-parenting A under B would
        // cycle the tree, so this must just jump to A, leaving A a root.
        nav.visit_link("B", "A");
        assert!(
            !nav.can_back(),
            "A must remain a root — it was not re-parented under B"
        );
    }

    #[test]
    fn linking_to_a_previously_fresh_node_reparents_it() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        nav.visit_fresh("X"); // an unrelated second root
        nav.visit_fresh("A"); // jump back to A

        nav.visit_link("A", "X"); // X is not an ancestor of A: re-parent it
        assert!(nav.can_back());
        assert_eq!(nav.step_back(), Some("A".to_string()));
    }

    #[test]
    fn jump_repositions_the_cursor_without_touching_any_edges() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        nav.visit_link("A", "B");
        nav.step_back(); // current = A
        nav.visit_link("A", "C"); // current = C

        assert!(nav.jump("B"));
        assert!(
            nav.can_back(),
            "jumping to B should not disturb B's own parent edge"
        );
        assert_eq!(nav.step_back(), Some("A".to_string()));

        let keys: Vec<String> = nav.entries().into_iter().map(|e| e.key).collect();
        assert!(
            keys.contains(&"C".to_string()),
            "C must not be lost: {keys:?}"
        );

        assert!(!nav.jump("NOPE"), "jumping to an unknown key is a no-op");
    }

    #[test]
    fn back_depth_counts_true_ancestors_not_recency() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("A");
        assert_eq!(nav.back_depth(), 0);
        nav.visit_link("A", "B");
        assert_eq!(nav.back_depth(), 1);
        nav.visit_link("B", "C");
        assert_eq!(nav.back_depth(), 2);
        nav.step_back();
        assert_eq!(nav.back_depth(), 1);
    }

    #[test]
    fn eviction_caps_total_nodes_and_keeps_the_most_recent() {
        let mut nav = NavHistory::default();
        for i in 0..(NAV_CAP + 5) {
            nav.visit_fresh(&format!("ISSUE-{i}"));
        }
        let keys: Vec<String> = nav.entries().into_iter().map(|e| e.key).collect();
        assert_eq!(keys.len(), NAV_CAP);
        assert!(
            !keys.contains(&"ISSUE-0".to_string()),
            "the earliest-visited, now-unprotected root should have been evicted first"
        );
        assert!(keys.contains(&format!("ISSUE-{}", NAV_CAP + 4)));
    }

    #[test]
    fn eviction_never_removes_a_node_on_the_active_back_path() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("ROOT");
        // Two long branches off the same root — B-chain first, then a
        // fresh A-chain that keeps the cursor (and its whole ancestor
        // chain back to ROOT) protected for the rest of the test.
        let mut parent = "ROOT".to_string();
        for i in 0..10 {
            let child = format!("OLD-{i}");
            nav.visit_link(&parent, &child);
            parent = child;
        }
        nav.jump("ROOT");
        let mut parent = "ROOT".to_string();
        for i in 0..20 {
            let child = format!("NEW-{i}");
            nav.visit_link(&parent, &child);
            parent = child;
        }
        // The active back path (ROOT and every NEW-* ancestor of the
        // current node) must all still be walkable.
        for _ in 0..20 {
            assert!(nav.can_back());
            nav.step_back();
        }
        assert!(
            !nav.can_back(),
            "ROOT itself should be reachable and be a root"
        );
    }

    /// A single unbranching chain of link-follows puts every node on the
    /// current back path, so there's never an unprotected leaf to evict —
    /// the cap is a soft target in this specific edge case, not a hard
    /// one. This is a deliberate, documented trade-off (see
    /// `evict_over_cap`'s doc comment), not a bug: nothing panics or
    /// stalls, and the whole chain stays walkable.
    #[test]
    fn a_single_unbranching_chain_can_exceed_the_cap_without_panicking_or_stalling() {
        let mut nav = NavHistory::default();
        nav.visit_fresh("ROOT");
        let mut parent = "ROOT".to_string();
        for i in 0..(NAV_CAP + 5) {
            let child = format!("CHAIN-{i}");
            nav.visit_link(&parent, &child);
            parent = child;
        }
        assert!(nav.entries().len() > NAV_CAP);
        assert!(nav.can_back());
    }
}
