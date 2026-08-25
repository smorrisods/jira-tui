//! The attachment picker modal: list an issue's attachments, open the
//! highlighted one in the browser or download it to disk. Modelled closely
//! on `transition_picker`.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::render::human_size;

use super::{accent, centered_rect_h, maple, muted, selected_style};

pub(crate) fn draw_attachment_picker(f: &mut Frame, app: &App, area: Rect) {
    let attachments = match app.detail.as_ref() {
        Some(d) => &d.attachments,
        None => return,
    };
    let height = (attachments.len() as u16)
        .saturating_add(4)
        .min(area.height);
    let popup = centered_rect_h(60, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  attachments  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    for (i, a) in attachments.iter().enumerate() {
        let selected = i == app.attachment_index;
        let cursor = if selected { "▌ " } else { "  " };
        let name_style = selected_style(
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
            Span::styled(a.filename.clone(), name_style),
            Span::styled(
                format!(" · {} · {}", human_size(a.size), a.mime_type),
                selected_style(Style::default().fg(muted()), selected),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎/o open · d download · esc/← close",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
