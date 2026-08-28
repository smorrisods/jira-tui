//! ADF (Atlassian Document Format) -> styled terminal text.
//!
//! A faithful-enough port of the Python `_render_issue.py` renderer so the TUI
//! shows Jira rich text the way it is actually stored: headings as headings,
//! task lists as checkboxes, code as code. Display only — never round-tripped
//! back to Jira from here.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use serde_json::Value;

mod compile;
mod markdown;
mod media;

pub use compile::compile;
pub use markdown::to_markdown;

const HEADING: Color = Color::Cyan;
const CODE_FG: Color = Color::LightGreen;
const MUTED: Color = Color::DarkGray;

/// A table column's natural width is capped here so one huge cell (a long
/// URL, a paragraph dumped into a single cell) can't blow out every other
/// column — content past this just wraps across more sub-rows instead, the
/// same way an overlong word already hard-wraps in
/// `render::wrapped_row_ranges`.
const TABLE_MAX_COL_WIDTH: usize = 28;

/// A table column is never shrunk narrower than this even when the pane is
/// too small to fit every column's natural width — below this, wrapped
/// cell text turns into an unreadable one-or-two-char-per-row ladder. If
/// the pane itself is narrower than `n_cols * TABLE_MIN_COL_WIDTH`, the
/// table simply renders wider than the pane; there is no layout that
/// avoids that.
const TABLE_MIN_COL_WIDTH: usize = 6;

/// Tells `render_with_media` whether a `media` node's image is already
/// decoded and ready to have terminal rows reserved for it, or whether it
/// should keep emitting today's `[image: alt]`/`[embedded media]`
/// placeholder text.
///
/// `Disabled` is today's behaviour and this phase's only production
/// caller — `description_lines`'s four call sites (`wide_detail`,
/// `narrow_detail`, `quick_view_wide`, `quick_view_narrow`) all pass it,
/// since nothing yet computes real readiness: that requires knowing which
/// images are actually decoded, which lives on `App::inline_images` (an
/// `App`-specific, `image`-crate-typed cache) — wiring a real `Ready`
/// context through from there is a later phase's job, not this one's.
///
/// A borrowed callback rather than an owned `HashMap<InlineMediaRef, (u16,
/// u16)>`: `render_block` recurses through arbitrarily nested containers
/// (lists, blockquotes, `mediaGroup`) via `&mut RenderCtx`, so a borrowed
/// `sizing` threads through that recursion exactly the way `width` already
/// does, with nothing to clone at each call — and it leaves a future
/// caller free to back the callback with a `HashMap` lookup, an `App`
/// method call, or anything else, without this module caring which.
pub enum MediaSizing<'a> {
    Disabled,
    /// `Some((rows, cols))` if this media node's image is decoded and
    /// ready to reserve space for; `None` to keep the placeholder (e.g.
    /// still fetching, failed to decode, or past the eager-fetch cap).
    Ready(&'a dyn Fn(&InlineMediaRef) -> Option<(u16, u16)>),
}

/// Identifies one `media` node for `MediaSizing::Ready`'s callback, and for
/// the `ImagePlacement` recorded when it reports readiness — carrying
/// enough for a caller to look the node back up in whatever readiness
/// source it built, without `adf` needing to re-parse the node itself.
///
/// Deliberately narrower than `app::inline_images::InlineImageKey` (which
/// is keyed by a resolved attachment id or an external URL): this module has
/// no `IssueDetail`/attachment list to resolve against, only the ADF node in
/// front of it, so it's keyed the same way `resolve_inline_images` itself
/// matches a media node in the first place — an attachment-backed node's
/// (non-empty) `alt` text, or an external node's `url` (issue #130 phase 4).
/// `url` is `None` for an attachment-backed node and `Some` for an external
/// one; the two never collide because `render_block`'s `"media"` arm only
/// ever sets one or the other, never both — see the callers below.
///
/// `id` is the node's own `attrs.id` Media Services uuid (issue #130's
/// DS-1880 follow-up) — always `None` for an external node (nothing to
/// probe there), and `Some` whenever a `type: "file"` node carries one,
/// independent of whether `alt` also matched. Jira doesn't always stamp a
/// node's `alt` to the original filename (confirmed live), so the
/// readiness lookup this feeds (`App::inline_image_key_for`) tries `alt`
/// first and falls back to `id` — carrying both lets a node with no (or a
/// mismatched) `alt` still resolve.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InlineMediaRef {
    pub alt: String,
    pub url: Option<String>,
    pub id: Option<String>,
}

/// One media node's reserved space, recorded by `render_with_media`
/// whenever `MediaSizing::Ready` reports a node decoded and ready — in
/// place of its usual placeholder line, `rows` blank `Line`s are emitted
/// instead (so wrapped-text-based scroll math still counts them as real
/// lines) and this records where. `line_start` is an index into the
/// `Vec<Line>` returned alongside it from the *same* `render_with_media`
/// call — a caller that concatenates multiple line-producing sections
/// (e.g. `description_lines` composing with activity/comments) must rebase
/// it, the same way `comment_starts`/`comments_header` already get rebased
/// by their own callers.
#[derive(Clone, Debug, PartialEq)]
pub struct ImagePlacement {
    pub media: InlineMediaRef,
    pub line_start: usize,
    pub rows: u16,
    pub cols: u16,
}

/// The width/sizing-context/placement-accumulator threaded through
/// `render_block`'s recursion, bundled into one struct rather than three
/// separate parameters growing every call site. `depth` stays its own
/// parameter (it varies per call, e.g. `depth + 1` for a nested list);
/// everything in here is constant for a whole `render_with_media` call
/// except `placements`, which only ever grows.
struct RenderCtx<'a, 'b> {
    width: usize,
    sizing: &'a MediaSizing<'b>,
    placements: Vec<ImagePlacement>,
}

