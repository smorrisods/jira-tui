//! The command palette modal (SPEC.md §8): a type-to-filter action list
//! grouped into "on {KEY}", "view", and "app", mirroring
//! `ui/assignee_picker.rs`'s query-line-above-a-row-list shape.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, PaletteGroup, PaletteRow};

use super::{accent, accent2, centered_rect_h, maple, muted, selected_style, truncate};

pub(crate) fn draw_palette(f: &mut Frame, app: &App, area: Rect) {
    let visible: Vec<&PaletteRow> = app
        .palette
        .visible
        .iter()
        .map(|&i| &app.palette.all_rows[i])
        .collect();
    // Count group-header lines that'll actually be rendered: one per run of
    // consecutive same-group rows.
    let group_count = visible
        .iter()
        .map(|r| r.group)
        .fold((None, 0usize), |(prev, count), group| {
            if prev == Some(group) {
                (prev, count)
            } else {
                (Some(group), count + 1)
            }
        })
        .1;

    let max_popup_height = area.height.saturating_sub(4).max(1);
    // +5 for the border, the query line, its blank separator, and the
    // footer hint; +group_count for each group's own header line.
    let body_budget = (max_popup_height.saturating_sub(5) as usize).max(1);
    let height = ((visible.len() + group_count).min(body_budget) as u16)
        .saturating_add(5)
        .min(area.height);
    let popup = centered_rect_h(60, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(
            "  command palette  ",
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
        Span::styled(app.palette.query.clone(), Style::default().fg(Color::White)),
        Span::styled("▏", Style::default().fg(accent())),
    ]));
    lines.push(Line::from(""));

    if visible.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matching actions.",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )));
    }

    let (context_key, _) = app.palette_context();
    let mut current_group = None;
    for (i, row) in visible.iter().enumerate() {
        if current_group != Some(row.group) {
            current_group = Some(row.group);
            lines.push(group_header(row.group, context_key.as_deref()));
        }
        let selected = i == app.palette.selected;
        lines.push(row_line(row, &app.palette.query, selected, inner.width));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎ run · esc close",
        Style::default().fg(muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn group_header(group: PaletteGroup, context_key: Option<&str>) -> Line<'static> {
    let text = match group {
        PaletteGroup::OnKey => format!("on {}", context_key.unwrap_or("issue")),
        PaletteGroup::View => "view".to_string(),
        PaletteGroup::App => "app".to_string(),
    };
    Line::from(Span::styled(
        text.to_uppercase(),
        Style::default().fg(muted()).add_modifier(Modifier::BOLD),
    ))
}

fn row_line(row: &PaletteRow, query: &str, selected: bool, width: u16) -> Line<'static> {
    let cursor = if selected { "▌ " } else { "  " };
    let mut spans = vec![Span::styled(
        cursor.to_string(),
        selected_style(Style::default().fg(maple()), selected),
    )];
    spans.extend(highlight_matches(&row.label, query, selected));

    let label_width = row.label.chars().count() + cursor.chars().count();
    let hint_width = row.hint.chars().count();
    let pad = (width as usize)
        .saturating_sub(label_width)
        .saturating_sub(hint_width)
        .saturating_sub(1);
    if !row.hint.is_empty() {
        spans.push(Span::raw(" ".repeat(pad.max(1))));
        spans.push(Span::styled(row.hint, Style::default().fg(muted())));
    }
    Line::from(spans)
}

/// Splits `label` into spans around the first case-insensitive match of
/// `query`, styling the matched range cyan+bold (SPEC.md §8: "matched
/// chars cyan bold") — a small dedicated helper rather than adapting
/// `render.rs`'s `restyle_range` (built for restyling an existing
/// multi-span `Line`, not building spans from a plain label fresh).
fn highlight_matches(label: &str, query: &str, selected: bool) -> Vec<Span<'static>> {
    let base = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let base = selected_style(base, selected);
    if query.is_empty() {
        return vec![Span::styled(truncate(label, 200), base)];
    }
    let lower_label = label.to_lowercase();
    let lower_query = query.to_lowercase();
    let Some(start) = lower_label.find(&lower_query) else {
        return vec![Span::styled(truncate(label, 200), base)];
    };
    let end = start + lower_query.len();
    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::styled(label[..start].to_string(), base));
    }
    spans.push(Span::styled(
        label[start..end].to_string(),
        Style::default().fg(accent()).add_modifier(Modifier::BOLD),
    ));
    if end < label.len() {
        spans.push(Span::styled(label[end..].to_string(), base));
    }
    spans
}
