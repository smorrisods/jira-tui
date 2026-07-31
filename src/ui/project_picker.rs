//! The new-issue form's project picker modal: a type-to-filter search over
//! every accessible project, laid out the same way as `assignee_picker.rs`
//! (query line, blank separator, scrolling row list, footer hint).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::{accent, accent2, centered_rect_h, maple, muted, selected_style};

pub(crate) fn draw_project_picker(f: &mut Frame, app: &App, area: Rect) {
    let rows = &app.project_picker.rows;
    // Same scroll-window sizing as `draw_assignee_picker`: a Jira instance
    // can have far more projects than fit on screen, so cap the popup
    // height and keep the highlighted row in view rather than growing
    // unbounded.
    let max_popup_height = area.height.saturating_sub(4).max(1);
    let visible = (max_popup_height.saturating_sub(5) as usize).max(1);
    let height = (rows.len().min(visible) as u16)
        .saturating_add(5)
        .min(area.height);
    let popup = centered_rect_h(46, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  project…  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "› ",
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.project_picker.query.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled("▏", Style::default().fg(accent())),
    ]));
    lines.push(Line::from(""));

    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matching projects.",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )));
    }
    // Simple scroll window around the selection, same shape as
    // `draw_assignee_picker`/`list.rs`'s tree view.
    let start = app
        .project_picker
        .selected
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(rows.len().saturating_sub(visible));
    for (i, project) in rows.iter().enumerate().skip(start).take(visible) {
        let selected = i == app.project_picker.selected;
        let cursor = if selected { "▌ " } else { "  " };
        let style = selected_style(
            if selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
            selected,
        );
        lines.push(Line::from(vec![
            Span::styled(
                cursor.to_string(),
                selected_style(Style::default().fg(maple()), selected),
            ),
            Span::styled(format!("{}  ", project.key), style),
            Span::styled(
                project.name.clone(),
                selected_style(Style::default().fg(muted()), selected),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎ select · esc cancel",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
