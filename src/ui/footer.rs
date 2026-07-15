//! The footer's hint groups (SPEC.md §2): a faint uppercase label (`NAV`,
//! `VIEW`, `ACT`, `GO`) followed by `key description` pairs, with a
//! `? all keys` group always pinned last. The footer must never wrap to a
//! second line — [`fit_footer_groups`] measures the rendered width and
//! drops whole groups right-to-left (excluding the pinned last group) until
//! it fits, so a narrow terminal loses whole groups instead of wrapping or
//! truncating mid-hint.
//!
//! Per-screen content deliberately mirrors `docs/design/ui-refresh.html`'s
//! mockup footers where they cover a screen; screens the mockup doesn't
//! show keep their existing hint set, just restructured into groups. Hints
//! dropped from a group when width is tight still work — they live in the
//! help overlay (`?`, rendered from `keymap::KEYMAP`) regardless of whether
//! the footer currently has room to advertise them.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::{App, EditTarget, ListFocus, Screen, WelcomePhase};

use super::{accent, muted};

pub(crate) struct FooterHint {
    pub key: &'static str,
    pub desc: String,
}

fn hint(key: &'static str, desc: impl Into<String>) -> FooterHint {
    FooterHint {
        key,
        desc: desc.into(),
    }
}

pub(crate) struct FooterGroup {
    /// `None` for the ungrouped, always-last `? all keys` survivor.
    pub label: Option<&'static str>,
    pub hints: Vec<FooterHint>,
}

fn group(label: &'static str, hints: Vec<FooterHint>) -> FooterGroup {
    FooterGroup {
        label: Some(label),
        hints,
    }
}

fn tail(hints: Vec<FooterHint>) -> FooterGroup {
    FooterGroup { label: None, hints }
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

/// Drop whole groups right-to-left — excluding the pinned last group, which
/// SPEC.md §2 requires survive regardless — until the rendered width fits
/// `available_width`. Pure logic per SPEC.md §13, unit-tested below.
pub(crate) fn fit_footer_groups(
    mut groups: Vec<FooterGroup>,
    available_width: usize,
) -> Vec<FooterGroup> {
    while groups.len() > 1 && total_width(&groups) > available_width {
        let drop_at = groups.len() - 2;
        groups.remove(drop_at);
    }
    groups
}

fn render_footer_groups(groups: &[FooterGroup]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        if gi > 0 {
            spans.push(Span::raw("   "));
        }
        if let Some(label) = g.label {
            spans.push(Span::styled(
                format!("{label} "),
                Style::default().fg(muted()).add_modifier(Modifier::BOLD),
            ));
        }
        for (hi, h) in g.hints.iter().enumerate() {
            if hi > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(h.key, Style::default().fg(accent())));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(h.desc.clone(), Style::default().fg(muted())));
        }
    }
    Line::from(spans)
}

/// The full, unclipped set of hint groups for the current screen — clipped
/// to the footer's actual width by [`fit_footer_groups`] at render time.
fn footer_groups(app: &App) -> Vec<FooterGroup> {
    match app.screen {
        Screen::Welcome => match app.onboarding.welcome_phase {
            WelcomePhase::Intro => vec![tail(vec![
                hint("s", "connect"),
                hint("d", "demo"),
                hint("w", "write config"),
                hint("?", "help"),
                hint("q", "quit"),
            ])],
            WelcomePhase::Setup => vec![tail(vec![
                hint("type", "to edit"),
                hint("tab", "next"),
                hint("⏎", "verify & save"),
                hint("esc", "back"),
            ])],
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
            vec![tail(vec![
                hint("y/⏎", apply),
                hint("esc/←", "cancel"),
                hint("↑/↓", "scroll"),
            ])]
        }
        Screen::Edit => {
            let compose = match app.edit_target {
                EditTarget::Description => "to edit",
                EditTarget::Comment => "your comment",
            };
            vec![tail(vec![
                hint("^S", "preview"),
                hint("esc", "cancel"),
                hint("type", compose),
            ])]
        }
        Screen::Search => vec![tail(vec![
            hint("type", "to filter"),
            hint("↑/↓", "move"),
            hint("⏎", "open"),
            hint("esc", "cancel"),
        ])],
        Screen::FieldMapping => vec![tail(vec![
            hint("type", "to search fields"),
            hint("↑/↓", "move"),
            hint("⏎", "map"),
            hint("esc", "cancel"),
        ])],
        Screen::Board => vec![
            group(
                "NAV",
                vec![
                    hint("↑/↓", "card"),
                    hint("←/→", "column"),
                    hint("pg↕", "lane"),
                ],
            ),
            group("ACT", vec![hint("⏎", "open"), hint("t", "transition")]),
            group("GO", vec![hint("/", "search"), hint("V", "view")]),
            tail(vec![hint("esc/q", "back"), hint("?", "all keys")]),
        ],
        Screen::About => vec![tail(vec![
            hint("esc/←", "back"),
            hint("?", "help"),
            hint("q", "quit"),
        ])],
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
                        hint("]/[", "comments/top"),
                        hint("n/p", "prev/next comment"),
                        hint("{/}", "cycle links"),
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
    render_footer_groups(&groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(label: Option<&'static str>, hints: Vec<(&'static str, &'static str)>) -> FooterGroup {
        FooterGroup {
            label,
            hints: hints.into_iter().map(|(k, d)| hint(k, d)).collect(),
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
    fn fit_drops_groups_right_to_left_but_never_the_last_one() {
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
    fn fit_keeps_the_last_group_even_when_nothing_fits() {
        let groups = vec![
            g(Some("NAV"), vec![("↑", "move")]),
            g(None, vec![("?", "all keys")]),
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
            let fitted = fit_footer_groups(
                vec![
                    g(Some("NAV"), vec![("↑", "move"), ("↓", "down")]),
                    g(Some("VIEW"), vec![("v", "quick")]),
                    g(Some("ACT"), vec![("t", "transition")]),
                    g(None, vec![("?", "all keys")]),
                ],
                budget,
            );
            assert!(
                total_width(&fitted) <= total_width(&groups),
                "fitted width should never exceed the unclipped width (budget={budget})"
            );
        }
    }
}