/// Render an ADF document into styled lines. `width` is the column width
/// the caller is about to hand these lines to a `Paragraph::wrap(Wrap {
/// trim: false })` at — needed so blockquote/code-block content can be
/// pre-wrapped ourselves (see `crate::render::wrap_with_bar`) rather than
/// left to ratatui's own wrap, which has no way to repeat a left-margin bar
/// span on a line's wrapped continuation rows.
///
/// Exactly `render_with_media(doc, width, &MediaSizing::Disabled).0` —
/// the two share every bit of block-walking logic; `Disabled` guarantees
/// this keeps producing byte-identical output to before `render_with_media`
/// existed (see the `media_sizing_disabled_matches_render` test).
pub fn render(doc: &Value, width: usize) -> Text<'static> {
    Text::from(render_with_media(doc, width, &MediaSizing::Disabled).0)
}

/// As `render`, but reserves blank rows for any `media` node `sizing`
/// reports ready instead of emitting its usual `[image: alt]` placeholder,
/// and returns where each reservation landed alongside the lines.
pub fn render_with_media(
    doc: &Value,
    width: usize,
    sizing: &MediaSizing<'_>,
) -> (Vec<Line<'static>>, Vec<ImagePlacement>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut ctx = RenderCtx {
        width,
        sizing,
        placements: Vec::new(),
    };
    if let Some(content) = doc.get("content").and_then(|c| c.as_array()) {
        for (i, node) in content.iter().enumerate() {
            render_block(node, &mut lines, 0, &mut ctx);
            // breathing room between top-level blocks
            if i + 1 < content.len() {
                lines.push(Line::from(""));
            }
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no rich content)",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        )));
    }
    (lines, ctx.placements)
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn render_block(node: &Value, out: &mut Vec<Line<'static>>, depth: usize, ctx: &mut RenderCtx) {
    let ty = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(3);
            let prefix = format!("{} ", "#".repeat(level as usize));
            let mut spans = vec![Span::styled(prefix, Style::default().fg(MUTED))];
            let mut inner = inline_spans(node.get("content"));
            for s in inner.iter_mut() {
                s.style = s.style.fg(HEADING).add_modifier(Modifier::BOLD);
            }
            spans.extend(inner);
            out.push(Line::from(spans));
        }
        "paragraph" => {
            let mut spans = inline_spans(node.get("content"));
            if depth > 0 {
                spans.insert(0, Span::raw(indent(depth)));
            }
            out.push(Line::from(spans));
        }
        "bulletList" | "orderedList" => {
            let ordered = ty == "orderedList";
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for (i, item) in items.iter().enumerate() {
                    let marker = if ordered {
                        format!("{}. ", i + 1)
                    } else {
                        "• ".to_string()
                    };
                    render_list_item(item, out, depth, &marker, ctx);
                }
            }
        }
        "taskList" => {
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for item in items {
                    let done = item
                        .get("attrs")
                        .and_then(|a| a.get("state"))
                        .and_then(|s| s.as_str())
                        == Some("DONE");
                    let (box_glyph, box_color) = if done {
                        ("[✓] ", Color::Green)
                    } else {
                        ("[ ] ", Color::Yellow)
                    };
                    let mut spans = vec![
                        Span::raw(indent(depth)),
                        Span::styled(box_glyph, Style::default().fg(box_color)),
                    ];
                    let mut inner = inline_spans(item.get("content"));
                    if done {
                        for s in inner.iter_mut() {
                            s.style = s.style.fg(MUTED).add_modifier(Modifier::CROSSED_OUT);
                        }
                    }
                    spans.extend(inner);
                    out.push(Line::from(spans));
                }
            }
        }
        "codeBlock" => {
            let lang = node
                .get("attrs")
                .and_then(|a| a.get("language"))
                .and_then(|l| l.as_str())
                .unwrap_or("");
            let fence = if lang.is_empty() {
                "```".to_string()
            } else {
                format!("``` {lang}")
            };
            out.push(Line::from(Span::styled(fence, Style::default().fg(MUTED))));
            let text = collect_text(node.get("content"));
            for raw in text.split('\n') {
                let line = Line::from(Span::styled(raw.to_string(), Style::default().fg(CODE_FG)));
                out.extend(crate::render::wrap_with_bar(&line, ctx.width, "│ ", MUTED));
            }
            out.push(Line::from(Span::styled("```", Style::default().fg(MUTED))));
        }
        "rule" => {
            out.push(Line::from(Span::styled(
                "─".repeat(48),
                Style::default().fg(MUTED),
            )));
        }
        "blockquote" => {
            let mut inner: Vec<Line<'static>> = Vec::new();
            let placements_before = ctx.placements.len();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    render_block(child, &mut inner, depth, ctx);
                }
            }
            // Any `ImagePlacement` recorded while rendering into `inner`
            // above has a `line_start` indexed into that private buffer,
            // not `out` — and bar-wrapping below can turn one `inner` line
            // into several wrapped rows, so a placement can't simply be
            // shifted by `out.len()` either. Build a per-line cumulative
            // wrapped-row-count table so each placement's `line_start`
            // rebases onto the row it actually lands on in `out`, the same
            // way `comment_starts`/`comments_header` get rebased by their
            // own callers (see this struct's own doc comment).
            let base = out.len();
            let mut cumulative_rows = Vec::with_capacity(inner.len() + 1);
            cumulative_rows.push(0usize);
            let mut wrapped: Vec<Line<'static>> = Vec::new();
            for line in &inner {
                let rows = crate::render::wrap_with_bar(line, ctx.width, "┃ ", MUTED);
                cumulative_rows.push(cumulative_rows.last().copied().unwrap_or(0) + rows.len());
                wrapped.extend(rows);
            }
            for placement in &mut ctx.placements[placements_before..] {
                let offset = cumulative_rows
                    .get(placement.line_start)
                    .copied()
                    .unwrap_or(0);
                placement.line_start = base + offset;
            }
            out.extend(wrapped);
        }
        "table" => render_table(node, out, ctx.width),
        "mediaSingle" | "mediaGroup" => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    render_block(child, out, depth, ctx);
                }
            }
        }
        "media" => {
            let attrs = node.get("attrs");
            let alt = attrs
                .and_then(|a| a.get("alt"))
                .and_then(|a| a.as_str())
                .filter(|a| !a.is_empty());
            let external_url =
                attrs.and_then(|a| a.get("type")).and_then(|t| t.as_str()) == Some("external");
            let url = attrs.and_then(|a| a.get("url")).and_then(|u| u.as_str());
            let id = attrs.and_then(|a| a.get("id")).and_then(|i| i.as_str());

            // An attachment-backed node is identified by its (non-empty)
            // `alt` text where possible, matching `resolve_inline_images`'s
            // own matching, falling back to its own Media Services uuid
            // (`id`) when there's no `alt` to go on at all — see
            // `InlineMediaRef::id`'s own doc comment. An external node is
            // identified by its `url` instead — its `alt` may be empty or
            // absent, but the `url` *is* the fetch target, so it alone is
            // enough to ask for readiness.
            let media_ref = if external_url {
                url.map(|url| InlineMediaRef {
                    alt: alt.unwrap_or_default().to_string(),
                    url: Some(url.to_string()),
                    id: None,
                })
            } else if alt.is_some() || id.is_some() {
                Some(InlineMediaRef {
                    alt: alt.unwrap_or_default().to_string(),
                    url: None,
                    id: id.map(str::to_string),
                })
            } else {
                None
            };

            if let Some(media_ref) = media_ref {
                if let MediaSizing::Ready(ready) = ctx.sizing {
                    if let Some((rows, cols)) = ready(&media_ref) {
                        let line_start = out.len();
                        for _ in 0..rows {
                            out.push(Line::default());
                        }
                        ctx.placements.push(ImagePlacement {
                            media: media_ref,
                            line_start,
                            rows,
                            cols,
                        });
                        return;
                    }
                }
            }

            let text = if let Some(alt) = alt {
                format!("[image: {alt}]")
            } else if external_url {
                match url {
                    Some(url) => format!("[image: {url}]"),
                    None => "[embedded media]".to_string(),
                }
            } else {
                "[embedded media]".to_string()
            };
            let mut spans = Vec::new();
            if depth > 0 {
                spans.push(Span::raw(indent(depth)));
            }
            spans.push(Span::styled(
                text,
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ));
            out.push(Line::from(spans));
        }
        _ => {
            // generic container: descend if possible
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    render_block(child, out, depth, ctx);
                }
            }
        }
    }
}

