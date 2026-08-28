//! The sprint picker modal: moving the viewed issue into a sprint, or back
//! to the backlog. Mirrors `ui/assignee_picker.rs`'s popup/scroll-window
//! shape closely, minus its type-to-filter query line — `list_open_sprints`
//! already returns a short, server-filtered (active/future only) list, so
//! there's nothing worth filtering.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, SprintRow};

use super::{accent, centered_rect_h, maple, muted, selected_style};

pub(crate) fn draw_sprint_picker(f: &mut Frame, app: &App, area: Rect) {
    let rows = &app.sprint_picker.rows;
    let max_popup_height = area.height.saturating_sub(4).max(1);
    // +4 for the two border rows, a blank separator, and the footer hint —
    // no query line to budget for, unlike the assignee picker.
    let visible = (max_popup_height.saturating_sub(4) as usize).max(1);
    let height = (rows.len().min(visible) as u16)
        .saturating_add(4)
        .min(area.height);
    let popup = centered_rect_h(46, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  sprint  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "No open sprints — check sprint_board_id in config.toml.",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )));
    }
    // Simple scroll window around the selection, same shape as the assignee
    // picker's.
    let start = app
        .sprint_picker
        .selected
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(rows.len().saturating_sub(visible));
    for (i, row) in rows.iter().enumerate().skip(start).take(visible) {
        let selected = i == app.sprint_picker.selected;
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
        let label = match row {
            SprintRow::RemoveFromSprint => "Remove from sprint".to_string(),
            SprintRow::Sprint(s) if s.state == "active" => s.name.clone(),
            SprintRow::Sprint(s) => format!("{} ({})", s.name, s.state),
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
    lines.push(Line::from(Span::styled(
        "⏎ apply · esc cancel",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
