//! The assignee picker modal: reassigning or unassigning the viewed issue,
//! with a type-to-filter query line above the row list (mirroring
//! `ui/search.rs`'s input row).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, AssigneeRow};

use super::{centered_rect_h, ACCENT, ACCENT2, MUTED};

pub(crate) fn draw_assignee_picker(f: &mut Frame, app: &App, area: Rect) {
    let rows = &app.assignee_picker.rows;
    // +4 for the border, the query line, and the footer hint; +1 blank
    // separator between the query line and the row list.
    let height = (rows.len() as u16).saturating_add(5).min(area.height);
    let popup = centered_rect_h(46, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            "  assign to…  ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "› ",
            Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.assignee_picker.query.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled("▏", Style::default().fg(ACCENT)),
    ]));
    lines.push(Line::from(""));

    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matching teammates.",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let selected = i == app.assignee_picker.selected;
        let cursor = if selected { "▌ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let label = match row {
            AssigneeRow::Unassign => "Unassign".to_string(),
            AssigneeRow::User(u) => u.display_name.clone(),
        };
        lines.push(Line::from(vec![
            Span::styled(cursor.to_string(), Style::default().fg(ACCENT2)),
            Span::styled(label, style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎ assign · esc cancel",
        Style::default().fg(MUTED),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
