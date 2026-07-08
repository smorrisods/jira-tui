//! Rendering layer: theme, screen dispatch, and shared chrome/helpers.
//!
//! Each screen has its own submodule (`home`, `list`, `detail`, `board`,
//! `search`, `preview`, `editor`, `welcome`, `about`) plus a couple of
//! overlays (`transition_picker`, `help`, `jax_companion`). This file holds
//! the `draw()` dispatcher, the header/footer/toast chrome shared by every
//! screen, the colour theme, and small rendering helpers (`card`, `chip`,
//! `truncate`, priority/status colouring, centred-rect math) that child
//! modules use directly as private items of their parent.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::domain::Priority;

mod about;
mod board;
mod detail;
mod editor;
mod field_mapping;
mod help;
mod home;
mod jax_companion;
mod list;
mod preview;
mod search;
mod transition_picker;
mod welcome;

use about::draw_about;
use board::draw_board;
use detail::draw_detail;
use editor::draw_editor;
use field_mapping::draw_field_mapping;
use help::draw_help_overlay;
use home::draw_home;
use jax_companion::draw_jax_companion;
use list::{draw_list, draw_quick_view};
use preview::draw_preview;
use search::draw_search;
use transition_picker::draw_transition_picker;
use welcome::draw_welcome;

// ── Theme ────────────────────────────────────────────────────────────────────
pub(crate) const ACCENT: Color = Color::Cyan;
pub(crate) const ACCENT2: Color = Color::Magenta;
pub(crate) const MUTED: Color = Color::DarkGray;
const OK: Color = Color::Green;
const WARN: Color = Color::Yellow;
pub(crate) const DANGER: Color = Color::Red;

pub fn draw(f: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),    // body
            Constraint::Length(3), // footer
        ])
        .split(f.area());

    draw_header(f, app, root[0]);

    // The quick-view panel spans the full width beneath Home/List, taking a
    // generous share of the remaining height so fields and the ADF body are
    // both readable at a glance.
    let quick_view_active = app.quick_view
        && matches!(
            app.screen,
            crate::app::Screen::Home | crate::app::Screen::List
        );
    let (body_area, quick_area) = if quick_view_active {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Percentage(50)])
            .split(root[1]);
        (rows[0], Some(rows[1]))
    } else {
        (root[1], None)
    };

    use crate::app::Screen;
    match app.screen {
        Screen::Welcome => draw_welcome(f, app, body_area),
        Screen::Home => draw_home(f, app, body_area),
        Screen::List => draw_list(f, app, body_area, true),
        Screen::Detail => draw_detail(f, app, body_area),
        Screen::Preview => draw_preview(f, app, body_area),
        Screen::Edit => draw_editor(f, app, body_area),
        Screen::Search => draw_search(f, app, body_area),
        Screen::Board => draw_board(f, app, body_area),
        Screen::About => draw_about(f, app, body_area),
        Screen::FieldMapping => draw_field_mapping(f, app, body_area),
    }

    if let Some(qa) = quick_area {
        draw_quick_view(f, app, qa);
    }

    draw_footer(f, app, root[2]);

    // The ambient Jax companion floats above the quick-view panel when it's
    // open (so it never covers it), or at the bottom of the body otherwise.
    if app.show_jax && !matches!(app.screen, Screen::Welcome | Screen::Edit | Screen::About) {
        draw_jax_companion(f, app, body_area);
    }

    if app.picker_open {
        draw_transition_picker(f, app, f.area());
    }

    // Highlight the active drag selection by inverting the covered rows.
    if let Some((y0, y1)) = app.selection_range() {
        let area = f.area();
        let buf = f.buffer_mut();
        for y in y0..=y1.min(area.height.saturating_sub(1)) {
            for x in 0..area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
                }
            }
        }
    }

    if app.show_help {
        draw_help_overlay(f, f.area());
    }

    // A transient toast (e.g. clipboard confirmations) floats above everything.
    if let Some(msg) = app.active_flash() {
        draw_toast(f, msg, f.area());
    }
}

/// A small centred confirmation banner near the top of the screen.
fn draw_toast(f: &mut Frame, msg: &str, area: Rect) {
    let width = (msg.chars().count() as u16 + 4).min(area.width);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + 4;
    let rect = Rect::new(x, y, width, 3);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(OK))
        .style(Style::default().bg(Color::Rgb(20, 40, 20)));
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(OK).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center)
        .block(block),
        rect,
    );
}

