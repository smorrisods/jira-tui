//! Shared `(key, description, screens)` keybinding registry.
//!
//! Single source of truth for the help overlay today; later UI-refresh
//! phases (footer hint groups, command palette) render from this same table
//! instead of hand-packed per-screen strings — see `docs/design/SPEC.md`
//! §13.
//!
//! `screens` lists the screens a binding is known to be scoped to via an
//! explicit `matches!(app.screen, ...)` guard in `src/keys.rs`. An empty
//! slice means the binding is either global or its scope is already spelled
//! out in the description text (e.g. "in an issue or quick view") — it does
//! *not* claim the binding works on every screen. Phases that filter by
//! screen should sharpen individual entries as they consume this field.

use crate::app::Screen;

pub(crate) struct KeyHint {
    pub key: &'static str,
    pub desc: &'static str,
    #[allow(dead_code)] // not yet consumed; wired up by later UI-refresh phases
    pub screens: &'static [Screen],
}

const NONE: &[Screen] = &[];

pub(crate) const KEYMAP: &[KeyHint] = &[
    KeyHint {
        key: "↑ / k",
        desc: "move up",
        screens: NONE,
    },
    KeyHint {
        key: "↓ / j",
        desc: "move down",
        screens: NONE,
    },
    KeyHint {
        key: "→ / ⏎",
        desc: "open selected issue",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "esc/←/⌫",
        desc: "back",
        screens: NONE,
    },
    KeyHint {
        key: "g",
        desc: "go to Home",
        screens: &[Screen::Home, Screen::List, Screen::Detail, Screen::About],
    },
    KeyHint {
        key: "l",
        desc: "go to List",
        screens: &[Screen::Home, Screen::List, Screen::About],
    },
    KeyHint {
        key: "PgUp / PgDn",
        desc: "jump ±8 rows (list/detail/preview); previous/next lane (board)",
        screens: &[
            Screen::Home,
            Screen::List,
            Screen::Detail,
            Screen::Preview,
            Screen::Board,
        ],
    },
    KeyHint {
        key: "/",
        desc: "search or go to an issue by key",
        screens: &[Screen::Home, Screen::List, Screen::Detail],
    },
    KeyHint {
        key: "s / S",
        desc: "cycle sort / flip direction",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "f",
        desc: "cycle status filter",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "v",
        desc: "toggle quick-view panel",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "T",
        desc: "toggle parent ↔ child tree view (nests children under parents)",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "tab",
        desc: "focus list ↔ quick view (enables arrow scroll)",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "b",
        desc: "swimlane board (Kanban-style, grouped by epic)",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "h/j/k/l (board)",
        desc: "vim-style card/column nav — arrow keys also work",
        screens: &[Screen::Board],
    },
    KeyHint {
        key: "t",
        desc: "change status (in an issue)",
        screens: &[Screen::Detail],
    },
    KeyHint {
        key: "A",
        desc: "assign/unassign (in an issue or quick view) — type to filter, ↑/↓ move",
        screens: NONE,
    },
    KeyHint {
        key: "e / E",
        desc: "edit description (in-TUI / $EDITOR)",
        screens: &[Screen::Detail],
    },
    KeyHint {
        key: "c",
        desc: "add a comment (in an issue or quick view)",
        screens: NONE,
    },
    KeyHint {
        key: "] / [",
        desc: "jump to comments section / back to top",
        screens: NONE,
    },
    KeyHint {
        key: "n / p",
        desc: "next / previous comment",
        screens: NONE,
    },
    KeyHint {
        key: "{ / }",
        desc: "cycle highlighted in-body link (issue key / URL)",
        screens: NONE,
    },
    KeyHint {
        key: "⏎ (on a link)",
        desc: "open it — jump to the issue, or open the URL",
        screens: NONE,
    },
    KeyHint {
        key: "← / →",
        desc: "step back/forward through issues followed via links (in an issue)",
        screens: &[Screen::Detail],
    },
    KeyHint {
        key: "F",
        desc: "map a custom field (e.g. Acceptance Criteria)",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "V",
        desc: "switch view (My Work / All Project Issues / teammate)",
        screens: &[Screen::Home, Screen::List],
    },
    KeyHint {
        key: "a",
        desc: "about panel",
        screens: NONE,
    },
    KeyHint {
        key: "m",
        desc: "toggle mouse mode",
        screens: NONE,
    },
    KeyHint {
        key: "J",
        desc: "toggle Jax companion 🦦",
        screens: NONE,
    },
    KeyHint {
        key: "y / Y",
        desc: "copy issue key / URL",
        screens: NONE,
    },
    KeyHint {
        key: "r",
        desc: "refresh — the list, or the open issue/focused quick view",
        screens: NONE,
    },
    KeyHint {
        key: "? / q",
        desc: "toggle help / quit",
        screens: NONE,
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
}
