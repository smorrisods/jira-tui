//! The upload flow's two overlays (`u`, Detail only — see
//! `app::AttachmentUpload`): a slim path-entry line, laid out like
//! `project_picker.rs`'s query box, and a full preview modal matching
//! `preview.rs`'s style/copy as closely as this flow's content allows — the
//! mandatory confirm step CLAUDE.md's "Preview before any mutating Jira
//! call" requires before `App::confirm_attachment_upload` actually
//! dispatches anything.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, AttachmentUpload};
use crate::render::human_size;

use super::{accent, accent2, centered_rect_h, divider, muted, ok, warn};

pub(crate) fn draw_attachment_upload(f: &mut Frame, app: &App, area: Rect) {
    let Some(stage) = app.attachment_upload.as_ref() else {
        return;
    };
    match stage {
        AttachmentUpload::Input { path } => draw_input(f, path, area),
        AttachmentUpload::Confirm {
            filename,
            size,
            mime,
            ..
        } => draw_confirm(f, app, filename, *size, mime, area),
    }
}

fn draw_input(f: &mut Frame, path: &str, area: Rect) {
    let popup = centered_rect_h(60, 5, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  upload attachment…  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let lines = vec![
        Line::from(vec![
            Span::styled(
                "path› ",
                Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(path.to_string(), Style::default().fg(Color::White)),
            Span::styled("▏", Style::default().fg(accent())),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "⏎ next · esc cancel",
            Style::default().fg(muted()),
        )),
    ];
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_confirm(f: &mut Frame, app: &App, filename: &str, size: u64, mime: &str, area: Rect) {
    let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
    let popup = centered_rect_h(60, 10, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ok()))
        .title(Span::styled(
            format!("  upload · {key}  "),
            Style::default().fg(ok()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let lines = vec![
        Line::from(Span::styled(
            format!("This file will be uploaded to {key}."),
            Style::default().fg(muted()),
        )),
        Line::from(Span::styled(
            "Press y/⏎ to upload, or esc to go back and change the path.",
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        )),
        divider(),
        Line::from(vec![
            Span::styled("Filename: ", Style::default().fg(muted())),
            Span::raw(filename.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Size: ", Style::default().fg(muted())),
            Span::raw(human_size(size)),
            Span::styled("   Type: ", Style::default().fg(muted())),
            Span::raw(mime.to_string()),
        ]),
    ];
    let para = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}
