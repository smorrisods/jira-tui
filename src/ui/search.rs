//! The Search / go-to-issue screen: a query input plus filtered results.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, SearchRow};

use super::list::{flat_guide, issue_row};
use super::list_columns::column_set_for_width;
use super::{accent, accent2, card, card_bordered, maple, muted, selected_style, warn};

pub(crate) fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Query input line.
    let input_block = card_bordered("  search / go to issue  ", accent(), accent());
    let input_inner = input_block.inner(rows[0]);
    f.render_widget(input_block, rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "› ",
                Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.search.query.clone(), Style::default().fg(Color::White)),
            Span::styled("▏", Style::default().fg(accent())),
        ])),
        input_inner,
    );

    // Results.
    let results_block = card("  results  ", accent2());
    let inner = results_block.inner(rows[1]);
    f.render_widget(results_block, rows[1]);

    if app.search.rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No matches. Type an issue key like DS-123 to jump to it directly.",
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let columns = column_set_for_width(inner.width);
    let guide = flat_guide();
    let mut lines: Vec<Line> = Vec::new();
    for (i, row) in app.search.rows.iter().enumerate() {
        let selected = i == app.search.selected;
        let cursor = if selected { "▌" } else { " " };
        let cursor_style = selected_style(
            if selected {
                Style::default().fg(maple())
            } else {
                Style::default()
            },
            selected,
        );
        match row {
            SearchRow::Goto(key) => {
                lines.push(Line::from(vec![
                    Span::styled(cursor.to_string(), cursor_style),
                    Span::styled(
                        "↵ go to ",
                        selected_style(
                            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
                            selected,
                        ),
                    ),
                    Span::styled(
                        key.clone(),
                        selected_style(
                            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
                            selected,
                        ),
                    ),
                    Span::styled(
                        "  (fetch directly, even if it's not in your list)",
                        selected_style(Style::default().fg(muted()), selected),
                    ),
                ]));
            }
            SearchRow::Match(idx) => {
                if let Some(issue) = app.all_issues.get(*idx) {
                    lines.extend(issue_row(issue, selected, &guide, &columns));
                }
            }
        }
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
