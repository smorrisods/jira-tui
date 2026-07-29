//! The release review screen: a list of the project's versions, drilling
//! into one to see its issues grouped by status with a done/total progress
//! line. Reuses `list.rs`'s `issue_row`/`flat_guide` for the drill-down
//! (the same rendering the Search screen's results reuse), and a plain
//! highlighted-row list for the version list itself.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::domain::Version;

use super::list::{flat_guide, issue_row};
use super::list_columns::column_set_for_width;
use super::{accent, accent2, card, chip, maple, muted, ok, selected_style, warn};

/// Whether `version` is unreleased with a target date already in the past
/// — a plain string comparison is safe since Jira's `releaseDate` (and our
/// own `today`) are both zero-padded ISO 8601 (`YYYY-MM-DD`). A pure
/// function taking "today" as a parameter (rather than reading the clock
/// itself) purely so it stays trivially unit-testable.
fn is_overdue(version: &Version, today: &str) -> bool {
    !version.released && version.release_date.as_deref().is_some_and(|d| d < today)
}

pub(crate) fn draw_release(f: &mut Frame, app: &App, area: Rect) {
    match &app.release.drilled {
        Some(version) => draw_drill(f, app, area, version),
        None => draw_version_list(f, app, area),
    }
}

fn draw_version_list(f: &mut Frame, app: &App, area: Rect) {
    let mode_hint = match app.release.list_mode {
        crate::app::ReleaseListMode::Split => "split unreleased/released",
        crate::app::ReleaseListMode::Flat => "flat",
    };
    let title = format!("  releases · {mode_hint} (s to cycle)  ");
    let block = card(&title, accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.release.versions.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No versions defined on this project.",
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    // One "today" per render pass (not re-read per row) — Jira's
    // `releaseDate` and this are both zero-padded ISO 8601, so a plain
    // string comparison in `is_overdue` is safe.
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let mut lines: Vec<Line> = Vec::new();
    let mut idx = 0usize;
    for (label, versions) in app.release_version_groups() {
        if let Some(label) = label {
            lines.push(Line::from(Span::styled(
                format!("── {label} ({}) ──", versions.len()),
                Style::default().fg(muted()).add_modifier(Modifier::BOLD),
            )));
        }
        for version in versions {
            let selected = idx == app.release.cursor;
            let cursor = if selected { "▌ " } else { "  " };
            let (status_text, status_colour) = if version.released {
                ("released", ok())
            } else {
                ("unreleased", accent2())
            };
            let date = version.release_date.as_deref().unwrap_or("no target date");
            let mut spans = vec![
                Span::styled(
                    cursor.to_string(),
                    selected_style(Style::default().fg(maple()), selected),
                ),
                Span::styled(
                    version.name.clone(),
                    selected_style(
                        Style::default().fg(accent()).add_modifier(Modifier::BOLD),
                        selected,
                    ),
                ),
                Span::raw("  "),
                chip(status_text, status_colour),
                Span::raw("  "),
                Span::styled(
                    date.to_string(),
                    selected_style(Style::default().fg(muted()), selected),
                ),
            ];
            if is_overdue(version, &today) {
                spans.push(Span::raw("  "));
                spans.push(chip("overdue", warn()));
            }
            lines.push(Line::from(spans));
            idx += 1;
        }
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_drill(f: &mut Frame, app: &App, area: Rect, version: &Version) {
    let title = format!("  release · {}  ", version.name);
    let block = card(&title, accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(3)])
        .split(inner);

    let (done, total) = app.release_progress();
    let pct = (done * 100).checked_div(total).unwrap_or(0);
    const BAR_WIDTH: usize = 20;
    let filled = (BAR_WIDTH * done).checked_div(total).unwrap_or(0);
    let bar: String = "█".repeat(filled) + &"░".repeat(BAR_WIDTH.saturating_sub(filled));
    let mut progress_spans = vec![
        Span::styled(bar, Style::default().fg(ok())),
        Span::styled(
            format!("  {done}/{total} done ({pct}%)"),
            Style::default().fg(muted()),
        ),
    ];
    if !app.release.selected.is_empty() {
        progress_spans.push(Span::styled(
            format!("  ·  {} selected", app.release.selected.len()),
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(progress_spans)), rows[0]);

    if app.release.issues_loading {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "loading issues…",
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            ))),
            rows[1],
        );
        return;
    }
    if app.release.issues.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No issues target this release.",
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            ))),
            rows[1],
        );
        return;
    }

    let columns = column_set_for_width(rows[1].width);
    let guide = flat_guide();
    let mut lines: Vec<Line> = Vec::new();
    let mut idx = 0usize;
    for (status, issues) in app.release_status_groups() {
        lines.push(Line::from(Span::styled(
            format!("── {status} ({}) ──", issues.len()),
            Style::default().fg(muted()).add_modifier(Modifier::BOLD),
        )));
        for issue in issues {
            let row_selected = idx == app.release.issue_cursor;
            let checked = app.release.selected.contains(&issue.key);
            let mut row_lines = issue_row(issue, row_selected, &guide, &columns);
            if let Some(first) = row_lines.first_mut() {
                let checkbox = if checked { "✓ " } else { "  " };
                let mut spans = vec![Span::styled(
                    checkbox,
                    Style::default().fg(if checked { ok() } else { muted() }),
                )];
                spans.extend(std::mem::take(&mut first.spans));
                *first = Line::from(spans);
            }
            lines.extend(row_lines);
            idx += 1;
        }
    }
    f.render_widget(Paragraph::new(Text::from(lines)), rows[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(name: &str, released: bool, release_date: Option<&str>) -> Version {
        Version {
            id: "1".into(),
            name: name.into(),
            released,
            release_date: release_date.map(String::from),
        }
    }

    #[test]
    fn is_overdue_only_for_unreleased_versions_past_their_target_date() {
        assert!(is_overdue(
            &version("v1", false, Some("2026-01-01")),
            "2026-06-01"
        ));
        assert!(
            !is_overdue(&version("v1", true, Some("2026-01-01")), "2026-06-01"),
            "an already-released version is never overdue, regardless of date"
        );
        assert!(
            !is_overdue(&version("v1", false, Some("2026-12-31")), "2026-06-01"),
            "a target date still in the future is not overdue"
        );
        assert!(
            !is_overdue(&version("v1", false, None), "2026-06-01"),
            "no target date at all means nothing to be overdue against"
        );
    }
}
