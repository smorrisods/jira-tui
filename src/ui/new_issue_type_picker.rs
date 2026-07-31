//! The new-issue form's issue-type dropdown modal (`Enter` on the IssueType
//! field, or a click — see `App::open_new_issue_type_picker`): lists every
//! creatable issue type for the chosen project at once, replacing the old
//! left/right scroller so the full catalog is visible at a glance. Mirrors
//! `transition_picker.rs`'s shape (no type-to-filter query line needed —
//! the catalog is always small).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::{accent, centered_rect_h, maple, muted, selected_style};

pub(crate) fn draw_new_issue_type_picker(f: &mut Frame, app: &App, area: Rect) {
    let types = &app.new_issue.available_types;
    if types.is_empty() {
        return;
    }
    let height = (types.len() as u16).saturating_add(4).min(area.height);
    let popup = centered_rect_h(46, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  issue type  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    for (i, t) in types.iter().enumerate() {
        let selected = i == app.new_issue.type_picker_cursor;
        let is_current = i == app.new_issue.issue_type_index;
        let cursor = if selected { "▌ " } else { "  " };
        let mut style = selected_style(
            if selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
            selected,
        );
        if is_current {
            style = style.fg(accent());
        }
        let suffix = if is_current { "  (current)" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(
                cursor.to_string(),
                selected_style(Style::default().fg(maple()), selected),
            ),
            Span::styled(t.name.clone(), style),
            Span::styled(
                suffix.to_string(),
                selected_style(Style::default().fg(muted()), selected),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎ select · esc cancel",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
