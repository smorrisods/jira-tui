//! The spelling-suggestion picker modal (`F2`, built-in editor only).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::{accent, centered_rect_h, muted, selected_style};

pub(crate) fn draw_spell_suggest(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.spell_suggest;
    let height = (s.suggestions.len() as u16)
        .saturating_add(4)
        .min(area.height);
    let popup = centered_rect_h(40, height, area);
    f.render_widget(Clear, popup);
    let word = app
        .editor
        .lines
        .get(s.line)
        .and_then(|line| line.get(s.start..s.end))
        .unwrap_or("");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            format!("  replace \"{word}\" with…  "),
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    for (i, suggestion) in s.suggestions.iter().enumerate() {
        let selected = i == s.selected;
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
            Span::raw(cursor.to_string()),
            Span::styled(suggestion.clone(), style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎ replace · esc cancel",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