fn render_list_item(
    item: &Value,
    out: &mut Vec<Line<'static>>,
    depth: usize,
    marker: &str,
    ctx: &mut RenderCtx,
) {
    let content = match item.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return,
    };
    let mut first = true;
    for child in content {
        let ty = child.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty == "bulletList" || ty == "orderedList" || ty == "taskList" {
            render_block(child, out, depth + 1, ctx);
            continue;
        }
        let mut spans = vec![
            Span::raw(indent(depth)),
            Span::styled(
                if first {
                    marker.to_string()
                } else {
                    "  ".to_string()
                },
                Style::default().fg(Color::Blue),
            ),
        ];
        spans.extend(inline_spans(child.get("content")));
        out.push(Line::from(spans));
        first = false;
    }
}

/// Largest-remainder (Hamilton) apportionment: distribute `available`
/// columns across `natural.len()` columns proportionally to `natural`,
/// clamped so no column ends up narrower than `min_col`. If the natural
/// widths already fit in `available`, they're used as-is — a table never
/// stretches to fill the pane, the same way a blockquote/code block
/// doesn't either. If `available` is too small to give every column even
/// `min_col`, every column is set to `min_col` regardless (the table then
/// renders wider than the pane — a degenerate but non-panicking outcome).
fn table_col_widths(natural: &[usize], available: usize, min_col: usize) -> Vec<usize> {
    let n = natural.len();
    if n == 0 {
        return Vec::new();
    }
    let total: usize = natural.iter().sum();
    if total == 0 || total <= available {
        return natural.to_vec();
    }
    let mut widths = vec![0usize; n];
    let mut remainders: Vec<(usize, usize)> = Vec::with_capacity(n);
    let mut assigned = 0usize;
    for (i, &w) in natural.iter().enumerate() {
        let scaled = w * available;
        widths[i] = scaled / total;
        remainders.push((i, scaled % total));
        assigned += widths[i];
    }
    let leftover = available.saturating_sub(assigned);
    remainders.sort_by_key(|&(_, rem)| std::cmp::Reverse(rem));
    for &(i, _) in remainders.iter().take(leftover) {
        widths[i] += 1;
    }
    for w in widths.iter_mut() {
        *w = (*w).max(min_col);
    }
    widths
}

/// Convert one table cell's content into styled spans. A cell whose
/// content is a single `paragraph` (the overwhelming common case for a
/// real-world table) keeps its inline marks via `inline_spans`. Anything
/// more structurally complex (multiple blocks, a list, a nested code
/// block) falls back to `collect_text`'s flat-string behaviour — a
/// superset of the old fidelity, never a regression. Never panics: an
/// absent/malformed `content` array yields an empty cell.
fn cell_content_spans(cell: &Value) -> Vec<Span<'static>> {
    let content = match cell.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return Vec::new(),
    };
    if content.len() == 1 && content[0].get("type").and_then(|t| t.as_str()) == Some("paragraph") {
        return inline_spans(content[0].get("content"));
    }
    let text = collect_text(cell.get("content"));
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Span::raw(text)]
    }
}

