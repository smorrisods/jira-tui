//! The Home dashboard (SPEC.md §5): current Git context, at-a-glance stats
//! (with proportion bars), recently opened issues, and the shared "my work"
//! list panel.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::home_columns::{bar_fill, home_layout_for_width, HomeLayout};
use super::{accent, accent2, card, chip, danger, list::draw_list, muted, ok, status_colour, warn};

/// Cells per glance-tile proportion bar.
const BAR_CELLS: u16 = 4;
/// SPEC.md §11: "height < ~30 rows: recent line hides; glance tiles drop to
/// 2" — a shorter terminal has less room to spare for the rail, so the
/// least essential pieces (recent, the newer half of the glance stats) give
/// their space back first.
pub(crate) const SHORT_HEIGHT: u16 = 30;

pub(crate) fn draw_home(f: &mut Frame, app: &App, area: Rect) {
    match home_layout_for_width(area.width) {
        HomeLayout::Wide => draw_wide(f, app, area),
        HomeLayout::Narrow => draw_narrow(f, app, area),
    }
}

fn draw_wide(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);

    draw_rail_wide(f, app, cols[0]);
    draw_list(f, app, cols[1], false);
}

fn draw_rail_wide(f: &mut Frame, app: &App, area: Rect) {
    let short = area.height < SHORT_HEIGHT;
    let recent = home_recent(app);
    let show_recent = !short && !recent.is_empty();
    let glance_rows = if short { 5 } else { 7 };

    let mut constraints = vec![Constraint::Length(5), Constraint::Length(glance_rows + 2)];
    if show_recent {
        constraints.push(Constraint::Length(recent.len() as u16 + 2));
    }
    constraints.push(Constraint::Min(0));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    draw_context_card(f, app, rows[0]);
    draw_glance_card(f, app, rows[1], short);
    if show_recent {
        draw_recent_card(f, &recent, rows[2]);
    }
}

fn draw_narrow(f: &mut Frame, app: &App, area: Rect) {
    let short = area.height < SHORT_HEIGHT;
    let recent = home_recent(app);
    let show_recent = !short && !recent.is_empty();

    // Context and glance each get a bordered "short panel"/"tile" per
    // SPEC.md §5, so +2 rows apiece over their one/three lines of content
    // for the top/bottom border.
    let mut constraints = vec![Constraint::Length(3), Constraint::Length(5)];
    if show_recent {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(3));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let context_block = card("  context  ", accent()).padding(Padding::left(1));
    let context_inner = context_block.inner(rows[0]);
    f.render_widget(context_block, rows[0]);
    f.render_widget(Paragraph::new(home_context_strip_line(app)), context_inner);
    draw_glance_tiles(f, app, rows[1], short);
    let list_area = if show_recent {
        f.render_widget(Paragraph::new(home_recent_strip_line(&recent)), rows[2]);
        rows[3]
    } else {
        rows[2]
    };
    draw_list(f, app, list_area, false);
}

// ── Shared data helpers ─────────────────────────────────────────────────────

/// The Jira status of the branch-linked issue (SPEC.md §5's context-card
/// status chip), looked up from the already-loaded issue list rather than
/// fetched fresh — omitted entirely if the key isn't currently loaded.
fn linked_issue_status(app: &App) -> Option<&str> {
    let key = app.git.issue_key.as_deref()?;
    app.all_issues
        .iter()
        .find(|i| i.key == key)
        .map(|i| i.status.as_str())
}

/// The at-a-glance counts (SPEC.md §5): assigned/blocked always shown;
/// in-review/done-this-week drop under a short terminal (SPEC.md §11).
fn glance_stats(app: &App, short: bool) -> Vec<(&'static str, usize, Color)> {
    let mut stats = vec![
        ("assigned", app.assigned_to_me().len(), ok()),
        ("blocked", app.blocked().len(), danger()),
    ];
    if !short {
        stats.push(("in review", app.in_review().len(), accent()));
        stats.push(("done this week", app.done_this_week().len(), muted()));
    }
    stats
}

/// Up to 3 recently-opened issues, newest first, with their summary looked
/// up from the currently loaded list (dropped if not found rather than
/// showing a bare key with no context).
fn home_recent(app: &App) -> Vec<(&str, &str)> {
    app.recent
        .iter()
        .filter_map(|key| {
            app.all_issues
                .iter()
                .find(|i| &i.key == key)
                .map(|i| (i.key.as_str(), i.summary.as_str()))
        })
        .collect()
}

// ── Wide layout ──────────────────────────────────────────────────────────────

