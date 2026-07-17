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
pub(crate) mod board_columns;
mod detail;
pub(crate) mod detail_columns;
mod editor;
mod field_mapping;
mod footer;
mod header;
mod help;
mod home;
pub(crate) mod home_columns;
mod jax_companion;
mod keymap;
mod list;
mod list_columns;
mod palette;
mod preview;
mod quick_view;
pub(crate) mod quick_view_columns;
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
use footer::footer_line;
use header::draw_header;
use help::draw_help_overlay;
use home::draw_home;
use jax_companion::{draw_jax_companion, draw_jax_mini, JaxMode, MINI_DOCK_WIDTH};
use list::draw_list;
use palette::draw_palette;
use preview::draw_preview;
use quick_view::draw_quick_view;
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
pub(crate) fn ok() -> Color {
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
/// Tertiary text, tree guides, and column-header labels.
pub(crate) fn faint() -> Color {
    theme_colour((0x4A, 0x57, 0x63), Color::DarkGray)
}
/// Not part of the 8-token palette table — shared by the Task type chip and
/// `priority_colour`'s Low/Lowest arm, the two places that want "a blue"
/// without it being a named theme concept of its own.
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

/// Pure so the truecolor-vs-fallback branch is unit-testable without
/// depending on `maple()`'s env-gated, memoized result.
fn selection_bg_for(maple: Color) -> Color {
    match maple {
        rgb @ Color::Rgb(..) => blend(rgb, 0.20),
        _ => Color::DarkGray,
    }
}

/// One selection language shared by the list, board, and pickers: a maple
/// bar/border plus this low-alpha background tint on the selected row/card.
/// On a truecolor terminal that's a genuine alpha blend; on a named-colour
/// terminal `blend()` can't scale `maple()`'s fallback down, and painting a
/// full-brightness accent colour behind the row's own text would be a loud,
/// low-contrast mess rather than a subtle tint — so the fallback is a plain
/// muted grey instead.
pub(crate) fn selection_bg() -> Color {
    selection_bg_for(maple())
}

/// Apply the shared selection background to `style` when `selected` — the
/// single place every list row, board cell, and picker row should reach for
/// this instead of re-deriving the same `if selected { .bg(...) }` branch,
/// so the tint can never have gaps between spans.
pub(crate) fn selected_style(style: Style, selected: bool) -> Style {
    if selected {
        style.bg(selection_bg())
    } else {
        style
    }
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
        // SPEC.md §11: "height < ~30 rows: ... quick view caps at 40%
        // height" — a shorter terminal has less room to spare for the list
        // above it, so the quick-view strip gives some of its share back.
        let quick_view_share = if root[1].height < home::SHORT_HEIGHT {
            40
        } else {
            50
        };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Percentage(quick_view_share)])
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
    // At narrow widths it docks into the footer instead (see `draw_footer`)
    // unless explicitly popped out — see `jax_companion::jax_mode`.
    if jax_companion::jax_mode(app, body_area.width) == JaxMode::Full {
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

    if app.palette_open {
        draw_palette(f, app, f.area());
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
// The breadcrumb, sync pill, and their layout live in `header` — see its
// module doc for the collapse-below-90-cols rule.

// ── Footer ───────────────────────────────────────────────────────────────────
/// Group content per screen, the width-measuring drop rule, and rendering
/// live in `footer` — see its module doc for the no-wrap contract.
fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(muted()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // SPEC.md §9: mini-Jax docks into the footer's right side at narrow
    // widths, unless the full box has been explicitly popped out instead.
    // `area.width` (this function's own un-bordered parameter), not
    // `inner.width` (post-border) — matching the same raw width `draw()`
    // passes for the full box's own `jax_mode` check, since both represent
    // the same overall terminal width (header/body/footer are equal-width
    // siblings of one root vertical split) and must agree on one threshold.
    let mini_jax = jax_companion::jax_mode(app, area.width) == JaxMode::Mini;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if mini_jax {
            vec![
                Constraint::Percentage(70),
                Constraint::Percentage(20),
                Constraint::Length(MINI_DOCK_WIDTH),
            ]
        } else {
            vec![Constraint::Percentage(80), Constraint::Percentage(20)]
        })
        .split(inner);

    f.render_widget(
        Paragraph::new(footer_line(app, cols[0].width as usize)),
        cols[0],
    );
    let prefix = loading_prefix(app);
    // Truncate with an ellipsis rather than letting the Paragraph hard-clip
    // mid-word — the status column can carry real error text (e.g. a live
    // Jira failure reason) that's worth keeping legible even when narrow.
    let budget = (cols[1].width as usize)
        .saturating_sub(prefix.chars().count())
        .saturating_sub(1); // trailing space
    let status = truncate(&app.status, budget);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{prefix}{status} "),
            Style::default().fg(accent()),
        )))
        .alignment(Alignment::Right),
        cols[1],
    );
    if mini_jax {
        draw_jax_mini(f, app, cols[2]);
    } else {
        // Every frame either records a fresh area above or clears it here —
        // never leaves a stale one behind — so `App::point_in_jax_mini`
        // can't misfire against a leftover `Rect` from a previous frame
        // where the terminal was narrower (its own gates re-check
        // `jax_popped`/the hidden-screens list, but not width, since width
        // isn't otherwise available at click time).
        app.jax_mini_area.set(Rect::default());
    }
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

