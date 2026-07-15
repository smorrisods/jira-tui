//! Rendering layer: theme, screen dispatch, and shared chrome/helpers.
//!
//! Each screen has its own submodule (`home`, `list`, `detail`, `board`,
//! `search`, `preview`, `editor`, `welcome`, `about`) plus a couple of
//! overlays (`transition_picker`, `help`, `jax_companion`). This file holds
//! the `draw()` dispatcher, the header/footer/toast chrome shared by every
//! screen, the colour theme, and small rendering helpers (`card`, `chip`,
//! `truncate`, priority/status colouring, centred-rect math) that child
//! modules use directly as private items of their parent.

use std::sync::OnceLock;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::domain::Priority;

mod about;
mod assignee_picker;
mod board;
mod detail;
mod editor;
mod field_mapping;
mod help;
mod home;
mod jax_companion;
mod keymap;
mod list;
mod preview;
mod search;
mod transition_picker;
mod view_picker;
mod welcome;

use about::draw_about;
use assignee_picker::draw_assignee_picker;
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
use view_picker::draw_view_picker;
use welcome::draw_welcome;

// ── Theme ────────────────────────────────────────────────────────────────────
//
// Terminal palettes are indexed colours in practice; the hex values below are
// truecolor targets, each with a named-colour fallback for terminals that
// can't do 24-bit colour. Detection defaults to truecolor-capable — most
// modern terminals (including transparent-background ones like kitty/
// alacritty/wezterm/iTerm2) support it even when `COLORTERM` is stripped by
// tmux/ssh — and only opts out for terminals that explicitly say they can't.

/// Pure so it's unit-testable without touching real process env vars or
/// fighting the memoized `OnceLock` below (which, being process-global, can
/// only be initialized once per test binary).
fn detect_truecolor(colorterm: Option<&str>, term: Option<&str>) -> bool {
    if matches!(colorterm, Some("truecolor") | Some("24bit")) {
        return true;
    }
    // The Linux virtual console and `TERM=dumb` are the only terminals that
    // reliably can't do 24-bit colour; everything else is assumed capable.
    !matches!(term, Some("dumb") | Some("linux"))
}

static TRUECOLOR: OnceLock<bool> = OnceLock::new();

fn truecolor_supported() -> bool {
    *TRUECOLOR.get_or_init(|| {
        detect_truecolor(
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        )
    })
}

fn theme_colour(rgb: (u8, u8, u8), fallback: Color) -> Color {
    if truecolor_supported() {
        Color::Rgb(rgb.0, rgb.1, rgb.2)
    } else {
        fallback
    }
}

pub(crate) fn accent() -> Color {
    theme_colour((0x62, 0xD8, 0xD3), Color::Cyan)
}
pub(crate) fn accent2() -> Color {
    theme_colour((0xC7, 0x9B, 0xF0), Color::Magenta)
}
/// Selection bar/tint, focus, and Jax moments — nothing else.
pub(crate) fn maple() -> Color {
    theme_colour((0xE8, 0x83, 0x4A), Color::LightRed)
}
fn ok() -> Color {
    theme_colour((0x8F, 0xCB, 0x7A), Color::Green)
}
fn warn() -> Color {
    theme_colour((0xE3, 0xB5, 0x64), Color::Yellow)
}
pub(crate) fn danger() -> Color {
    theme_colour((0xE5, 0x71, 0x6B), Color::Red)
}
pub(crate) fn muted() -> Color {
    theme_colour((0x77, 0x83, 0x8F), Color::DarkGray)
}
// FAINT (tertiary text, tree guides, group labels) isn't added yet — nothing
// in this phase uses it (tree guides are phase 4, footer group labels are
// phase 2). Add it alongside whichever phase first needs it.
/// The Task type chip only — not part of the 8-token palette table.
fn task_blue() -> Color {
    theme_colour((0x6F, 0xB3, 0xE0), Color::Blue)
}

/// Alpha-blend an RGB theme colour toward the panel background (assumed
/// black — no panel in this app sets an explicit background). Named ANSI
/// colours pass through unchanged: terminals without truecolor have no alpha
/// compositing available, so blending isn't meaningful there.
fn blend(fg: Color, alpha: f32) -> Color {
    match fg {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * alpha) as u8,
            (g as f32 * alpha) as u8,
            (b as f32 * alpha) as u8,
        ),
        other => other,
    }
}

