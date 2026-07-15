//! The assignee picker modal: reassigning or unassigning the viewed issue,
//! with a type-to-filter query line above the row list (mirroring
//! `ui/search.rs`'s input row).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, AssigneeRow};

use super::{accent, accent2, centered_rect_h, maple, muted, selected_style};

pub(crate) fn draw_assignee_picker(f: &mut Frame, app: &App, area: Rect) {
    let rows = &app.assignee_picker.rows;
    // Cap how many rows the popup ever tries to show at once — unlike the
    // transition/view pickers (whose lists are inherently small), the
    // assignee list is every assignable project member, which can easily
    // run past what fits on screen. `visible` is a scroll *window*, not the
    // full row count, so the popup stays a reasonable size and the
    // highlighted row is always kept in view (mirroring `list.rs`'s
    // "simple scroll window around the selection").
    let max_popup_height = area.height.saturating_sub(4).max(1);
    // +5 for the border, the query line, its blank separator, and the
    // footer hint.
    let visible = (max_popup_height.saturating_sub(5) as usize).max(1);
    let height = (rows.len().min(visible) as u16)
        .saturating_add(5)
        .min(area.height);
    let popup = centered_rect_h(46, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  assign to…  ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "› ",
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.assignee_picker.query.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled("▏", Style::default().fg(accent())),
    ]));
    lines.push(Line::from(""));

    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matching teammates.",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )));
    }
    // Simple scroll window around the selection, same shape as `list.rs`'s
    // tree view — keeps `selected` on screen even when `rows` is longer
    // than `visible`.
    let start = app
        .assignee_picker
        .selected
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(rows.len().saturating_sub(visible));
    for (i, row) in rows.iter().enumerate().skip(start).take(visible) {
        let selected = i == app.assignee_picker.selected;
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
            AssigneeRow::Unassign => "Unassign".to_string(),
            AssigneeRow::User(u) => u.display_name.clone(),
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
        "⏎ assign · esc cancel",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
