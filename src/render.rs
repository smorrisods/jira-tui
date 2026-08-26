//! Issue detail rendering: builds the `Line` content shared by the
//! quick-view panel, the full Detail screen's wide two-column layout, and
//! its narrow single-column layout, plus the line offsets `app` needs to
//! jump the scroll position to the comments section or step between
//! individual comments.
//!
//! The body is built from small section-builder functions (`identity_lines`,
//! `meta_lines`, `workflow_lines`, `links_lines`, `children_lines`,
//! `attachments_lines`, `description_lines`, `activity_lines_cards`, plus
//! quick view's own
//! `quick_view_chip_line`/`quick_view_kv_fields`/`quick_view_meta_lines`/
//! `quick_view_inline_kv_line`), each producing a self-contained `Vec<Line>`.
//! The composers below concatenate them in different orders/arrangements
//! without ever slicing a shared vec, so
//! `comments_header`/`comment_starts`/`LinkTarget.line` always stay absolute
//! indices into whatever `Vec<Line>` is actually being displayed:
//!
//! - `quick_view_wide`/`quick_view_narrow` — the quick-view panel's split
//!   layouts (SPEC.md §4): a description excerpt plus a compact meta-field
//!   grid (assignee/parent/labels/updated, with type/status/priority shown
//!   as chips), each independently linkified — a narrower field set than
//!   Detail's own meta/facts panels (no reporter/components), and no
//!   workflow/activity sections at all.
//! - `wide_detail` — the Detail screen's wide layout (SPEC.md §6): identity
//!   + a scrollable `main` column (description + activity) plus five static
//!   side-rail panels (workflow/meta/links/children/attachments), each
//!   independently linkified and tagged with the `DetailPane` they live in
//!   so `{`/`}` cycling can reach every link regardless of which pane it's
//!   in.
//! - `narrow_detail` — the Detail screen's narrow layout: identity → facts
//!   panel (foldable) → description → linked (links+children merged) →
//!   activity.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::adf;
use crate::domain::{Comment, IssueDetail};
use crate::ui::{
    accent, accent2, chip, danger, divider, maple, muted, ok, priority_colour, priority_glyph,
    priority_style, status_colour, truncate, type_colour, workflow_chip,
};

/// Which Detail-screen pane a navigable link lives in. Quick view and the
/// narrow Detail layout only ever use `Main` (there's one scrollable
/// document); the wide layout's five side-rail panels get their own tags so
/// mouse hit-testing can stay scoped to `Main` while keyboard `{`/`}`
/// cycling still reaches every pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailPane {
    /// The wide layout's identity line (key/summary/chips) — a distinct
    /// pane from `Main` even though both render above/below each other in
    /// the same column, since each restarts its own line numbering from 0
    /// and a target's `line` is only unambiguous within its own pane.
    Identity,
    Main,
    Workflow,
    Meta,
    Links,
    Children,
    Attachments,
}

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

/// A single navigable span within a rendered `Vec<Line>`: which line, and
/// which char range within that line's flattened text (i.e. the
/// concatenation of all its spans' content).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTarget {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub kind: LinkKind,
    pub pane: DetailPane,
}

/// A rendered document plus line offsets into it for comment-navigation
/// keybindings (`]`/`[`, `n`/`p`).
pub struct IssueLines {
    pub lines: Vec<Line<'static>>,
    /// Line index of the "💬 N comments" section header, if there are any
    /// comments.
    pub comments_header: Option<usize>,
    /// Line index of each individual comment's "author · created" header,
    /// in display order.
    pub comment_starts: Vec<usize>,
    /// Every navigable issue key/URL found in `lines`, in reading order —
    /// powers `Tab`/`Shift+Tab` cycling and `Enter`-to-open.
    pub links: Vec<LinkTarget>,
}

/// One Wide-layout side-rail panel: its own lines plus its own navigable
/// links, linkified independently of every other panel.
pub struct Panel {
    pub lines: Vec<Line<'static>>,
    pub links: Vec<LinkTarget>,
}

/// The Detail screen's wide (≥ ~90 cols) layout: a scrollable main column
/// (description + activity) beside a static five-panel side rail.
pub struct WideDetail {
    pub identity: Panel,
    pub main: IssueLines,
    pub workflow: Panel,
    pub meta: Panel,
    pub links: Panel,
    pub children: Panel,
    pub attachments: Panel,
}

/// The Detail screen's narrow (< ~90 cols) layout: one scrollable document,
/// identity → facts → description → linked → activity.
pub struct NarrowDetail {
    pub lines: IssueLines,
}

fn identity_lines(detail: &IssueDetail) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
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
        ]),
        Line::from(vec![
            chip(&detail.issue_type, type_colour(&detail.issue_type)),
            Span::raw(" "),
            chip(&detail.status, status_colour(&detail.status)),
            Span::raw(" "),
            chip(detail.priority.label(), priority_colour(&detail.priority)),
        ]),
    ]
}

/// The "sprint" fact's value, shared by the meta panel (`meta_lines`) and
/// the narrow layout's facts grid (`facts_pairs`) — `None` when Sprint isn't
/// tracked on this instance at all (`sprint_configured` false, see
/// `App::sprint_field_configured`), so the row is omitted entirely rather
/// than showing a possibly-misleading "none" for an instance that was never
/// asked about Sprint in the first place. `Some("none")` when it *is*
/// tracked but this particular issue has no current sprint (e.g. it's in
/// the backlog) — a real, displayable fact, not an absence of one.
fn sprint_fact(detail: &IssueDetail, sprint_configured: bool) -> Option<String> {
    if !sprint_configured {
        return None;
    }
    Some(match &detail.sprint {
        Some(s) if s.state == "active" => s.name.clone(),
        Some(s) => format!("{} ({})", s.name, s.state),
        None => "none".to_string(),
    })
}

