//! The upload flow's three overlays (`u`, Detail only — see
//! `app::AttachmentUpload`): a scrollable filesystem browser (the default
//! entry point), a slim path-entry line laid out like `project_picker.rs`'s
//! query box (`Tab`-accessible fallback), and a full preview modal matching
//! `preview.rs`'s style/copy as closely as this flow's content allows — the
//! mandatory confirm step CLAUDE.md's "Preview before any mutating Jira
//! call" requires before `App::confirm_attachment_upload` actually
//! dispatches anything.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, AttachmentUpload, FileBrowserState};
use crate::render::human_size;

use super::{accent, accent2, centered_rect_h, divider, maple, muted, ok, selected_style, warn};

pub(crate) fn draw_attachment_upload(f: &mut Frame, app: &App, area: Rect) {
    let Some(stage) = app.attachment_upload.as_ref() else {
        return;
    };
    match stage {
        AttachmentUpload::Browse { browser } => draw_browse(f, browser, area),
        AttachmentUpload::Input { path } => draw_input(f, path, area),
        AttachmentUpload::Confirm {
            filename,
            size,
            mime,
            content_preview,
            ..
        } => draw_confirm(
            f,
            app,
            filename,
            *size,
            mime,
            content_preview.as_deref(),
            area,
        ),
    }
}

fn draw_browse(f: &mut Frame, browser: &FileBrowserState, area: Rect) {
    let rows = &browser.entries;
    // Same scroll-window sizing as `draw_project_picker`: +2 borders, a
    // header line for `cwd`, a blank separator, and the footer hint.
    let max_popup_height = area.height.saturating_sub(4).max(1);
    let visible = (max_popup_height.saturating_sub(5) as usize).max(1);
    let height = (rows.len().min(visible) as u16)
        .saturating_add(5)
        .min(area.height);
    let popup = centered_rect_h(70, height, area);
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

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        browser.cwd.display().to_string(),
        Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
    )));

    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            if browser.filter.is_empty() {
                "(empty directory)".to_string()
            } else {
                "No matching entries.".to_string()
            },
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )));
    }
    // Simple scroll window around the selection, same shape as
    // `draw_project_picker`/`draw_sprint_picker`.
    let start = browser
        .selected
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(rows.len().saturating_sub(visible));
    for (i, entry) in rows.iter().enumerate().skip(start).take(visible) {
        let selected = i == browser.selected;
        let cursor = if selected { "▌ " } else { "  " };
        let style = selected_style(
            if selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if entry.is_dir {
                Style::default().fg(accent2())
            } else {
                Style::default().fg(Color::Gray)
            },
            selected,
        );
        let label = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                cursor.to_string(),
                selected_style(Style::default().fg(maple()), selected),
            ),
            Span::styled(label, style),
        ]));
    }
    lines.push(Line::from(""));
    let footer = if browser.filter.is_empty() {
        "⏎ open/select · ↑↓ move · ⌫ up a level · tab type a path · esc cancel".to_string()
    } else {
        format!(
            "filter› {}▏ · ⏎ open/select · ⌫ edit filter · tab type a path · esc cancel",
            browser.filter
        )
    };
    lines.push(Line::from(Span::styled(
        footer,
        Style::default().fg(muted()),
    )));
    let para = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
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
            "⏎ next · tab browse · esc cancel",
            Style::default().fg(muted()),
        )),
    ];
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_confirm(
    f: &mut Frame,
    app: &App,
    filename: &str,
    size: u64,
    mime: &str,
    content_preview: Option<&str>,
    area: Rect,
) {
    let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
    // A text preview needs real room to breathe; a metadata-only preview
    // keeps the original slim height.
    let height = if content_preview.is_some() { 20 } else { 10 };
    let popup = centered_rect_h(60, height, area);
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

    let mut lines = vec![
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
    if let Some(preview) = content_preview {
        lines.push(divider());
        lines.push(Line::from(Span::styled(
            "Preview:",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )));
        for line in preview.lines() {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::Gray),
            )));
        }
    }
    let para = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}
