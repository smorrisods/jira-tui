//! The swimlane Kanban board: status columns and Epic-grouped lanes,
//! rendered as a bordered-card grid when wide (SPEC.md §7) or a
//! one-column-at-a-time pager when narrow — see
//! `board_columns::board_layout_for_width`.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::domain::IssueSummary;

use super::board_columns::{self, BoardLayout, CARD_HEIGHT};
use super::{
    accent, accent2, card, chip, danger, faint, initials, maple, muted, priority_glyph,
    priority_style, selection_bg, status_colour, truncate, type_colour,
};

pub(crate) fn draw_board(f: &mut Frame, app: &App, area: Rect) {
    let cols = app.board_columns();
    let lanes = app.board_lanes();

    let title = format!(
        "  board · {} lane{} · {} column{}  ",
        lanes.len(),
        if lanes.len() == 1 { "" } else { "s" },
        cols.len(),
        if cols.len() == 1 { "" } else { "s" },
    );
    let block = card(&title, accent());
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.board_area.set(inner);

    if app.issues.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No issues in the current view.",
                Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }

    match board_columns::board_layout_for_width(inner.width) {
        BoardLayout::Wide => draw_wide(f, app, inner, &cols),
        BoardLayout::Narrow => draw_narrow(f, app, inner, &cols),
    }
}

/// Which lanes to actually render this frame — however many fit within
/// `body.height` starting at `app.board_scroll` — plus their `Constraint`s
/// and whether the trailing collapsed-lanes ghost line also fits. Shared by
/// `draw_wide`/`draw_narrow`; `height_for` computes a lane's height for
/// whichever layout is currently rendering (mirrors `App::board_lane_height`
/// so the renderer and the keyboard-scroll code can never disagree).
fn fit_lanes(
    lanes: &[Option<String>],
    scroll: usize,
    body_height: u16,
    hidden: usize,
    height_for: impl Fn(&Option<String>) -> u16,
) -> (Vec<&Option<String>>, Vec<Constraint>, bool) {
    let mut shown: Vec<&Option<String>> = Vec::new();
    let mut constraints: Vec<Constraint> = Vec::new();
    let mut used = 0u16;
    for lane in lanes.iter().skip(scroll) {
        let h = height_for(lane) + 1; // +1 gap after each lane's band
        if used + h > body_height && !shown.is_empty() {
            break;
        }
        shown.push(lane);
        constraints.push(Constraint::Length(h));
        used += h;
    }
    let show_ghost = hidden > 0 && used < body_height;
    if show_ghost {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(0));
    (shown, constraints, show_ghost)
}

fn draw_wide(f: &mut Frame, app: &App, area: Rect, cols: &[String]) {
    let (lanes, hidden) = app.board_wide_lanes();
    let all_lanes = app.board_lanes();
    let selected_lane = all_lanes.get(app.board_sel.lane).cloned();

    let n = cols.len().max(1);
    let col_width = board_columns::board_card_col_width(area.width, n);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    f.render_widget(Paragraph::new(header_line(app, cols, col_width)), rows[0]);

    let body = rows[1];
    let scroll = (app.board_scroll as usize).min(lanes.len().saturating_sub(1));
    let (shown, constraints, show_ghost) = fit_lanes(&lanes, scroll, body.height, hidden, |lane| {
        app.board_lane_height(lane, BoardLayout::Wide)
    });

    let lane_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(body);

    for (i, lane) in shown.iter().enumerate() {
        let is_selected_lane = selected_lane.as_ref() == Some(*lane);
        draw_wide_lane(
            f,
            app,
            lane_areas[i],
            lane,
            cols,
            col_width,
            is_selected_lane,
        );
    }
    if show_ghost {
        let text = format!(
            "▸ {hidden} lane{} fully done — pgdn to peek",
            if hidden == 1 { "" } else { "s" }
        );
        f.render_widget(ghost_line(&text), lane_areas[shown.len()]);
    }
}