/// Assignee/reporter/parent/component/labels, one per line, plus a trailing
/// "updated" line.
fn meta_lines(detail: &IssueDetail, updated: &str, sprint_configured: bool) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "assignee: {}",
            detail
                .assignee
                .clone()
                .unwrap_or_else(|| "unassigned".into())
        ),
        Style::default().fg(muted()),
    ))];
    if let Some(reporter) = &detail.reporter {
        lines.push(Line::from(Span::styled(
            format!("reporter: {reporter}"),
            Style::default().fg(muted()),
        )));
    }
    if let Some(parent) = &detail.parent {
        // Jira's `parent` field is generic — an Epic for stories/tasks, but
        // a Story/Task for sub-tasks — so "parent" rather than "epic" here.
        lines.push(Line::from(Span::styled(
            format!("parent: {parent}"),
            Style::default().fg(muted()),
        )));
    }
    if !detail.components.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("components: {}", detail.components.join(", ")),
            Style::default().fg(muted()),
        )));
    }
    if !detail.labels.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("labels: {}", detail.labels.join(", ")),
            Style::default().fg(muted()),
        )));
    }
    if !detail.fix_versions.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("fix version(s): {}", detail.fix_versions.join(", ")),
            Style::default().fg(muted()),
        )));
    }
    if !detail.affects_versions.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("affects version(s): {}", detail.affects_versions.join(", ")),
            Style::default().fg(muted()),
        )));
    }
    if let Some(sprint) = sprint_fact(detail, sprint_configured) {
        lines.push(Line::from(Span::styled(
            format!("sprint: {sprint}"),
            Style::default().fg(muted()),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!("updated: {updated}"),
        Style::default().fg(muted()),
    )));
    lines
}

/// The facts panel's kv pairs, in display order — shared by `facts_kv_lines`
/// (two per row) and `folded_facts_line` (one compact summary).
fn facts_pairs(
    detail: &IssueDetail,
    updated: &str,
    sprint_configured: bool,
) -> Vec<(&'static str, String)> {
    let mut pairs = vec![(
        "assignee",
        detail
            .assignee
            .clone()
            .unwrap_or_else(|| "unassigned".into()),
    )];
    if let Some(reporter) = &detail.reporter {
        pairs.push(("reporter", reporter.clone()));
    }
    if let Some(parent) = &detail.parent {
        pairs.push(("parent", parent.clone()));
    }
    if !detail.components.is_empty() {
        pairs.push(("components", detail.components.join(", ")));
    }
    if !detail.labels.is_empty() {
        pairs.push(("labels", detail.labels.join(", ")));
    }
    if !detail.fix_versions.is_empty() {
        pairs.push(("fix version(s)", detail.fix_versions.join(", ")));
    }
    if !detail.affects_versions.is_empty() {
        pairs.push(("affects version(s)", detail.affects_versions.join(", ")));
    }
    if let Some(sprint) = sprint_fact(detail, sprint_configured) {
        pairs.push(("sprint", sprint));
    }
    pairs.push(("updated", updated.to_string()));
    pairs
}

/// Narrow Detail's facts panel body (SPEC.md §6): "two-pairs-per-row kv
/// grid".
fn facts_kv_lines(
    detail: &IssueDetail,
    updated: &str,
    sprint_configured: bool,
) -> Vec<Line<'static>> {
    facts_pairs(detail, updated, sprint_configured)
        .chunks(2)
        .map(|chunk| {
            let mut spans = Vec::new();
            for (i, (label, value)) in chunk.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw("   "));
                }
                spans.push(Span::styled(
                    format!("{label}: "),
                    Style::default().fg(muted()),
                ));
                spans.push(Span::styled(
                    value.clone(),
                    Style::default().fg(Color::White),
                ));
            }
            Line::from(spans)
        })
        .collect()
}

/// Narrow Detail's facts panel title, naming the `x` fold/unfold key.
fn facts_panel_title(facts_folded: bool) -> Line<'static> {
    let hint = if facts_folded { "unfold" } else { "fold" };
    Line::from(Span::styled(
        format!("facts · x to {hint}"),
        Style::default().fg(accent()).add_modifier(Modifier::BOLD),
    ))
}

/// The facts panel folded to one line (`x` key, narrow Detail only).
fn folded_facts_line(detail: &IssueDetail, updated: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            "{} · {} · updated {updated}",
            detail
                .assignee
                .clone()
                .unwrap_or_else(|| "unassigned".into()),
            detail.status,
        ),
        Style::default().fg(muted()),
    ))
}

/// The workflow/transition strip: each status as a chip, the current one
/// solid+bold (SPEC.md §6). Shared by the legacy flat document, the wide
/// layout's workflow rail panel, and the narrow layout's facts panel.
fn workflow_lines(detail: &IssueDetail) -> Vec<Line<'static>> {
    if detail.transitions.is_empty() {
        return Vec::new();
    }
    let mut strip = Vec::new();
    for (i, t) in detail.transitions.iter().enumerate() {
        let current = t.to == detail.status || t.name == detail.status;
        strip.push(workflow_chip(&t.name, current));
        if i + 1 < detail.transitions.len() {
            strip.push(Span::styled(" → ", Style::default().fg(muted())));
        }
    }
    vec![
        Line::from(strip),
        Line::from(Span::styled(
            "t to change",
            Style::default().fg(muted()).add_modifier(Modifier::ITALIC),
        )),
    ]
}

/// "relates to" reads fern/`ok()`; every other relation (blocks, is blocked
/// by, duplicates, ...) reads ember/`danger()` (SPEC.md §6/§7).
fn relation_colour(relation: &str) -> Color {
    if relation.contains("relates") {
        ok()
    } else {
        danger()
    }
}

fn links_lines(detail: &IssueDetail) -> Vec<Line<'static>> {
    detail
        .links
        .iter()
        .map(|link| {
            Line::from(vec![
                Span::styled(
                    format!("{} ", link.relation),
                    Style::default().fg(relation_colour(&link.relation)),
                ),
                Span::styled(link.key.clone(), Style::default().fg(accent())),
                Span::styled(
                    format!(" · {}", truncate(&link.summary, 40)),
                    Style::default().fg(muted()),
                ),
            ])
        })
        .collect()
}