/// One selection language shared by the list, board, and pickers: a maple
/// bar/border plus this low-alpha background tint on the selected row/card.
pub(crate) fn selection_bg() -> Color {
    blend(maple(), 0.20)
}

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

    if app.view_picker_open {
        draw_view_picker(f, app, f.area());
    }

    if app.assignee_picker_open {
        draw_assignee_picker(f, app, f.area());
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
        .border_style(Style::default().fg(ok()))
        .style(Style::default().bg(Color::Rgb(20, 40, 20)));
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(ok()).add_modifier(Modifier::BOLD),
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
        Span::styled(format!(" {spinner} "), Style::default().fg(accent2())),
        Span::styled(
            "jira",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "-tui",
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(muted())),
        Span::styled(app.source.label(), Style::default().fg(muted())),
        Span::styled(
            if app.current_view != crate::domain::ViewKind::MyWork {
                format!("  ·  viewing: {}", app.current_view.label())
            } else {
                String::new()
            },
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if app.mouse.enabled {
                "  🖱 mouse"
            } else {
                ""
            },
            Style::default().fg(ok()),
        ),
    ]);
    let right = Line::from(vec![
        Span::styled("", Style::default()),
        Span::styled(format!("⎇ {branch}"), Style::default().fg(Color::Blue)),
        Span::styled(
            ctx_key,
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ])
    .alignment(Alignment::Right);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(muted()));
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
    use crate::app::{EditTarget, ListFocus, Screen};
    let keys: String = match app.screen {
        Screen::Welcome => match app.onboarding.welcome_phase {
            crate::app::WelcomePhase::Intro => {
                "s set up live · d demo · w write config · ? help · q quit".into()
            }
            crate::app::WelcomePhase::Setup => {
                "type to edit · tab next · ⏎ verify & save · esc back".into()
            }
        },
        Screen::Detail => {
            let history = match (app.can_go_back(), app.can_go_forward()) {
                (true, true) => " · ←/→ history back/forward",
                (true, false) => " · ← history back",
                (false, true) => " · → history forward",
                (false, false) => "",
            };
            format!(
                "↑/↓ scroll · t transition · A assign · e edit · c comment · ]/[ comments/top · \
                 n/p next/prev comment · {{/}} cycle links · ⏎ open link · r refresh · esc{history} back · a about · ? help · q quit"
            )
        }
        Screen::Preview => match app.edit_target {
            EditTarget::Description => "y/⏎ apply to Jira · esc/← cancel · ↑/↓ scroll".into(),
            EditTarget::Comment => "y/⏎ post comment · esc/← cancel · ↑/↓ scroll".into(),
        },
        Screen::Edit => match app.edit_target {
            EditTarget::Description => "type to edit · ^S preview · esc cancel".into(),
            EditTarget::Comment => "type your comment · ^S preview · esc cancel".into(),
        },
        Screen::Search => "type to filter · ↑/↓ move · ⏎ open · esc cancel".into(),
        Screen::FieldMapping => "type to search fields · ↑/↓ move · ⏎ map · esc cancel".into(),
        Screen::Board => {
            "↑/↓ card · ←/→ column · pgup/pgdn lane · ⏎ open · / search · V switch view · esc/q back".into()
        }
        Screen::About => "esc/← back · ? help · q quit".into(),
        Screen::Home | Screen::List if app.quick_view => {
            let refresh = if app.list_focus == ListFocus::QuickView {
                "r refresh focused issue"
            } else {
                "r refresh list"
            };
            format!(
                "↑/↓ move · →/⏎ open · tab focus quick view · A assign · c comment · ]/[ comments/top · \
                 n/p next/prev comment · {{/}} cycle links · ⏎ open link (focused) · {refresh} · \
                 b board · / search · V switch view · ? help · q quit"
            )
        }
        _ => "↑/↓ move · →/⏎ open · s sort · f filter · v quick · T tree view · r refresh · b board · / search · V switch view · ? help · q quit".into(),
    };
    let keys = keys.as_str();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(muted()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(keys, Style::default().fg(muted())))),
        cols[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{}{} ", loading_prefix(app), app.status),
            Style::default().fg(accent()),
        )))
        .alignment(Alignment::Right),
        cols[1],
    );
}

/// A braille spinner frame while a background fetch (`refresh`/
/// `switch_view` against live Jira) is in flight, empty otherwise — see
/// `app::async_ops`.
fn loading_prefix(app: &App) -> String {
    if !app.loading {
        return String::new();
    }
    const FRAMES: [char; 8] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧'];
    let frame = FRAMES[(app.tick as usize) % FRAMES.len()];
    format!("{frame} ")
}

// ── Small helpers (visible to all child screen modules) ──────────────────────
fn card(title: &str, colour: Color) -> Block<'static> {
    card_bordered(title, colour, muted())
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

