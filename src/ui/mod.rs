//! Rendering layer: theme, screens, chrome, and the animated About panel.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::adf;
use crate::app::{App, Screen};
use crate::domain::{IssueSummary, Priority};

// ── Theme ──────────────────────────────────────────────────────────────────
const ACCENT: Color = Color::Cyan;
const ACCENT2: Color = Color::Magenta;
const MUTED: Color = Color::DarkGray;
const OK: Color = Color::Green;
const WARN: Color = Color::Yellow;
const DANGER: Color = Color::Red;

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

    match app.screen {
        Screen::Welcome => draw_welcome(f, app, root[1]),
        Screen::Home => draw_home(f, app, root[1]),
        Screen::List => draw_list(f, app, root[1], true),
        Screen::Detail => draw_detail(f, app, root[1]),
        Screen::Preview => draw_preview(f, app, root[1]),
        Screen::Edit => draw_editor(f, app, root[1]),
        Screen::About => draw_about(f, app, root[1]),
    }

    draw_footer(f, app, root[2]);

    // The ambient Jax companion floats in a bottom corner (pure fun 🦦).
    if app.show_jax && !matches!(app.screen, Screen::Welcome | Screen::Edit | Screen::About) {
        draw_jax_companion(f, app, f.area());
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
            if app.mouse_enabled {
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
    let keys = match app.screen {
        Screen::Welcome => match app.welcome_phase {
            crate::app::WelcomePhase::Intro => {
                "s set up live · d demo · w write config · ? help · q quit"
            }
            crate::app::WelcomePhase::Setup => {
                "type to edit · tab next · ⏎ verify & save · esc back"
            }
        },
        Screen::Detail => {
            "↑/↓ scroll · t transition · e edit · esc/← back · a about · ? help · q quit"
        }
        Screen::Preview => "y apply to Jira · esc/← cancel · ↑/↓ scroll",
        Screen::Edit => "type to edit · ^S preview · esc cancel",
        Screen::About => "esc/← back · ? help · q quit",
        _ => "↑/↓ move · →/⏎ open · s sort · f filter · v quick · J jax · ? help · q quit",
    };
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

fn draw_welcome(f: &mut Frame, app: &App, area: Rect) {
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

    match app.welcome_phase {
        crate::app::WelcomePhase::Intro => draw_welcome_intro(f, app, inner),
        crate::app::WelcomePhase::Setup => draw_welcome_setup(f, app, inner),
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
    use crate::app::Field;
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
        app.field_site.clone(),
        app.focus == Field::Site,
        false,
    ));
    lines.push(Line::from(""));
    lines.push(field(
        "email",
        app.field_email.clone(),
        app.focus == Field::Email,
        false,
    ));
    lines.push(Line::from(""));
    lines.push(field(
        "token",
        app.field_token.clone(),
        app.focus == Field::Token,
        true,
    ));
    lines.push(Line::from(""));

    if !app.setup_msg.is_empty() {
        lines.push(
            Line::from(Span::styled(
                app.setup_msg.clone(),
                Style::default().fg(WARN),
            ))
            .alignment(Alignment::Center),
        );
    }

    f.render_widget(Paragraph::new(Text::from(lines)), area);
}

// ── Home dashboard ───────────────────────────────────────────────────────────
fn draw_home(f: &mut Frame, app: &App, area: Rect) {
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

// ── Issue list ───────────────────────────────────────────────────────────────
fn draw_list(f: &mut Frame, app: &App, area: Rect, full: bool) {
    // Split off a quick-view panel at the bottom when enabled.
    let (list_area, quick_area) = if app.quick_view {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(9)])
            .split(area);
        (rows[0], Some(rows[1]))
    } else {
        (area, None)
    };

    let base = if full { "all my work" } else { "my work" };
    let mut title = format!("  {base} · {}", app.sort_label());
    if let Some(filter) = app.filter_label() {
        title.push_str(&format!(" · {filter}"));
    }
    title.push_str(&format!("  ({})  ", app.issues.len()));
    let block = card(&title, ACCENT);
    let inner = block.inner(list_area);
    f.render_widget(block, list_area);

    let mut lines: Vec<Line> = Vec::new();
    let height = inner.height as usize;
    // simple scroll window around the selection
    let start = app
        .selected
        .saturating_sub(height.saturating_sub(2).max(1) / 2);
    for (i, issue) in app.issues.iter().enumerate().skip(start).take(height) {
        lines.push(issue_row(issue, i == app.selected));
    }
    // Record geometry so mouse clicks can be mapped back to issues.
    app.list_area.set(inner);
    app.list_start.set(start);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);

    if let Some(qa) = quick_area {
        draw_quick_view(f, app, qa);
    }
}

