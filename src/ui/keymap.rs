//! Shared `(key, description)` keybinding registry.
//!
//! Single source of truth for the help overlay today; `docs/design/SPEC.md`
//! §13 suggests later UI-refresh phases (footer hint groups, command
//! palette) could render from a table like this instead of hand-packed
//! per-screen strings. Deliberately kept to just `key`/`desc` for now — the
//! footer's `NAV`/`VIEW`/`ACT`/`GO` grouping and the palette's dispatch-fn
//! references are different shapes of metadata than a flat per-screen list,
//! so that's a decision for whichever phase actually consumes it, against
//! its own real requirements, rather than a field guessed at here and left
//! unverified.

pub(crate) struct KeyHint {
    pub key: &'static str,
    pub desc: &'static str,
}

pub(crate) const KEYMAP: &[KeyHint] = &[
    KeyHint {
        key: "↑ / k",
        desc: "move up",
    },
    KeyHint {
        key: "↓ / j",
        desc: "move down",
    },
    KeyHint {
        key: "→ / ⏎",
        desc: "open selected issue",
    },
    KeyHint {
        key: "esc/←/⌫",
        desc: "back",
    },
    KeyHint {
        key: "g",
        desc: "go to Home",
    },
    KeyHint {
        key: "l",
        desc: "go to List",
    },
    KeyHint {
        key: "PgUp / PgDn",
        desc: "jump ±8 rows (list/detail/preview); previous/next lane (board)",
    },
    KeyHint {
        key: "/",
        desc: "search or go to an issue by key",
    },
    KeyHint {
        key: "s / S",
        desc: "cycle sort / flip direction",
    },
    KeyHint {
        key: "f",
        desc: "cycle status filter",
    },
    KeyHint {
        key: "v",
        desc: "toggle quick-view panel",
    },
    KeyHint {
        key: "T",
        desc: "toggle parent ↔ child tree view (nests children under parents)",
    },
    KeyHint {
        key: "tab",
        desc: "focus list ↔ quick view (enables arrow scroll)",
    },
    KeyHint {
        key: "b",
        desc: "swimlane board (Kanban-style, grouped by epic)",
    },
    KeyHint {
        key: "h/j/k/l (board)",
        desc: "vim-style card/column nav — arrow keys also work",
    },
    KeyHint {
        key: "t",
        desc: "change status (in an issue)",
    },
    KeyHint {
        key: "A",
        desc: "assign/unassign (in an issue or quick view) — type to filter, ↑/↓ move",
    },
    KeyHint {
        key: "e / E",
        desc: "edit description (in-TUI / $EDITOR)",
    },
    KeyHint {
        key: "c",
        desc: "add a comment (in an issue or quick view)",
    },
    KeyHint {
        key: "] / [",
        desc: "jump to comments section / back to top",
    },
    KeyHint {
        key: "n / p",
        desc: "next / previous comment",
    },
    KeyHint {
        key: "{ / }",
        desc: "cycle highlighted in-body link (issue key / URL)",
    },
    KeyHint {
        key: "⏎ (on a link)",
        desc: "open it — jump to the issue, or open the URL",
    },
    KeyHint {
        key: "← / →",
        desc: "step back/forward through issues followed via links (in an issue)",
    },
    KeyHint {
        key: "x",
        desc: "fold/unfold the facts panel (narrow Detail layout)",
    },
    KeyHint {
        key: "F",
        desc: "map a custom field (e.g. Acceptance Criteria)",
    },
    KeyHint {
        key: "V",
        desc: "switch view (My Work / All Project Issues / teammate)",
    },
    KeyHint {
        key: "‹ / ›",
        desc: "cycle view in place (same order as V's picker)",
    },
    KeyHint {
        key: "a",
        desc: "about panel",
    },
    KeyHint {
        key: "m",
        desc: "toggle mouse mode",
    },
    KeyHint {
        key: "J",
        desc: "toggle Jax companion 🦦",
    },
    KeyHint {
        key: "y / Y",
        desc: "copy issue key / URL",
    },
    KeyHint {
        key: "r",
        desc: "refresh — the list, or the open issue/focused quick view",
    },
    KeyHint {
        key: "⌃K",
        desc: "command palette — search every action, from any screen",
    },
    KeyHint {
        key: "? / q",
        desc: "toggle help / quit",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn has_key(key: &str) -> bool {
        KEYMAP.iter().any(|h| h.key == key)
    }

    /// SPEC.md §10's keybinding audit: these were bound in `src/keys.rs` but
    /// missing from the help overlay. Regression-guards the fix.
    #[test]
    fn audited_keys_are_registered() {
        assert!(has_key("g"), "`g` (go Home) should be documented");
        assert!(has_key("l"), "`l` (go List) should be documented");
        assert!(has_key("PgUp / PgDn"), "PgUp/PgDn should be documented");
        assert!(
            has_key("h/j/k/l (board)"),
            "board vim-key support should be documented"
        );
    }

    /// SPEC.md §6/§10: `x` folds the narrow Detail layout's facts panel —
    /// a new binding introduced by this phase, verified unbound before it.
    #[test]
    fn facts_panel_fold_key_is_registered() {
        assert!(has_key("x"), "`x` (fold facts panel) should be documented");
    }

    /// SPEC.md §8: `ctrl-k` opens the command palette — a new binding
    /// introduced by this phase, verified unbound before it. Not
    /// screen-scoped footer real estate (the palette's own discoverability
    /// is the point of this phase), so the help overlay is where it lives.
    #[test]
    fn command_palette_key_is_registered() {
        assert!(
            has_key("⌃K"),
            "`ctrl-k` (command palette) should be documented"
        );
    }
}
