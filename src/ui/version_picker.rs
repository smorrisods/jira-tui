//! The Fix/Affects Version picker modal: a checklist over the project's
//! versions, `Tab` switching which field is being edited. Mirrors
//! `ui/assignee_picker.rs`'s popup/scroll-window shape, with a checkbox
//! glyph per row instead of a single highlighted selection.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, VersionField};

use super::{accent, accent2, centered_rect_h, maple, muted, ok, selected_style};

pub(crate) fn draw_version_picker(f: &mut Frame, app: &App, area: Rect) {
    let picker = &app.version_picker;
    let versions = &picker.versions;

    let max_popup_height = area.height.saturating_sub(4).max(1);
    // +5 for the border, the field-tab line, its blank separator, and the
    // footer hint — same budget shape as the assignee picker.
    let visible = (max_popup_height.saturating_sub(5) as usize).max(1);
    let height = (versions.len().min(visible) as u16)
        .saturating_add(5)
        .min(area.height);
    let popup = centered_rect_h(46, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  versions  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(field_tab_line(picker.field));
    lines.push(Line::from(""));

    if versions.is_empty() {
        lines.push(Line::from(Span::styled(
            "No versions defined on this project.",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )));
    }
    let selected_set = match picker.field {
        VersionField::Fix => &picker.fix_selected,
        VersionField::Affects => &picker.affects_selected,
    };
    // Simple scroll window around the cursor, same shape as the assignee
    // picker's `selected` window.
    let start = picker
        .cursor
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(versions.len().saturating_sub(visible));
    for (i, version) in versions.iter().enumerate().skip(start).take(visible) {
        let highlighted = i == picker.cursor;
        let cursor = if highlighted { "▌ " } else { "  " };
        let checked = selected_set.contains(&version.name);
        let checkbox = if checked { "✓ " } else { "○ " };
        let style = selected_style(
            if highlighted {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
            highlighted,
        );
        let label = match &version.release_date {
            Some(date) if version.released => format!("{} (released {date})", version.name),
            Some(date) => format!("{} (target {date})", version.name),
            None if version.released => format!("{} (released)", version.name),
            None => version.name.clone(),
        };
        lines.push(Line::from(vec![
            Span::styled(
                cursor.to_string(),
                selected_style(Style::default().fg(maple()), highlighted),
            ),
            Span::styled(
                checkbox.to_string(),
                selected_style(
                    Style::default().fg(if checked { ok() } else { muted() }),
                    highlighted,
                ),
            ),
            Span::styled(label, style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "space toggle · tab switch field · ⏎ apply · esc cancel",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// The two-tab "Fix Version(s) / Affects Version(s)" header line, the active
/// field bold and accented.
fn field_tab_line(field: VersionField) -> Line<'static> {
    let (fix_style, affects_style) = match field {
        VersionField::Fix => (
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
            Style::default().fg(muted()),
        ),
        VersionField::Affects => (
            Style::default().fg(muted()),
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ),
    };
    Line::from(vec![
        Span::styled("Fix Version(s)", fix_style),
        Span::styled("   ", Style::default()),
        Span::styled("Affects Version(s)", affects_style),
    ])
}