fn children_lines(detail: &IssueDetail) -> Vec<Line<'static>> {
    detail
        .children
        .iter()
        .map(|child| {
            // Jira has no single "children" field — an Epic's child stories
            // and a Story/Task's sub-tasks are both just "child" here (see
            // `meta_lines`'s "parent" comment for the matching asymmetry).
            Line::from(vec![
                Span::styled("child ", Style::default().fg(accent2())),
                Span::styled(child.key.clone(), Style::default().fg(accent())),
                Span::raw(" "),
                chip(&child.issue_type, type_colour(&child.issue_type)),
                Span::raw(" "),
                chip(&child.status, status_colour(&child.status)),
                Span::styled(
                    format!(" · {}", truncate(&child.summary, 40)),
                    Style::default().fg(muted()),
                ),
            ])
        })
        .collect()
}

fn attachments_lines(detail: &IssueDetail) -> Vec<Line<'static>> {
    detail
        .attachments
        .iter()
        .map(|a| {
            Line::from(vec![
                Span::styled(a.filename.clone(), Style::default().fg(accent())),
                Span::styled(
                    format!(" · {} · {}", human_size(a.size), a.mime_type),
                    Style::default().fg(muted()),
                ),
            ])
        })
        .collect()
}

/// Human-readable file size (base-1024, matching common OS conventions):
/// bytes below 1 KiB as a bare "N B", otherwise "N KB"/"N MB"/... with one
/// decimal place, dropped when it would just be ".0" (so an exact "240 KB"
/// reads as such, not "240.0 KB").
pub(crate) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut size = bytes as f64 / 1024.0;
    let mut unit = 0usize;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    // Rounding for display can itself cross a unit boundary (e.g. 1023.98 KB
    // rounds to "1024 KB" instead of promoting to "1 MB") — the loop above
    // only promotes on the *unrounded* value, so check again after it.
    if size.round() >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if (size - size.round()).abs() < 0.05 {
        format!("{:.0} {}", size, UNITS[unit])
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

/// Narrow Detail's attachments section title, matching `linked_panel_title`'s
/// "only rendered when non-empty" shape — the count is always ≥ 1 wherever
/// this is actually used.
fn attachments_panel_title(detail: &IssueDetail) -> Line<'static> {
    Line::from(Span::styled(
        format!("attachments · {}", detail.attachments.len()),
        Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
    ))
}

/// Narrow Detail's "linked" panel (SPEC.md §6): links and children merged
/// into one list.
fn linked_lines(detail: &IssueDetail) -> Vec<Line<'static>> {
    let mut lines = links_lines(detail);
    lines.extend(children_lines(detail));
    lines
}

fn linked_panel_title(detail: &IssueDetail) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            "linked · {} link{} · {} child{}",
            detail.links.len(),
            if detail.links.len() == 1 { "" } else { "s" },
            detail.children.len(),
            if detail.children.len() == 1 {
                ""
            } else {
                "ren"
            },
        ),
        Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
    ))
}

/// `width` is the column width this description will actually be wrapped
/// to (see `adf::render`'s doc comment) — needed so a blockquote/code-block
/// bar can be pre-wrapped to survive line wrapping instead of vanishing on
/// continuation rows.
pub(crate) fn description_lines(detail: &IssueDetail, width: usize) -> Vec<Line<'static>> {
    let mut lines = adf::render(&detail.description, width).lines;
    if let Some(ac) = &detail.acceptance_criteria {
        lines.push(divider());
        lines.push(Line::from(Span::styled(
            "Acceptance Criteria",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        )));
        lines.extend(adf::render(ac, width).lines);
    }
    lines
}

/// The wide/narrow layouts' comment-card rendering (SPEC.md §6): a 2-cell
/// left rule — maple for the current user's own comments, orchid otherwise
/// — ahead of the author/timestamp header and every ADF body line, each
/// pre-wrapped to `width` (via `wrap_with_bar`) so the rule survives a long
/// line wrapping instead of only showing on its first on-screen row. Same
/// section-local-offset contract as `activity_lines_plain`.
fn activity_lines_cards(
    comments: &[Comment],
    current_user: &str,
    width: usize,
) -> (Vec<Line<'static>>, Option<usize>, Vec<usize>) {
    let mut lines = Vec::new();
    let mut header = None;
    let mut starts = Vec::with_capacity(comments.len());
    if !comments.is_empty() {
        header = Some(lines.len());
        lines.push(Line::from(Span::styled(
            format!(
                "💬 {} comment{} · n/p to jump",
                comments.len(),
                if comments.len() == 1 { "" } else { "s" }
            ),
            Style::default().fg(accent2()).add_modifier(Modifier::BOLD),
        )));
        for comment in comments {
            let rule = if comment.author == current_user {
                maple()
            } else {
                accent2()
            };
            lines.push(Line::default());
            starts.push(lines.len());
            let header_line = Line::from(vec![
                Span::styled(
                    comment.author.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {}", comment.created),
                    Style::default().fg(muted()),
                ),
            ]);
            lines.extend(wrap_with_bar(&header_line, width, "▌ ", rule));
            // Nested content (e.g. a blockquote inside the comment) is
            // rendered at `width` minus this card's own 2-column bar, so
            // any *inner* bar it adds is sized against the space actually
            // left over rather than the full column width.
            let body_width = width.saturating_sub(2).max(1);
            for body_line in adf::render(&comment.body, body_width).lines {
                lines.extend(wrap_with_bar(&body_line, width, "▌ ", rule));
            }
        }
    }
    (lines, header, starts)
}

