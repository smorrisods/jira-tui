//! The attachment picker modal: list an issue's attachments, open the
//! highlighted one in the browser or download it to disk. Modelled closely
//! on `transition_picker`.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
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

    // A budget of extra rows for the image preview, only when one is
    // actually available for the highlighted attachment — every other case
    // (feature not compiled, no detected terminal capability, demo/cache
    // session, non-image attachment, fetch still in flight or failed) draws
    // exactly what this picker always drew, unchanged.
    #[cfg(feature = "images")]
    let preview_rows: u16 = if app.attachment_preview.borrow().as_ref().is_some_and(|p| {
        attachments
            .get(app.attachment_index)
            .is_some_and(|a| a.id == p.attachment_id)
    }) {
        12
    } else {
        0
    };
    #[cfg(not(feature = "images"))]
    let preview_rows: u16 = 0;

    let list_rows = (attachments.len() as u16).saturating_add(2); // items + blank + hint
    let height = list_rows
        .saturating_add(preview_rows)
        .saturating_add(2) // top/bottom border
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

    let (list_area, preview_area) = if preview_rows > 0 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(list_rows.min(inner.height)),
                Constraint::Min(0),
            ])
            .split(inner);
        (chunks[0], Some(chunks[1]))
    } else {
        (inner, None)
    };

    #[cfg(feature = "images")]
    if let Some(preview_area) = preview_area {
        let mut preview = app.attachment_preview.borrow_mut();
        if let Some(p) = preview.as_mut() {
            // `StatefulImage`'s default resize (`Resize::Fit`) leaves an
            // image at its native size whenever that's already smaller than
            // `preview_area` — for `Attachment::image_preview_url`'s source,
            // that used to mean rendering at whatever tiny size the source
            // happened to natively occupy. `Scale` always resizes to fill
            // `preview_area`.
            f.render_stateful_widget(
                ratatui_image::StatefulImage::default().resize(ratatui_image::Resize::Scale(None)),
                preview_area,
                &mut p.protocol,
            );
        }
    }
    #[cfg(not(feature = "images"))]
    let _ = preview_area;

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
    f.render_widget(Paragraph::new(Text::from(lines)), list_area);
}
