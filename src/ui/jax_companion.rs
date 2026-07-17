//! The ambient Jax companion: a toggleable, purely-for-fun mascot that
//! cycles through little animated scenes (SPEC.md §9) — a floating 30×8 box
//! at wide widths, or an ambient docked chip in the footer at narrow ones.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, Screen};

use super::{accent, accent2, maple, muted};

/// The footer column width `draw_footer` reserves for `draw_jax_mini`.
pub(crate) const MINI_DOCK_WIDTH: u16 = 14;

/// Which of Jax's three presentations should show right now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JaxMode {
    Hidden,
    /// Docked in the footer's right side — ambient, unconditional at narrow
    /// widths regardless of `jax_popped` (SPEC.md §9: "when the floating box
    /// would overlap content").
    Mini,
    /// The floating box — shown only when `jax_popped` is set (an explicit
    /// pop-out), regardless of width. At wide widths with `jax_popped`
    /// false, nothing shows at all (today's exact default behaviour,
    /// unchanged by this phase); it's only at narrow widths that the
    /// alternative to `Full` is the ambient `Mini` rather than `Hidden`.
    Full,
}

/// SPEC.md §9. `jax_popped` means "the user has explicitly popped the full
/// box out" (an override), not "Jax is enabled at all" — so at narrow
/// widths Jax still shows (as the mini dock) even when `jax_popped` is
/// false, and popping the full box out works regardless of width.
pub(crate) fn jax_mode(app: &App, width: u16) -> JaxMode {
    if matches!(app.screen, Screen::Welcome | Screen::Edit | Screen::About) {
        return JaxMode::Hidden;
    }
    if app.jax_popped {
        return JaxMode::Full;
    }
    if width < 90 {
        return JaxMode::Mini;
    }
    JaxMode::Hidden
}

