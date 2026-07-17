//! The command palette (SPEC.md §8): `ctrl-k` opens a centred, type-to-filter
//! action list grouped into "on {KEY}" (context-dependent issue actions),
//! "view", and "app".
//!
//! Actual dispatch (turning a selected `PaletteAction` into a real function
//! call) lives in `src/keys/mod.rs`, not here: one action (toggling mouse
//! mode) needs real terminal I/O only the binary layer can perform
//! (`crossterm::execute!`), so keeping every action's resolution in one
//! place — rather than splitting "most actions here, one action there" —
//! keeps the dispatch table honest about what "the same function the direct
//! key calls" actually means.

use crate::domain::IssueDetail;

use super::{App, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PaletteGroup {
    OnKey,
    View,
    App,
}

/// What a palette row does, once confirmed. A pure data description —
/// `src/keys/mod.rs` matches this to the real call. `pub`, not
/// `pub(crate)`: the binary crate's `keys/mod.rs` (a separate crate from
/// this library) needs to name these variants for dispatch.
///
/// `CopyKey`/`CopyUrl`/`OpenInBrowser`/`Transition` all carry the key (and,
/// for `Transition`, the issue it belongs to) that `build_palette_rows`
/// already resolved via `palette_context`/`board_selected_issue`, rather
/// than having dispatch re-resolve "the current issue" through a second,
/// narrower path (`selected_issue()`/`self.detail`) that can disagree with
/// what the palette actually showed — e.g. Board's selected card is never
/// reflected in `self.selected`, so a dispatcher that fell back to
/// `selected_issue()` there would silently act on the wrong issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteAction {
    /// A specific workflow transition on `key`, by id — dispatch verifies
    /// `self.detail`'s key still matches before routing through
    /// `App::confirm_transition` (the exact function the direct `t`-then-
    /// select flow calls), in case an async detail refresh landed while the
    /// palette was open.
    Transition {
        key: String,
        transition_id: String,
    },
    Assign,
    Comment,
    CopyKey(String),
    CopyUrl(String),
    OpenInBrowser(String),
    FlipView,
    CycleSort,
    CycleFilter,
    ToggleTree,
    ToggleQuickView,
    OpenBoard,
    Refresh,
    ToggleMouse,
    ToggleJax,
    OpenFieldMapping,
    OpenAbout,
    OpenHelp,
}

pub(crate) struct PaletteRow {
    pub label: String,
    /// Right-aligned keybinding hint — empty when the palette is the only
    /// way to reach this action (only `OpenInBrowser` today).
    pub hint: &'static str,
    pub group: PaletteGroup,
    pub action: PaletteAction,
}

#[derive(Default)]
pub struct PaletteState {
    pub query: String,
    pub(crate) all_rows: Vec<PaletteRow>,
    /// Indices into `all_rows` passing the current filter, in group order.
    pub(crate) visible: Vec<usize>,
    /// Index into `visible`, not `all_rows`.
    pub selected: usize,
}

impl App {
    /// `ctrl-k`: open the command palette. Builds a fresh row list every
    /// time — "recompute, don't cache" — since it depends on live state
    /// (which issue is in context, what transitions it has) that can change
    /// between opens.
    pub fn open_palette(&mut self) {
        self.palette_open = true;
        self.palette.query.clear();
        self.palette.all_rows = self.build_palette_rows();
        self.recompute_palette_visible();
    }

    pub fn close_palette(&mut self) {
        self.palette_open = false;
    }

    pub fn palette_input_char(&mut self, c: char) {
        self.palette.query.push(c);
        self.recompute_palette_visible();
    }

    pub fn palette_backspace(&mut self) {
        self.palette.query.pop();
        self.recompute_palette_visible();
    }

    pub fn palette_move(&mut self, delta: isize) {
        if self.palette.visible.is_empty() {
            return;
        }
        let len = self.palette.visible.len() as isize;
        let pos = (self.palette.selected as isize + delta).rem_euclid(len);
        self.palette.selected = pos as usize;
    }

    /// The currently-highlighted row's action, if any — the filtered list
    /// can be empty.
    pub fn palette_selected_action(&self) -> Option<&PaletteAction> {
        let idx = *self.palette.visible.get(self.palette.selected)?;
        self.palette.all_rows.get(idx).map(|r| &r.action)
    }

    fn recompute_palette_visible(&mut self) {
        let query = self.palette.query.to_lowercase();
        self.palette.visible = self
            .palette
            .all_rows
            .iter()
            .enumerate()
            .filter(|(_, row)| query.is_empty() || row.label.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();
        self.palette.selected = 0;
    }

    /// The issue key (and, if already fetched, full detail) the palette's
    /// "on {KEY}" group acts on. Detail/Preview/Edit and quick view (once
    /// loaded) resolve both; Board and a bare List/Home selection resolve
    /// only a key — neither has (or triggers) a detail fetch of its own
    /// this phase, the same "no on-demand detail fetch" scope cut phase 6
    /// already made for Board's `t` key.
    pub(crate) fn palette_context(&self) -> (Option<String>, Option<&IssueDetail>) {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => {
                if let Some(d) = self.detail.as_ref() {
                    return (Some(d.key.clone()), Some(d));
                }
            }
            Screen::Board => {
                return match self.board_selected_issue() {
                    Some(issue) => (Some(issue.key.clone()), None),
                    None => (None, None),
                };
            }
            _ => {}
        }
        if let Some(d) = self.quick_view_detail() {
            return (Some(d.key.clone()), Some(d));
        }
        if let Some(issue) = self.selected_issue() {
            return (Some(issue.key.clone()), None);
        }
        (None, None)
    }