// ── Header ───────────────────────────────────────────────────────────────────
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let spinner = ['◐', '◓', '◑', '◒'][(app.tick / 2 % 4) as usize];
    let branch = app.git.branch.clone().unwrap_or_else(|| "no branch".into());
    let ctx_key = app
        .git
        .issue_key
        .clone()
        .map(|k| format!(" ⇢ {k}"))
        .unwrap_or_default();

    let left = Line::from(vec![
        Span::styled(format!(" {spinner} "), Style::default().fg(ACCENT2)),
        Span::styled(
            "jira",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "-tui",
            Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(MUTED)),
        Span::styled(app.source.label(), Style::default().fg(MUTED)),
        Span::styled(
            if app.mouse.enabled {
                "  🖱 mouse"
            } else {
                ""
            },
            Style::default().fg(OK),
        ),
    ]);
    let right = Line::from(vec![
        Span::styled("", Style::default()),
        Span::styled(format!("⎇ {branch}"), Style::default().fg(Color::Blue)),
        Span::styled(
            ctx_key,
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ])
    .alignment(Alignment::Right);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(inner);
    f.render_widget(Paragraph::new(left), cols[0]);
    f.render_widget(Paragraph::new(right), cols[1]);
}

// ── Footer ───────────────────────────────────────────────────────────────────
fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    use crate::app::{EditTarget, Screen};
    let keys: String = match app.screen {
        Screen::Welcome => match app.onboarding.welcome_phase {
            crate::app::WelcomePhase::Intro => {
                "s set up live · d demo · w write config · ? help · q quit".into()
            }
            crate::app::WelcomePhase::Setup => {
                "type to edit · tab next · ⏎ verify & save · esc back".into()
            }
        },
        Screen::Detail => "↑/↓ scroll · t transition · e edit · c comment · ]/[ comments/top · \
             n/p next/prev comment · esc/← back · a about · ? help · q quit"
            .into(),
        Screen::Preview => match app.edit_target {
            EditTarget::Description => "y apply to Jira · esc/← cancel · ↑/↓ scroll".into(),
            EditTarget::Comment => "y post comment · esc/← cancel · ↑/↓ scroll".into(),
        },
        Screen::Edit => match app.edit_target {
            EditTarget::Description => "type to edit · ^S preview · esc cancel".into(),
            EditTarget::Comment => "type your comment · ^S preview · esc cancel".into(),
        },
        Screen::Search => "type to filter · ↑/↓ move · ⏎ open · esc cancel".into(),
        Screen::FieldMapping => "type to search fields · ↑/↓ move · ⏎ map · esc cancel".into(),
        Screen::Board => {
            "↑/↓ card · ←/→ column · pgup/pgdn lane · ⏎ open · / search · esc/q back".into()
        }
        Screen::About => "esc/← back · ? help · q quit".into(),
        Screen::Home | Screen::List if app.quick_view => {
            "↑/↓ move · →/⏎ open · tab focus quick view · c comment · ]/[ comments/top · \
             n/p next/prev comment · b board · / search · ? help · q quit"
                .into()
        }
        _ => "↑/↓ move · →/⏎ open · s sort · f filter · v quick · b board · / search · ? help · q quit".into(),
    };
    let keys = keys.as_str();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(keys, Style::default().fg(MUTED)))),
        cols[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{} ", app.status),
            Style::default().fg(ACCENT),
        )))
        .alignment(Alignment::Right),
        cols[1],
    );
}

// ── Small helpers (visible to all child screen modules) ──────────────────────
fn card(title: &str, colour: Color) -> Block<'static> {
    card_bordered(title, colour, MUTED)
}

fn card_bordered(title: &str, title_colour: Color, border_colour: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_colour))
        .title(Span::styled(
            title.to_string(),
            Style::default()
                .fg(title_colour)
                .add_modifier(Modifier::BOLD),
        ))
}

pub(crate) fn chip(text: &str, colour: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default().fg(Color::Black).bg(colour),
    )
}

pub(crate) fn divider() -> Line<'static> {
    Line::from(Span::styled("─".repeat(52), Style::default().fg(MUTED)))
}

pub(crate) fn priority_glyph(p: &Priority) -> String {
    p.glyph().to_string()
}

pub(crate) fn priority_style(p: &Priority) -> Style {
    Style::default().fg(priority_colour(p))
}

pub(crate) fn priority_colour(p: &Priority) -> Color {
    match p {
        Priority::Highest | Priority::High => DANGER,
        Priority::Medium => WARN,
        Priority::Low | Priority::Lowest => Color::Blue,
    }
}

fn status_short(s: &str) -> String {
    truncate(s, 10)
}

pub(crate) fn status_colour(s: &str) -> Color {
    match s {
        "Done" => OK,
        "In Progress" | "In Review" => ACCENT,
        "To Do" | "Backlog" => MUTED,
        _ => Color::White,
    }
}

fn status_style(s: &str) -> Style {
    Style::default().fg(status_colour(s))
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn centered_rect_h(width_pct: u16, height: u16, area: Rect) -> Rect {
    let y = area.y + area.height.saturating_sub(height) / 2;
    let w = area.width * width_pct / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    Rect::new(x, y, w, height.min(area.height))
}