/// Terminal display width (not char count) of a cell's spans — matches
/// what `Line::width()`/`Span::width()` already use for word-wrap
/// decisions in `render::wrapped_row_ranges`, so a fullwidth/CJK cell
/// measures the same number of columns here as it wraps to there.
fn span_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// A border row (top/row-divider/bottom) built from per-column `─`
/// segments sized to `col_widths[j] + 2` (the `+2` accounts for the
/// 1-space pad on each side of a cell's content in the content rows), so
/// every junction glyph lines up exactly with the `│` separators below it.
fn table_border_line(left: &str, mid: &str, right: &str, col_widths: &[usize]) -> Line<'static> {
    let segments: Vec<String> = col_widths.iter().map(|w| "─".repeat(w + 2)).collect();
    let text = format!("{left}{}{right}", segments.join(mid));
    Line::from(Span::styled(text, Style::default().fg(MUTED)))
}

fn render_table(node: &Value, out: &mut Vec<Line<'static>>, width: usize) {
    let rows = match node.get("content").and_then(|c| c.as_array()) {
        Some(r) => r,
        None => return,
    };

    let mut grid: Vec<Vec<(bool, Vec<Span<'static>>)>> = Vec::new();
    for row in rows {
        let cells = match row.get("content").and_then(|c| c.as_array()) {
            Some(c) => c,
            None => continue,
        };
        let mut grid_row = Vec::with_capacity(cells.len());
        for cell in cells {
            let is_header = cell.get("type").and_then(|t| t.as_str()) == Some("tableHeader");
            let mut spans = cell_content_spans(cell);
            if is_header {
                for s in spans.iter_mut() {
                    s.style = s.style.fg(HEADING).add_modifier(Modifier::BOLD);
                }
            }
            grid_row.push((is_header, spans));
        }
        grid.push(grid_row);
    }

    let n_cols = grid.iter().map(|r| r.len()).max().unwrap_or(0);
    if n_cols == 0 {
        return;
    }

    let natural: Vec<usize> = (0..n_cols)
        .map(|j| {
            grid.iter()
                .filter_map(|row| row.get(j))
                .map(|(_, spans)| span_width(spans))
                .max()
                .unwrap_or(0)
                .clamp(1, TABLE_MAX_COL_WIDTH)
        })
        .collect();

    // Border budget: a leading "│ " (2 cols) plus a trailing " │ " (3
    // cols) per column — every rendered row costs exactly this many
    // non-content columns (see the content-row assembly below), so
    // undercounting here would let column widths add up to a row wider
    // than `width`, re-triggering the outer `Paragraph::wrap` re-flow this
    // whole rewrite exists to avoid.
    let available = width.saturating_sub(2 + 3 * n_cols);
    let col_widths = table_col_widths(&natural, available, TABLE_MIN_COL_WIDTH);

    out.push(table_border_line("┌", "┬", "┐", &col_widths));
    for (row_idx, row) in grid.iter().enumerate() {
        let mut per_col_sub_rows: Vec<Vec<(Line<'static>, usize)>> = Vec::with_capacity(n_cols);
        for (j, &col_w) in col_widths.iter().enumerate() {
            let empty = (false, Vec::new());
            let (_, spans) = row.get(j).unwrap_or(&empty);
            let cell_line = Line::from(spans.clone());
            let ranges = crate::render::wrapped_row_ranges(&cell_line, col_w);
            let sub_rows = ranges
                .into_iter()
                .map(|r| {
                    let sliced = crate::render::slice_line(&cell_line, r);
                    let pad = col_w.saturating_sub(sliced.width());
                    (sliced, pad)
                })
                .collect();
            per_col_sub_rows.push(sub_rows);
        }
        let row_height = per_col_sub_rows.iter().map(|c| c.len()).max().unwrap_or(1);

        for sub_idx in 0..row_height {
            let mut spans: Vec<Span<'static>> =
                vec![Span::styled("│ ", Style::default().fg(MUTED))];
            for (j, col_sub_rows) in per_col_sub_rows.iter().enumerate() {
                if let Some((line, pad)) = col_sub_rows.get(sub_idx) {
                    spans.extend(line.spans.iter().cloned());
                    spans.push(Span::raw(" ".repeat(*pad)));
                } else {
                    spans.push(Span::raw(" ".repeat(col_widths[j])));
                }
                spans.push(Span::styled(" │ ", Style::default().fg(MUTED)));
            }
            out.push(Line::from(spans));
        }

        if row_idx + 1 < grid.len() {
            out.push(table_border_line("├", "┼", "┤", &col_widths));
        }
    }
    out.push(table_border_line("└", "┴", "┘", &col_widths));
}

/// Convert an array of inline nodes into styled spans (applying marks).
fn inline_spans(content: Option<&Value>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let arr = match content.and_then(|c| c.as_array()) {
        Some(a) => a,
        None => return spans,
    };
    for node in arr {
        let ty = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "text" => {
                let text = node.get("text").and_then(|t| t.as_str()).unwrap_or("");
                let mut style = Style::default();
                if let Some(marks) = node.get("marks").and_then(|m| m.as_array()) {
                    for mark in marks {
                        match mark.get("type").and_then(|t| t.as_str()) {
                            Some("strong") => style = style.add_modifier(Modifier::BOLD),
                            Some("em") => style = style.add_modifier(Modifier::ITALIC),
                            Some("code") => {
                                style = style.fg(CODE_FG).bg(Color::Rgb(30, 30, 30));
                            }
                            Some("strike") => style = style.add_modifier(Modifier::CROSSED_OUT),
                            Some("underline") => style = style.add_modifier(Modifier::UNDERLINED),
                            Some("link") => {
                                style = style.fg(Color::Blue).add_modifier(Modifier::UNDERLINED);
                            }
                            _ => {}
                        }
                    }
                }
                spans.push(Span::styled(text.to_string(), style));
            }
            "hardBreak" => spans.push(Span::raw(" ")),
            "emoji" => {
                let t = node
                    .get("attrs")
                    .and_then(|a| a.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                spans.push(Span::raw(t.to_string()));
            }
            "mention" => {
                let t = node
                    .get("attrs")
                    .and_then(|a| a.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("@user");
                spans.push(Span::styled(
                    t.to_string(),
                    Style::default().fg(Color::Magenta),
                ));
            }
            _ => {}
        }
    }
    spans
}

/// Flatten all descendant text nodes into a plain string.
fn collect_text(content: Option<&Value>) -> String {
    let mut out = String::new();
    fn walk(node: &Value, out: &mut String) {
        if node.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = node.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
        if let Some(arr) = node.get("content").and_then(|c| c.as_array()) {
            for child in arr {
                walk(child, out);
            }
        }
    }
    if let Some(arr) = content.and_then(|c| c.as_array()) {
        for node in arr {
            walk(node, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_heading_and_task_list() {
        let doc = json!({
            "type": "doc", "version": 1,
            "content": [
                { "type": "heading", "attrs": { "level": 3 },
                  "content": [ { "type": "text", "text": "Done" } ] },
                { "type": "taskList", "content": [
                    { "type": "taskItem", "attrs": { "state": "DONE" },
                      "content": [ { "type": "text", "text": "ship it" } ] }
                ] }
            ]
        });
        let text = render(&doc, 120);
        let joined: String = text
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.clone()))
            .collect();
        assert!(joined.contains("Done"));
        assert!(joined.contains("[✓]"));
        assert!(joined.contains("ship it"));
    }

    fn flat(doc: &serde_json::Value) -> String {
        render(doc, 120)
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.clone()))
            .collect()
    }

    #[test]
    fn media_single_with_alt_renders_alt_text_placeholder() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "mediaSingle", "content": [
                    { "type": "media", "attrs": { "id": "abc-123", "type": "file", "alt": "a screenshot" } }
                ] }
            ]
        });
        let s = flat(&doc);
        assert!(s.contains("[image: a screenshot]"));
    }

    #[test]
    fn media_without_alt_or_external_url_renders_generic_placeholder() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "media", "attrs": { "id": "abc-123", "type": "file" } }
            ]
        });
        let s = flat(&doc);
        assert!(s.contains("[embedded media]"));
    }

    #[test]
    fn media_group_renders_one_placeholder_line_per_child() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "mediaGroup", "content": [
                    { "type": "media", "attrs": { "id": "one", "type": "file", "alt": "first" } },
                    { "type": "media", "attrs": { "id": "two", "type": "file", "alt": "second" } }
                ] }
            ]
        });
        let text = render(&doc, 120);
        let placeholder_lines = text
            .lines
            .iter()
            .filter(|l| {
                let s: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
                s.starts_with("[image:")
            })
            .count();
        assert_eq!(
            placeholder_lines, 2,
            "expected two separate placeholder lines"
        );
        let s = flat(&doc);
        assert!(s.contains("[image: first]"));
        assert!(s.contains("[image: second]"));
    }

    fn single_media_doc(alt: &str) -> serde_json::Value {
        json!({
            "type": "doc",
            "content": [
                { "type": "mediaSingle", "content": [
                    { "type": "media", "attrs": { "id": "abc-123", "type": "file", "alt": alt } }
                ] }
            ]
        })
    }

    /// Locks in this phase's success criterion: `Disabled` must produce
    /// output byte-identical to plain `render`, for a doc containing a
    /// media node — the only sizing mode any production call site actually
    /// uses this phase.
    #[test]
    fn media_sizing_disabled_matches_render() {
        let doc = single_media_doc("a screenshot");
        let plain = render(&doc, 120);
        let (with_media_lines, placements) = render_with_media(&doc, 120, &MediaSizing::Disabled);
        assert_eq!(plain.lines, with_media_lines);
        assert!(placements.is_empty());
    }

    /// A `Ready` context reporting readiness for the doc's one media node:
    /// the placeholder line is replaced by the reported number of blank
    /// lines, and the returned placement correctly identifies the node and
    /// its line range.
    #[test]
    fn media_sizing_ready_reserves_blank_lines_and_records_a_placement() {
        let doc = single_media_doc("a screenshot");
        let ready = |media: &InlineMediaRef| -> Option<(u16, u16)> {
            (media.alt == "a screenshot").then_some((3, 40))
        };
        let sizing = MediaSizing::Ready(&ready);
        let (lines, placements) = render_with_media(&doc, 120, &sizing);

        assert_eq!(placements.len(), 1);
        let placement = &placements[0];
        assert_eq!(
            placement.media,
            InlineMediaRef {
                alt: "a screenshot".into(),
                url: None,
                id: Some("abc-123".into()),
            }
        );
        assert_eq!(placement.rows, 3);
        assert_eq!(placement.cols, 40);
        assert_eq!(placement.line_start, 0);

        assert_eq!(lines.len(), 3, "exactly the reserved rows, nothing else");
        for line in &lines {
            assert!(
                line.spans.is_empty() || line.spans.iter().all(|s| s.content.is_empty()),
                "a reserved row should be blank, got {line:?}"
            );
        }
        let s: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(!s.contains("[image:"), "placeholder must not appear");
    }

    /// DS-1880 follow-up: a `type: "file"` node with no `alt` at all (Jira
    /// doesn't always stamp one, confirmed live) must still get an
    /// `InlineMediaRef` built — carrying its `id` — so a `Ready` context
    /// keyed off the node's own uuid (rather than its now-missing `alt`)
    /// can still report it ready. Before this, a `None` `alt` short-circuited
    /// `render_block`'s `"media"` arm straight to `[embedded media]` with no
    /// `InlineMediaRef` ever built, so no readiness lookup was even
    /// possible.
    #[test]
    fn a_media_node_with_no_alt_still_builds_a_ref_keyed_by_its_id() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "mediaSingle", "content": [
                    { "type": "media", "attrs": { "id": "be2818ad-2e36-4d40-94f1-c6826e1def49", "type": "file" } }
                ] }
            ]
        });
        let ready = |media: &InlineMediaRef| -> Option<(u16, u16)> {
            (media.id.as_deref() == Some("be2818ad-2e36-4d40-94f1-c6826e1def49")).then_some((3, 40))
        };
        let sizing = MediaSizing::Ready(&ready);
        let (lines, placements) = render_with_media(&doc, 120, &sizing);

        assert_eq!(placements.len(), 1);
        assert_eq!(
            placements[0].media,
            InlineMediaRef {
                alt: String::new(),
                url: None,
                id: Some("be2818ad-2e36-4d40-94f1-c6826e1def49".into()),
            }
        );
        let s: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(
            !s.contains("[embedded media]"),
            "placeholder must not appear"
        );
    }

    /// A `mediaGroup` with two children, only one reported ready: the ready
    /// one gets blank rows + a placement, the other still shows its
    /// placeholder text, and line-index bookkeeping stays correct for both
    /// (the placement's `line_start` must land on the reserved rows, not
    /// wherever the still-placeholder sibling ended up).
    #[test]
    fn media_group_mixed_readiness_only_reserves_the_ready_child() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "mediaGroup", "content": [
                    { "type": "media", "attrs": { "id": "one", "type": "file", "alt": "ready one" } },
                    { "type": "media", "attrs": { "id": "two", "type": "file", "alt": "not ready" } }
                ] }
            ]
        });
        let ready = |media: &InlineMediaRef| -> Option<(u16, u16)> {
            (media.alt == "ready one").then_some((2, 30))
        };
        let sizing = MediaSizing::Ready(&ready);
        let (lines, placements) = render_with_media(&doc, 120, &sizing);

        assert_eq!(placements.len(), 1, "only the ready child gets a placement");
        let placement = &placements[0];
        assert_eq!(
            placement.media,
            InlineMediaRef {
                alt: "ready one".into(),
                url: None,
                id: Some("one".into()),
            }
        );
        assert_eq!(placement.line_start, 0);
        assert_eq!(placement.rows, 2);
        assert_eq!(placement.cols, 30);

        // Lines 0..2 are the reserved blank rows for "ready one"; line 2 is
        // "not ready"'s placeholder, still showing its alt text.
        assert_eq!(lines.len(), 3);
        for line in &lines[0..2] {
            assert!(line.spans.iter().all(|s| s.content.is_empty()));
        }
        let placeholder_text: String = lines[2].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(placeholder_text, "[image: not ready]");
    }

    /// Code-review regression test: a media node nested inside a
    /// blockquote used to record a `line_start` indexed into the
    /// blockquote's own private line buffer (see the `"blockquote"` arm's
    /// `inner`) rather than the final `out` the caller actually receives —
    /// so a placement for an image inside (or after enough of) a blockquote
    /// landed at the wrong row, or a row that didn't even exist yet, once
    /// bar-wrapping and preceding content were accounted for. Here the
    /// blockquote holds two paragraphs before the image, so a correct
    /// `line_start` must land on the third bar-wrapped row (index 2), not 0.
    #[test]
    fn media_inside_a_blockquote_rebases_its_line_start_past_preceding_content() {
        // A top-level paragraph (plus the breathing-room blank line between
        // top-level blocks) precedes the blockquote, so a correct
        // `line_start` must account for both `out.len()` *before* the
        // blockquote started (2) and the one quoted paragraph line ahead of
        // the image *inside* the blockquote (1) — landing on row 3. Before
        // this was fixed, `line_start` was computed against the blockquote's
        // own private `inner` buffer and never rebased at all, so it would
        // have come out as 1 (the media node's position within `inner`
        // alone), pointing at the quoted paragraph's own bar-wrapped row
        // instead of the image's reserved space.
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "intro" }] },
                { "type": "blockquote", "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "quoted line" }] },
                    { "type": "mediaSingle", "content": [
                        { "type": "media", "attrs": { "id": "x", "type": "file", "alt": "quoted shot" } }
                    ] }
                ] }
            ]
        });
        let ready = |media: &InlineMediaRef| -> Option<(u16, u16)> {
            (media.alt == "quoted shot").then_some((2, 30))
        };
        let sizing = MediaSizing::Ready(&ready);
        let (lines, placements) = render_with_media(&doc, 120, &sizing);

        assert_eq!(placements.len(), 1);
        let placement = &placements[0];
        assert_eq!(
            placement.line_start, 3,
            "line_start must be rebased past the intro paragraph, the breathing-room blank \
             line, and the one quoted paragraph line ahead of the image, got lines: {lines:?}"
        );
        let reserved_row_text: String = lines[placement.line_start]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(
            reserved_row_text, "┃ ",
            "line_start must point at a reserved (blank, bar-prefixed) row, not quoted text or \
             the wrong row entirely, got: {:?}",
            lines[placement.line_start]
        );
    }

    /// Issue #130 phase 4: an external (`type: "external"`) media node with
    /// no `alt` at all is still offered readiness, keyed by its `url` —
    /// unlike an attachment-backed node (gated on a non-empty `alt`), the
    /// URL alone is enough identity to reserve space for it.
    #[test]
    fn media_sizing_ready_reserves_space_for_an_external_node_by_url() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "media", "attrs": {
                    "id": "x", "type": "external",
                    "url": "https://third-party.example.com/pic.png"
                } }
            ]
        });
        let ready = |media: &InlineMediaRef| -> Option<(u16, u16)> {
            (media.url.as_deref() == Some("https://third-party.example.com/pic.png"))
                .then_some((4, 20))
        };
        let sizing = MediaSizing::Ready(&ready);
        let (lines, placements) = render_with_media(&doc, 120, &sizing);

        assert_eq!(placements.len(), 1);
        assert_eq!(
            placements[0].media,
            InlineMediaRef {
                alt: String::new(),
                url: Some("https://third-party.example.com/pic.png".into()),
                id: None,
            }
        );
        assert_eq!(placements[0].rows, 4);
        assert_eq!(placements[0].cols, 20);
        assert_eq!(lines.len(), 4, "exactly the reserved rows, nothing else");
    }

    #[test]
    fn doc_with_only_a_media_node_renders_visibly_instead_of_vanishing() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "media", "attrs": { "id": "abc-123", "type": "external", "url": "https://example.com/img.png" } }
            ]
        });
        let text = render(&doc, 120);
        assert!(
            !text.lines.is_empty(),
            "a doc containing only a media node must not render as empty"
        );
        let s = flat(&doc);
        assert!(s.contains("[image: https://example.com/img.png]"));
        assert!(!s.contains("no rich content"));
    }
}