/// Quick view's compact kv fields (SPEC.md §4): assignee, parent (if any),
/// labels (if any), fix version(s) (if any), updated — a narrower set than
/// `facts_pairs`' (no reporter/components/affects versions), since quick
/// view aims for a compact excerpt rather than the full field list. Fix
/// version(s) earns a spot in that excerpt anyway (unlike affects versions)
/// since quick view is one of the two places `R` opens the version picker
/// from — worth being visible at a glance, not just editable blind. Type/
/// status/priority are shown separately, as chips (`quick_view_chip_line`),
/// not in this kv list.
fn quick_view_kv_fields(detail: &IssueDetail, updated: &str) -> Vec<(&'static str, String)> {
    let mut pairs = vec![(
        "assignee",
        detail
            .assignee
            .clone()
            .unwrap_or_else(|| "unassigned".into()),
    )];
    if let Some(parent) = &detail.parent {
        pairs.push(("parent", parent.clone()));
    }
    if !detail.labels.is_empty() {
        pairs.push(("labels", detail.labels.join(", ")));
    }
    if !detail.fix_versions.is_empty() {
        pairs.push(("fix version(s)", detail.fix_versions.join(", ")));
    }
    pairs.push(("updated", updated.to_string()));
    pairs
}

fn quick_view_chip_line(detail: &IssueDetail) -> Line<'static> {
    Line::from(vec![
        chip(&detail.issue_type, type_colour(&detail.issue_type)),
        Span::raw(" "),
        chip(&detail.status, status_colour(&detail.status)),
        Span::raw(" "),
        chip(detail.priority.label(), priority_colour(&detail.priority)),
    ])
}

/// Quick view's wide-layout meta panel body: the chips line, then one
/// `label: value` row per kv field — the same "label: value" idiom
/// `meta_lines`/`facts_kv_lines` already use, rather than the mockup's
/// literal two-column `dl`, since that's this app's established
/// terminal-rendering convention for field lists.
fn quick_view_meta_lines(detail: &IssueDetail, updated: &str) -> Vec<Line<'static>> {
    let mut lines = vec![quick_view_chip_line(detail)];
    for (label, value) in quick_view_kv_fields(detail, updated) {
        lines.push(Line::from(vec![
            Span::styled(format!("{label}: "), Style::default().fg(muted())),
            Span::styled(value, Style::default().fg(Color::White)),
        ]));
    }
    lines
}

/// One `label value` pair per kv field, packed onto a single flowing
/// `Line` — the narrow layout's "wrapping flex" requirement (SPEC.md §4)
/// needs no dedicated wrap primitive: packing every pair's spans onto one
/// logical `Line` and letting `Paragraph::wrap(Wrap { trim: false })`
/// reflow it at render time reuses the same mechanism every other screen
/// already relies on for word-wrap.
fn quick_view_inline_kv_line(detail: &IssueDetail, updated: &str) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (label, value)) in quick_view_kv_fields(detail, updated)
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(
            format!("{label}: "),
            Style::default().fg(muted()),
        ));
        spans.push(Span::styled(value, Style::default().fg(Color::White)));
    }
    Line::from(spans)
}

fn linkify_panel(mut lines: Vec<Line<'static>>, pane: DetailPane) -> Panel {
    let links = linkify(&mut lines, pane);
    Panel { lines, links }
}

/// The Detail screen's wide layout (SPEC.md §6). `main` is the scrollable
/// column (description + activity); the other five fields are the static
/// side rail, each independently linkified and pane-tagged so `{`/`}`
/// cycling reaches every one of them even though only `main` gets mouse
/// hit-testing (see `app::mouse::link_at`). `main_width` is the actual
/// column width `main` will be rendered/wrapped at — see `adf::render`'s
/// doc comment for why the description/comment bars need it up front.
pub fn wide_detail(
    detail: &IssueDetail,
    current_user: &str,
    updated: &str,
    sprint_configured: bool,
    main_width: usize,
) -> WideDetail {
    let identity = linkify_panel(identity_lines(detail), DetailPane::Identity);
    let workflow = linkify_panel(workflow_lines(detail), DetailPane::Workflow);
    let meta = linkify_panel(
        meta_lines(detail, updated, sprint_configured),
        DetailPane::Meta,
    );
    let links = linkify_panel(links_lines(detail), DetailPane::Links);
    let children = linkify_panel(children_lines(detail), DetailPane::Children);
    let attachments = linkify_panel(attachments_lines(detail), DetailPane::Attachments);

    let mut main_lines = description_lines(detail, main_width);
    let (activity, header, starts) =
        activity_lines_cards(&detail.comments, current_user, main_width);
    if !activity.is_empty() {
        main_lines.push(divider());
    }
    let base = main_lines.len();
    main_lines.extend(activity);
    let comments_header = header.map(|h| h + base);
    let comment_starts = starts.into_iter().map(|s| s + base).collect();
    let main_links = linkify(&mut main_lines, DetailPane::Main);

    WideDetail {
        identity,
        main: IssueLines {
            lines: main_lines,
            comments_header,
            comment_starts,
            links: main_links,
        },
        workflow,
        meta,
        links,
        children,
        attachments,
    }
}

/// The wide layout's link-cycling order: identity, then `main` (description
/// and activity, reading all the way down the primary column), then the
/// side rail top-to-bottom (workflow, meta, links, children, attachments) —
/// so `{`/`}` cycling reaches every link in the layout, not just `main`'s.
/// Shared by `app::links::active_links` and `ui::detail`'s highlight logic
/// so both agree on what `link_index` N actually refers to.
pub fn wide_detail_links(wide: &WideDetail) -> Vec<LinkTarget> {
    wide.identity
        .links
        .iter()
        .chain(wide.main.links.iter())
        .chain(wide.workflow.links.iter())
        .chain(wide.meta.links.iter())
        .chain(wide.links.links.iter())
        .chain(wide.children.links.iter())
        .chain(wide.attachments.links.iter())
        .cloned()
        .collect()
}