/// The Detail screen's workflow strip (SPEC.md §6): each status as a chip,
/// the current one solid and bold (a terminal stand-in for the mockup's
/// "dashed outline chip, current one solid orchid bold" — box-drawn dashed
/// borders per chip aren't practical inline, so non-current statuses fall
/// back to plain faint text instead).
pub(crate) fn workflow_chip(text: &str, current: bool) -> Span<'static> {
    if current {
        Span::styled(
            format!(" {text} "),
            Style::default()
                .fg(Color::Black)
                .bg(accent2())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(text.to_string(), Style::default().fg(faint()))
    }
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
        Priority::Low | Priority::Lowest => task_blue(),
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
        // Everything else — including "To Do"/"Backlog" and any workflow
        // status this codebase doesn't special-case (e.g. "Blocked", "In
        // QA") — falls back to the theme-aware muted tone rather than a
        // bare `Color::White`, so an unrecognized status still reads as a
        // subdued chip instead of a jarring solid-white block.
        _ => muted(),
    }
}

/// Type chip colour (SPEC.md §1): Bug/Story/Task each get a distinct
/// colour; Epic and anything else fall back to the previous uniform
/// `accent2()` (Epic is listed explicitly, matching the same value the
/// wildcard already gives it, so the mapping stays legible on its own).
pub(crate) fn type_colour(issue_type: &str) -> Color {
    match issue_type {
        "Bug" => danger(),
        "Story" => ok(),
        "Task" => task_blue(),
        // Epic and anything else share this fallback intentionally — Epic
        // isn't listed as its own arm because clippy (rightly) treats a
        // wildcard-covered literal arm as redundant.
        _ => accent2(),
    }
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// The assignee column's "initials avatar" (SPEC.md §3): first+last initial
/// for a "First Last"-shaped display name, the first two characters of a
/// single-word name, or `?` for an empty one.
pub(crate) fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect::<String>().to_uppercase(),
        [first, .., last] => format!(
            "{}{}",
            first.chars().next().unwrap_or('?'),
            last.chars().next().unwrap_or('?')
        )
        .to_uppercase(),
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

    #[test]
    fn priority_colour_low_and_lowest_are_theme_aware() {
        // Regression test: this arm used to return the bare `Color::Blue`
        // instead of a theme_colour()-backed function, so it never got the
        // new alpha-tinted chip() treatment truecolor terminals give every
        // other priority.
        assert_eq!(priority_colour(&Priority::Low), task_blue());
        assert_eq!(priority_colour(&Priority::Lowest), task_blue());
    }

    /// Regression test: `selection_bg()` used to pass `maple()`'s named
    /// fallback straight through `blend()` unchanged, so a non-truecolor
    /// terminal painted a full-brightness `Color::LightRed` background
    /// behind every selected row instead of a subtle tint.
    #[test]
    fn selection_bg_for_falls_back_to_a_muted_grey_on_named_colours() {
        assert_eq!(selection_bg_for(Color::LightRed), Color::DarkGray);
        assert_eq!(selection_bg_for(Color::Cyan), Color::DarkGray);
    }

    #[test]
    fn selection_bg_for_blends_rgb_maple() {
        assert_eq!(
            selection_bg_for(Color::Rgb(0xE8, 0x83, 0x4A)),
            blend(Color::Rgb(0xE8, 0x83, 0x4A), 0.20)
        );
    }

    #[test]
    fn initials_handles_empty_single_and_multi_word_names() {
        assert_eq!(initials(""), "?");
        assert_eq!(initials("   "), "?");
        assert_eq!(initials("Zephyr"), "ZE");
        assert_eq!(initials("Scott Morris"), "SM");
        assert_eq!(initials("Alex J. Chen"), "AC");
    }
}
