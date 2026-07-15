//! The work list panel and its full-width quick-view companion.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, ListFocus};
use crate::domain::IssueSummary;

use super::{
    accent, accent2, card_bordered, chip, danger, maple, muted, priority_colour, priority_glyph,
    priority_style, selected_style, status_colour, status_short, status_style, truncate,
    type_colour,
};

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
    let mut title = format!("  {base} · {}", app.sort_label());
    if let Some(filter) = app.filter_label() {
        title.push_str(&format!(" · {filter}"));
    }
    if app.list_view_mode == crate::app::ListViewMode::Tree {
        title.push_str(" · tree");
    }
    title.push_str(&format!("  ({})  ", app.issues.len()));
    // Dim the list's border when the quick-view panel has keyboard focus
    // (Tab), so it's clear which panel arrow keys currently affect.
    let list_focused = !(app.quick_view && app.list_focus == ListFocus::QuickView);
    let border = if list_focused { accent() } else { muted() };
    let block = card_bordered(&title, accent(), border);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = app.tree_rows();
    let mut lines: Vec<Line> = Vec::new();
    let height = inner.height as usize;
    let cur_pos = rows
        .iter()
        .position(|(i, _)| *i == app.selected)
        .unwrap_or(0);
    // simple scroll window around the selection
    let start = cur_pos.saturating_sub(height.saturating_sub(2).max(1) / 2);
    for &(idx, depth) in rows.iter().skip(start).take(height) {
        lines.push(issue_row(&app.issues[idx], idx == app.selected, depth));
    }
    // Record geometry so mouse clicks can be mapped back to issues (via
    // `tree_rows` again — `list_start` is a position within it, not a raw
    // index into `app.issues`).
    app.list_area.set(inner);
    app.list_start.set(start);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
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

/// A single row in the work list. Also reused by the search results list
/// (always at `depth` 0) and the swimlane board's card summaries. `depth`
/// nests a row under its parent in the tree view mode (see `app::tree`);
/// pass 0 for the flat list.
pub(crate) fn issue_row(issue: &IssueSummary, selected: bool, depth: usize) -> Line<'static> {
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

    let updated_style = selected_style(
        if selected {
            Style::default().fg(Color::Gray)
        } else {
            Style::default().fg(muted())
        },
        selected,
    );

    let indent = if depth > 0 {
        Span::styled(
            format!("{}└ ", "  ".repeat(depth - 1)),
            selected_style(Style::default().fg(muted()), selected),
        )
    } else {
        Span::raw("")
    };

    Line::from(vec![
        Span::styled(cursor.to_string(), cursor_style),
        Span::styled(
            priority_glyph(&issue.priority),
            selected_style(priority_style(&issue.priority), selected),
        ),
        Span::styled(" ", selected_style(Style::default(), selected)),
        indent,
        Span::styled(format!("{:<8}", issue.key), key_style),
        Span::styled(
            format!("{:<11}", status_short(&issue.status)),
            selected_style(status_style(&issue.status), selected),
        ),
        Span::styled(truncate(&issue.summary, 40), summary_style),
        block_flag,
        Span::styled(format!("  {}", issue.updated), updated_style),
    ])
}
