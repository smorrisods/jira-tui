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
use super::{accent, accent2, card, maple, muted, ok, selected_style};

pub(crate) fn draw_release(f: &mut Frame, app: &App, area: Rect) {
    match &app.release.drilled {
        Some(version) => draw_drill(f, app, area, version),
        None => draw_version_list(f, app, area),
    }
}

fn draw_version_list(f: &mut Frame, app: &App, area: Rect) {
    let block = card("  releases  ", accent());
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

    let mut lines: Vec<Line> = Vec::new();
    for (i, version) in app.release.versions.iter().enumerate() {
        let selected = i == app.release.cursor;
        let cursor = if selected { "▌ " } else { "  " };
        let (status, status_colour) = if version.released {
            ("released", ok())
        } else {
            ("unreleased", accent2())
        };
        let date = version.release_date.as_deref().unwrap_or("no target date");
        lines.push(Line::from(vec![
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
            Span::styled(
                format!(" {status} "),
                selected_style(Style::default().fg(status_colour), selected),
            ),
            Span::raw("  "),
            Span::styled(
                date.to_string(),
                selected_style(Style::default().fg(muted()), selected),
            ),
        ]));
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
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(bar, Style::default().fg(ok())),
            Span::styled(
                format!("  {done}/{total} done ({pct}%)"),
                Style::default().fg(muted()),
            ),
        ])),
        rows[0],
    );

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
            let selected = idx == app.release.issue_cursor;
            lines.extend(issue_row(issue, selected, &guide, &columns));
            idx += 1;
        }
    }
    f.render_widget(Paragraph::new(Text::from(lines)), rows[1]);
}
