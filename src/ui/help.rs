//! The keyboard shortcuts help overlay.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::keymap::KEYMAP;
use super::{centered_rect, ACCENT};

pub(crate) fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup = centered_rect(56, 62, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            "  keys  ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let lines: Vec<Line> = KEYMAP
        .iter()
        .map(|hint| {
            Line::from(vec![
                Span::styled(
                    format!("  {:<9}", hint.key),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(hint.desc.to_string(), Style::default().fg(Color::White)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(Text::from(lines)).block(block), popup);
}
