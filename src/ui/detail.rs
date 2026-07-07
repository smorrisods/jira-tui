//! The full issue Detail screen and the shared field/ADF-body renderer used
//! by both Detail and the quick-view panel.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::adf;
use crate::app::App;
use crate::domain::IssueDetail;

use super::{
    card, chip, divider, priority_colour, priority_glyph, priority_style, status_colour, truncate,
    ACCENT, ACCENT2, DANGER, MUTED,
};

pub(crate) fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
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

    let lines = issue_detail_lines(detail);
    let para = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, inner);
}

/// Render an issue's fields and ADF body (identity, metadata, description,
/// acceptance criteria, transitions strip). Shared by the full Detail screen
/// and the inline quick-view panel so both show the same content.
pub(crate) fn issue_detail_lines(detail: &IssueDetail) -> Vec<Line<'static>> {
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

    lines
}
