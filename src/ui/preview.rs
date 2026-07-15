//! The edit-preview confirmation screen: shows the recompiled ADF before
//! anything is sent to Jira.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::adf;
use crate::app::App;

use super::{divider, MUTED, OK, WARN};

pub(crate) fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(OK))
        .title(Span::styled(
            format!("  preview · {key}  "),
            Style::default().fg(OK).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "This is how your edited description will look in Jira (rendered from ADF).",
            Style::default().fg(MUTED),
        )),
        Line::from(Span::styled(
            "Press y/⏎ to apply, or esc to cancel.",
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        )),
        divider(),
    ];
    if let Some(adf) = app.pending_edit.as_ref() {
        lines.extend(adf::render(adf).lines);
    }
    let para = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, inner);
}
