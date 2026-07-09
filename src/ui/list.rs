//! The work list panel and its full-width quick-view companion.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, ListFocus};
use crate::domain::IssueSummary;

use super::{
    card_bordered, chip, priority_colour, priority_glyph, priority_style, status_colour,
    status_short, status_style, truncate, ACCENT, ACCENT2, DANGER, MUTED,
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
    title.push_str(&format!("  ({})  ", app.issues.len()));
    // Dim the list's border when the quick-view panel has keyboard focus
    // (Tab), so it's clear which panel arrow keys currently affect.
    let list_focused = !(app.quick_view && app.list_focus == ListFocus::QuickView);
    let border = if list_focused { ACCENT } else { MUTED };
    let block = card_bordered(&title, ACCENT, border);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let height = inner.height as usize;
    // simple scroll window around the selection
    let start = app
        .selected
        .saturating_sub(height.saturating_sub(2).max(1) / 2);
    for (i, issue) in app.issues.iter().enumerate().skip(start).take(height) {
        lines.push(issue_row(issue, i == app.selected));
    }
    // Record geometry so mouse clicks can be mapped back to issues.
    app.list_area.set(inner);
    app.list_start.set(start);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Full-width quick-view panel: the selected issue's fields and full ADF body,
/// once loaded into the detail cache (loaded lazily by the app's event loop).
pub(crate) fn draw_quick_view(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.list_focus == ListFocus::QuickView;
    let border = if focused { ACCENT2 } else { MUTED };

    let Some(issue) = app.selected_issue() else {
        let block = card_bordered("  quick view  ", ACCENT2, border);
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
    let block = card_bordered(&title, ACCENT2, border);
    let inner = block.inner(area);
    app.quick_view_area.set(inner);
    f.render_widget(block, area);

    let lines: Vec<Line> = if let Some(detail) = app.quick_view_detail() {
        crate::render::issue_detail_lines(detail).lines
    } else {
        // Not cached yet: show what we know from the summary row while the
        // full detail loads in the background.
        vec![
            Line::from(vec![
                Span::styled(
                    format!("{} ", issue.key),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                chip(&issue.issue_type, ACCENT2),
                Span::raw(" "),
                chip(&issue.status, status_colour(&issue.status)),
                Span::raw(" "),
                chip(issue.priority.label(), priority_colour(&issue.priority)),
                if issue.blocked {
                    Span::styled("  ⛔ blocked", Style::default().fg(DANGER))
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
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "loading full detail…",
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
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

/// A single row in the work list. Also reused by the search results list and
/// the swimlane board's card summaries.
pub(crate) fn issue_row(issue: &IssueSummary, selected: bool) -> Line<'static> {
    let cursor = if selected { "▌" } else { " " };
    let cursor_style = if selected {
        Style::default().fg(ACCENT2)
    } else {
        Style::default()
    };
    let key_style = if selected {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACCENT)
    };
    let summary_style = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let block_flag = if issue.blocked {
        Span::styled(" ⛔", Style::default().fg(DANGER))
    } else {
        Span::raw("")
    };

    let updated_style = if selected {
        Style::default().fg(Color::Gray)
    } else {
        Style::default().fg(MUTED)
    };

    Line::from(vec![
        Span::styled(cursor.to_string(), cursor_style),
        Span::styled(
            priority_glyph(&issue.priority),
            priority_style(&issue.priority),
        ),
        Span::raw(" "),
        Span::styled(format!("{:<8}", issue.key), key_style),
        Span::styled(
            format!("{:<11}", status_short(&issue.status)),
            status_style(&issue.status),
        ),
        Span::styled(truncate(&issue.summary, 40), summary_style),
        block_flag,
        Span::styled(format!("  {}", issue.updated), updated_style),
    ])
}
