//! The Search / go-to-issue screen: a query input plus filtered results.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, SearchRow};

use super::list::issue_row;
use super::{card, card_bordered, ACCENT, ACCENT2, MUTED, WARN};

pub(crate) fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Query input line.
    let input_block = card_bordered("  search / go to issue  ", ACCENT, ACCENT);
    let input_inner = input_block.inner(rows[0]);
    f.render_widget(input_block, rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "› ",
                Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.search.query.clone(), Style::default().fg(Color::White)),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ])),
        input_inner,
    );

    // Results.
    let results_block = card("  results  ", ACCENT2);
    let inner = results_block.inner(rows[1]);
    f.render_widget(results_block, rows[1]);

    if app.search.rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No matches. Type an issue key like DS-123 to jump to it directly.",
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (i, row) in app.search.rows.iter().enumerate() {
        let selected = i == app.search.selected;
        let cursor = if selected { "▌" } else { " " };
        let cursor_style = if selected {
            Style::default().fg(ACCENT2)
        } else {
            Style::default()
        };
        match row {
            SearchRow::Goto(key) => {
                lines.push(Line::from(vec![
                    Span::styled(cursor.to_string(), cursor_style),
                    Span::styled(
                        "↵ go to ",
                        Style::default().fg(WARN).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        key.clone(),
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "  (fetch directly, even if it's not in your list)",
                        Style::default().fg(MUTED),
                    ),
                ]));
            }
            SearchRow::Match(idx) => {
                if let Some(issue) = app.all_issues.get(*idx) {
                    lines.push(issue_row(issue, selected));
                }
            }
        }
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