    fn build_palette_rows(&self) -> Vec<PaletteRow> {
        let mut rows = Vec::new();
        let (key, detail) = self.palette_context();
        if let Some(key) = key.clone() {
            rows.push(PaletteRow {
                label: "copy issue key".into(),
                hint: "y",
                group: PaletteGroup::OnKey,
                action: PaletteAction::CopyKey(key.clone()),
            });
            rows.push(PaletteRow {
                label: "copy issue URL".into(),
                hint: "Y",
                group: PaletteGroup::OnKey,
                action: PaletteAction::CopyUrl(key.clone()),
            });
            rows.push(PaletteRow {
                label: "open in browser".into(),
                hint: "",
                group: PaletteGroup::OnKey,
                action: PaletteAction::OpenInBrowser(key.clone()),
            });
            if let Some(detail) = detail {
                rows.push(PaletteRow {
                    label: "assign/unassign".into(),
                    hint: "A",
                    group: PaletteGroup::OnKey,
                    action: PaletteAction::Assign,
                });
                rows.push(PaletteRow {
                    label: "add a comment".into(),
                    hint: "c",
                    group: PaletteGroup::OnKey,
                    action: PaletteAction::Comment,
                });
                // `confirm_transition` only ever acts on `self.detail`, and
                // the direct `t` key is already gated to `Screen::Detail`
                // only — so transitions must stay gated to the screens
                // where `detail` is guaranteed to have come from
                // `self.detail` (not a loaded-but-different quick view),
                // matching that same boundary rather than silently
                // no-oping when confirmed from a quick view.
                if matches!(self.screen, Screen::Detail | Screen::Preview | Screen::Edit) {
                    for t in &detail.transitions {
                        rows.push(PaletteRow {
                            label: format!("Transition {key} → {}", t.to),
                            hint: "t",
                            group: PaletteGroup::OnKey,
                            action: PaletteAction::Transition {
                                key: key.clone(),
                                transition_id: t.id.clone(),
                            },
                        });
                    }
                }
            }
        }

        rows.extend([
            PaletteRow {
                label: "flip view".into(),
                hint: "›",
                group: PaletteGroup::View,
                action: PaletteAction::FlipView,
            },
            PaletteRow {
                label: "cycle sort".into(),
                hint: "s",
                group: PaletteGroup::View,
                action: PaletteAction::CycleSort,
            },
            PaletteRow {
                label: "cycle filter".into(),
                hint: "f",
                group: PaletteGroup::View,
                action: PaletteAction::CycleFilter,
            },
            PaletteRow {
                label: "toggle tree/flat".into(),
                hint: "T",
                group: PaletteGroup::View,
                action: PaletteAction::ToggleTree,
            },
            PaletteRow {
                label: "toggle quick view".into(),
                hint: "v",
                group: PaletteGroup::View,
                action: PaletteAction::ToggleQuickView,
            },
            PaletteRow {
                label: "open board".into(),
                hint: "b",
                group: PaletteGroup::View,
                action: PaletteAction::OpenBoard,
            },
        ]);

        rows.extend([
            PaletteRow {
                label: "refresh".into(),
                hint: "r",
                group: PaletteGroup::App,
                action: PaletteAction::Refresh,
            },
            PaletteRow {
                label: "toggle mouse mode".into(),
                hint: "m",
                group: PaletteGroup::App,
                action: PaletteAction::ToggleMouse,
            },
            PaletteRow {
                label: "toggle Jax".into(),
                hint: "J",
                group: PaletteGroup::App,
                action: PaletteAction::ToggleJax,
            },
            PaletteRow {
                label: "about".into(),
                hint: "a",
                group: PaletteGroup::App,
                action: PaletteAction::OpenAbout,
            },
            PaletteRow {
                label: "help".into(),
                hint: "?",
                group: PaletteGroup::App,
                action: PaletteAction::OpenHelp,
            },
        ]);
        // The direct `F` key is Home/List-only (mapping a custom field only
        // makes sense against the work list), and `close_field_mapping`
        // always returns to `Screen::Home` rather than tracking an origin
        // screen — offering it from elsewhere would strand the user
        // somewhere other than where they opened it from.
        if matches!(self.screen, Screen::Home | Screen::List) {
            rows.push(PaletteRow {
                label: "field mapping".into(),
                hint: "F",
                group: PaletteGroup::App,
                action: PaletteAction::OpenFieldMapping,
            });
        }

        rows
    }
}