/// The Detail screen's narrow layout (SPEC.md §6): identity → facts panel
/// (foldable) → description → linked → activity, all one scrollable
/// document (same single-pane model as `issue_detail_lines`). `width` is
/// the actual column width this single pane will be rendered/wrapped at —
/// see `wide_detail`'s `main_width` doc comment.
pub fn narrow_detail(
    detail: &IssueDetail,
    current_user: &str,
    updated: &str,
    facts_folded: bool,
    sprint_configured: bool,
    width: usize,
) -> NarrowDetail {
    let mut lines = identity_lines(detail);
    lines.push(facts_panel_title(facts_folded));
    if facts_folded {
        lines.push(folded_facts_line(detail, updated));
    } else {
        lines.extend(facts_kv_lines(detail, updated, sprint_configured));
        lines.extend(workflow_lines(detail));
    }
    lines.push(divider());
    lines.extend(description_lines(detail, width));

    let linked = linked_lines(detail);
    if !linked.is_empty() {
        lines.push(divider());
        lines.push(linked_panel_title(detail));
        lines.extend(linked);
    }

    let attachments = attachments_lines(detail);
    if !attachments.is_empty() {
        lines.push(divider());
        lines.push(attachments_panel_title(detail));
        lines.extend(attachments);
    }

    let (activity, header, starts) = activity_lines_cards(&detail.comments, current_user, width);
    if !activity.is_empty() {
        lines.push(divider());
    }
    let base = lines.len();
    lines.extend(activity);
    let comments_header = header.map(|h| h + base);
    let comment_starts = starts.into_iter().map(|s| s + base).collect();

    let links = linkify(&mut lines, DetailPane::Main);
    NarrowDetail {
        lines: IssueLines {
            lines,
            comments_header,
            comment_starts,
            links,
        },
    }
}

/// The quick-view panel's wide layout (SPEC.md §4): a description excerpt
/// panel beside a compact meta grid — reuses `DetailPane::Main`/`Meta` for
/// pane-tagging (quick view's shape is close enough to Detail's own
/// main/meta split that new variants aren't needed) so `{`/`}` cycling and
/// highlighting work the same way Detail's side rail does.
pub struct QuickViewWide {
    pub description: Panel,
    pub meta: Panel,
}

pub fn quick_view_wide(
    detail: &IssueDetail,
    updated: &str,
    description_width: usize,
) -> QuickViewWide {
    QuickViewWide {
        description: linkify_panel(
            description_lines(detail, description_width),
            DetailPane::Main,
        ),
        meta: linkify_panel(quick_view_meta_lines(detail, updated), DetailPane::Meta),
    }
}

/// Reading order for `{`/`}` link cycling in the wide layout: description
/// first, then the meta grid — mirrors `wide_detail_links`' shape.
pub fn quick_view_wide_links(wide: &QuickViewWide) -> Vec<LinkTarget> {
    wide.description
        .links
        .iter()
        .chain(wide.meta.links.iter())
        .cloned()
        .collect()
}

/// The quick-view panel's narrow layout (SPEC.md §4): chips line, kv fields
/// packed onto one flowing/wrapping line, then the description excerpt —
/// all one scrollable document (same single-pane model as `narrow_detail`).
/// No workflow/activity sections — quick view shows neither, so this is a
/// plain `Panel` rather than `IssueLines` (whose `comments_header`/
/// `comment_starts` would always be empty here).
pub struct QuickViewNarrow {
    pub panel: Panel,
}

