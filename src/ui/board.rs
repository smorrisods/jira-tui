//! The swimlane Kanban board: status columns across the top, Epic-grouped
//! lanes down the side, rendered as a scrollable text grid.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::domain::IssueSummary;

use super::{card, status_colour, truncate, ACCENT, ACCENT2, MUTED};

pub(crate) fn draw_board(f: &mut Frame, app: &App, area: Rect) {
    let cols = app.board_columns();
    let lanes = app.board_lanes();

    let title = format!(
        "  board · {} lane{} · {} column{}  ",
        lanes.len(),
        if lanes.len() == 1 { "" } else { "s" },
        cols.len(),
        if cols.len() == 1 { "" } else { "s" },
    );
    let block = card(&title, ACCENT);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.board_area.set(inner);

    if app.issues.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No issues in the current view.",
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let n = cols.len().max(1);
    let sep_width = n.saturating_sub(1);
    let col_width = ((inner.width as usize).saturating_sub(sep_width)) / n;
    let col_width = col_width.max(12);

    let mut lines: Vec<Line> = Vec::new();

    // Column header row: status name + total count in that column.
    let mut header: Vec<Span> = Vec::new();
    for (ci, status) in cols.iter().enumerate() {
        let count = app.issues.iter().filter(|i| &i.status == status).count();
        let label = truncate(&format!("{status} ({count})"), col_width);
        header.push(Span::styled(
            format!("{label:<col_width$}"),
            Style::default()
                .fg(status_colour(status))
                .add_modifier(Modifier::BOLD),
        ));
        if ci + 1 < n {
            header.push(Span::styled(" │ ", Style::default().fg(MUTED)));
        }
    }
    lines.push(Line::from(header));
    lines.push(Line::from(Span::styled(
        "─".repeat(inner.width as usize),
        Style::default().fg(MUTED),
    )));

    for (li, lane) in lanes.iter().enumerate() {
        let cell_lists: Vec<Vec<&IssueSummary>> = cols
            .iter()
            .map(|status| app.board_cell(lane, status))
            .collect();
        let lane_count: usize = cell_lists.iter().map(|c| c.len()).sum();

        lines.push(Line::from(Span::styled(
            format!("▸ {} ({lane_count})", app.board_lane_label(lane)),
            Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
        )));

        let max_rows = cell_lists.iter().map(|c| c.len()).max().unwrap_or(0).max(1);
        for row in 0..max_rows {
            let mut spans: Vec<Span> = Vec::new();
            for (ci, cell) in cell_lists.iter().enumerate() {
                let selected = li == app.board_sel.lane
                    && ci == app.board_sel.col
                    && row == app.board_sel.card;
                let text = match cell.get(row) {
                    Some(issue) => {
                        let head = format!("{}{} ", issue.priority.glyph(), issue.key);
                        let remaining = col_width.saturating_sub(head.chars().count());
                        format!("{head}{}", truncate(&issue.summary, remaining))
                    }
                    None => String::new(),
                };
                let mut style = Style::default().fg(if cell.get(row).is_some() {
                    Color::White
                } else {
                    MUTED
                });
                if selected {
                    style = style
                        .bg(Color::Rgb(40, 40, 80))
                        .add_modifier(Modifier::BOLD);
                }
                spans.push(Span::styled(format!("{text:<col_width$}"), style));
                if ci + 1 < n {
                    spans.push(Span::styled(" │ ", Style::default().fg(MUTED)));
                }
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(""));
    }

    let para = Paragraph::new(Text::from(lines)).scroll((app.board_scroll, 0));
    f.render_widget(para, inner);
}
