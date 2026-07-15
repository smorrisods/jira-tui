//! The built-in multi-line Markdown editor for description edits.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::{muted, warn};

pub(crate) fn draw_editor(f: &mut Frame, app: &App, area: Rect) {
    let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(warn()))
        .title(Span::styled(
            format!("  editing {key} · Markdown  "),
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            "  ^S preview · esc cancel  ",
            Style::default().fg(muted()),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let ed = &app.editor;
    let height = inner.height.max(1) as usize;
    let scroll = if ed.cy >= height {
        ed.cy - height + 1
    } else {
        0
    };

    let gutter_w = 4u16;
    let mut lines: Vec<Line> = Vec::new();
    for (i, line) in ed.lines.iter().enumerate().skip(scroll).take(height) {
        lines.push(Line::from(vec![
            Span::styled(format!("{:>3} ", i + 1), Style::default().fg(muted())),
            Span::raw(line.clone()),
        ]));
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);

    // Place the real terminal cursor.
    let cx = inner.x + gutter_w + ed.cx as u16;
    let cy = inner.y + (ed.cy - scroll) as u16;
    if cx < inner.x + inner.width && cy < inner.y + inner.height {
        f.set_cursor_position((cx, cy));
    }
}