pub fn quick_view_narrow(detail: &IssueDetail, updated: &str, width: usize) -> QuickViewNarrow {
    let mut lines = vec![
        quick_view_chip_line(detail),
        Line::default(),
        quick_view_inline_kv_line(detail, updated),
        Line::default(),
    ];
    lines.extend(description_lines(detail, width));
    let links = linkify(&mut lines, DetailPane::Main);
    QuickViewNarrow {
        panel: Panel { lines, links },
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

/// Extract the character range `range` (into `line`'s flattened text, same
/// space `restyle_range`/`wrapped_row_ranges` use) as a standalone `Line`,
/// preserving each character's original span style. The building block
/// `wrap_with_bar` uses to turn one logical (pre-wrap) `Line` into several
/// physical rows that each still carry their own copy of a persistent
/// left-margin bar; `adf::render_table` uses it the same way to wrap a
/// table cell's content into per-column sub-rows.
pub(crate) fn slice_line(line: &Line<'static>, range: std::ops::Range<usize>) -> Line<'static> {
    let mut chars: Vec<char> = Vec::new();
    let mut owner: Vec<usize> = Vec::new();
    for (si, span) in line.spans.iter().enumerate() {
        for c in span.content.chars() {
            chars.push(c);
            owner.push(si);
        }
    }
    let end = range.end.min(chars.len());
    let start = range.start.min(end);
    let mut new_spans: Vec<Span<'static>> = Vec::new();
    let mut idx = start;
    while idx < end {
        let span_idx = owner[idx];
        let s0 = idx;
        while idx < end && owner[idx] == span_idx {
            idx += 1;
        }
        let run: String = chars[s0..idx].iter().collect();
        new_spans.push(Span::styled(run, line.spans[span_idx].style));
    }
    Line::from(new_spans)
}

/// Word-wrap `line` to `width` columns (matching ratatui's own
/// `Wrap { trim: false }`, via `wrapped_row_ranges`) and prepend
/// `bar_glyph` (styled `bar_colour`) to *every* resulting row, not just the
/// first. `Paragraph`'s automatic wrap only re-flows text — it has no
/// notion of a prefix that should repeat on every wrapped visual row, so a
/// `"┃ "`/`"▌ "` span added once to a long logical `Line` only shows up on
/// that line's first on-screen row, leaving continuation rows with no bar.
/// Used by ADF blockquote/code-block rendering (`adf::render`) and the
/// comment-card left rule (`activity_lines_cards`) — the two places that
/// need a bar to survive a line wrapping.
pub(crate) fn wrap_with_bar(
    line: &Line<'static>,
    width: usize,
    bar_glyph: &'static str,
    bar_colour: Color,
) -> Vec<Line<'static>> {
    let bar_width = Line::from(bar_glyph).width();
    let content_width = width.saturating_sub(bar_width).max(1);
    wrapped_row_ranges(line, content_width)
        .into_iter()
        .map(|range| {
            let mut spans = vec![Span::styled(bar_glyph, Style::default().fg(bar_colour))];
            spans.extend(slice_line(line, range).spans);
            Line::from(spans)
        })
        .collect()
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

/// Char-offset ranges (into `line`'s flattened text, i.e. the same char
/// space `LinkTarget::start`/`end` use) of each row `line` wraps into at
/// `width` columns via ratatui's `Wrap { trim: false }` — matches
/// `ui::detail_columns::wrapped_row_count`'s own greedy word-wrap (which
/// delegates to this) so a rail panel's height and a click's row-to-line
/// mapping (`line_col_at_row`) always agree on where rows break. A width of
/// 0 or an empty line is treated as a single row spanning the whole line.
pub(crate) fn wrapped_row_ranges(line: &Line, width: usize) -> Vec<std::ops::Range<usize>> {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let chars: Vec<char> = text.chars().collect();
    if width == 0 || chars.is_empty() {
        // A genuine single-element `Vec<Range>`, not the `Vec<usize>` the
        // lint's own suggested fix would produce.
        #[allow(clippy::single_range_in_vec_init)]
        return vec![0..chars.len()];
    }

    let mut words: Vec<std::ops::Range<usize>> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        words.push(start..i);
    }
    if words.is_empty() {
        // A genuine single-element `Vec<Range>`, not the `Vec<usize>` the
        // lint's own suggested fix would produce.
        #[allow(clippy::single_range_in_vec_init)]
        return vec![0..chars.len()];
    }

    // Leading whitespace (e.g. code-block indentation) is real rendered
    // width, not free — `Wrap { trim: false }` (what this approximates)
    // preserves it, so the first row starts at char 0 with that whitespace
    // already spent from its column budget, rather than at the first word
    // as if the line began there (which silently dropped it entirely once
    // this range was used to slice out the actual displayed text, not just
    // to count rows).
    let leading_ws = words[0].start;

    let mut rows: Vec<std::ops::Range<usize>> = Vec::new();
    let mut row_start = 0usize;
    let mut row_end = 0usize;
    let mut col = leading_ws;

    for (word_idx, word) in words.iter().enumerate() {
        let word_str: String = chars[word.clone()].iter().collect();
        let word_width = Line::from(word_str.as_str()).width();
        if word_width > width {
            // Wider than any row on its own — hard-wrap across ceil(word/width)
            // rows, closing out whatever was already on the current row first.
            if col > 0 {
                rows.push(row_start..row_end);
            }
            let mut wi = word.start;
            while word.end - wi > width {
                rows.push(wi..wi + width);
                wi += width;
            }
            row_start = wi;
            row_end = word.end;
            col = word.end - wi;
            continue;
        }
        // The first word's leading gap is already spent via the seeded
        // `col` above (the line's real leading whitespace); every later
        // word gets the usual single-column approximation for whatever
        // whitespace separates it from the previous one.
        let gap = if word_idx == 0 || col == 0 { 0 } else { 1 };
        if col + gap + word_width > width {
            rows.push(row_start..row_end);
            row_start = word.start;
            row_end = word.end;
            col = word_width;
        } else {
            row_end = word.end;
            col += gap + word_width;
        }
    }
    rows.push(row_start..row_end);
    rows
}

/// Maps a rendered row (0-indexed from the top of `lines`, in the same
/// space a `Paragraph`'s `.scroll()` offset counts in) and a column within
/// that row back to the logical `(line_index, char_column)` the
/// `LinkTarget`s in `lines` were computed against — the inverse of the
/// wrapping `wrapped_row_ranges` describes, used by `app::mouse::link_at` so
/// a click on wrapped text resolves to the right link instead of assuming
/// one rendered row per logical line. Returns `None` once `row` runs past
/// the last wrapped row across every line.
pub(crate) fn line_col_at_row(
    lines: &[Line],
    width: usize,
    mut row: usize,
    col: usize,
) -> Option<(usize, usize)> {
    for (idx, line) in lines.iter().enumerate() {
        let ranges = wrapped_row_ranges(line, width);
        if row < ranges.len() {
            let r = &ranges[row];
            let local = col.min(r.end.saturating_sub(r.start));
            return Some((idx, r.start + local));
        }
        row -= ranges.len();
    }
    None
}

/// Scan every line for issue keys and bare URLs, restyling matched spans
/// with an underline (in addition to whatever colour/weight they already
/// carry) and returning their locations for keyboard navigation, tagged
/// with the pane they were found in. Mutates `lines` in place.
fn linkify(lines: &mut [Line<'static>], pane: DetailPane) -> Vec<LinkTarget> {
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
                pane,
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
    fn quick_view_narrow_finds_parent_and_link_and_body_keys() {
        let detail = demo_detail(&demo_issues()[1].key);
        let rendered = quick_view_narrow(&detail, "12m ago", 120);
        assert!(!rendered.panel.links.is_empty());
        // Every recorded target actually points at text within its line's
        // bounds.
        for target in &rendered.panel.links {
            let line = &rendered.panel.lines[target.line];
            let len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(target.end <= len);
            assert!(target.start < target.end);
        }
    }

    #[test]
    fn quick_view_wide_meta_shows_only_the_seven_spec_fields() {
        let detail = demo_detail(&demo_issues()[1].key);
        let wide = quick_view_wide(&detail, "12m ago", 120);
        let meta_text: String = wide
            .meta
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(meta_text.contains("assignee:"));
        assert!(meta_text.contains("updated:"));
        assert!(
            !meta_text.contains("reporter:"),
            "quick view's meta grid should omit reporter, unlike the facts/meta panels"
        );
        assert!(
            !meta_text.contains("components:"),
            "quick view's meta grid should omit components, unlike the facts/meta panels"
        );
    }

    #[test]
    fn quick_view_wide_links_reads_description_before_meta() {
        let detail = demo_detail(&demo_issues()[1].key);
        let wide = quick_view_wide(&detail, "12m ago", 120);
        let combined = quick_view_wide_links(&wide);
        assert_eq!(
            combined.len(),
            wide.description.links.len() + wide.meta.links.len()
        );
        if !wide.description.links.is_empty() {
            assert_eq!(combined[0].pane, DetailPane::Main);
        }
    }

    #[test]
    fn human_size_formats_bytes_below_a_kib_as_a_bare_count() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
    }

    #[test]
    fn human_size_drops_a_trailing_zero_decimal() {
        assert_eq!(human_size(1024), "1 KB");
        assert_eq!(human_size(245_760), "240 KB");
    }

    #[test]
    fn human_size_keeps_a_decimal_for_non_round_sizes() {
        assert_eq!(human_size(88_213), "86.1 KB");
        assert_eq!(human_size(1_258_291), "1.2 MB");
    }

    #[test]
    fn human_size_scales_up_through_larger_units() {
        assert_eq!(human_size(1024 * 1024), "1 MB");
        assert_eq!(human_size(1024 * 1024 * 1024), "1 GB");
    }

    #[test]
    fn human_size_promotes_a_value_that_rounds_up_to_the_next_unit() {
        // Regression: 1048555 B is ~1023.98 KB, which used to round for
        // display to a bare "1024 KB" instead of promoting to "1 MB".
        assert_eq!(human_size(1_048_555), "1 MB");
        assert_eq!(human_size(1_073_741_823), "1 GB");
    }

    #[test]
    fn attachments_lines_render_filename_size_and_mime_type() {
        let detail = demo_detail(&demo_issues()[1].key);
        let lines = attachments_lines(&detail);
        assert!(!lines.is_empty());
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("accordion-mockup.png"));
        assert!(text.contains("KB"));
        assert!(text.contains("image/png"));
    }

    #[test]
    fn relation_colour_distinguishes_relates_from_everything_else() {
        assert_eq!(relation_colour("relates to"), ok());
        assert_eq!(relation_colour("is blocked by"), danger());
        assert_eq!(relation_colour("blocks"), danger());
    }

    #[test]
    fn wide_detail_main_offsets_are_valid_indices() {
        let detail = demo_detail(&demo_issues()[1].key);
        let wide = wide_detail(&detail, "you", "12m ago", true, 120);
        if let Some(header) = wide.main.comments_header {
            assert!(header < wide.main.lines.len());
        }
        for start in &wide.main.comment_starts {
            assert!(*start < wide.main.lines.len());
        }
        for panel in [
            &wide.identity,
            &wide.workflow,
            &wide.meta,
            &wide.links,
            &wide.children,
            &wide.attachments,
        ] {
            for target in &panel.links {
                assert!(target.line < panel.lines.len());
            }
        }
    }

    #[test]
    fn meta_lines_omits_the_sprint_row_when_not_configured() {
        let detail = demo_detail(&demo_issues()[1].key); // DS-2725, has a demo sprint
        assert!(detail.sprint.is_some());
        let lines = meta_lines(&detail, "12m ago", false);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            !text.contains("sprint"),
            "sprint_configured=false must hide the row entirely, even though this \
             issue has a sprint in demo data"
        );
    }

    #[test]
    fn meta_lines_shows_the_active_sprint_by_name_when_configured() {
        let detail = demo_detail(&demo_issues()[1].key); // DS-2725
        let lines = meta_lines(&detail, "12m ago", true);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains(&format!("sprint: {}", detail.sprint.unwrap().name)));
    }

    #[test]
    fn meta_lines_shows_none_for_a_configured_issue_with_no_current_sprint() {
        let detail = demo_detail(&demo_issues()[0].key); // DS-2722 has a sprint; use one without
        let mut detail = detail;
        detail.sprint = None;
        let lines = meta_lines(&detail, "12m ago", true);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            text.contains("sprint: none"),
            "configured but no current sprint must still show a row, not hide it"
        );
    }

    #[test]
    fn facts_pairs_mirrors_meta_lines_sprint_visibility() {
        let detail = demo_detail(&demo_issues()[1].key);
        let hidden = facts_pairs(&detail, "12m ago", false);
        assert!(!hidden.iter().any(|(label, _)| *label == "sprint"));
        let shown = facts_pairs(&detail, "12m ago", true);
        assert!(shown.iter().any(|(label, _)| *label == "sprint"));
    }

    #[test]
    fn narrow_detail_folded_facts_has_fewer_lines_than_unfolded() {
        let detail = demo_detail(&demo_issues()[1].key);
        let folded = narrow_detail(&detail, "you", "12m ago", true, true, 120);
        let unfolded = narrow_detail(&detail, "you", "12m ago", false, true, 120);
        assert!(folded.lines.lines.len() < unfolded.lines.lines.len());
    }

    /// Regression test: a comment's left rule (`▌ `) used to be added once
    /// per logical ADF line, so a long comment body only showed the bar on
    /// each paragraph's *first* on-screen row — once `Paragraph::wrap`
    /// re-flowed a long line, its continuation rows had no bar at all.
    /// `activity_lines_cards` now pre-wraps each line (`wrap_with_bar`) at
    /// the actual column width, so every wrapped row carries its own copy.
    #[test]
    fn comment_card_bar_repeats_on_every_wrapped_row() {
        let long_text = "word ".repeat(30);
        let comment = Comment {
            id: "1".into(),
            author: "Ada".into(),
            created: "1h ago".into(),
            body: serde_json::json!({
                "type": "doc", "version": 1,
                "content": [
                    { "type": "paragraph", "content": [ { "type": "text", "text": long_text } ] }
                ]
            }),
        };
        let (lines, _header, starts) = activity_lines_cards(&[comment], "someone else", 20);
        assert_eq!(starts.len(), 1);
        let comment_lines = &lines[starts[0]..];
        assert!(
            comment_lines.len() > 3,
            "a long comment body at width 20 should wrap across several rows"
        );
        for l in comment_lines {
            assert_eq!(l.spans[0].content.as_ref(), "▌ ");
        }
    }
}

