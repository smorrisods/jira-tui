//! The footer's hint groups (SPEC.md §2): a faint uppercase label (`NAV`,
//! `VIEW`, `ACT`, `GO`) followed by `key description` pairs, with a
//! `? all keys` group always pinned last. The footer must never wrap to a
//! second line — [`fit_footer_groups`] measures the rendered width and
//! drops whole groups right-to-left (excluding any pinned group) until it
//! fits, so a narrow terminal loses whole groups instead of wrapping or
//! truncating mid-hint.
//!
//! Per-screen content deliberately mirrors `docs/archive/design/ui-refresh.html`'s
//! mockup footers where they cover a screen; screens the mockup doesn't
//! show keep their existing hint set, just restructured into groups. Hints
//! dropped from a group when width is tight still work — they live in the
//! help overlay (`?`, rendered from `keymap::KEYMAP`) regardless of whether
//! the footer currently has room to advertise them.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::{App, EditTarget, ListFocus, Screen, WelcomePhase};

use super::detail_columns::{detail_layout_for_width, DetailLayout};
use super::{accent, muted};

#[derive(Clone)]
struct FooterHint {
    key: &'static str,
    desc: String,
}

fn hint(key: &'static str, desc: impl Into<String>) -> FooterHint {
    FooterHint {
        key,
        desc: desc.into(),
    }
}

#[derive(Clone)]
struct FooterGroup {
    /// `None` for the ungrouped `? all keys`/single-line screens.
    label: Option<&'static str>,
    hints: Vec<FooterHint>,
    /// Pinned groups are never dropped by [`fit_footer_groups`], regardless
    /// of their position in the list — an explicit flag rather than an
    /// "always last" positional convention, so a future caller that
    /// assembles a screen's groups from several pieces can't accidentally
    /// make the pinned group droppable by placing it somewhere else.
    pinned: bool,
}

fn group(label: &'static str, hints: Vec<FooterHint>) -> FooterGroup {
    FooterGroup {
        label: Some(label),
        hints,
        pinned: false,
    }
}

/// The always-visible trailing group (typically `? all keys`).
fn tail(hints: Vec<FooterHint>) -> FooterGroup {
    FooterGroup {
        label: None,
        hints,
        pinned: true,
    }
}

/// A screen whose whole footer is just one pinned line — no grouping, so
/// none of its hints are ever dropped even on a narrow terminal (matches
/// every one of these screens' modal/single-purpose nature).
fn single(hints: Vec<FooterHint>) -> Vec<FooterGroup> {
    vec![tail(hints)]
}

/// Rendered width of one group: its label (if any) plus its `key desc`
/// pairs, each pair separated by two spaces.
fn group_width(g: &FooterGroup) -> usize {
    let label_width = g.label.map_or(0, |l| l.chars().count() + 1);
    let hints_width: usize = g
        .hints
        .iter()
        .map(|h| h.key.chars().count() + 1 + h.desc.chars().count())
        .sum();
    let hint_seps = g.hints.len().saturating_sub(1) * 2;
    label_width + hints_width + hint_seps
}

/// Rendered width of the whole footer: every group's width plus a 3-space
/// separator between groups.
fn total_width(groups: &[FooterGroup]) -> usize {
    if groups.is_empty() {
        return 0;
    }
    let sum: usize = groups.iter().map(group_width).sum();
    sum + (groups.len() - 1) * 3
}

/// Drop whole non-pinned groups right-to-left until the rendered width fits
/// `available_width`. Pure logic per SPEC.md §13, unit-tested below.
fn fit_footer_groups(mut groups: Vec<FooterGroup>, available_width: usize) -> Vec<FooterGroup> {
    while total_width(&groups) > available_width {
        match groups.iter().rposition(|g| !g.pinned) {
            Some(idx) => {
                groups.remove(idx);
            }
            None => break, // nothing left that's droppable
        }
    }
    groups
}

fn render_footer_groups(groups: Vec<FooterGroup>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (gi, g) in groups.into_iter().enumerate() {
        if gi > 0 {
            spans.push(Span::raw("   "));
        }
        if let Some(label) = g.label {
            spans.push(Span::styled(
                label,
                Style::default().fg(muted()).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
        }
        for (hi, h) in g.hints.into_iter().enumerate() {
            if hi > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(h.key, Style::default().fg(accent())));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(h.desc, Style::default().fg(muted())));
        }
    }
    Line::from(spans)
}

