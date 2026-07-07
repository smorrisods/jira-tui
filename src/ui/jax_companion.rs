//! The ambient Jax companion: a toggleable, purely-for-fun mascot that
//! cycles through little animated scenes.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::{ACCENT, ACCENT2, MUTED};

/// Draws Jax bottom-aligned within `area`. Callers pass a bounding region that
/// already excludes anything Jax must not cover (e.g. the quick-view panel),
/// so this simply hugs the bottom-left corner of whatever it's given.
pub(crate) fn draw_jax_companion(f: &mut Frame, app: &App, area: Rect) {
    let w = 30u16.min(area.width);
    let h = 8u16.min(area.height.saturating_sub(1));
    let x = area.x + 2;
    let y = area.y + area.height.saturating_sub(h + 1);
    let rect = Rect::new(x, y, w, h);
    f.render_widget(Clear, rect);

    let (caption, body) = jax_scene(app.tick);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT2))
        .title(Span::styled(
            format!("  jax · {caption}  "),
            Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(
        Paragraph::new(Text::from(body)).alignment(Alignment::Center),
        inner,
    );
}

/// A rotating cast of little animated Jax scenes.
fn jax_scene(tick: u64) -> (&'static str, Vec<Line<'static>>) {
    let scene = (tick / 45) % 6;
    let frame = (tick / 3) % 4;
    let blink = (tick / 6).is_multiple_of(9);
    let eyes = if blink { "- ‿ -" } else { "●‿●" };
    let a = Style::default().fg(ACCENT);
    let w = Style::default().fg(Color::White);
    let m = Style::default().fg(MUTED);

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
            // party
            let confetti = ["✦ ˚ ✧", "˚ ✧ ✦", "✧ ✦ ˚", "✦ ✧ ˚"][frame as usize];
            (
                "🎉 woo! 🪅",
                vec![
                    ln(confetti.into(), Style::default().fg(ACCENT2)),
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
