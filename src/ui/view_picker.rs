//! The view-switcher picker modal: My Work / All Project Issues / a
//! teammate's work.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::{accent, centered_rect_h, maple, muted, selected_style};

pub(crate) fn draw_view_picker(f: &mut Frame, app: &App, area: Rect) {
    let options = &app.view_picker_options;
    let height = (options.len() as u16).saturating_add(4).min(area.height);
    let popup = centered_rect_h(46, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  switch view…  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    for (i, view) in options.iter().enumerate() {
        let selected = i == app.view_picker_index;
        let is_current = *view == app.current_view;
        let cursor = if selected { "▌ " } else { "  " };
        let mut style = selected_style(
            if selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
            selected,
        );
        if is_current {
            style = style.fg(accent());
        }
        let suffix = if is_current { "  (current)" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(
                cursor.to_string(),
                selected_style(Style::default().fg(maple()), selected),
            ),
            Span::styled(view.label(), style),
            Span::styled(
                suffix.to_string(),
                selected_style(Style::default().fg(muted()), selected),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎ switch · esc/← cancel",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