/// Draws Jax bottom-aligned within `area`, matching the mockup's bottom-right
/// placement (docs/archive/design/ui-refresh.html). Callers pass a bounding
/// region that already excludes anything Jax must not cover (e.g. the
/// quick-view panel), so this simply hugs the bottom-right corner of
/// whatever it's given.
pub(crate) fn draw_jax_companion(f: &mut Frame, app: &App, area: Rect) {
    let w = 30u16.min(area.width);
    let h = 8u16.min(area.height.saturating_sub(1));
    let x = area.x + area.width.saturating_sub(w + 2);
    let y = area.y + area.height.saturating_sub(h + 1);
    let rect = Rect::new(x, y, w, h);
    f.render_widget(Clear, rect);

    let scene = jax_scene_index(app);
    let (caption, mut body) = jax_scene(scene, app.tick);
    body.push(Line::default());
    body.push(Line::from(Span::styled(
        format!("mood: {}", jax_mood(scene)),
        Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent2()))
        .title(Span::styled(
            format!("  jax · {caption}  "),
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(
        Paragraph::new(Text::from(body)).alignment(Alignment::Center),
        inner,
    );
}

/// The mini-Jax footer dock (SPEC.md §9, narrow widths): `"●‿● jax {emoji}"`,
/// cycling the scene emoji in place — a static face, unlike the full box's
/// blinking one, matching the mockup's static mini face. Records its own
/// area for `App::point_in_jax_mini`'s click hit-testing.
pub(crate) fn draw_jax_mini(f: &mut Frame, app: &App, area: Rect) {
    app.jax_mini_area.set(area);
    let scene = jax_scene_index(app);
    let line = Line::from(Span::styled(
        format!("●‿● jax {}", jax_emoji(scene)),
        Style::default().fg(accent2()),
    ));
    f.render_widget(Paragraph::new(line).alignment(Alignment::Right), area);
}

/// Which scene is showing right now: the normal tick-driven rotation,
/// unless a reactive "party" moment (`App::trigger_jax_party` — a
/// successful transition-to-Done/edit/comment) is still active, in which
/// case the party scene is forced regardless of the rotation. Mirrors
/// `flash`/`flash_until`'s "forced state until a tick deadline" shape.
fn jax_scene_index(app: &App) -> u64 {
    if app.tick < app.jax_party_until {
        4
    } else {
        (app.tick / 45) % 6
    }
}

/// The emoji representing each scene — the single source of truth for both
/// the full box's caption and the mini dock, so they can never drift apart.
fn jax_emoji(scene: u64) -> &'static str {
    match scene {
        0 => "👋",
        1 => "😴",
        2 => "🤓",
        3 => "🎣",
        4 => "🎉",
        _ => "🦦",
    }
}

/// One faint mood line per scene (SPEC.md §9), copy taken verbatim from the
/// design mockup.
fn jax_mood(scene: u64) -> &'static str {
    match scene {
        0 => "happy to see you",
        1 => "five more minutes",
        2 => "focused",
        3 => "patient",
        4 => "celebrating your merge",
        _ => "it's the little things",
    }
}

/// A rotating cast of little animated Jax scenes. Captions repeat the same
/// emoji `jax_emoji` returns for this scene as a literal (rather than
/// building the string at render time via `jax_emoji`, which would need an
/// owned `String` every frame) — kept in sync by the `jax_emoji_and_mood_*`
/// test below.
fn jax_scene(scene: u64, tick: u64) -> (&'static str, Vec<Line<'static>>) {
    let frame = (tick / 3) % 4;
    let blink = (tick / 6).is_multiple_of(9);
    let eyes = if blink { "- ‿ -" } else { "●‿●" };
    let a = Style::default().fg(accent());
    let w = Style::default().fg(Color::White);
    let m = Style::default().fg(muted());

    let ln = |s: String, st: Style| Line::from(Span::styled(s, st));

    match scene {
        0 => {
            // waving
            let arm = if frame.is_multiple_of(2) {
                "  o/"
            } else {
                "  \\o"
            };
            (
                "👋 hi!",
                vec![
                    ln(" .---.".into(), a),
                    ln(format!(" |{eyes}|"), w),
                    ln(format!("{arm}|  |"), a),
                    ln("  '--'".into(), a),
                ],
            )
        }
        1 => {
            // sleeping
            let z = ["z  ", " Z ", "  z", " Z "][frame as usize];
            (
                "😴 zzz…",
                vec![
                    ln(format!("      {z}"), m),
                    ln(" .---.".into(), a),
                    ln(" |-‿-|".into(), w),
                    ln("  '--'".into(), a),
                ],
            )
        }
        2 => {
            // nerd, reading a spec
            let cur = if frame.is_multiple_of(2) { "▌" } else { " " };
            (
                "🤓 reading specs",
                vec![
                    ln(" .---.  __".into(), a),
                    ln(format!(" |◕‿◕| |{cur}|"), w),
                    ln("  '--'  ‾‾".into(), a),
                    ln(" // TODO".into(), m),
                ],
            )
        }
        3 => {
            // fishing
            let bob = ["°", ".", "°", "o"][frame as usize];
            let fish = if frame == 3 { "><>" } else { "   " };
            (
                "🎣 gone fishin'",
                vec![
                    ln(" .---. /".into(), a),
                    ln(format!(" |{eyes}|/"), w),
                    ln("  '--' ".into(), a),
                    ln(
                        format!("~~~~{bob}~{fish}~~"),
                        Style::default().fg(Color::Blue),
                    ),
                ],
            )
        }
        4 => {
            // party — SPEC.md §9: confetti uses maple, not the usual accent2.
            let confetti = ["✦ ˚ ✧", "˚ ✧ ✦", "✧ ✦ ˚", "✦ ✧ ˚"][frame as usize];
            (
                "🎉 woo! 🪅",
                vec![
                    ln(confetti.into(), Style::default().fg(maple())),
                    ln(" .---.".into(), a),
                    ln(format!(" |{eyes}| 🪅"), w),
                    ln(" \\'--'/".into(), a),
                ],
            )
        }
        _ => {
            // otter friend floats by
            let pos = (frame * 3) as usize;
            let pad = " ".repeat(pos.min(10));
            (
                "🦦 otter break",
                vec![
                    ln(" .---.".into(), a),
                    ln(format!(" |{eyes}|"), w),
                    ln("  '--'".into(), a),
                    ln(format!("{pad}🦦~~"), Style::default().fg(Color::Blue)),
                ],
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_at(screen: Screen, jax_popped: bool) -> App {
        let mut app = App::new(true);
        app.screen = screen;
        app.jax_popped = jax_popped;
        app
    }

    #[test]
    fn hidden_on_welcome_edit_about_regardless_of_jax_popped_or_width() {
        for screen in [Screen::Welcome, Screen::Edit, Screen::About] {
            for jax_popped in [false, true] {
                let app = app_at(screen, jax_popped);
                assert_eq!(jax_mode(&app, 40), JaxMode::Hidden);
                assert_eq!(jax_mode(&app, 200), JaxMode::Hidden);
            }
        }
    }

    #[test]
    fn full_whenever_jax_popped_is_set_regardless_of_width() {
        let app = app_at(Screen::Home, true);
        assert_eq!(jax_mode(&app, 40), JaxMode::Full);
        assert_eq!(jax_mode(&app, 200), JaxMode::Full);
    }

    #[test]
    fn mini_only_when_narrow_and_not_popped() {
        let app = app_at(Screen::Home, false);
        assert_eq!(jax_mode(&app, 89), JaxMode::Mini);
        assert_eq!(jax_mode(&app, 90), JaxMode::Hidden);
    }

    #[test]
    fn jax_emoji_and_mood_cover_every_scene_distinctly() {
        let emojis: Vec<_> = (0..6).map(jax_emoji).collect();
        let moods: Vec<_> = (0..6).map(jax_mood).collect();
        for e in &emojis {
            assert!(!e.is_empty());
        }
        for m in &moods {
            assert!(!m.is_empty());
        }
        let unique_emojis: std::collections::HashSet<_> = emojis.iter().collect();
        assert_eq!(
            unique_emojis.len(),
            6,
            "every scene should have a distinct emoji"
        );
        let unique_moods: std::collections::HashSet<_> = moods.iter().collect();
        assert_eq!(
            unique_moods.len(),
            6,
            "every scene should have a distinct mood"
        );
    }

    #[test]
    fn jax_scenes_own_caption_starts_with_jax_emojis_glyph() {
        // Regression guard for the two independent copies of "which emoji
        // goes with which scene" (the mini dock uses `jax_emoji` directly;
        // the full box's captions are separate literals to avoid building a
        // `String` — and thus a heap allocation — every render frame).
        for scene in 0..6u64 {
            let (caption, _) = jax_scene(scene, 0);
            assert!(
                caption.starts_with(jax_emoji(scene)),
                "scene {scene}'s caption {caption:?} should start with jax_emoji's {:?}",
                jax_emoji(scene)
            );
        }
    }

    #[test]
    fn party_scenes_confetti_uses_maple_not_accent2() {
        let (_, body) = jax_scene(4, 0);
        let confetti = &body[0];
        assert_eq!(confetti.spans[0].style.fg, Some(maple()));
    }

    #[test]
    fn jax_scene_index_forces_party_while_the_reactive_window_is_active() {
        let mut app = App::new(true);
        app.tick = 10;
        app.jax_party_until = 20;
        assert_eq!(
            jax_scene_index(&app),
            4,
            "tick 10 is inside the [10, 20) party window"
        );

        app.tick = 20;
        let tick = app.tick;
        assert_eq!(
            jax_scene_index(&app),
            (tick / 45) % 6,
            "tick 20 is at the party window's exclusive end, so the normal rotation should resume"
        );

        // A window that already elapsed doesn't force anything either.
        app.jax_party_until = 5;
        app.tick = 10;
        let tick = app.tick;
        assert_eq!(jax_scene_index(&app), (tick / 45) % 6);
    }
}
