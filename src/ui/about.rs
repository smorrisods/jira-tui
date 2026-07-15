//! The animated About panel: a colour-wave ASCII banner, rotating taglines,
//! and a drifting starfield.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::{accent, accent2, muted};

const BANNER: [&str; 6] = [
    "     ██╗██╗██████╗  █████╗   ████████╗██╗   ██╗██╗",
    "     ██║██║██╔══██╗██╔══██╗  ╚══██╔══╝██║   ██║██║",
    "     ██║██║██████╔╝███████║     ██║   ██║   ██║██║",
    "██   ██║██║██╔══██╗██╔══██║     ██║   ██║   ██║██║",
    "╚█████╔╝██║██║  ██║██║  ██║     ██║   ╚██████╔╝██║",
    " ╚════╝ ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝     ╚═╝    ╚═════╝ ╚═╝",
];

const TAGLINES: [&str; 4] = [
    "Jira, without leaving the terminal.",
    "ADF-native. Keyboard-first. A little bit of soul.",
    "Draft in Markdown, ship as ADF, verify at a glance.",
    "Built from the jira-tasks proof-of-concept.",
];

pub(crate) fn draw_about(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent2()))
        .title(Span::styled(
            "  about  ",
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let banner_width = BANNER.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let pad_top = inner.height.saturating_sub(15) / 2;

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..pad_top {
        lines.push(Line::from(""));
    }

    // Animated banner: a colour wave sweeps across the glyphs.
    for (row, raw) in BANNER.iter().enumerate() {
        let mut spans: Vec<Span> = Vec::new();
        for (col, ch) in raw.chars().enumerate() {
            if ch == ' ' {
                spans.push(Span::raw(" "));
                continue;
            }
            let phase = (col + row * 2) as u64;
            let colour = wave_colour(app.tick.wrapping_add(phase));
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(colour).add_modifier(Modifier::BOLD),
            ));
        }
        lines.push(Line::from(spans).alignment(Alignment::Center));
    }

    lines.push(Line::from(""));

    // Rotating tagline with a soft fade via a leading spinner.
    let spinner = ["✦", "✧", "✦", "✧", "∗", "✧"][(app.tick / 2 % 6) as usize];
    let tagline = TAGLINES[(app.tick / 24 % TAGLINES.len() as u64) as usize];
    lines.push(
        Line::from(vec![
            Span::styled(format!("{spinner} "), Style::default().fg(accent())),
            Span::styled(
                tagline.to_string(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(format!(" {spinner}"), Style::default().fg(accent())),
        ])
        .alignment(Alignment::Center),
    );

    lines.push(Line::from(""));

    // A little starfield that drifts across the width.
    lines.push(starfield(app.tick, banner_width).alignment(Alignment::Center));

    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            format!("v{}  ·  press esc to return", env!("CARGO_PKG_VERSION")),
            Style::default().fg(muted()),
        ))
        .alignment(Alignment::Center),
    );

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Map an animation phase to a cool gradient (blue → cyan → magenta).
fn wave_colour(phase: u64) -> Color {
    let palette = [
        Color::Blue,
        Color::LightBlue,
        Color::Cyan,
        Color::LightCyan,
        Color::White,
        Color::LightMagenta,
        Color::Magenta,
    ];
    palette[(phase % palette.len() as u64) as usize]
}

fn starfield(tick: u64, width: usize) -> Line<'static> {
    let width = width.max(10);
    let mut chars = vec![' '; width];
    // three stars at different speeds
    let stars = [('✦', 1u64), ('·', 2), ('✧', 3)];
    for (glyph, speed) in stars {
        let pos = ((tick * speed) % width as u64) as usize;
        chars[pos] = glyph;
    }
    let spans: Vec<Span> = chars
        .into_iter()
        .map(|c| {
            if c == ' ' {
                Span::raw(" ")
            } else {
                Span::styled(c.to_string(), Style::default().fg(accent()))
            }
        })
        .collect();
    Line::from(spans)
}
