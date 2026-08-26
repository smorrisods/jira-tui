//! The "nerd info" popup: a small diagnostics panel showing this build's
//! version, whether `crate::debug`'s opt-in tracing is currently on, the
//! terminal it thinks it's running in (from environment variables
//! terminals commonly self-report), and — on an `images`-feature build —
//! exactly what graphics capability was detected at startup (protocol,
//! cell pixel size, and the raw capability list `ratatui-image`'s startup
//! probe reported). Reachable only from the
//! command palette (no dedicated key, same "palette-only" precedent as
//! `PaletteAction::OpenInBrowser`) — this is a debugging aid, not a core
//! workflow action.
//!
//! Exists because "why did my image render small/flicker/not render at
//! all" is otherwise unanswerable from inside the running app — a user
//! can't easily tell whether their terminal was detected as Kitty, Sixel,
//! or Halfblocks, or whether no graphics protocol was found at all,
//! without this. Deliberately reads `App::image_picker` (the result
//! `main::detect_image_picker` already stored once at startup) rather than
//! re-querying the terminal here — re-running that probe mid-session,
//! after `crossterm::EventStream` has started polling stdin, is exactly
//! the race `detect_image_picker`'s own doc comment warns against.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

use super::{accent2, centered_rect_h, muted};

/// One of the terminal-identifying environment variables most terminal
/// emulators set — `TERM_PROGRAM`/`TERM_PROGRAM_VERSION` are the closest
/// thing to a de facto standard (set by iTerm2, Windows Terminal, VS Code,
/// Ghostty, and others); `TERM`/`COLORTERM` are the older, coarser
/// signals every terminal sets; `WT_SESSION` is Windows Terminal-specific
/// (present even when `TERM_PROGRAM` isn't, on older Windows Terminal
/// releases); `SSH_TTY` flags a remote session, which matters here since
/// graphics-protocol passthrough over SSH/tmux is often flaky even when
/// the local terminal itself supports one.
const ENV_VARS: &[(&str, &str)] = &[
    ("TERM_PROGRAM", "TERM_PROGRAM"),
    ("TERM_PROGRAM_VERSION", "TERM_PROGRAM_VERSION"),
    ("TERM", "TERM"),
    ("COLORTERM", "COLORTERM"),
    ("WT_SESSION", "WT_SESSION (Windows Terminal)"),
    ("SSH_TTY", "SSH_TTY (remote session)"),
];

pub(crate) fn draw_nerd_info(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(section_title("jira-tui"));
    lines.push(kv("version", env!("CARGO_PKG_VERSION")));
    lines.push(kv(
        "images feature",
        if cfg!(feature = "images") {
            "compiled in"
        } else {
            "not compiled in (cargo build --features images)"
        },
    ));
    lines.push(kv(
        "debug logging",
        if crate::debug::is_enabled() {
            "on — traced to stderr (toggle from the command palette)"
        } else {
            "off (JIRA_TUI_DEBUG env var, or toggle from the command palette)"
        },
    ));
    lines.push(Line::from(""));

    lines.push(section_title("terminal"));
    let mut any_env = false;
    for (var, label) in ENV_VARS {
        if let Ok(value) = std::env::var(var) {
            any_env = true;
            lines.push(kv(label, &value));
        }
    }
    if !any_env {
        lines.push(muted_line(
            "no terminal-identifying environment variables set",
        ));
    }
    lines.push(Line::from(""));

    lines.push(section_title("graphics"));
    lines.extend(graphics_lines(app));

    let width = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.chars().count()).sum())
        .max()
        .unwrap_or(0)
        .max(30) as u16
        + 4;
    let width_pct = (width as u32 * 100 / area.width.max(1) as u32).clamp(30, 90) as u16;
    let height = (lines.len() as u16).saturating_add(2).min(area.height);
    let popup = centered_rect_h(width_pct, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(accent2()))
        .title(Span::styled(
            "  nerd info · any key closes  ",
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

#[cfg(feature = "images")]
fn graphics_lines(app: &App) -> Vec<Line<'static>> {
    match app.image_picker.as_ref() {
        Some(picker) => {
            let font = picker.font_size();
            let mut lines = vec![
                kv("protocol", &format!("{:?}", picker.protocol_type())),
                kv("cell size", &format!("{}x{}px", font.width, font.height)),
            ];
            let caps: Vec<String> = picker
                .capabilities()
                .iter()
                .map(|c| format!("{c:?}"))
                .collect();
            lines.push(kv(
                "reported capabilities",
                if caps.is_empty() {
                    "(none)".to_string()
                } else {
                    caps.join(", ")
                }
                .as_str(),
            ));
            lines
        }
        None => vec![muted_line(
            "no terminal graphics capability detected — falling back to the \
             [image: alt] placeholder everywhere",
        )],
    }
}

#[cfg(not(feature = "images"))]
fn graphics_lines(_app: &App) -> Vec<Line<'static>> {
    vec![muted_line(
        "this binary wasn't built with the images feature",
    )]
}

fn section_title(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
    ))
}

fn kv(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key}: "), Style::default().fg(muted())),
        Span::raw(value.to_string()),
    ])
}

fn muted_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {text}"),
        Style::default().fg(muted()),
    ))
}
