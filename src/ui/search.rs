//! The Search / go-to-issue screen: a query input plus filtered results.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, SearchPurpose, SearchRow};
use crate::domain::Source;

use super::list::{flat_guide, issue_row};
use super::list_columns::column_set_for_width;
use super::{
    accent, accent2, card, card_bordered, maple, muted, ok, scroll_center_offset, selected_style,
    warn,
};

pub(crate) fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Local matching always works; the live text-search fallback (see
    // `App::schedule_live_search`) only ever fires for a genuine `Live`
    // source — surfaced here so a `Cache`/`Demo` session (e.g. the live
    // fetch is currently failing and falling back to cache) doesn't look
    // like search is silently broken.
    let live_available = matches!(app.source, Source::Live { .. });

    // Query input line.
    let input_title = match &app.search.purpose {
        SearchPurpose::AddToRelease(version_name) => {
            format!("  add issues to {version_name} — tab select, ⏎ confirm  ",)
        }
        SearchPurpose::GoTo if live_available => "  search / go to issue  ".to_string(),
        SearchPurpose::GoTo => {
            "  search / go to issue — local only, not a live session  ".to_string()
        }
    };
    let input_block = card_bordered(&input_title, accent(), accent());
    let input_inner = input_block.inner(rows[0]);
    f.render_widget(input_block, rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "› ",
                Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.search.query.clone(), Style::default().fg(Color::White)),
            Span::styled("▏", Style::default().fg(accent())),
        ])),
        input_inner,
    );

    // Results. Once a live search has completed at least once, keep its
    // outcome visible in the title (not just a transient status message) —
    // "0 found" for a genuinely empty result reads very differently from a
    // live search that never got dispatched at all.
    let mut results_title = if app.search.live_loading {
        "  results — searching Jira…  ".to_string()
    } else if let Some(live_q) = &app.search.live_query {
        format!(
            "  results — live search for \"{live_q}\": {} found  ",
            app.search.live_results.len()
        )
    } else {
        "  results  ".to_string()
    };
    if !app.search.bulk_selected.is_empty() {
        results_title = format!(
            "{} ({} selected)  ",
            results_title.trim_end(),
            app.search.bulk_selected.len()
        );
    }
    let results_block = card(&results_title, accent2());
    let inner = results_block.inner(rows[1]);
    f.render_widget(results_block, rows[1]);

    if app.search.rows.is_empty() {
        let hint = if app.search.live_loading {
            "Searching Jira for matching issues…"
        } else if live_available {
            "No matches. Type an issue key like DS-123 to jump to it directly."
        } else {
            "No matches beyond your current view. Type an issue key like DS-123 to jump to it \
             directly — live text search needs a connected Jira session (see the sync pill above)."
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                hint,
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    let columns = column_set_for_width(inner.width);
    let guide = flat_guide();
    let mut lines: Vec<Line> = Vec::new();
    // Line index the selected row starts at — a row can span more than one
    // line (`SearchRow::Live`'s "found via live search" annotation, or a
    // narrow-terminal two-line `issue_row`), so this has to be tracked
    // alongside `lines` rather than derived from `app.search.selected`
    // after the fact.
    let mut selected_line = 0usize;
    for (i, row) in app.search.rows.iter().enumerate() {
        let selected = i == app.search.selected;
        let cursor = if selected { "▌" } else { " " };
        let cursor_style = selected_style(
            if selected {
                Style::default().fg(maple())
            } else {
                Style::default()
            },
            selected,
        );
        let row_start = lines.len();
        if selected {
            selected_line = row_start;
        }
        match row {
            SearchRow::Goto(key) => {
                lines.push(Line::from(vec![
                    Span::styled(cursor.to_string(), cursor_style),
                    Span::styled(
                        "↵ go to ",
                        selected_style(
                            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
                            selected,
                        ),
                    ),
                    Span::styled(
                        key.clone(),
                        selected_style(
                            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
                            selected,
                        ),
                    ),
                    Span::styled(
                        "  (fetch directly, even if it's not in your list)",
                        selected_style(Style::default().fg(muted()), selected),
                    ),
                ]));
            }
            SearchRow::Match(idx) => {
                if let Some(issue) = app.all_issues.get(*idx) {
                    lines.extend(issue_row(issue, selected, &guide, &columns));
                }
            }
            SearchRow::Live(idx) => {
                if let Some(issue) = app.search.live_results.get(*idx) {
                    lines.extend(issue_row(issue, selected, &guide, &columns));
                    lines.push(Line::from(Span::styled(
                        "    ↳ found via live search (outside your current view)",
                        selected_style(
                            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
                            selected,
                        ),
                    )));
                }
            }
        }
        // Bulk-add mode: prepend a checkbox onto this row's first line —
        // `app.search_row_key` is the same key resolution
        // `search_toggle_bulk_selected` toggles against, so a row's
        // checkbox always agrees with whether `Tab` would check or uncheck it.
        if app.search.purpose != crate::app::SearchPurpose::GoTo {
            if let Some(first) = lines.get_mut(row_start) {
                let checked = app
                    .search_row_key(row)
                    .is_some_and(|key| app.search.bulk_selected.contains(&key));
                let checkbox = if checked { "✓ " } else { "○ " };
                let mut spans = vec![Span::styled(
                    checkbox,
                    Style::default().fg(if checked { ok() } else { muted() }),
                )];
                spans.extend(std::mem::take(&mut first.spans));
                *first = Line::from(spans);
            }
        }
    }
    // Keep the selected row in view — mirrors `ui::list`'s scroll window,
    // but by line offset (not row index) since a row here can span more
    // than one line.
    let height = inner.height as usize;
    let max_scroll = lines.len().saturating_sub(height);
    let scroll = scroll_center_offset(selected_line, height).min(max_scroll);
    f.render_widget(
        // `Paragraph::scroll` takes a `u16`; a result set large enough to
        // overflow it would otherwise wrap the offset to somewhere
        // arbitrary instead of just clamping, so this clamps explicitly
        // rather than relying on the (already-bounded-by-`max_scroll`)
        // value happening to fit.
        Paragraph::new(Text::from(lines)).scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}
