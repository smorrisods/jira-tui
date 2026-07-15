//! The swimlane Kanban board: status columns across the top, Epic-grouped
//! lanes down the side, rendered as a scrollable text grid.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::domain::IssueSummary;

use super::{accent, accent2, card, maple, muted, selected_style, status_colour, truncate};

pub(crate) fn draw_board(f: &mut Frame, app: &App, area: Rect) {
    let cols = app.board_columns();
    let lanes = app.board_lanes();

    let title = format!(
        "  board · {} lane{} · {} column{}  ",
        lanes.len(),
        if lanes.len() == 1 { "" } else { "s" },
        cols.len(),
        if cols.len() == 1 { "" } else { "s" },
    );
    let block = card(&title, accent());
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.board_area.set(inner);

    if app.issues.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No issues in the current view.",
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let n = cols.len().max(1);
    let col_width = board_col_width(inner.width as usize, n);

    let mut lines: Vec<Line> = Vec::new();

    // Column header row: status name + total count in that column.
    let mut header: Vec<Span> = Vec::new();
    for (ci, status) in cols.iter().enumerate() {
        let count = app.issues.iter().filter(|i| &i.status == status).count();
        let label = truncate(&format!("{status} ({count})"), col_width);
        header.push(Span::raw(" ")); // aligns with the selection bar column below
        header.push(Span::styled(
            format!("{label:<col_width$}"),
            Style::default()
                .fg(status_colour(status))
                .add_modifier(Modifier::BOLD),
        ));
        if ci + 1 < n {
            header.push(Span::styled(" │ ", Style::default().fg(muted())));
        }
    }
    lines.push(Line::from(header));
    lines.push(Line::from(Span::styled(
        "─".repeat(inner.width as usize),
        Style::default().fg(muted()),
    )));

    for (li, lane) in lanes.iter().enumerate() {
        let cell_lists: Vec<Vec<&IssueSummary>> = cols
            .iter()
            .map(|status| app.board_cell(lane, status))
            .collect();
        let lane_count: usize = cell_lists.iter().map(|c| c.len()).sum();

        lines.push(Line::from(Span::styled(
            format!("▸ {} ({lane_count})", app.board_lane_label(lane)),
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        )));

        let max_rows = cell_lists.iter().map(|c| c.len()).max().unwrap_or(0).max(1);
        for row in 0..max_rows {
            let mut spans: Vec<Span> = Vec::new();
            for (ci, cell) in cell_lists.iter().enumerate() {
                let selected = li == app.board_sel.lane
                    && ci == app.board_sel.col
                    && row == app.board_sel.card;
                let text = match cell.get(row) {
                    Some(issue) => {
                        let head = format!("{}{} ", issue.priority.glyph(), issue.key);
                        let remaining = col_width.saturating_sub(head.chars().count());
                        format!("{head}{}", truncate(&issue.summary, remaining))
                    }
                    None => String::new(),
                };
                let base_fg = if cell.get(row).is_some() {
                    Color::White
                } else {
                    muted()
                };
                let mut style = selected_style(Style::default().fg(base_fg), selected);
                if selected {
                    style = style.add_modifier(Modifier::BOLD);
                }
                // One selection language shared with the list and pickers:
                // a leading maple bar, reserved as its own column so
                // unselected cells stay aligned.
                let bar = if selected { "▌" } else { " " };
                let bar_style = if selected {
                    Style::default().fg(maple())
                } else {
                    Style::default()
                };
                spans.push(Span::styled(bar, bar_style));
                spans.push(Span::styled(format!("{text:<col_width$}"), style));
                if ci + 1 < n {
                    spans.push(Span::styled(" │ ", Style::default().fg(muted())));
                }
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(""));
    }

    let para = Paragraph::new(Text::from(lines)).scroll((app.board_scroll, 0));
    f.render_widget(para, inner);
}

/// Per-column text width, leaving room for the real width of the " │ "
/// separator between columns and of the leading selection-bar column each
/// card/header cell reserves (see `bar` above and the header's leading
/// `Span::raw(" ")`) — both are subtracted from the budget up front so the
/// header and body rows come out to the same total rendered width instead
/// of drifting apart. Pure so it's unit-testable against the width formula
/// callers actually use.
fn board_col_width(inner_width: usize, n: usize) -> usize {
    let n = n.max(1);
    let sep_width = 3 * n.saturating_sub(1);
    let bar_width = n;
    (inner_width.saturating_sub(sep_width + bar_width) / n).max(12)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test: a leading 1-char selection bar used to be prepended
    /// to every body-row cell without a matching reservation in the width
    /// budget, so both header and body rows rendered wider than the pane —
    /// clipped inconsistently since the header (no bar) overflowed less
    /// than the body (with a bar) did, breaking column alignment between
    /// them. Every row now reserves the same 1-char lead-in (the header's
    /// `Span::raw(" ")`, the body's selection bar), so this asserts the
    /// shared total-row-width formula never exceeds the pane width — unless
    /// the terminal is so narrow that `col_width`'s 12-char readability
    /// floor is doing the clamping instead, which (like the pre-existing
    /// floor itself) is an accepted narrow-terminal tradeoff, not a bug.
    #[test]
    fn row_width_never_exceeds_the_pane_when_not_floor_clamped() {
        for n in 1..=6usize {
            for inner_width in [40usize, 80, 110, 200] {
                let sep_width = 3 * n.saturating_sub(1);
                let bar_width = n;
                let unclamped = inner_width.saturating_sub(sep_width + bar_width) / n;
                if unclamped < 12 {
                    continue; // the readability floor is clamping; overflow is expected here
                }
                let col_width = board_col_width(inner_width, n);
                let total_row_width = n * (1 + col_width) + sep_width;
                assert!(
                    total_row_width <= inner_width,
                    "n={n} inner_width={inner_width}: row width {total_row_width} exceeds pane"
                );
            }
        }
    }
}