fn draw_quick_view(f: &mut Frame, app: &App, area: Rect) {
    let block = card("  quick view  ", ACCENT2);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(issue) = app.selected_issue() else {
        return;
    };
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", issue.key),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        chip(&issue.issue_type, ACCENT2),
        Span::raw(" "),
        chip(&issue.status, status_colour(&issue.status)),
        Span::raw(" "),
        chip(issue.priority.label(), priority_colour(&issue.priority)),
        if issue.blocked {
            Span::styled("  ⛔ blocked", Style::default().fg(DANGER))
        } else {
            Span::raw("")
        },
    ]));
    lines.push(Line::from(Span::styled(
        issue.summary.clone(),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "assignee: {}   updated: {}",
            issue
                .assignee
                .clone()
                .unwrap_or_else(|| "unassigned".into()),
            issue.updated
        ),
        Style::default().fg(MUTED),
    )));

    // If we've opened this issue before, show a snippet of its description.
    if let Some(detail) = app.quick_view_detail() {
        lines.push(divider());
        let rendered = adf::render(&detail.description);
        let avail = inner.height.saturating_sub(lines.len() as u16) as usize;
        for line in rendered.lines.into_iter().take(avail) {
            lines.push(line);
        }
    } else {
        lines.push(Line::from(Span::styled(
            "→ or ⏎ to open full detail",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        )));
    }

    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn issue_row(issue: &IssueSummary, selected: bool) -> Line<'static> {
    let cursor = if selected { "▌" } else { " " };
    let cursor_style = if selected {
        Style::default().fg(ACCENT2)
    } else {
        Style::default()
    };
    let key_style = if selected {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACCENT)
    };
    let summary_style = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let block_flag = if issue.blocked {
        Span::styled(" ⛔", Style::default().fg(DANGER))
    } else {
        Span::raw("")
    };

    let updated_style = if selected {
        Style::default().fg(Color::Gray)
    } else {
        Style::default().fg(MUTED)
    };

    Line::from(vec![
        Span::styled(cursor.to_string(), cursor_style),
        Span::styled(
            priority_glyph(&issue.priority),
            priority_style(&issue.priority),
        ),
        Span::raw(" "),
        Span::styled(format!("{:<8}", issue.key), key_style),
        Span::styled(
            format!("{:<11}", status_short(&issue.status)),
            status_style(&issue.status),
        ),
        Span::styled(truncate(&issue.summary, 40), summary_style),
        block_flag,
        Span::styled(format!("  {}", issue.updated), updated_style),
    ])
}

