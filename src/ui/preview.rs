//! The edit-preview confirmation screen: shows the recompiled ADF before
//! anything is sent to Jira.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::adf;
use crate::app::{App, EditTarget};

use super::{divider, muted, ok, warn};

pub(crate) fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    // A new issue has no key yet — title on its project instead.
    let title = if app.edit_target == EditTarget::NewIssue {
        format!("  preview · new issue in {}  ", app.new_issue.project)
    } else {
        let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
        format!("  preview · {key}  ")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ok()))
        .title(Span::styled(
            title,
            Style::default().fg(ok()).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let subject = match app.edit_target {
        EditTarget::Description => "edited description",
        EditTarget::Comment => "comment",
        EditTarget::NewIssue => "new issue",
    };
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!("This is how your {subject} will look in Jira (rendered from ADF)."),
            Style::default().fg(muted()),
        )),
        Line::from(Span::styled(
            "Press y/⏎ to apply, or esc to go back and keep editing.",
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        )),
        divider(),
    ];
    if app.edit_target == EditTarget::NewIssue {
        let issue_type = app
            .new_issue
            .available_types
            .get(app.new_issue.issue_type_index)
            .map(|t| t.name.as_str())
            .unwrap_or("—");
        lines.push(Line::from(vec![
            Span::styled("Project: ", Style::default().fg(muted())),
            Span::raw(app.new_issue.project.clone()),
            Span::styled("   Type: ", Style::default().fg(muted())),
            Span::raw(issue_type.to_string()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Summary: ", Style::default().fg(muted())),
            Span::raw(app.new_issue.summary.clone()),
        ]));
        lines.push(divider());
        if app.pending_edit.is_none() {
            lines.push(Line::from(Span::styled(
                "(no description)",
                Style::default().fg(muted()),
            )));
        }
    }
    if let Some(adf) = app.pending_edit.as_ref() {
        lines.extend(adf::render(adf, inner.width as usize).lines);
    }
    let para = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, inner);
}