#[cfg(test)]
mod robustness_tests {
    use super::*;
    use serde_json::json;

    fn flat(doc: &serde_json::Value) -> String {
        render(doc, 120)
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.clone()))
            .collect()
    }

    #[test]
    fn empty_and_malformed_docs_do_not_panic() {
        let _ = render(&json!({}), 120);
        let _ = render(&json!({ "type": "doc" }), 120);
        let _ = render(&json!({ "type": "doc", "content": "not-an-array" }), 120);
        let _ = render(
            &json!({ "type": "doc", "content": [ { "type": "mystery" } ] }),
            120,
        );
        // A heading with no content array must still render safely.
        let out = render(
            &json!({
                "type": "doc",
                "content": [ { "type": "heading", "attrs": { "level": 2 } } ]
            }),
            120,
        );
        assert!(!out.lines.is_empty());
    }

    #[test]
    fn ordered_and_nested_lists_render() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "orderedList", "content": [
                    { "type": "listItem", "content": [
                        { "type": "paragraph", "content": [ { "type": "text", "text": "first" } ] },
                        { "type": "bulletList", "content": [
                            { "type": "listItem", "content": [
                                { "type": "paragraph", "content": [ { "type": "text", "text": "nested" } ] }
                            ] }
                        ] }
                    ] }
                ] }
            ]
        });
        let s = flat(&doc);
        assert!(s.contains("1."));
        assert!(s.contains("first"));
        assert!(s.contains("nested"));
    }

    #[test]
    fn code_block_is_fenced() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "codeBlock", "attrs": { "language": "rust" },
                  "content": [ { "type": "text", "text": "let x = 1;" } ] }
            ]
        });
        let s = flat(&doc);
        assert!(s.contains("```"));
        assert!(s.contains("rust"));
        assert!(s.contains("let x = 1;"));
    }

    #[test]
    fn table_renders_headers_and_cells() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "table", "content": [
                    { "type": "tableRow", "content": [
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "Name" } ] } ] }
                    ] },
                    { "type": "tableRow", "content": [
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "Ada" } ] } ] }
                    ] }
                ] }
            ]
        });
        let s = flat(&doc);
        assert!(s.contains("Name"));
        assert!(s.contains("Ada"));
    }

    fn one_col_table(header: &str, body: &str) -> serde_json::Value {
        json!({
            "type": "doc",
            "content": [
                { "type": "table", "content": [
                    { "type": "tableRow", "content": [
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": header } ] } ] }
                    ] },
                    { "type": "tableRow", "content": [
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": body } ] } ] }
                    ] }
                ] }
            ]
        })
    }

    /// Regression test for the reported bug: a table row's `"│ "`
    /// separators used to only line up on the first on-screen row once a
    /// cell's content overflowed the fixed 16-char column width and
    /// `Paragraph::wrap` re-flowed the whole row as one long logical line.
    /// `render_table` now wraps each cell to its own column width and
    /// repeats the border on every resulting sub-row, the same fix already
    /// applied to blockquotes/code blocks via `wrap_with_bar`.
    #[test]
    fn table_borders_stay_aligned_on_wrapped_rows() {
        let long_text = "word ".repeat(20);
        let doc = one_col_table("Col", &long_text);
        let text = render(&doc, 20);
        let mut content_rows = 0;
        for line in &text.lines {
            let first = line.spans[0].content.as_ref();
            let is_border = first.starts_with(['┌', '├', '└']);
            let is_content = first == "│ ";
            assert!(
                is_border || is_content,
                "row didn't start with a table border or a content bar: {first:?}"
            );
            if is_content {
                content_rows += 1;
            }
        }
        assert!(
            content_rows > 3,
            "expected multiple wrapped body sub-rows, got {content_rows}"
        );
    }

    /// A real box, not a markdown-pipe-table lookalike: corners and
    /// junctions must appear, not just flat `─` rules. Needs at least two
    /// columns — a single-column table has no interior junction to draw.
    #[test]
    fn table_draws_a_boxed_border_with_junctions() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "table", "content": [
                    { "type": "tableRow", "content": [
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "A" } ] } ] },
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "B" } ] } ] }
                    ] },
                    { "type": "tableRow", "content": [
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "x" } ] } ] },
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "y" } ] } ] }
                    ] }
                ] }
            ]
        });
        let s = flat(&doc);
        assert!(s.contains('┌') && s.contains('┐'), "missing top corners");
        assert!(
            s.contains('├') && s.contains('┼') && s.contains('┤'),
            "missing header divider"
        );
        assert!(s.contains('└') && s.contains('┘'), "missing bottom corners");
    }

    /// A full grid: every row (not just the header) is followed by a
    /// `├┼┤` divider, so a multi-row body table reads as a bordered grid
    /// rather than only ruling under the header.
    #[test]
    fn table_draws_a_divider_between_every_row() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "table", "content": [
                    { "type": "tableRow", "content": [
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "A" } ] } ] }
                    ] },
                    { "type": "tableRow", "content": [
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "one" } ] } ] }
                    ] },
                    { "type": "tableRow", "content": [
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "two" } ] } ] }
                    ] }
                ] }
            ]
        });
        let text = render(&doc, 40);
        let divider_rows = text
            .lines
            .iter()
            .filter(|l| l.spans[0].content.starts_with('├'))
            .count();
        // 3 rows -> 2 interior boundaries (header/one, one/two).
        assert_eq!(divider_rows, 2, "expected a divider between every row");
    }

    /// A `tableRow` with fewer cells than the table's max column count
    /// must not panic (indexing past a short row's cell array), and its
    /// missing trailing cells just render blank.
    #[test]
    fn ragged_table_row_does_not_panic() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "table", "content": [
                    { "type": "tableRow", "content": [
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "A" } ] } ] },
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "B" } ] } ] }
                    ] },
                    { "type": "tableRow", "content": [
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "only" } ] } ] }
                    ] }
                ] }
            ]
        });
        let s = flat(&doc);
        assert!(s.contains("only"));
    }

    #[test]
    fn table_col_widths_does_not_crush_columns_below_the_floor() {
        let widths = table_col_widths(&[40, 40, 40], 15, TABLE_MIN_COL_WIDTH);
        assert!(widths.iter().all(|&w| w >= TABLE_MIN_COL_WIDTH));
    }

    #[test]
    fn table_col_widths_uses_natural_widths_when_they_fit() {
        let widths = table_col_widths(&[4, 10, 6], 100, TABLE_MIN_COL_WIDTH);
        assert_eq!(widths, vec![4, 10, 6]);
    }

    /// Regression test: the `available` width budget must reserve the
    /// *full* per-row border overhead (a leading "│ " plus a trailing
    /// " │ " per column), not just 2 chars/column — undercounting it lets
    /// a row render wider than `width`, handing the outer `Paragraph::wrap`
    /// a line to re-flow and reintroducing the misaligned-border bug this
    /// rewrite exists to fix.
    #[test]
    fn table_rows_never_exceed_the_requested_width() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "table", "content": [
                    { "type": "tableRow", "content": [
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "AAAA" } ] } ] },
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "BBBB" } ] } ] },
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "CCCC" } ] } ] }
                    ] }
                ] }
            ]
        });
        // Widths below `2 + 3*n_cols + n_cols*TABLE_MIN_COL_WIDTH` (here,
        // 2 + 9 + 18 = 29 for 3 columns) hit the documented degenerate
        // floor case, where the table is allowed to render wider than the
        // pane because there's no layout that avoids it — this test is
        // about the budget math above that floor, so it stays clear of it.
        for width in [30usize, 45, 70, 110] {
            let text = render(&doc, width);
            for line in &text.lines {
                assert!(
                    line.width() <= width,
                    "row of width {} exceeded pane width {width}: {line:?}",
                    line.width()
                );
            }
        }
    }

    /// Regression test: a real ADF cell whose content includes fullwidth
    /// (CJK) characters must not misalign the table — column widths and
    /// wrap padding must be measured in terminal display width, not char
    /// count. A border row is always exactly 1 column narrower than a
    /// content row (the content row's trailing " │ " separator carries one
    /// more space than a border row's plain corner glyph) — that offset
    /// must hold uniformly whether or not a cell contains fullwidth text;
    /// under the old char-count measurement a fullwidth cell would measure
    /// half its true display width and blow that pattern up.
    #[test]
    fn table_measures_fullwidth_characters_by_display_width() {
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "table", "content": [
                    { "type": "tableRow", "content": [
                        { "type": "tableHeader", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "col" } ] } ] }
                    ] },
                    { "type": "tableRow", "content": [
                        { "type": "tableCell", "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "日本語テスト" } ] } ] }
                    ] }
                ] }
            ]
        });
        let text = render(&doc, 80);
        for line in &text.lines {
            let first = line.spans[0].content.as_ref();
            let is_border = first.starts_with(['┌', '├', '└']);
            let expected = if is_border { 16 } else { 17 };
            assert_eq!(
                line.width(),
                expected,
                "row {line:?} (border={is_border}) didn't match the expected border/content width pattern"
            );
        }
    }

    /// Regression test: a blockquote's `"┃ "` bar used to be added once per
    /// logical line, so it only showed up on a long paragraph's *first*
    /// on-screen row once `Paragraph::wrap` re-flowed it. `render_block`'s
    /// blockquote arm now pre-wraps each line to the actual column width
    /// (`crate::render::wrap_with_bar`), so every wrapped row carries its
    /// own copy of the bar.
    #[test]
    fn blockquote_bar_repeats_on_every_wrapped_row() {
        let long_text = "word ".repeat(30);
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "blockquote", "content": [
                    { "type": "paragraph", "content": [ { "type": "text", "text": long_text } ] }
                ] }
            ]
        });
        let text = render(&doc, 20);
        assert!(
            text.lines.len() > 3,
            "a long quoted paragraph at width 20 should wrap across several rows"
        );
        for line in &text.lines {
            assert_eq!(line.spans[0].content.as_ref(), "┃ ");
        }
    }

    /// Same bug, same fix, for a code block's `"│ "` bar.
    #[test]
    fn code_block_bar_repeats_on_every_wrapped_row() {
        let long_code = "x".repeat(60);
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "codeBlock",
                  "content": [ { "type": "text", "text": long_code } ] }
            ]
        });
        let text = render(&doc, 20);
        // fence, N wrapped code rows, fence.
        assert!(text.lines.len() > 4);
        for line in &text.lines[1..text.lines.len() - 1] {
            assert_eq!(line.spans[0].content.as_ref(), "│ ");
        }
    }
}
