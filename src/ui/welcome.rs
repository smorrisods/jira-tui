//! The first-run welcome screen: intro choices and the credential setup form.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Field, WelcomePhase};

use super::{ACCENT, ACCENT2, MUTED, WARN};

// ── Welcome / onboarding ─────────────────────────────────────────────────────

/// Jax, the terminal sidekick. Blinks now and then. 🍁
fn jax(tick: u64) -> Vec<Line<'static>> {
    // Blink for a couple of frames on a slow cycle.
    let blinking = (tick / 6).is_multiple_of(8);
    let eyes = if blinking { "-  -" } else { "●  ●" };
    let leaf = ['🍁', '🍂'][(tick / 8 % 2) as usize];
    let body = Style::default().fg(ACCENT);
    let face = Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD);
    vec![
        Line::from(Span::styled("   .------.   ", body)),
        Line::from(vec![
            Span::styled("   | ", body),
            Span::styled(eyes.to_string(), face),
            Span::styled(" |   ", body),
        ]),
        Line::from(Span::styled("   |  ‿   |   ", body)),
        Line::from(Span::styled("   '--++--'   ", body)),
        Line::from(vec![
            Span::styled("     /", body),
            Span::styled(leaf.to_string(), Style::default()),
            Span::styled("\\     ", body),
        ]),
    ]
}

pub(crate) fn draw_welcome(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT2))
        .title(Span::styled(
            "  welcome to jira-tui  ",
            Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    match app.onboarding.welcome_phase {
        WelcomePhase::Intro => draw_welcome_intro(f, app, inner),
        WelcomePhase::Setup => draw_welcome_setup(f, app, inner),
    }
}

fn draw_welcome_intro(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for l in jax(app.tick) {
        lines.push(l.alignment(Alignment::Center));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            "Hi, I'm Jax — your terminal Jira sidekick. 🍁",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            "You're exploring built-in sample data right now — nothing touches Jira.",
            Style::default().fg(MUTED),
        ))
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    let choices = [
        (
            "s",
            "Set up live access",
            "enter your Jira site, email, and API token",
        ),
        ("d", "Continue in demo", "keep browsing the sample data"),
        (
            "w",
            "Write config file",
            "scaffold ~/.config/jira-tui/config.toml",
        ),
    ];
    for (k, title, desc) in choices {
        lines.push(
            Line::from(vec![
                Span::styled(
                    format!("  [{k}]  "),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{title}  "),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc.to_string(), Style::default().fg(MUTED)),
            ])
            .alignment(Alignment::Center),
        );
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            "Tip: press 'm' any time for mouse mode; hold Shift-drag for native copy.",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ))
        .alignment(Alignment::Center),
    );

    f.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn draw_welcome_setup(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    // A smaller Jax cheering you on.
    for l in jax(app.tick).into_iter().take(4) {
        lines.push(l.alignment(Alignment::Center));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            "Set up live access",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
    );
    lines.push(
        Line::from(Span::styled(
            "Your token is verified against Jira and saved to ~/.config/jira-tui/token (0600).",
            Style::default().fg(MUTED),
        ))
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    let field = |label: &str, value: String, focused: bool, mask: bool| -> Line<'static> {
        let shown = if mask {
            "•".repeat(value.chars().count())
        } else {
            value
        };
        let caret = if focused { "▏" } else { " " };
        let label_style = if focused {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MUTED)
        };
        let box_style = if focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        Line::from(vec![
            Span::styled(format!("  {label:<8} "), label_style),
            Span::styled(format!("{shown}{caret}"), box_style),
        ])
    };

    lines.push(field(
        "site",
        app.onboarding.field_site.clone(),
        app.onboarding.focus == Field::Site,
        false,
    ));
    lines.push(Line::from(""));
    lines.push(field(
        "email",
        app.onboarding.field_email.clone(),
        app.onboarding.focus == Field::Email,
        false,
    ));
    lines.push(Line::from(""));
    lines.push(field(
        "token",
        app.onboarding.field_token.clone(),
        app.onboarding.focus == Field::Token,
        true,
    ));
    lines.push(Line::from(""));

    if !app.onboarding.setup_msg.is_empty() {
        lines.push(
            Line::from(Span::styled(
                app.onboarding.setup_msg.clone(),
                Style::default().fg(WARN),
            ))
            .alignment(Alignment::Center),
        );
    }

    f.render_widget(Paragraph::new(Text::from(lines)), area);
}
