//! The field-mapping screen: search a live Jira site's custom fields and
//! pick one to map "Acceptance Criteria" to (or clear the mapping).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

use super::{accent, accent2, card, card_bordered, maple, muted, ok, selected_style, warn};

pub(crate) fn draw_field_mapping(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Query input line.
    let input_block = card_bordered("  map acceptance criteria field  ", accent(), accent());
    let input_inner = input_block.inner(rows[0]);
    f.render_widget(input_block, rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "› ",
                Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.field_mapping.query.clone(),
                Style::default().fg(Color::White),
            ),
            Span::styled("▏", Style::default().fg(accent())),
        ])),
        input_inner,
    );

    // Field list — the title names whatever's currently mapped, so
    // re-opening this screen to change a mapping shows what you're editing.
    let current_label = match &app.field_mapping.current_mapping {
        Some(id) => app
            .field_mapping
            .catalog
            .iter()
            .find(|(fid, _)| fid == id)
            .map(|(_, name)| format!("{name} ({id})"))
            .unwrap_or_else(|| format!("{id} — no longer found on this site")),
        None => "none".to_string(),
    };
    let results_title = format!("  custom fields — currently: {current_label}  ");
    let results_block = card(&results_title, accent2());
    let inner = results_block.inner(rows[1]);
    f.render_widget(results_block, rows[1]);

    let filtered = app.filtered_field_catalog();
    if filtered.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No custom fields match that search.",
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (i, (id, name)) in filtered.iter().enumerate() {
        let selected = i == app.field_mapping.selected;
        let is_current = match &app.field_mapping.current_mapping {
            Some(mapped) => mapped.as_str() == id.as_str(),
            None => id.is_empty(),
        };
        let cursor = if selected { "▌" } else { " " };
        let cursor_style = selected_style(
            if selected {
                Style::default().fg(maple())
            } else {
                Style::default()
            },
            selected,
        );
        let current_marker = if is_current {
            Span::styled(
                " ✓ current",
                selected_style(
                    Style::default().fg(ok()).add_modifier(Modifier::BOLD),
                    selected,
                ),
            )
        } else {
            Span::raw("")
        };
        if id.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(cursor.to_string(), cursor_style),
                Span::styled(
                    name.clone(),
                    selected_style(
                        Style::default().fg(warn()).add_modifier(Modifier::ITALIC),
                        selected,
                    ),
                ),
                current_marker,
            ]));
            continue;
        }
        let name_style = selected_style(
            if selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
            selected,
        );
        lines.push(Line::from(vec![
            Span::styled(cursor.to_string(), cursor_style),
            Span::styled(name.clone(), name_style),
            Span::styled(
                format!("  ({id})"),
                selected_style(Style::default().fg(muted()), selected),
            ),
            current_marker,
        ]));
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
