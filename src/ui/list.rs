//! The work list panel and its full-width quick-view companion.
//!
//! `issue_row` renders one issue as columns (selection bar, priority glyph,
//! key with tree guide, type chip, status chip, summary, assignee, updated)
//! that drop by priority as the terminal narrows (`list_columns`), plus a
//! matching `column_header_line`. Below the narrow breakpoint the selected
//! row grows a second line carrying whatever got dropped (SPEC.md §3).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, ListFocus, TreeRow};
use crate::domain::IssueSummary;

use super::list_columns::{column_set_for_width, ColumnSet};
use super::{
    accent, accent2, card_bordered, chip, danger, faint, initials, maple, muted, priority_colour,
    priority_glyph, priority_style, selected_style, status_colour, status_short, truncate,
    type_colour,
};

const KEY_WIDTH: usize = 8;
const STATUS_WIDTH: usize = 10;
const SUMMARY_WIDTH: usize = 40;
const ASSIGNEE_WIDTH: usize = 16;

pub(crate) fn draw_list(f: &mut Frame, app: &App, area: Rect, full: bool) {
    // Reflect whichever view is active (My Work / All Project Issues / a
    // teammate's work) instead of a hardcoded "my work" — the full-screen
    // List view additionally gets an "all" prefix, except when the view's
    // own label already says "all" (avoids "all all project issues").
    let label = app.current_view.label().to_lowercase();
    let base = if full && !label.starts_with("all ") {
        format!("all {label}")
    } else {
        label
    };
    let mut title = format!("  ◂ {base} ▸ · {}", app.sort_label());
    if let Some(filter) = app.filter_label() {
        title.push_str(&format!(" · {filter}"));
    }
    if app.list_view_mode == crate::app::ListViewMode::Tree {
        title.push_str(" · tree");
    }
    title.push_str(&format!(
        "  ({} of {})  ",
        app.issues.len(),
        app.all_issues.len()
    ));
    // Dim the list's border when the quick-view panel has keyboard focus
    // (Tab), so it's clear which panel arrow keys currently affect.
    let list_focused = !(app.quick_view && app.list_focus == ListFocus::QuickView);
    let border = if list_focused { accent() } else { muted() };
    let block = card_bordered(&title, accent(), border);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let columns = column_set_for_width(inner.width);
    let rows = app.tree_rows_detailed();
    let mut lines: Vec<Line> = vec![column_header_line(&columns)];
    // One line already spent on the header above.
    let height = (inner.height as usize).saturating_sub(1);
    let cur_pos = rows.iter().position(|r| r.idx == app.selected).unwrap_or(0);
    // simple scroll window around the selection
    let start = cur_pos.saturating_sub(height.saturating_sub(2).max(1) / 2);
    for row in rows.iter().skip(start).take(height) {
        let is_selected = row.idx == app.selected;
        lines.extend(issue_row(&app.issues[row.idx], is_selected, row, &columns));
    }
    // Record geometry so mouse clicks can be mapped back to issues (via
    // `tree_rows` again — `list_start` is a position within it, not a raw
    // index into `app.issues`).
    app.list_area.set(inner);
    app.list_start.set(start);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// One faint uppercase line naming only the columns actually present.
fn column_header_line(columns: &ColumnSet) -> Line<'static> {
    let style = Style::default().fg(faint());
    let mut spans = vec![Span::styled(format!("   {:<KEY_WIDTH$} ", "KEY"), style)];
    if columns.type_chip {
        spans.push(Span::styled(format!("{:<6} ", "TYPE"), style));
    }
    spans.push(Span::styled(format!("{:<STATUS_WIDTH$} ", "STATUS"), style));
    spans.push(Span::styled(
        format!("{:<SUMMARY_WIDTH$}", "SUMMARY"),
        style,
    ));
    if columns.assignee {
        spans.push(Span::styled(
            format!("{:<ASSIGNEE_WIDTH$}", "ASSIGNEE"),
            style,
        ));
    }
    if columns.updated {
        spans.push(Span::styled("UPDATED", style));
    }
    Line::from(spans)
}

/// Full-width quick-view panel: the selected issue's fields and full ADF body,
/// once loaded into the detail cache (loaded lazily by the app's event loop).
pub(crate) fn draw_quick_view(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.list_focus == ListFocus::QuickView;
    let border = if focused { accent2() } else { muted() };

    let Some(issue) = app.selected_issue() else {
        let block = card_bordered("  quick view  ", accent2(), border);
        app.quick_view_area.set(block.inner(area));
        f.render_widget(block, area);
        return;
    };
    let hint = if focused {
        " · ↑/↓ scroll "
    } else {
        " · tab to scroll "
    };
    let title = format!("  quick view · {}{hint} ", issue.key);
    let block = card_bordered(&title, accent2(), border);
    let inner = block.inner(area);
    app.quick_view_area.set(inner);
    f.render_widget(block, area);

    let lines: Vec<Line> = if let Some(detail) = app.quick_view_detail() {
        let mut rendered = crate::render::issue_detail_lines(detail);
        if let Some(target) = rendered.links.get(app.link_index) {
            crate::render::highlight_target(&mut rendered.lines, target);
        }
        rendered.lines
    } else {
        // Not cached yet: show what we know from the summary row while the
        // full detail loads in the background.
        vec![
            Line::from(vec![
                Span::styled(
                    format!("{} ", issue.key),
                    Style::default().fg(accent()).add_modifier(Modifier::BOLD),
                ),
                chip(&issue.issue_type, type_colour(&issue.issue_type)),
                Span::raw(" "),
                chip(&issue.status, status_colour(&issue.status)),
                Span::raw(" "),
                chip(issue.priority.label(), priority_colour(&issue.priority)),
                if issue.blocked {
                    Span::styled("  ⛔ blocked", Style::default().fg(danger()))
                } else {
                    Span::raw("")
                },
            ]),
            Line::from(Span::styled(
                issue.summary.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(
                    "assignee: {}   updated: {}",
                    issue
                        .assignee
                        .clone()
                        .unwrap_or_else(|| "unassigned".into()),
                    issue.updated
                ),
                Style::default().fg(muted()),
            )),
            Line::from(Span::styled(
                "loading full detail…",
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            )),
        ]
    };

    f.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((app.quick_view_scroll, 0)),
        inner,
    );
}

