//! The header's breadcrumb and sync pill (SPEC.md §2).
//!
//! Left: brand, then a `{view} › {screen}` breadcrumb with the current node
//! bold and the active filter (if any) appended as an amber crumb. Right:
//! git context, then a sync pill — a coloured LED (fern when live and
//! fresh, amber when serving cache, muted in demo mode) plus how long ago
//! the current view last loaded. Below `COLLAPSE_WIDTH` columns the pill
//! drops its words down to just the LED and a short "2m" duration.

use std::time::Duration;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Screen};
use crate::domain::Source;

use super::{accent, accent2, muted, ok, warn};

/// Below this width the sync pill collapses to just its LED + short
/// duration, per SPEC.md §2 ("below ~90 cols the pill collapses to `● 2m`").
const COLLAPSE_WIDTH: u16 = 90;

fn screen_label(screen: Screen) -> Option<&'static str> {
    match screen {
        Screen::Home | Screen::Welcome | Screen::Detail => None,
        Screen::List => Some("List"),
        Screen::Board => Some("Board"),
        Screen::Search => Some("Search"),
        Screen::FieldMapping => Some("Field Mapping"),
        Screen::Preview => Some("Preview"),
        Screen::Edit => Some("Edit"),
        Screen::About => Some("About"),
    }
}

/// `{view} › {screen}` with the current (rightmost) node bold, an amber
/// crumb for an active filter, and Detail's special `{key} · ← N back`.
fn breadcrumb(app: &App) -> Vec<Span<'static>> {
    if app.screen == Screen::Welcome {
        return vec![];
    }

    let view = app.current_view.label();
    let mut spans = Vec::new();

    if app.screen == Screen::Detail {
        let key = app.detail.as_ref().map(|d| d.key.clone());
        spans.push(Span::styled(view, Style::default().fg(muted())));
        if let Some(key) = key {
            spans.push(Span::styled(" › ", Style::default().fg(muted())));
            spans.push(Span::styled(
                key,
                Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
            ));
            let back = app.detail_back.len();
            if back > 0 {
                spans.push(Span::styled(
                    format!(" · ← {back} back"),
                    Style::default().fg(muted()),
                ));
            }
        }
    } else if let Some(screen) = screen_label(app.screen) {
        spans.push(Span::styled(view, Style::default().fg(muted())));
        spans.push(Span::styled(" › ", Style::default().fg(muted())));
        spans.push(Span::styled(
            screen,
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ));
    } else {
        // Home: the view itself is the current (bold) node.
        spans.push(Span::styled(
            view,
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(filter) = app.filter_label() {
        spans.push(Span::styled(" · ", Style::default().fg(muted())));
        spans.push(Span::styled(filter, Style::default().fg(warn())));
    }

    spans
}

/// "just now" / "5s ago" / "2m ago" / "3h ago".
fn format_elapsed(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 5 {
        "just now".into()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

/// The collapsed pill's bare duration — "5s" / "2m" / "3h", no "ago".
fn format_elapsed_short(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

fn sync_pill(app: &App, collapsed: bool) -> Vec<Span<'static>> {
    if matches!(app.source, Source::Demo) {
        return vec![Span::styled("● demo", Style::default().fg(muted()))];
    }
    let (led, word) = match app.source {
        Source::Live { .. } => (ok(), "live"),
        Source::Cache { .. } => (warn(), "cache"),
        Source::Demo => unreachable!("handled above"),
    };
    let elapsed = app.last_synced.map(|t| t.elapsed());
    let dot = Span::styled("● ", Style::default().fg(led));
    let text = if collapsed {
        elapsed.map(format_elapsed_short).unwrap_or_default()
    } else {
        format!(
            "{word} · synced {}",
            elapsed
                .map(format_elapsed)
                .unwrap_or_else(|| "just now".into())
        )
    };
    vec![dot, Span::styled(text, Style::default().fg(muted()))]
}

pub(crate) fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let spinner = ['◐', '◓', '◑', '◒'][(app.tick / 2 % 4) as usize];
    let branch = app.git.branch.clone().unwrap_or_else(|| "no branch".into());
    let ctx_key = app
        .git
        .issue_key
        .clone()
        .map(|k| format!(" ⇢ {k}"))
        .unwrap_or_default();

    let mut left = vec![
        Span::styled(format!(" {spinner} "), Style::default().fg(accent2())),
        Span::styled(
            "jira",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "-tui",
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ),
    ];
    let crumb = breadcrumb(app);
    if !crumb.is_empty() {
        left.push(Span::styled("  ·  ", Style::default().fg(muted())));
        left.extend(crumb);
    }
    if app.mouse.enabled {
        left.push(Span::styled("  🖱 mouse", Style::default().fg(ok())));
    }

    let mut right = vec![
        Span::styled(format!("⎇ {branch}"), Style::default().fg(Color::Blue)),
        Span::styled(
            ctx_key,
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(muted())),
    ];
    right.extend(sync_pill(app, area.width < COLLAPSE_WIDTH));
    right.push(Span::raw(" "));

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
    f.render_widget(Paragraph::new(Line::from(left)), cols[0]);
    f.render_widget(
        Paragraph::new(Line::from(right)).alignment(Alignment::Right),
        cols[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_elapsed_buckets_by_magnitude() {
        assert_eq!(format_elapsed(Duration::from_secs(2)), "just now");
        assert_eq!(format_elapsed(Duration::from_secs(30)), "30s ago");
        assert_eq!(format_elapsed(Duration::from_secs(125)), "2m ago");
        assert_eq!(format_elapsed(Duration::from_secs(7500)), "2h ago");
    }

    #[test]
    fn format_elapsed_short_drops_the_ago_suffix() {
        assert_eq!(format_elapsed_short(Duration::from_secs(30)), "30s");
        assert_eq!(format_elapsed_short(Duration::from_secs(125)), "2m");
        assert_eq!(format_elapsed_short(Duration::from_secs(7500)), "2h");
    }

    #[test]
    fn screen_label_covers_every_screen_with_a_breadcrumb_segment() {
        assert_eq!(screen_label(Screen::List), Some("List"));
        assert_eq!(screen_label(Screen::Board), Some("Board"));
        assert_eq!(screen_label(Screen::Search), Some("Search"));
        assert_eq!(screen_label(Screen::FieldMapping), Some("Field Mapping"));
        assert_eq!(screen_label(Screen::Preview), Some("Preview"));
        assert_eq!(screen_label(Screen::Edit), Some("Edit"));
        assert_eq!(screen_label(Screen::About), Some("About"));
        // Home and Detail are handled specially by breadcrumb(), not via a
        // generic "{view} › {screen}" segment; Welcome shows no breadcrumb.
        assert_eq!(screen_label(Screen::Home), None);
        assert_eq!(screen_label(Screen::Detail), None);
        assert_eq!(screen_label(Screen::Welcome), None);
    }
}