fn header_line(app: &App, cols: &[String], col_width: u16) -> Line<'static> {
    let n = cols.len();
    let mut spans = Vec::new();
    for (i, status) in cols.iter().enumerate() {
        let count = app
            .issues
            .iter()
            .filter(|iss| &iss.status == status)
            .count();
        let label = truncate(&format!("{status} ({count})"), col_width as usize);
        spans.push(Span::styled(
            format!("{label:<width$}", width = col_width as usize),
            Style::default()
                .fg(status_colour(status))
                .add_modifier(Modifier::BOLD),
        ));
        if i + 1 < n {
            spans.push(Span::raw(" "));
        }
    }
    Line::from(spans)
}

fn draw_wide_lane(
    f: &mut Frame,
    app: &App,
    area: Rect,
    lane: &Option<String>,
    cols: &[String],
    col_width: u16,
    is_selected_lane: bool,
) {
    let cells: Vec<Vec<&IssueSummary>> = cols.iter().map(|s| app.board_cell(lane, s)).collect();
    let lane_count: usize = cells.iter().map(|c| c.len()).sum();
    let max_rows = cells.iter().map(|c| c.len()).max().unwrap_or(0).max(1);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(max_rows as u16 * CARD_HEIGHT),
        ])
        .split(area);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                "▾ {} · {lane_count} issue{}",
                app.board_lane_label(lane),
                if lane_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ))),
        rows[0],
    );

    let n = cols.len().max(1);
    let col_areas = Layout::horizontal(vec![Constraint::Length(col_width); n])
        .spacing(1)
        .split(rows[1]);

    for (ci, cell) in cells.iter().enumerate() {
        let col_area = col_areas[ci];
        if cell.is_empty() {
            f.render_widget(
                ghost_line(&"╌".repeat(col_area.width as usize)),
                Rect::new(col_area.x, col_area.y, col_area.width, 1),
            );
            continue;
        }
        let card_areas =
            Layout::vertical(vec![Constraint::Length(CARD_HEIGHT); max_rows]).split(col_area);
        for (ri, issue) in cell.iter().enumerate() {
            let selected = is_selected_lane && app.board_sel.col == ci && app.board_sel.card == ri;
            draw_card(f, card_areas[ri], issue, selected, None);
        }
    }
}

fn draw_narrow(f: &mut Frame, app: &App, area: Rect, cols: &[String]) {
    let status = cols
        .get(app.board_sel.col)
        .cloned()
        .unwrap_or_else(|| "No status".to_string());
    let (lanes, hidden) = app.board_narrow_lanes(&status);
    let all_lanes = app.board_lanes();
    let selected_lane = all_lanes.get(app.board_sel.lane).cloned();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    f.render_widget(Paragraph::new(tab_strip(cols, app.board_sel.col)), rows[0]);

    let body = rows[1];
    let scroll = (app.board_scroll as usize).min(lanes.len().saturating_sub(1));
    let (shown, constraints, show_ghost) = fit_lanes(&lanes, scroll, body.height, hidden, |lane| {
        app.board_lane_height(lane, BoardLayout::Narrow)
    });

    let lane_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(body);

    for (i, lane) in shown.iter().enumerate() {
        let is_selected_lane = selected_lane.as_ref() == Some(*lane);
        draw_narrow_lane(f, app, lane_areas[i], lane, &status, is_selected_lane);
    }
    if show_ghost {
        let text = format!(
            "▸ {hidden} lane{} with nothing {status} — pgdn to peek",
            if hidden == 1 { "" } else { "s" }
        );
        f.render_widget(ghost_line(&text), lane_areas[shown.len()]);
    }
}

fn tab_strip(cols: &[String], current: usize) -> Line<'static> {
    let mut spans = vec![Span::styled("← ", Style::default().fg(muted()))];
    for (i, status) in cols.iter().enumerate() {
        spans.push(board_tab(status, i == current));
        if i + 1 < cols.len() {
            spans.push(Span::raw(" "));
        }
    }
    spans.push(Span::styled(" →", Style::default().fg(muted())));
    Line::from(spans)
}

