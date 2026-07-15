//! The keyboard shortcuts help overlay.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::keymap::KEYMAP;
use super::{accent, centered_rect_h};

pub(crate) fn draw_help_overlay(f: &mut Frame, area: Rect) {
    // Size the popup to fit every row (clamped to the frame height) rather
    // than a fixed percentage, so the list — including its own `? / q`
    // close hint — doesn't get silently clipped as KEYMAP grows.
    let height = (KEYMAP.len() as u16).saturating_add(2).min(area.height);
    let popup = centered_rect_h(56, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  keys  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    // Pad the key column to the widest entry (plus a one-space gap) instead
    // of a fixed width, so a key longer than the old hardcoded width
    // doesn't run straight into its description with no separator.
    let key_width = KEYMAP
        .iter()
        .map(|h| h.key.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = KEYMAP
        .iter()
        .map(|hint| {
            let pad = " ".repeat(key_width - hint.key.chars().count() + 1);
            Line::from(vec![
                Span::styled(
                    format!("  {}{pad}", hint.key),
                    Style::default().fg(accent()).add_modifier(Modifier::BOLD),
                ),
                Span::styled(hint.desc.to_string(), Style::default().fg(Color::White)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(Text::from(lines)).block(block), popup);
}