#[cfg(test)]
mod wrap_row_tests {
    use super::*;

    fn line(text: &str) -> Line<'static> {
        Line::from(text.to_string())
    }

    #[test]
    fn a_short_line_is_a_single_row_spanning_the_whole_line() {
        let ranges = wrapped_row_ranges(&line("hello"), 20);
        assert_eq!(ranges, vec![0..5]);
    }

    #[test]
    fn a_wrapped_line_splits_on_the_word_boundary_not_mid_word() {
        // "aaaaa bbbbb ccccc" at width 10: "aaaaa" / "bbbbb" / "ccccc" — the
        // same case `wrapped_row_count`'s own regression test covers, here
        // asserting the actual char ranges rather than just the row count.
        let ranges = wrapped_row_ranges(&line("aaaaa bbbbb ccccc"), 10);
        assert_eq!(ranges, vec![0..5, 6..11, 12..17]);
    }

    #[test]
    fn line_col_at_row_maps_a_click_on_a_wrapped_row_back_to_the_original_line() {
        let lines = vec![line("hi"), line("see DS-1234 over there")];
        // At width 8: line 1 wraps into "see" / "DS-1234" / "over" / "there".
        let ranges = wrapped_row_ranges(&lines[1], 8);
        assert_eq!(ranges, vec![0..3, 4..11, 12..16, 17..22]);

        // Row 0 is "hi" (line 0's only row). Row 1 is "see" (line 1's first
        // wrapped row). Row 2 is "DS-1234" (line 1's second wrapped row) —
        // clicking local column 2 within it (the 'S' in "DS") must resolve
        // to line 1, absolute column 6 (4 + 2).
        assert_eq!(line_col_at_row(&lines, 8, 0, 0), Some((0, 0)));
        assert_eq!(line_col_at_row(&lines, 8, 1, 0), Some((1, 0)));
        assert_eq!(line_col_at_row(&lines, 8, 2, 2), Some((1, 6)));
    }

    #[test]
    fn line_col_at_row_returns_none_past_the_last_wrapped_row() {
        let lines = vec![line("one"), line("two")];
        assert_eq!(line_col_at_row(&lines, 10, 2, 0), None);
    }

    #[test]
    fn line_col_at_row_clamps_a_column_past_the_end_of_its_row() {
        let lines = vec![line("hi")];
        assert_eq!(line_col_at_row(&lines, 10, 0, 99), Some((0, 2)));
    }

    /// Regression test: a `"┃ "`/`"▌ "` bar added once to a long logical
    /// `Line` only showed up on that line's first on-screen row once
    /// `Paragraph::wrap` re-flowed it — `wrap_with_bar` exists so every
    /// wrapped row gets its own copy of the bar instead.
    #[test]
    fn wrap_with_bar_repeats_the_bar_on_every_wrapped_row() {
        let rows = wrap_with_bar(&line("aaaaa bbbbb ccccc"), 10, "┃ ", Color::DarkGray);
        assert_eq!(rows.len(), 3, "the same 3-row split wrapped_row_ranges finds at width 10 (minus the 2-col bar) for this text");
        for row in &rows {
            assert_eq!(row.spans[0].content.as_ref(), "┃ ");
            assert_eq!(row.spans[0].style.fg, Some(Color::DarkGray));
        }
        let joined: String = rows
            .iter()
            .flat_map(|l| l.spans[1..].iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(joined, "aaaaabbbbbccccc");
    }

    #[test]
    fn wrap_with_bar_preserves_span_styling_across_the_split() {
        let styled = Line::from(vec![
            Span::styled("bold ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("plain text that keeps going"),
        ]);
        let rows = wrap_with_bar(&styled, 12, "▌ ", Color::Red);
        assert!(rows.len() > 1);
        assert!(rows[0]
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD) && s.content.contains("bold")));
    }

    #[test]
    fn wrap_with_bar_leaves_a_short_line_as_a_single_bar_row() {
        let rows = wrap_with_bar(&line("short"), 40, "┃ ", Color::DarkGray);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].spans[0].content.as_ref(), "┃ ");
    }

    /// Regression test: `wrapped_row_ranges` used to start the first row at
    /// the first non-whitespace character (`words[0].start`), silently
    /// dropping any leading whitespace — harmless while this only fed a row
    /// *count*, but once `wrap_with_bar`/`slice_line` started using the
    /// range to slice out the actual displayed text (for code-block
    /// indentation, or an indented line inside a comment), that whitespace
    /// vanished from what's shown on screen.
    #[test]
    fn wrapped_row_ranges_preserves_leading_whitespace() {
        let ranges = wrapped_row_ranges(&line("    indented"), 40);
        assert_eq!(
            ranges,
            vec![0..12],
            "the leading 4 spaces must stay in range"
        );
    }

    #[test]
    fn wrap_with_bar_preserves_code_block_indentation() {
        let rows = wrap_with_bar(&line("    def f():"), 40, "│ ", Color::DarkGray);
        assert_eq!(rows.len(), 1);
        let text: String = rows[0]
            .spans
            .iter()
            .skip(1)
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(
            text, "    def f():",
            "leading indentation must survive wrapping"
        );
    }

    /// Leading whitespace must count against the row's width budget, not be
    /// free — otherwise a deeply indented line could overflow past `width`
    /// instead of wrapping.
    #[test]
    fn wrapped_row_ranges_counts_leading_whitespace_against_the_width_budget() {
        // 6 columns of indentation + a 6-char word == 12, past a width of 10,
        // so the word must wrap onto its own row rather than sharing the
        // first one.
        let ranges = wrapped_row_ranges(&line("      abcdef"), 10);
        assert_eq!(ranges.len(), 2, "the indent should force an early wrap");
    }
}
