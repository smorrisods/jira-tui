//! Issue detail rendering: builds the flat `Line` list shared by the full
//! Detail screen and the inline quick-view panel, plus the line offsets
//! `app` needs to jump the scroll position to the comments section or step
//! between individual comments.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::adf;
use crate::domain::IssueDetail;
use crate::ui::{
    chip, divider, priority_colour, priority_glyph, priority_style, status_colour, truncate,
    ACCENT, ACCENT2, DANGER, MUTED,
};

/// The rendered issue detail, plus line offsets into `lines` for
/// comment-navigation keybindings (`]`/`[`, `n`/`p`).
pub struct IssueLines {
    pub lines: Vec<Line<'static>>,
    /// Line index of the "💬 N comments" section header, if there are any
    /// comments.
    pub comments_header: Option<usize>,
    /// Line index of each individual comment's "author · created" header,
    /// in display order.
    pub comment_starts: Vec<usize>,
}

/// Render an issue's fields, ADF body (description, acceptance criteria),
/// transitions strip, and comments. Shared by the full Detail screen and the
/// inline quick-view panel so both show the same content.
pub fn issue_detail_lines(detail: &IssueDetail) -> IssueLines {
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

    // Comments — rendered ADF body per comment, oldest first.
    let mut comments_header = None;
    let mut comment_starts = Vec::with_capacity(detail.comments.len());
    if !detail.comments.is_empty() {
        lines.push(divider());
        comments_header = Some(lines.len());
        lines.push(Line::from(Span::styled(
            format!(
                "💬 {} comment{}",
                detail.comments.len(),
                if detail.comments.len() == 1 { "" } else { "s" }
            ),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        for comment in &detail.comments {
            lines.push(Line::default());
            comment_starts.push(lines.len());
            lines.push(Line::from(vec![
                Span::styled(
                    comment.author.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {}", comment.created),
                    Style::default().fg(MUTED),
                ),
            ]));
            lines.extend(adf::render(&comment.body).lines);
        }
    }

    IssueLines {
        lines,
        comments_header,
        comment_starts,
    }
}
