//! The quick-view panel (SPEC.md §4): a compact split of the selected
//! issue's description excerpt and meta fields, shown as a full-width strip
//! beneath Home/List. Replaces the previous full detail re-render — no
//! workflow strip or comments/activity section, unlike the Detail screen.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, ListFocus};
use crate::domain::IssueSummary;
use crate::render::{self, DetailPane, Panel};

use super::detail_columns::wrapped_row_count;
use super::quick_view_columns::{meta_width_for, quick_view_layout_for_width, QuickViewLayout};
use super::{
    accent, accent2, card_bordered, chip, danger, faint, muted, priority_colour, status_colour,
    type_colour,
};

pub(crate) fn draw_quick_view(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.list_focus == ListFocus::QuickView;
    let border = if focused { accent2() } else { muted() };

    let Some(issue) = app.selected_issue() else {
        let block = card_bordered("  quick view  ", accent2(), border);
        app.quick_view_area.set(block.inner(area));
        f.render_widget(block, area);
        return;
    };
    let hint = if focused {
        " · ↑/↓ scroll "
    } else {
        " · tab to scroll "
    };
    let title = format!("  quick view · {}{hint} ", issue.key);
    let block = card_bordered(&title, accent2(), border);
    let inner = block.inner(area);
    app.quick_view_area.set(inner);
    f.render_widget(block, area);

    let Some(detail) = app.quick_view_detail() else {
        draw_loading(f, inner, issue);
        return;
    };

    let updated = issue.updated.clone();
    match quick_view_layout_for_width(inner.width) {
        QuickViewLayout::Wide => draw_wide(f, app, inner, detail, &updated),
        QuickViewLayout::Narrow => draw_narrow(f, app, inner, detail, &updated),
    }
}

fn draw_wide(
    f: &mut Frame,
    app: &App,
    area: Rect,
    detail: &crate::domain::IssueDetail,
    updated: &str,
) {
    let mut wide = render::quick_view_wide(detail, updated);
    if let Some(target) = render::quick_view_wide_links(&wide)
        .get(app.link_index)
        .cloned()
    {
        let lines = match target.pane {
            DetailPane::Meta => &mut wide.meta.lines,
            _ => &mut wide.description.lines,
        };
        render::highlight_target(lines, &target);
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),
            Constraint::Length(meta_width_for(area.width)),
        ])
        .split(area);

    draw_with_overflow(f, cols[0], wide.description, app.quick_view_scroll);
    f.render_widget(
        Paragraph::new(wide.meta.lines).wrap(Wrap { trim: false }),
        cols[1],
    );
}

fn draw_narrow(
    f: &mut Frame,
    app: &App,
    area: Rect,
    detail: &crate::domain::IssueDetail,
    updated: &str,
) {
    let mut narrow = render::quick_view_narrow(detail, updated);
    if let Some(target) = narrow.lines.links.get(app.link_index).cloned() {
        render::highlight_target(&mut narrow.lines.lines, &target);
    }
    draw_with_overflow(
        f,
        area,
        Panel {
            lines: narrow.lines.lines,
            links: narrow.lines.links,
        },
        app.quick_view_scroll,
    );
}

/// Renders `panel.lines` scrolled by `scroll`, appending a trailing
/// `"… ↓ N more lines"` line (SPEC.md §4) whenever more wrapped rows remain
/// below the visible area than fit — reusing `wrapped_row_count` (the same
/// wrap-aware row counter Detail's side rail uses) rather than the raw
/// logical line count, which would under-count as soon as any line wraps.
fn draw_with_overflow(f: &mut Frame, area: Rect, panel: Panel, scroll: u16) {
    if area.height == 0 {
        return;
    }
    let total = wrapped_row_count(&panel.lines, area.width);
    let overflow_at_full_height = total.saturating_sub(scroll).saturating_sub(area.height);
    if overflow_at_full_height == 0 {
        f.render_widget(
            Paragraph::new(panel.lines)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0)),
            area,
        );
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    let remaining = total.saturating_sub(scroll).saturating_sub(rows[0].height);
    f.render_widget(
        Paragraph::new(panel.lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        rows[0],
    );
    let fade = Line::from(Span::styled(
        format!(
            "… ↓ {remaining} more line{}",
            if remaining == 1 { "" } else { "s" }
        ),
        Style::default().fg(faint()).add_modifier(Modifier::ITALIC),
    ));
    f.render_widget(Paragraph::new(fade), rows[1]);
}

/// Shown while the full detail is still loading in the background — what we
/// already know from the summary row.
fn draw_loading(f: &mut Frame, inner: Rect, issue: &IssueSummary) {
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", issue.key),
                Style::default().fg(accent()).add_modifier(Modifier::BOLD),
            ),
            chip(&issue.issue_type, type_colour(&issue.issue_type)),
            Span::raw(" "),
            chip(&issue.status, status_colour(&issue.status)),
            Span::raw(" "),
            chip(issue.priority.label(), priority_colour(&issue.priority)),
            if issue.blocked {
                Span::styled("  ⛔ blocked", Style::default().fg(danger()))
            } else {
                Span::raw("")
            },
        ]),
        Line::from(Span::styled(
            issue.summary.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "assignee: {}   updated: {}",
                issue
                    .assignee
                    .clone()
                    .unwrap_or_else(|| "unassigned".into()),
                issue.updated
            ),
            Style::default().fg(muted()),
        )),
        Line::from(Span::styled(
            "loading full detail…",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