/// The box-drawing tree guide for the key column (SPEC.md §3): `▾ ` on a
/// parent (never `▸ ` — collapse isn't implemented, see phase 4's plan),
/// `├─ `/`└─ ` on a non-last/last child, with a `│` continuation for every
/// ancestor level whose own siblings aren't exhausted yet. Empty in flat
/// mode (`guide.depth == 0` and no children never draws anything but the
/// parent marker).
fn tree_guide(guide: &TreeRow, selected: bool) -> Span<'static> {
    let style = selected_style(Style::default().fg(faint()), selected);
    if guide.depth == 0 {
        if guide.has_children {
            Span::styled("▾ ", style)
        } else {
            Span::raw("")
        }
    } else {
        let mut s = String::new();
        for &continues in &guide.rails[..guide.depth - 1] {
            s.push_str(if continues { "│ " } else { "  " });
        }
        s.push_str(if guide.is_last { "└─ " } else { "├─ " });
        Span::styled(s, style)
    }
}

/// A single row in the work list. Also reused by the search results list
/// (which computes its own `ColumnSet` from its own panel width, and passes
/// a trivial flat `TreeRow` since search results aren't nested). Returns
/// more than one `Line` only for the selected row on a narrow terminal
/// (`columns.two_line`), which grows a second line carrying exactly what
/// got dropped from the main columns (SPEC.md §3).
pub(crate) fn issue_row(
    issue: &IssueSummary,
    selected: bool,
    guide: &TreeRow,
    columns: &ColumnSet,
) -> Vec<Line<'static>> {
    // One selection language shared with the board and every picker: a
    // maple bar plus a low-alpha maple tint across the whole row — every
    // span below runs through `selected_style` so the tint has no gaps.
    let cursor = if selected { "▌" } else { " " };
    let cursor_style = selected_style(
        if selected {
            Style::default().fg(maple())
        } else {
            Style::default()
        },
        selected,
    );
    let key_style = selected_style(
        if selected {
            Style::default().fg(accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(accent())
        },
        selected,
    );
    let summary_style = selected_style(
        if selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        },
        selected,
    );

    let block_flag = if issue.blocked {
        Span::styled(
            " ⛔",
            selected_style(Style::default().fg(danger()), selected),
        )
    } else {
        Span::raw("")
    };

    let secondary_style = selected_style(
        if selected {
            Style::default().fg(Color::Gray)
        } else {
            Style::default().fg(muted())
        },
        selected,
    );

    let mut spans = vec![
        Span::styled(cursor.to_string(), cursor_style),
        Span::styled(
            priority_glyph(&issue.priority),
            selected_style(priority_style(&issue.priority), selected),
        ),
        Span::styled(" ", selected_style(Style::default(), selected)),
        tree_guide(guide, selected),
        Span::styled(format!("{:<KEY_WIDTH$}", issue.key), key_style),
        Span::styled(" ", selected_style(Style::default(), selected)),
    ];

    if columns.type_chip {
        spans.push(chip(
            &format!("{:<4}", issue.issue_type),
            type_colour(&issue.issue_type),
        ));
        spans.push(Span::styled(
            " ",
            selected_style(Style::default(), selected),
        ));
    }

    spans.push(chip(
        &format!("{:<STATUS_WIDTH$}", status_short(&issue.status)),
        status_colour(&issue.status),
    ));
    spans.push(Span::styled(
        " ",
        selected_style(Style::default(), selected),
    ));
    spans.push(Span::styled(
        truncate(&issue.summary, SUMMARY_WIDTH),
        summary_style,
    ));
    spans.push(block_flag);

    if columns.assignee {
        let name = issue.assignee.as_deref().unwrap_or("unassigned");
        let text = format!(
            "  {} {}",
            initials(name),
            truncate(name, ASSIGNEE_WIDTH.saturating_sub(3))
        );
        spans.push(Span::styled(text, secondary_style));
    }
    if columns.updated {
        spans.push(Span::styled(
            format!("  {}", issue.updated),
            secondary_style,
        ));
    }

    let mut lines = vec![Line::from(spans)];

    if columns.two_line && selected {
        let mut second = vec![Span::raw("     ")];
        second.push(chip(&issue.issue_type, type_colour(&issue.issue_type)));
        second.push(Span::raw(" "));
        let name = issue.assignee.as_deref().unwrap_or("unassigned");
        second.push(Span::styled(
            format!("{} {}", initials(name), name),
            Style::default().fg(muted()),
        ));
        if let Some(parent) = &issue.epic {
            second.push(Span::styled(
                format!("  ↳ {parent}"),
                Style::default().fg(muted()),
            ));
        }
        lines.push(Line::from(second));
    }

    lines
}

/// A trivial, un-nested `TreeRow` for screens (like search results) that
/// reuse `issue_row` outside tree mode.
pub(crate) fn flat_guide() -> TreeRow {
    TreeRow {
        idx: 0,
        depth: 0,
        has_children: false,
        is_last: true,
        rails: Vec::new(),
    }
}