/// Coloured text on a tinted background (the colour at ~14% alpha over the
/// panel background) on truecolor terminals; a solid block on named-colour
/// terminals, where alpha compositing isn't available.
pub(crate) fn chip(text: &str, colour: Color) -> Span<'static> {
    let style = match colour {
        Color::Rgb(..) => Style::default().fg(colour).bg(blend(colour, 0.14)),
        _ => Style::default().fg(Color::Black).bg(colour),
    };
    Span::styled(format!(" {text} "), style)
}

pub(crate) fn divider() -> Line<'static> {
    Line::from(Span::styled("─".repeat(52), Style::default().fg(muted())))
}

pub(crate) fn priority_glyph(p: &Priority) -> String {
    p.glyph().to_string()
}

pub(crate) fn priority_style(p: &Priority) -> Style {
    Style::default().fg(priority_colour(p))
}

pub(crate) fn priority_colour(p: &Priority) -> Color {
    match p {
        Priority::Highest | Priority::High => danger(),
        Priority::Medium => warn(),
        Priority::Low | Priority::Lowest => Color::Blue,
    }
}

fn status_short(s: &str) -> String {
    truncate(s, 10)
}

pub(crate) fn status_colour(s: &str) -> Color {
    match s {
        "Done" => ok(),
        "In Progress" => accent(),
        "In Review" => accent2(),
        "To Do" | "Backlog" => muted(),
        _ => Color::White,
    }
}

/// Type chip colour (SPEC.md §1): Bug/Story/Task/Epic each get a distinct
/// colour; anything else falls back to the previous uniform `accent2()`.
pub(crate) fn type_colour(issue_type: &str) -> Color {
    match issue_type {
        "Bug" => danger(),
        "Story" => ok(),
        "Task" => task_blue(),
        _ => accent2(),
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

fn centered_rect_h(width_pct: u16, height: u16, area: Rect) -> Rect {
    let y = area.y + area.height.saturating_sub(height) / 2;
    let w = area.width * width_pct / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    Rect::new(x, y, w, height.min(area.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_truecolor_trusts_an_explicit_colorterm() {
        assert!(detect_truecolor(Some("truecolor"), Some("dumb")));
        assert!(detect_truecolor(Some("24bit"), None));
    }

    #[test]
    fn detect_truecolor_opts_out_only_for_known_limited_terminals() {
        assert!(!detect_truecolor(None, Some("dumb")));
        assert!(!detect_truecolor(None, Some("linux")));
    }

    /// The user asked for transparent-terminal setups (often truecolor-
    /// capable but frequently missing `COLORTERM` under tmux/ssh) to get the
    /// rich theme by default — including the common CI/dev case where
    /// neither env var is set at all.
    #[test]
    fn detect_truecolor_defaults_on_for_everything_else() {
        assert!(detect_truecolor(None, None));
        assert!(detect_truecolor(None, Some("xterm-256color")));
        assert!(detect_truecolor(Some("unknown"), Some("screen")));
    }

    #[test]
    fn blend_scales_rgb_but_passes_named_colours_through() {
        assert_eq!(
            blend(Color::Rgb(100, 200, 40), 0.5),
            Color::Rgb(50, 100, 20)
        );
        assert_eq!(blend(Color::Cyan, 0.5), Color::Cyan);
    }

    #[test]
    fn chip_uses_a_tinted_background_for_rgb_but_solid_for_named_colours() {
        let rgb_chip = chip("x", Color::Rgb(200, 100, 50));
        assert_eq!(rgb_chip.style.fg, Some(Color::Rgb(200, 100, 50)));
        assert_eq!(
            rgb_chip.style.bg,
            Some(blend(Color::Rgb(200, 100, 50), 0.14))
        );

        let named_chip = chip("x", Color::Cyan);
        assert_eq!(named_chip.style.fg, Some(Color::Black));
        assert_eq!(named_chip.style.bg, Some(Color::Cyan));
    }

    #[test]
    fn status_colour_distinguishes_in_review_from_in_progress() {
        assert_ne!(status_colour("In Review"), status_colour("In Progress"));
        assert_eq!(status_colour("In Review"), accent2());
        assert_eq!(status_colour("In Progress"), accent());
    }

    #[test]
    fn type_colour_covers_each_known_type() {
        assert_eq!(type_colour("Bug"), danger());
        assert_eq!(type_colour("Story"), ok());
        assert_eq!(type_colour("Task"), task_blue());
        assert_eq!(type_colour("Epic"), accent2());
        assert_eq!(type_colour("Something else"), accent2());
    }
}
