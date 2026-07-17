//! The command palette modal (SPEC.md §8): a type-to-filter action list
//! grouped into "on {KEY}", "view", and "app", mirroring
//! `ui/assignee_picker.rs`'s query-line-above-a-row-list shape (including
//! its scroll-window-around-the-selection treatment for a list too tall to
//! fit).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, PaletteGroup, PaletteRow};

use super::{accent, accent2, centered_rect_h, maple, muted, selected_style, truncate};

const POPUP_WIDTH_PCT: u16 = 60;

pub(crate) fn draw_palette(f: &mut Frame, app: &App, area: Rect) {
    let visible: Vec<&PaletteRow> = app
        .palette
        .visible
        .iter()
        .map(|&i| &app.palette.all_rows[i])
        .collect();
    let query_lower = app.palette.query.to_lowercase();
    let inner_width = (area.width * POPUP_WIDTH_PCT / 100).saturating_sub(2);

    // Build every body line (group headers interspersed with rows) before
    // deciding the popup's height, so the scroll window below can work off
    // the real rendered line count rather than just the row count.
    let (context_key, _) = app.palette_context();
    let mut body: Vec<Line> = Vec::new();
    let mut selected_line = 0usize;
    let mut current_group = None;
    for (i, row) in visible.iter().enumerate() {
        if current_group != Some(row.group) {
            current_group = Some(row.group);
            body.push(group_header(row.group, context_key.as_deref()));
        }
        let selected = i == app.palette.selected;
        if selected {
            selected_line = body.len();
        }
        body.push(row_line(row, &query_lower, selected, inner_width));
    }
    if visible.is_empty() {
        body.push(Line::from(Span::styled(
            "No matching actions.",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )));
    }

    let max_popup_height = area.height.saturating_sub(4).max(1);
    // +5 for the border, the query line, its blank separator, and the
    // footer hint.
    let body_budget = (max_popup_height.saturating_sub(5) as usize).max(1);
    let height = (body.len().min(body_budget) as u16)
        .saturating_add(5)
        .min(area.height);
    let popup = centered_rect_h(POPUP_WIDTH_PCT, height, area);
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

    // Scroll window around the selected line — same shape as
    // `assignee_picker.rs`'s "simple scroll window around the selection",
    // generalized from rows to lines since headers add extra ones.
    let start = selected_line
        .saturating_sub(body_budget.saturating_sub(1) / 2)
        .min(body.len().saturating_sub(body_budget.min(body.len())));
    let windowed: Vec<Line> = body.into_iter().skip(start).take(body_budget).collect();

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
    lines.extend(windowed);
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

/// `query_lower` is the palette's filter query, already lowercased once by
/// the caller — hoisted out of the per-row loop since it's identical for
/// every row on a given frame.
fn row_line(row: &PaletteRow, query_lower: &str, selected: bool, width: u16) -> Line<'static> {
    let cursor = if selected { "▌ " } else { "  " };
    let mut spans = vec![Span::styled(
        cursor.to_string(),
        selected_style(Style::default().fg(maple()), selected),
    )];
    spans.extend(highlight_matches(&row.label, query_lower, selected));

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

/// Splits `label` into spans around the first match of the already-
/// lowercased `query_lower`, styling the matched range cyan+bold (SPEC.md
/// §8: "matched chars cyan bold") — a small dedicated helper rather than
/// adapting `render.rs`'s `restyle_range` (built for restyling an existing
/// multi-span `Line`, not building spans from a plain label fresh).
///
/// Works entirely in char-space (never slices by a byte offset found in a
/// *different* string) — `label.to_lowercase()` can both shift byte offsets
/// (multi-byte chars) and, for a handful of codepoints, change the char
/// count itself (e.g. German `ß` → `"ss"`), and row labels can embed a live
/// Jira status name that isn't guaranteed to be ASCII.
fn highlight_matches(label: &str, query_lower: &str, selected: bool) -> Vec<Span<'static>> {
    let base = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let base = selected_style(base, selected);
    let unmatched = || vec![Span::styled(truncate(label, 200), base)];
    if query_lower.is_empty() {
        return unmatched();
    }

    let label_chars: Vec<char> = label.chars().collect();
    let lower_chars: Vec<char> = label.to_lowercase().chars().collect();
    let query_chars: Vec<char> = query_lower.chars().collect();
    if lower_chars.len() != label_chars.len() {
        // Lowercasing changed the char count for this label — bail out to
        // an unhighlighted (but still fully shown) row rather than risk an
        // index into `label_chars` that no longer lines up with `lower_chars`.
        return unmatched();
    }
    let Some(start) = lower_chars
        .windows(query_chars.len().max(1))
        .position(|w| w == query_chars.as_slice())
    else {
        return unmatched();
    };
    let end = start + query_chars.len();

    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::styled(
            label_chars[..start].iter().collect::<String>(),
            base,
        ));
    }
    spans.push(Span::styled(
        label_chars[start..end].iter().collect::<String>(),
        Style::default().fg(accent()).add_modifier(Modifier::BOLD),
    ));
    if end < label_chars.len() {
        spans.push(Span::styled(
            label_chars[end..].iter().collect::<String>(),
            base,
        ));
    }
    spans
}