/// The full, unclipped set of hint groups for the current screen — clipped
/// to the footer's actual width by [`fit_footer_groups`] at render time.
fn footer_groups(app: &App) -> Vec<FooterGroup> {
    match app.screen {
        Screen::Welcome => match app.onboarding.welcome_phase {
            WelcomePhase::Intro => single(vec![
                hint("s", "connect"),
                hint("d", "demo"),
                hint("w", "write config"),
                hint("?", "help"),
                hint("q", "quit"),
            ]),
            WelcomePhase::Setup => single(vec![
                hint("type", "to edit"),
                hint("tab", "next"),
                hint("⏎", "verify & save"),
                hint("esc", "back"),
            ]),
        },
        Screen::Detail => {
            let history_hints = match (app.can_go_back(), app.can_go_forward()) {
                (true, true) => vec![hint("←/→", "history back/forward")],
                (true, false) => vec![hint("←", "history back")],
                (false, true) => vec![hint("→", "history forward")],
                (false, false) => vec![],
            };
            let mut nav = vec![
                hint("↑/↓", "scroll"),
                hint("]/[", "comments/top"),
                hint("n/p", "prev/next comment"),
                hint("{/}", "cycle links"),
            ];
            nav.extend(history_hints);
            // `x` only does anything in the narrow single-column layout —
            // there's no facts panel to fold in the wide rail.
            if detail_layout_for_width(app.detail_area.get().width) == DetailLayout::Narrow {
                nav.push(hint("x", "fold facts"));
            }
            vec![
                group(
                    "ACT",
                    vec![
                        hint("t", "transition"),
                        hint("A", "assign"),
                        hint("e", "edit"),
                        hint("c", "comment"),
                    ],
                ),
                group("NAV", nav),
                tail(vec![hint("?", "all keys")]),
            ]
        }
        Screen::Preview => {
            let apply = match app.edit_target {
                EditTarget::Description => "apply to Jira",
                EditTarget::Comment => "post comment",
            };
            single(vec![
                hint("y/⏎", apply),
                hint("esc/←", "cancel"),
                hint("↑/↓", "scroll"),
            ])
        }
        Screen::Edit => {
            let compose = match app.edit_target {
                EditTarget::Description => "to edit",
                EditTarget::Comment => "your comment",
            };
            single(vec![
                hint("type", compose),
                hint("^S", "preview"),
                hint("esc", "cancel"),
            ])
        }
        Screen::Search => single(vec![
            hint("type", "to filter"),
            hint("↑/↓", "move"),
            hint("⏎", "open"),
            hint("esc", "cancel"),
        ]),
        Screen::FieldMapping => single(vec![
            hint("type", "to search fields"),
            hint("↑/↓", "move"),
            hint("⏎", "map"),
            hint("esc", "cancel"),
        ]),
        Screen::Board => vec![
            group(
                "NAV",
                vec![
                    hint("↑/↓", "card"),
                    hint("←/→", "column"),
                    hint("pg↕", "lane"),
                ],
            ),
            // `t` isn't bound on Board (only within Detail) — SPEC.md §7's
            // "open the transition picker from a card" is a proposed
            // addition for a later phase, not implemented yet, so it's not
            // advertised here.
            group("ACT", vec![hint("⏎", "open")]),
            group("GO", vec![hint("/", "search"), hint("V", "view")]),
            tail(vec![hint("esc/q", "back"), hint("?", "all keys")]),
        ],
        Screen::About => single(vec![
            hint("esc/←", "back"),
            hint("?", "help"),
            hint("q", "quit"),
        ]),
        Screen::Home | Screen::List if app.quick_view => {
            let refresh = if app.list_focus == ListFocus::QuickView {
                "refresh focused issue"
            } else {
                "refresh list"
            };
            vec![
                group(
                    "NAV",
                    vec![
                        hint("↑/↓", "move"),
                        hint("→/⏎", "open"),
                        hint("tab", "focus quick view"),
                    ],
                ),
                group(
                    "ACT",
                    vec![
                        hint("A", "assign"),
                        hint("c", "comment"),
                        hint("r", refresh),
                    ],
                ),
                tail(vec![hint("?", "all keys")]),
            ]
        }
        _ => vec![
            group("NAV", vec![hint("↑/↓", "move"), hint("→/⏎", "open")]),
            group(
                "VIEW",
                vec![hint("v", "quick"), hint("s", "sort"), hint("f", "filter")],
            ),
            group("GO", vec![hint("b", "board"), hint("/", "search")]),
            tail(vec![hint("?", "all keys")]),
        ],
    }
}