fn draw_context_card(f: &mut Frame, app: &App, area: Rect) {
    let repo = app.git.repo.clone().unwrap_or_else(|| "—".into());
    let branch = app.git.branch.clone().unwrap_or_else(|| "—".into());
    let key = app
        .git
        .issue_key
        .clone()
        .unwrap_or_else(|| "none detected".into());
    let mut issue_line = vec![
        Span::styled("issue   ", Style::default().fg(muted())),
        Span::styled(
            key,
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(status) = linked_issue_status(app) {
        issue_line.push(Span::raw(" "));
        issue_line.push(chip(status, status_colour(status)));
    }
    let ctx = Text::from(vec![
        Line::from(vec![
            Span::styled("repo    ", Style::default().fg(muted())),
            Span::styled(repo, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("branch  ", Style::default().fg(muted())),
            Span::styled(branch, Style::default().fg(Color::Blue)),
        ]),
        Line::from(issue_line),
    ]);
    f.render_widget(
        Paragraph::new(ctx).block(card("  current context  ", accent())),
        area,
    );
}

fn draw_glance_card(f: &mut Frame, app: &App, area: Rect, short: bool) {
    let stats = glance_stats(app, short);
    let max = stats.iter().map(|(_, n, _)| *n).max().unwrap_or(0);
    let mut lines: Vec<Line> = stats
        .iter()
        .map(|(label, n, colour)| glance_stat_line(label, *n, max, *colour))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "next: press ⏎ to open the",
        Style::default().fg(muted()),
    )));
    lines.push(Line::from(Span::styled(
        "highlighted issue",
        Style::default().fg(muted()),
    )));
    f.render_widget(
        Paragraph::new(Text::from(lines)).block(card("  at a glance  ", accent2())),
        area,
    );
}

fn glance_stat_line(label: &str, n: usize, max: usize, colour: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{n:>3} "),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{label:<15}"), Style::default().fg(Color::White)),
        Span::styled(bar_text(n, max), Style::default().fg(colour)),
    ])
}

fn draw_recent_card(f: &mut Frame, recent: &[(&str, &str)], area: Rect) {
    let lines: Vec<Line> = recent
        .iter()
        .map(|(key, summary)| {
            Line::from(vec![
                Span::styled(
                    format!("{key} "),
                    Style::default().fg(accent()).add_modifier(Modifier::BOLD),
                ),
                Span::styled(summary.to_string(), Style::default().fg(muted())),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(Text::from(lines)).block(card("  recent  ", muted())),
        area,
    );
}

// ── Narrow layout ────────────────────────────────────────────────────────────

fn home_context_strip_line(app: &App) -> Line<'static> {
    let mut parts = Vec::new();
    if let Some(repo) = &app.git.repo {
        parts.push(Span::styled(
            repo.clone(),
            Style::default().fg(Color::White),
        ));
    }
    if let Some(branch) = &app.git.branch {
        if !parts.is_empty() {
            parts.push(Span::styled(" · ", Style::default().fg(muted())));
        }
        parts.push(Span::styled(
            branch.clone(),
            Style::default().fg(Color::Blue),
        ));
    }
    if let Some(key) = &app.git.issue_key {
        if !parts.is_empty() {
            parts.push(Span::styled(" · ", Style::default().fg(muted())));
        }
        parts.push(Span::styled(
            key.clone(),
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        ));
        if let Some(status) = linked_issue_status(app) {
            parts.push(Span::raw(" "));
            parts.push(chip(status, status_colour(status)));
        }
    }
    if parts.is_empty() {
        parts.push(Span::styled(
            "no git context detected",
            Style::default().fg(muted()),
        ));
    }
    Line::from(parts)
}

fn draw_glance_tiles(f: &mut Frame, app: &App, area: Rect, short: bool) {
    let stats = glance_stats(app, short);
    let max = stats.iter().map(|(_, n, _)| *n).max().unwrap_or(0);
    // A 1-col gap between tiles, alongside the equal-width tiles themselves —
    // ratatui's solver satisfies the fixed gaps first, then splits whatever
    // is left evenly across the Ratio segments.
    let mut constraints = Vec::with_capacity(stats.len() * 2);
    for i in 0..stats.len() {
        if i > 0 {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Ratio(1, stats.len() as u32));
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    for (col, (label, n, colour)) in cols.iter().step_by(2).zip(stats.iter()) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(muted()))
            .padding(Padding::left(1));
        let inner = block.inner(*col);
        f.render_widget(block, *col);
        let tile = Text::from(vec![
            Line::from(Span::styled(*label, Style::default().fg(muted()))),
            Line::from(Span::styled(
                n.to_string(),
                Style::default().fg(*colour).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                bar_text(*n, max),
                Style::default().fg(*colour),
            )),
        ]);
        f.render_widget(Paragraph::new(tile), inner);
    }
}

fn home_recent_strip_line(recent: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::styled("recent: ", Style::default().fg(muted()))];
    for (i, (key, _)) in recent.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(muted())));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            Style::default().fg(accent()),
        ));
    }
    Line::from(spans)
}

// ── Shared rendering ─────────────────────────────────────────────────────────

fn bar_text(n: usize, max: usize) -> String {
    let fill = bar_fill(n, max, BAR_CELLS) as usize;
    format!(
        " {}{}",
        "█".repeat(fill),
        "░".repeat((BAR_CELLS as usize).saturating_sub(fill))
    )
}