// ── Issue detail ─────────────────────────────────────────────────────────────
fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let Some(detail) = app.detail.as_ref() else {
        f.render_widget(
            Paragraph::new("No issue loaded").block(card("  detail  ", ACCENT)),
            area,
        );
        return;
    };

    let block = card(&format!("  {}  ", detail.key), ACCENT);
    let inner = block.inner(area);
    app.detail_area.set(inner);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    // Identity line
    lines.push(Line::from(vec![
        Span::styled(
            priority_glyph(&detail.priority),
            priority_style(&detail.priority),
        ),
        Span::raw(" "),
        Span::styled(
            detail.summary.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(vec![
        chip(&detail.issue_type, ACCENT2),
        Span::raw(" "),
        chip(&detail.status, status_colour(&detail.status)),
        Span::raw(" "),
        chip(detail.priority.label(), priority_colour(&detail.priority)),
        Span::styled(
            format!(
                "   assignee: {}",
                detail
                    .assignee
                    .clone()
                    .unwrap_or_else(|| "unassigned".into())
            ),
            Style::default().fg(MUTED),
        ),
    ]));
    if let Some(reporter) = &detail.reporter {
        lines.push(Line::from(Span::styled(
            format!("reporter: {reporter}"),
            Style::default().fg(MUTED),
        )));
    }
    if let Some(parent) = &detail.parent {
        lines.push(Line::from(Span::styled(
            format!("epic: {parent}"),
            Style::default().fg(MUTED),
        )));
    }
    if !detail.components.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("components: {}", detail.components.join(", ")),
            Style::default().fg(MUTED),
        )));
    }
    if !detail.labels.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("labels: {}", detail.labels.join(", ")),
            Style::default().fg(MUTED),
        )));
    }
    for link in &detail.links {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", link.relation), Style::default().fg(DANGER)),
            Span::styled(link.key.clone(), Style::default().fg(ACCENT)),
            Span::styled(
                format!(" · {}", truncate(&link.summary, 40)),
                Style::default().fg(MUTED),
            ),
        ]));
    }
    lines.push(divider());

    // Description (ADF)
    let desc = adf::render(&detail.description);
    lines.extend(desc.lines);

    // Acceptance criteria
    if let Some(ac) = &detail.acceptance_criteria {
        lines.push(divider());
        lines.push(Line::from(Span::styled(
            "Acceptance Criteria",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        lines.extend(adf::render(ac).lines);
    }

    // Quick transitions strip (preview only — mutation lands in a later phase).
    if !detail.transitions.is_empty() {
        lines.push(divider());
        let mut strip = vec![Span::styled("transitions  ", Style::default().fg(MUTED))];
        for (i, t) in detail.transitions.iter().enumerate() {
            let current = t.to == detail.status || t.name == detail.status;
            let colour = if current { ACCENT } else { Color::White };
            let modifier = if current {
                Modifier::BOLD | Modifier::UNDERLINED
            } else {
                Modifier::empty()
            };
            strip.push(Span::styled(
                t.name.clone(),
                Style::default().fg(colour).add_modifier(modifier),
            ));
            if i + 1 < detail.transitions.len() {
                strip.push(Span::styled(" → ", Style::default().fg(MUTED)));
            }
        }
        strip.push(Span::styled(
            "   (t to change)",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ));
        lines.push(Line::from(strip));
    }

    let para = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, inner);
}

// ── Edit preview ─────────────────────────────────────────────────────────────
fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(OK))
        .title(Span::styled(
            format!("  preview · {key}  "),
            Style::default().fg(OK).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "This is how your edited description will look in Jira (rendered from ADF).",
            Style::default().fg(MUTED),
        )),
        Line::from(Span::styled(
            "Press y to apply, or esc to cancel.",
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        )),
        divider(),
    ];
    if let Some(adf) = app.pending_edit.as_ref() {
        lines.extend(adf::render(adf).lines);
    }
    let para = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, inner);
}