pub(crate) fn footer_line(app: &App, available_width: usize) -> Line<'static> {
    let groups = fit_footer_groups(footer_groups(app), available_width);
    render_footer_groups(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::layout::Rect;

    /// Checked against the pre-fit group content directly, not a rendered
    /// `TestBackend` frame: Detail's NAV group (which the `x` hint joins) is
    /// already wide enough to get dropped by `fit_footer_groups` at typical
    /// terminal sizes, regardless of this hint — a pre-existing footer
    /// width/content tradeoff, not something this test should get tangled
    /// up in.
    #[test]
    fn detail_nav_group_advertises_fold_facts_only_when_narrow() {
        let mut app = App::new(true);
        app.selected = 0;
        app.open_detail();

        app.detail_area.set(Rect::new(0, 0, 120, 40));
        let wide_nav = footer_groups(&app)
            .into_iter()
            .find(|g| g.label == Some("NAV"))
            .unwrap();
        assert!(
            !wide_nav.hints.iter().any(|h| h.key == "x"),
            "the wide layout has no facts panel to fold"
        );

        app.detail_area.set(Rect::new(0, 0, 80, 40));
        let narrow_nav = footer_groups(&app)
            .into_iter()
            .find(|g| g.label == Some("NAV"))
            .unwrap();
        assert!(
            narrow_nav.hints.iter().any(|h| h.key == "x"),
            "the narrow layout should advertise 'x' to fold the facts panel"
        );
    }

    fn g(label: Option<&'static str>, hints: Vec<(&'static str, &'static str)>) -> FooterGroup {
        let hints = hints.into_iter().map(|(k, d)| hint(k, d)).collect();
        match label {
            Some(l) => group(l, hints),
            None => tail(hints),
        }
    }

    #[test]
    fn fit_keeps_everything_when_it_already_fits() {
        let groups = vec![
            g(Some("NAV"), vec![("↑", "move")]),
            g(None, vec![("?", "all keys")]),
        ];
        let width = total_width(&groups);
        let fitted = fit_footer_groups(groups, width);
        assert_eq!(fitted.len(), 2);
    }

    #[test]
    fn fit_drops_groups_right_to_left_but_never_a_pinned_one() {
        let groups = vec![
            g(Some("NAV"), vec![("↑", "move")]),
            g(Some("VIEW"), vec![("v", "quick")]),
            g(Some("GO"), vec![("b", "board")]),
            g(None, vec![("?", "all keys")]),
        ];
        // Too narrow for all four groups, but wide enough for the first two
        // plus the pinned tail.
        let nav_and_tail = total_width(&[
            g(Some("NAV"), vec![("↑", "move")]),
            g(None, vec![("?", "all keys")]),
        ]);
        let fitted = fit_footer_groups(groups, nav_and_tail);
        let labels: Vec<Option<&str>> = fitted.iter().map(|g| g.label).collect();
        assert_eq!(labels, vec![Some("NAV"), None]);
    }

    #[test]
    fn fit_keeps_a_pinned_group_even_when_nothing_fits() {
        let groups = vec![
            g(Some("NAV"), vec![("↑", "move")]),
            g(None, vec![("?", "all keys")]),
        ];
        let fitted = fit_footer_groups(groups, 0);
        assert_eq!(fitted.len(), 1);
        assert_eq!(fitted[0].label, None);
    }

    /// The pinned invariant is a `pinned: bool` field, not "whichever group
    /// happens to be last" — regression test that a pinned group placed
    /// anywhere in the list still survives.
    #[test]
    fn fit_keeps_a_pinned_group_regardless_of_its_position() {
        let groups = vec![
            g(None, vec![("?", "all keys")]),
            g(Some("NAV"), vec![("↑", "move")]),
            g(Some("GO"), vec![("b", "board")]),
        ];
        let fitted = fit_footer_groups(groups, 0);
        assert_eq!(fitted.len(), 1);
        assert_eq!(fitted[0].label, None);
    }

    #[test]
    fn fit_never_increases_width() {
        let groups = vec![
            g(Some("NAV"), vec![("↑", "move"), ("↓", "down")]),
            g(Some("VIEW"), vec![("v", "quick")]),
            g(Some("ACT"), vec![("t", "transition")]),
            g(None, vec![("?", "all keys")]),
        ];
        for budget in [0usize, 5, 10, 20, 40, 200] {
            let fitted = fit_footer_groups(groups.clone(), budget);
            assert!(
                total_width(&fitted) <= total_width(&groups),
                "fitted width should never exceed the unclipped width (budget={budget})"
            );
        }
    }
}
