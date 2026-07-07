//! The Home dashboard: current Git context, at-a-glance stats, and the
//! "my work" list panel.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

use super::{card, list::draw_list, ACCENT, ACCENT2, DANGER, MUTED, OK, WARN};

pub(crate) fn draw_home(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);

    draw_context_panel(f, app, cols[0]);
    draw_list(f, app, cols[1], false);
}

fn draw_context_panel(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(3)])
        .split(area);

    // Current context card
    let repo = app.git.repo.clone().unwrap_or_else(|| "—".into());
    let branch = app.git.branch.clone().unwrap_or_else(|| "—".into());
    let key = app
        .git
        .issue_key
        .clone()
        .unwrap_or_else(|| "none detected".into());
    let ctx = Text::from(vec![
        Line::from(vec![
            Span::styled("repo    ", Style::default().fg(MUTED)),
            Span::styled(repo, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("branch  ", Style::default().fg(MUTED)),
            Span::styled(branch, Style::default().fg(Color::Blue)),
        ]),
        Line::from(vec![
            Span::styled("issue   ", Style::default().fg(MUTED)),
            Span::styled(key, Style::default().fg(WARN).add_modifier(Modifier::BOLD)),
        ]),
    ]);
    f.render_widget(
        Paragraph::new(ctx).block(card("  current context  ", ACCENT)),
        rows[0],
    );

    // Stats card
    let assigned = app.assigned_to_me().len();
    let blocked = app.blocked().len();
    let total = app.issues.len();
    let stats = Text::from(vec![
        stat_line("assigned to me", assigned, OK),
        stat_line("blocked", blocked, if blocked > 0 { DANGER } else { MUTED }),
        stat_line("in view", total, ACCENT),
        Line::from(""),
        Line::from(Span::styled(
            "next: press ⏎ to open the",
            Style::default().fg(MUTED),
        )),
        Line::from(Span::styled(
            "highlighted issue",
            Style::default().fg(MUTED),
        )),
    ]);
    f.render_widget(
        Paragraph::new(stats).block(card("  at a glance  ", ACCENT2)),
        rows[1],
    );
}

fn stat_line(label: &str, n: usize, colour: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{n:>3} "),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(Color::White)),
    ])
}
