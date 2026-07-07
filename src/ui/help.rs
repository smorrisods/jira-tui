//! The keyboard shortcuts help overlay.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

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
    let rows = [
        ("↑ / k", "move up"),
        ("↓ / j", "move down"),
        ("→ / ⏎", "open selected issue"),
        ("esc/←/⌫", "back"),
        ("/", "search or go to an issue by key"),
        ("s / S", "cycle sort / flip direction"),
        ("f", "cycle status filter"),
        ("v", "toggle quick-view panel"),
        ("tab", "focus list ↔ quick view (enables arrow scroll)"),
        ("b", "swimlane board (Kanban-style, grouped by epic)"),
        ("t", "change status (in an issue)"),
        ("e / E", "edit description (in-TUI / $EDITOR)"),
        ("a", "about panel"),
        ("m", "toggle mouse mode"),
        ("J", "toggle Jax companion 🦦"),
        ("y / Y", "copy issue key / URL"),
        ("r", "refresh from source"),
        ("? / q", "toggle help / quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(
                    format!("  {k:<9}"),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(d.to_string(), Style::default().fg(Color::White)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(Text::from(lines)).block(block), popup);
}