/// A status tab in the narrow pager's strip (SPEC.md §7): the current
/// column filled and bold in `accent()`/cyan, others plain muted text.
fn board_tab(text: &str, current: bool) -> Span<'static> {
    if current {
        Span::styled(
            format!(" {text} "),
            Style::default()
                .fg(Color::Black)
                .bg(accent())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(text.to_string(), Style::default().fg(muted()))
    }
}

fn draw_narrow_lane(
    f: &mut Frame,
    app: &App,
    area: Rect,
    lane: &Option<String>,
    status: &str,
    is_selected_lane: bool,
) {
    let (here, total) = app.board_lane_counts(lane, status);
    let cards = app.board_cell(lane, status);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                "▾ {} · {here} here · {total} total",
                app.board_lane_label(lane)
            ),
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        ))),
        rows[0],
    );

    let constraints: Vec<Constraint> = (0..cards.len())
        .map(|ci| {
            let selected = is_selected_lane && app.board_sel.card == ci;
            Constraint::Length(if selected {
                CARD_HEIGHT + 1
            } else {
                CARD_HEIGHT
            })
        })
        .chain(std::iter::once(Constraint::Min(0)))
        .collect();
    let card_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(rows[1]);

    for (ci, issue) in cards.iter().enumerate() {
        let selected = is_selected_lane && app.board_sel.card == ci;
        let peek = selected.then(|| neighbour_peek_text(app, lane));
        draw_card(f, card_areas[ci], issue, selected, peek.as_deref());
    }
}

/// The narrow layout's selected-card neighbour-peek line (SPEC.md §7):
/// this lane's issue counts in the immediately adjacent columns, dropping
/// whichever side (and its arrow) is absent at the first/last column.
fn neighbour_peek_text(app: &App, lane: &Option<String>) -> String {
    let (prev, next) = app.board_neighbour_counts(lane);
    match (prev, next) {
        (Some((p, pn)), Some((n, nn))) => format!("◂ {p} {pn} · {n} {nn} ▸"),
        (Some((p, pn)), None) => format!("◂ {p} {pn}"),
        (None, Some((n, nn))) => format!("{n} {nn} ▸"),
        (None, None) => String::new(),
    }
}

fn ghost_line(text: &str) -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
    )))
}

/// One card, shared by both layouts. `peek` is the narrow pager's
/// neighbour-peek line — only ever `Some` for the selected card, and only
/// ever passed by `draw_narrow_lane` (the wide grid has room for the side
/// rail instead, so it never needs one).
fn draw_card(f: &mut Frame, area: Rect, issue: &IssueSummary, selected: bool, peek: Option<&str>) {
    let border_colour = if selected { maple() } else { muted() };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_colour));
    if selected {
        block = block.style(Style::default().bg(selection_bg()));
    }
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut top = vec![
        Span::styled(
            priority_glyph(&issue.priority),
            priority_style(&issue.priority),
        ),
        Span::raw(" "),
        Span::styled(
            issue.key.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        chip(&issue.issue_type, type_colour(&issue.issue_type)),
    ];
    if issue.blocked {
        top.push(Span::raw(" "));
        top.push(chip("⛔", danger()));
    }

    let width = inner.width as usize;
    let assignee_text = match &issue.assignee {
        Some(name) => format!("{} {name}", initials(name)),
        None => "unassigned".to_string(),
    };
    let mut lines = vec![
        Line::from(top),
        Line::from(Span::raw(truncate(&issue.summary, width))),
        Line::from(vec![
            Span::styled(
                truncate(
                    &assignee_text,
                    width.saturating_sub(issue.updated.len() + 1),
                ),
                Style::default().fg(if issue.assignee.is_some() {
                    Color::White
                } else {
                    faint()
                }),
            ),
            Span::raw(" "),
            Span::styled(issue.updated.clone(), Style::default().fg(faint())),
        ]),
    ];
    if let Some(p) = peek {
        lines.push(Line::from(Span::styled(
            p.to_string(),
            Style::default().fg(faint()),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}
