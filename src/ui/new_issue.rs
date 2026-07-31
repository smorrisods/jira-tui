//! The new-issue compose form (`Screen::NewIssue`, `a` on Home/List):
//! project / issue-type / summary. The description step and confirmation
//! preview reuse the existing editor/preview screens (`EditTarget::NewIssue`)
//! rather than anything in this file.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, NewIssueField};

use super::{accent, accent2, card_bordered, muted, warn};

pub(crate) fn draw_new_issue(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(area);

    app.new_issue_project_area.set(rows[0]);
    app.new_issue_summary_area.set(rows[2]);

    draw_text_field(
        f,
        rows[0],
        "  project  ",
        &app.new_issue.project,
        app.new_issue.focus == NewIssueField::Project,
    );
    draw_issue_type_field(f, rows[1], app);
    draw_text_field(
        f,
        rows[2],
        "  summary  ",
        &app.new_issue.summary,
        app.new_issue.focus == NewIssueField::Summary,
    );

    let hint_style = Style::default().fg(muted()).add_modifier(Modifier::ITALIC);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "tab moves between fields · ⏎ continues to the description (optional)",
            hint_style,
        ))),
        rows[3],
    );
}

fn draw_text_field(f: &mut Frame, area: Rect, title: &str, value: &str, focused: bool) {
    let colour = if focused { accent() } else { muted() };
    let block = card_bordered(title, accent2(), colour);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans = vec![Span::styled(
        value.to_string(),
        Style::default().fg(Color::White),
    )];
    if focused {
        spans.push(Span::styled("▏", Style::default().fg(accent())));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn draw_issue_type_field(f: &mut Frame, area: Rect, app: &App) {
    app.new_issue_type_area.set(area);
    let focused = app.new_issue.focus == NewIssueField::IssueType;
    let colour = if focused { accent() } else { muted() };
    let block = card_bordered("  issue type  ", accent2(), colour);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let line = if app.new_issue.types_loading {
        Line::from(Span::styled(
            "loading issue types…",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        ))
    } else if app.new_issue.available_types.is_empty() {
        Line::from(Span::styled(
            "no issue types available for this project",
            Style::default().fg(warn()),
        ))
    } else {
        let name = app
            .new_issue
            .available_types
            .get(app.new_issue.issue_type_index)
            .map(|t| t.name.as_str())
            .unwrap_or("—");
        let mut spans = vec![Span::styled(
            name.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )];
        // A dropdown affordance rather than the old left/right scroller's
        // `◂ ▸` — Enter (or a click) while focused opens the full list
        // (`new_issue_type_picker_open`) instead of stepping through one
        // entry at a time.
        spans.push(Span::styled(
            "  ▾",
            Style::default().fg(if focused { accent() } else { muted() }),
        ));
        Line::from(spans)
    };
    f.render_widget(Paragraph::new(line), inner);
}
