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

/// Something a navigable span in the rendered detail points to — either
/// another Jira issue (by key) or a bare URL. Found by scanning the
/// rendered text after the fact (see `linkify`), rather than threading ADF
/// link-mark hrefs through the renderer: the common case in Jira issues is
/// a pasted URL whose display text *is* the link, and the same scan also
/// picks up bare issue-key mentions anywhere in the body, not just in the
/// dedicated parent/links fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Issue(String),
    Url(String),
}

/// A single navigable span within `IssueLines::lines`: which line, and
/// which char range within that line's flattened text (i.e. the
/// concatenation of all its spans' content).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTarget {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub kind: LinkKind,
}

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
    /// Every navigable issue key / URL found in `lines`, in reading order —
    /// powers `Tab`/`Shift+Tab` cycling and `Enter`-to-open in the Detail
    /// screen and quick-view panel.
    pub links: Vec<LinkTarget>,
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
        // Jira's `parent` field is generic — an Epic for stories/tasks, but
        // a Story/Task for sub-tasks — so "parent" rather than "epic" here.
        lines.push(Line::from(Span::styled(
            format!("parent: {parent}"),
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
    for child in &detail.children {
        // Jira has no single "children" field — an Epic's child stories and
        // a Story/Task's sub-tasks are both just "child" here (see the
        // `parent` comment above for the matching asymmetry).
        lines.push(Line::from(vec![
            Span::styled("child ", Style::default().fg(ACCENT2)),
            Span::styled(child.key.clone(), Style::default().fg(ACCENT)),
            Span::raw(" "),
            chip(&child.issue_type, ACCENT2),
            Span::raw(" "),
            chip(&child.status, status_colour(&child.status)),
            Span::styled(
                format!(" · {}", truncate(&child.summary, 40)),
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

    let links = linkify(&mut lines);

    IssueLines {
        lines,
        comments_header,
        comment_starts,
        links,
    }
}

/// Give one navigable target a reversed-video highlight (the same idiom
/// `ui::draw` uses for the mouse drag-selection range) so the currently
/// `Tab`-cycled link stands out from the rest, which only carry the plain
/// underline `linkify` applied.
pub fn highlight_target(lines: &mut [Line<'static>], target: &LinkTarget) {
    if let Some(line) = lines.get_mut(target.line) {
        let owned = std::mem::take(line);
        *line = restyle_range(
            owned,
            target.start,
            target.end,
            Style::default().add_modifier(Modifier::REVERSED),
        );
    }
}

/// Patch every span covering `[start, end)` chars of `line`'s flattened
/// text with `extra`, splitting spans at the range boundary as needed.
fn restyle_range(line: Line<'static>, start: usize, end: usize, extra: Style) -> Line<'static> {
    let mut chars: Vec<char> = Vec::new();
    let mut owner: Vec<usize> = Vec::new();
    for (si, span) in line.spans.iter().enumerate() {
        for c in span.content.chars() {
            chars.push(c);
            owner.push(si);
        }
    }
    let mut new_spans: Vec<Span<'static>> = Vec::new();
    let mut idx = 0usize;
    while idx < chars.len() {
        let span_idx = owner[idx];
        let matched = idx >= start && idx < end;
        let s0 = idx;
        while idx < chars.len() && owner[idx] == span_idx && (idx >= start && idx < end) == matched
        {
            idx += 1;
        }
        let run: String = chars[s0..idx].iter().collect();
        let mut style = line.spans[span_idx].style;
        if matched {
            style = style.patch(extra);
        }
        new_spans.push(Span::styled(run, style));
    }
    Line::from(new_spans)
}

/// Scan every line for issue keys and bare URLs, restyling matched spans
/// with an underline (in addition to whatever colour/weight they already
/// carry) and returning their locations for keyboard navigation. Mutates
/// `lines` in place.
fn linkify(lines: &mut [Line<'static>]) -> Vec<LinkTarget> {
    let mut targets = Vec::new();
    for (i, line) in lines.iter_mut().enumerate() {
        let owned = std::mem::take(line);
        let (restyled, found) = linkify_line(owned);
        *line = restyled;
        for (start, end, kind) in found {
            targets.push(LinkTarget {
                line: i,
                start,
                end,
                kind,
            });
        }
    }
    targets
}

/// Restyle one line's matched spans and return the char ranges/kinds found,
/// relative to that line's flattened text.
fn linkify_line(line: Line<'static>) -> (Line<'static>, Vec<(usize, usize, LinkKind)>) {
    let mut chars: Vec<char> = Vec::new();
    let mut owner: Vec<usize> = Vec::new();
    for (si, span) in line.spans.iter().enumerate() {
        for c in span.content.chars() {
            chars.push(c);
            owner.push(si);
        }
    }
    let matches = find_link_matches(&chars);
    if matches.is_empty() {
        return (line, Vec::new());
    }

    let in_match = |idx: usize| matches.iter().any(|(s, e, _)| idx >= *s && idx < *e);
    let mut new_spans: Vec<Span<'static>> = Vec::new();
    let mut idx = 0usize;
    while idx < chars.len() {
        let span_idx = owner[idx];
        let matched = in_match(idx);
        let start = idx;
        while idx < chars.len() && owner[idx] == span_idx && in_match(idx) == matched {
            idx += 1;
        }
        let run: String = chars[start..idx].iter().collect();
        let mut style = line.spans[span_idx].style;
        if matched {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        new_spans.push(Span::styled(run, style));
    }
    (Line::from(new_spans), matches)
}

/// Find every issue-key/URL match in a line's flattened chars, in order,
/// non-overlapping.
fn find_link_matches(chars: &[char]) -> Vec<(usize, usize, LinkKind)> {
    let mut out = Vec::new();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if let Some(end) = match_url(chars, i) {
            let text: String = chars[i..end].iter().collect();
            out.push((i, end, LinkKind::Url(text)));
            i = end;
            continue;
        }
        let boundary_ok = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
        if boundary_ok {
            if let Some((end, key)) = match_issue_key(chars, i) {
                out.push((i, end, LinkKind::Issue(key)));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Match a `http://`/`https://` URL starting at `i`, trimming trailing
/// punctuation that's more likely to be prose than part of the link (a
/// period ending a sentence, a closing paren, etc).
fn match_url(chars: &[char], i: usize) -> Option<usize> {
    const PREFIXES: [&str; 2] = ["https://", "http://"];
    let prefix_len = PREFIXES.iter().find_map(|p| {
        let plen = p.chars().count();
        (i + plen <= chars.len() && chars[i..i + plen].iter().collect::<String>() == *p)
            .then_some(plen)
    })?;
    let mut end = i + prefix_len;
    while end < chars.len() && !chars[end].is_whitespace() {
        end += 1;
    }
    while end > i + prefix_len
        && matches!(
            chars[end - 1],
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '>' | '\'' | '"'
        )
    {
        end -= 1;
    }
    (end > i + prefix_len).then_some(end)
}

/// Match a Jira-style issue key (`DS-123`) starting at `i`, mirroring
/// `git::parse_issue_key`'s shape rules but anchored at a fixed start and
/// with a trailing word-boundary check (so `DS-123abc` isn't matched).
fn match_issue_key(chars: &[char], i: usize) -> Option<(usize, String)> {
    if !chars.get(i)?.is_ascii_uppercase() {
        return None;
    }
    let mut j = i;
    while j < chars.len() && chars[j].is_ascii_uppercase() {
        j += 1;
    }
    if !(2..=10).contains(&(j - i)) || chars.get(j) != Some(&'-') {
        return None;
    }
    let dstart = j + 1;
    let mut k = dstart;
    while k < chars.len() && chars[k].is_ascii_digit() {
        k += 1;
    }
    if k == dstart {
        return None;
    }
    if chars.get(k).is_some_and(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((k, chars[i..k].iter().collect()))
}

#[cfg(test)]
mod link_tests {
    use super::*;
    use crate::domain::{demo_detail, demo_issues};

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn matches_issue_keys_with_word_boundaries() {
        let c = chars("see DS-123 and DS-4-old and XDS-123abc done");
        let m = find_link_matches(&c);
        let keys: Vec<&LinkKind> = m.iter().map(|(_, _, k)| k).collect();
        assert!(matches!(keys[0], LinkKind::Issue(k) if k == "DS-123"));
        // DS-4-old: "DS-4" matches, trailing "-old" is separate text.
        assert!(matches!(keys[1], LinkKind::Issue(k) if k == "DS-4"));
        // XDS-123abc: the alnum suffix means no match at all for that token
        // (word-boundary check on both ends).
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn matches_urls_and_trims_trailing_punctuation() {
        let c = chars("see (https://example.com/foo) and https://x.io/y.");
        let m = find_link_matches(&c);
        let urls: Vec<String> = m
            .iter()
            .filter_map(|(_, _, k)| match k {
                LinkKind::Url(u) => Some(u.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(urls, vec!["https://example.com/foo", "https://x.io/y"]);
    }

    #[test]
    fn issue_detail_lines_finds_parent_and_link_and_body_keys() {
        let detail = demo_detail(&demo_issues()[1].key);
        let rendered = issue_detail_lines(&detail);
        assert!(!rendered.links.is_empty());
        // Every recorded target actually points at text within its line's
        // bounds.
        for target in &rendered.links {
            let line = &rendered.lines[target.line];
            let len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(target.end <= len);
            assert!(target.start < target.end);
        }
    }
}
