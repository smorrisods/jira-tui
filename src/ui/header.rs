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

use super::{accent, accent2, muted, ok, truncate, warn};

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
/// `budget` is the header's left column width — `ViewKind::Teammate`'s
/// display name is arbitrary Jira user input with no length limit, so it's
/// capped, and the filter crumb (the least essential part) is dropped
/// entirely rather than rendered if there's no room left for it.
fn breadcrumb(app: &App, budget: usize) -> Vec<Span<'static>> {
    if app.screen == Screen::Welcome {
        return vec![];
    }

    let view = truncate(&app.current_view.label(), 30);
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
        let base_width: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let filter_width = 3 + filter.chars().count(); // " · " + filter
        if base_width + filter_width <= budget {
            spans.push(Span::styled(" · ", Style::default().fg(muted())));
            spans.push(Span::styled(filter, Style::default().fg(warn())));
        }
    }

    spans
}

/// "just now" / "5s ago" / "2m ago" / "3h ago".
fn format_elapsed(d: Duration) -> String {
    if d.as_secs() < 5 {
        "just now".into()
    } else {
        format!("{} ago", format_elapsed_short(d))
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

/// The LED colour and a list of candidate texts, most-detailed first — the
/// first one that fits `budget` wins, falling all the way back to the last
/// (shortest) candidate if none do. `Live`/`Cache` get a site/username
/// detail tier (site for `Live`, the more useful disambiguator when more
/// than one Jira instance is configured; `Cache` has no site of its own, so
/// its username stands in) that `Demo` has no equivalent for. `budget` is
/// how many characters remain in the header's right column after the git
/// branch/issue-key context *and* this pill's own leading `● ` LED — every
/// candidate is checked against the same budget, so unlike a fixed-width
/// guess, a long branch name or site hostname can only ever push a source
/// down to a shorter candidate, never past the edge of the column.
fn sync_pill(app: &App, collapsed: bool, budget: usize) -> Vec<Span<'static>> {
    let short = app
        .last_synced
        .map(|t| format_elapsed_short(t.elapsed()))
        .unwrap_or_default();
    let synced = app
        .last_synced
        .map(|t| format_elapsed(t.elapsed()))
        .unwrap_or_else(|| "just now".into());

    let (led, candidates) = match &app.source {
        Source::Demo => (muted(), vec!["demo".to_string()]),
        Source::Live { site, .. } => (
            ok(),
            vec![
                format!("live · {site} · synced {synced}"),
                format!("live · synced {synced}"),
                short.clone(),
            ],
        ),
        Source::Cache { user } => (
            warn(),
            vec![
                format!("cache · {user} · synced {synced}"),
                format!("cache · synced {synced}"),
                short.clone(),
            ],
        ),
    };

    let dot = Span::styled("● ", Style::default().fg(led));
    const DOT_WIDTH: usize = 2; // "● "
    let text = if collapsed {
        candidates.last().cloned().unwrap_or_default()
    } else {
        candidates
            .iter()
            .find(|c| c.chars().count() + DOT_WIDTH <= budget)
            .or(candidates.last())
            .cloned()
            .unwrap_or_default()
    };
    vec![dot, Span::styled(text, Style::default().fg(muted()))]
}

pub(crate) fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let spinner = ['◐', '◓', '◑', '◒'][(app.tick / 2 % 4) as usize];
    // Capped so one runaway-long branch name can't eat the whole right
    // column on its own; the sync pill's own detail segment separately
    // adapts to whatever's left (see below).
    let branch = truncate(app.git.branch.as_deref().unwrap_or("no branch"), 30);
    let ctx_key = app
        .git
        .issue_key
        .as_deref()
        .map(|k| format!(" ⇢ {k}"))
        .unwrap_or_default();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(muted()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

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
    const BRAND_WIDTH: usize = 11; // " {spinner} " (3) + "jira" (4) + "-tui" (4)
    let crumb_budget = (cols[0].width as usize).saturating_sub(BRAND_WIDTH + 5); // "  ·  "
    let crumb = breadcrumb(app, crumb_budget);
    if !crumb.is_empty() {
        left.push(Span::styled("  ·  ", Style::default().fg(muted())));
        left.extend(crumb);
    }
    if app.mouse.enabled {
        left.push(Span::styled("  🖱 mouse", Style::default().fg(ok())));
    }

    // How much room the sync pill actually has, after the git context
    // ("⎇ " + branch + issue key + "  ·  " separator) and a trailing space —
    // this is what lets `sync_pill` decide whether its site/user detail
    // segment fits, instead of a fixed-width guess that a long branch name
    // or site hostname could still overflow. Summed from the same pieces
    // rendered into `right` below, rather than reformatting them into a
    // throwaway string just to measure it.
    let git_prefix_width = 2 + branch.chars().count() + ctx_key.chars().count() + 5;
    let pill_budget = (cols[1].width as usize)
        .saturating_sub(git_prefix_width)
        .saturating_sub(1);
    let mut right = vec![
        Span::styled(format!("⎇ {branch}"), Style::default().fg(Color::Blue)),
        Span::styled(
            ctx_key,
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(muted())),
    ];
    right.extend(sync_pill(app, area.width < COLLAPSE_WIDTH, pill_budget));
    right.push(Span::raw(" "));

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
