//! The field-mapping screen: search a live Jira site's custom fields and
//! pick one to map "Acceptance Criteria" to (or clear the mapping).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

use super::{card, card_bordered, ACCENT, ACCENT2, MUTED, WARN};

pub(crate) fn draw_field_mapping(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Query input line.
    let input_block = card_bordered("  map acceptance criteria field  ", ACCENT, ACCENT);
    let input_inner = input_block.inner(rows[0]);
    f.render_widget(input_block, rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "› ",
                Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.field_query.clone(), Style::default().fg(Color::White)),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ])),
        input_inner,
    );

    // Field list.
    let results_block = card("  custom fields  ", ACCENT2);
    let inner = results_block.inner(rows[1]);
    f.render_widget(results_block, rows[1]);

    let filtered = app.filtered_field_catalog();
    if filtered.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No custom fields match that search.",
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (i, (id, name)) in filtered.iter().enumerate() {
        let selected = i == app.field_selected;
        let cursor = if selected { "▌" } else { " " };
        let cursor_style = if selected {
            Style::default().fg(ACCENT2)
        } else {
            Style::default()
        };
        if id.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(cursor.to_string(), cursor_style),
                Span::styled(
                    name.clone(),
                    Style::default().fg(WARN).add_modifier(Modifier::ITALIC),
                ),
            ]));
            continue;
        }
        let name_style = if selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![
            Span::styled(cursor.to_string(), cursor_style),
            Span::styled(name.clone(), name_style),
            Span::styled(format!("  ({id})"), Style::default().fg(MUTED)),
        ]));
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
