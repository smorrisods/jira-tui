//! The priority picker modal: changing the viewed issue's priority. Mirrors
//! `ui/sprint_picker.rs`'s popup/scroll-window shape — no type-to-filter,
//! since the row list is always the same fixed 5 (`Priority::ALL`).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::domain::Priority;

use super::{accent, centered_rect_h, maple, muted, priority_colour, selected_style};

pub(crate) fn draw_priority_picker(f: &mut Frame, app: &App, area: Rect) {
    let rows = &Priority::ALL;
    // +4 for the two border rows, a blank separator, and the footer hint.
    let height = (rows.len() as u16).saturating_add(4).min(area.height);
    let popup = centered_rect_h(36, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  priority  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    for (i, priority) in rows.iter().enumerate() {
        let selected = i == app.priority_picker.selected;
        let cursor = if selected { "▌ " } else { "  " };
        let style = selected_style(
            Style::default()
                .fg(priority_colour(priority))
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            selected,
        );
        lines.push(Line::from(vec![
            Span::styled(
                cursor.to_string(),
                selected_style(Style::default().fg(maple()), selected),
            ),
            Span::styled(format!("{} ", priority.glyph()), style),
            Span::styled(priority.label(), style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎ apply · esc cancel",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