// ── Transition picker ────────────────────────────────────────────────────────
fn draw_transition_picker(f: &mut Frame, app: &App, area: Rect) {
    let transitions = match app.detail.as_ref() {
        Some(d) => &d.transitions,
        None => return,
    };
    let current = app.detail.as_ref().map(|d| d.status.as_str()).unwrap_or("");
    let height = (transitions.len() as u16)
        .saturating_add(4)
        .min(area.height);
    let popup = centered_rect_h(46, height, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            "  move to…  ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    for (i, t) in transitions.iter().enumerate() {
        let selected = i == app.picker_index;
        let is_current = t.to == current;
        let cursor = if selected { "▌ " } else { "  " };
        let mut style = if selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        if is_current {
            style = style.fg(ACCENT);
        }
        let suffix = if is_current { "  (current)" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(cursor.to_string(), Style::default().fg(ACCENT2)),
            Span::styled(t.name.clone(), style),
            Span::styled(suffix.to_string(), Style::default().fg(MUTED)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⏎ apply · esc/← cancel",
        Style::default().fg(MUTED),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn centered_rect_h(width_pct: u16, height: u16, area: Rect) -> Rect {
    let y = area.y + area.height.saturating_sub(height) / 2;
    let w = area.width * width_pct / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    Rect::new(x, y, w, height.min(area.height))
}

// ── In-TUI editor ────────────────────────────────────────────────────────────
fn draw_editor(f: &mut Frame, app: &App, area: Rect) {
    let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(WARN))
        .title(Span::styled(
            format!("  editing {key} · Markdown  "),
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            "  ^S preview · esc cancel  ",
            Style::default().fg(MUTED),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let ed = &app.editor;
    let height = inner.height.max(1) as usize;
    let scroll = if ed.cy >= height {
        ed.cy - height + 1
    } else {
        0
    };

    let gutter_w = 4u16;
    let mut lines: Vec<Line> = Vec::new();
    for (i, line) in ed.lines.iter().enumerate().skip(scroll).take(height) {
        lines.push(Line::from(vec![
            Span::styled(format!("{:>3} ", i + 1), Style::default().fg(MUTED)),
            Span::raw(line.clone()),
        ]));
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);

    // Place the real terminal cursor.
    let cx = inner.x + gutter_w + ed.cx as u16;
    let cy = inner.y + (ed.cy - scroll) as u16;
    if cx < inner.x + inner.width && cy < inner.y + inner.height {
        f.set_cursor_position((cx, cy));
    }
}

// ── Jax companion (entertainment) ────────────────────────────────────────────
fn draw_jax_companion(f: &mut Frame, app: &App, area: Rect) {
    let w = 30u16.min(area.width);
    let h = 8u16.min(area.height.saturating_sub(3));
    let x = area.x + 2;
    let y = area.y + area.height.saturating_sub(h + 3);
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

// ── About (animated) ─────────────────────────────────────────────────────────
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

fn draw_about(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT2))
        .title(Span::styled(
            "  about  ",
            Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
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
            Span::styled(format!("{spinner} "), Style::default().fg(ACCENT)),
            Span::styled(
                tagline.to_string(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(format!(" {spinner}"), Style::default().fg(ACCENT)),
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
            Style::default().fg(MUTED),
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
                Span::styled(c.to_string(), Style::default().fg(ACCENT))
            }
        })
        .collect();
    Line::from(spans)
}

// ── Help overlay ─────────────────────────────────────────────────────────────
fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup = centered_rect(56, 62, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            "  keys  ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let rows = [
        ("↑ / k", "move up"),
        ("↓ / j", "move down"),
        ("→ / ⏎", "open selected issue"),
        ("esc/←/⌫", "back"),
        ("s / S", "cycle sort / flip direction"),
        ("f", "cycle status filter"),
        ("v", "toggle quick-view panel"),
        ("t", "change status (in an issue)"),
        ("e / E", "edit description (in-TUI / $EDITOR)"),
        ("a", "about panel"),
        ("m", "toggle mouse mode"),
        ("J", "toggle Jax companion 🦦"),
        ("y / Y", "copy issue key / URL"),
        ("r", "refresh from source"),
        ("? / q", "toggle help / quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(
                    format!("  {k:<9}"),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(d.to_string(), Style::default().fg(Color::White)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(Text::from(lines)).block(block), popup);
}

// ── Small helpers ────────────────────────────────────────────────────────────
fn card(title: &str, colour: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ))
}

fn chip(text: &str, colour: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default().fg(Color::Black).bg(colour),
    )
}

fn divider() -> Line<'static> {
    Line::from(Span::styled("─".repeat(52), Style::default().fg(MUTED)))
}

fn priority_glyph(p: &Priority) -> String {
    p.glyph().to_string()
}

fn priority_style(p: &Priority) -> Style {
    Style::default().fg(priority_colour(p))
}

fn priority_colour(p: &Priority) -> Color {
    match p {
        Priority::Highest | Priority::High => DANGER,
        Priority::Medium => WARN,
        Priority::Low | Priority::Lowest => Color::Blue,
    }
}

fn status_short(s: &str) -> String {
    truncate(s, 10)
}

fn status_colour(s: &str) -> Color {
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

fn truncate(s: &str, max: usize) -> String {
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
