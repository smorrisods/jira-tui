//! The Home dashboard: current Git context, at-a-glance stats, and the
//! "my work" list panel.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

use super::{accent, accent2, card, danger, list::draw_list, muted, ok, warn};

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
            Span::styled("repo    ", Style::default().fg(muted())),
            Span::styled(repo, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("branch  ", Style::default().fg(muted())),
            Span::styled(branch, Style::default().fg(Color::Blue)),
        ]),
        Line::from(vec![
            Span::styled("issue   ", Style::default().fg(muted())),
            Span::styled(
                key,
                Style::default().fg(warn()).add_modifier(Modifier::BOLD),
            ),
        ]),
    ]);
    f.render_widget(
        Paragraph::new(ctx).block(card("  current context  ", accent())),
        rows[0],
    );

    // Stats card
    let assigned = app.assigned_to_me().len();
    let blocked = app.blocked().len();
    let total = app.issues.len();
    let stats = Text::from(vec![
        stat_line("assigned to me", assigned, ok()),
        stat_line(
            "blocked",
            blocked,
            if blocked > 0 { danger() } else { muted() },
        ),
        stat_line("in view", total, accent()),
        Line::from(""),
        Line::from(Span::styled(
            "next: press ⏎ to open the",
            Style::default().fg(muted()),
        )),
        Line::from(Span::styled(
            "highlighted issue",
            Style::default().fg(muted()),
        )),
    ]);
    f.render_widget(
        Paragraph::new(stats).block(card("  at a glance  ", accent2())),
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
