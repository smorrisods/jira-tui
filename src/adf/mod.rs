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

/// Render an ADF document into styled lines. `width` is the column width
/// the caller is about to hand these lines to a `Paragraph::wrap(Wrap {
/// trim: false })` at — needed so blockquote/code-block content can be
/// pre-wrapped ourselves (see `crate::render::wrap_with_bar`) rather than
/// left to ratatui's own wrap, which has no way to repeat a left-margin bar
/// span on a line's wrapped continuation rows.
pub fn render(doc: &Value, width: usize) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(content) = doc.get("content").and_then(|c| c.as_array()) {
        for (i, node) in content.iter().enumerate() {
            render_block(node, &mut lines, 0, width);
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
    Text::from(lines)
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn render_block(node: &Value, out: &mut Vec<Line<'static>>, depth: usize, width: usize) {
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
                    render_list_item(item, out, depth, &marker, width);
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
                out.extend(crate::render::wrap_with_bar(&line, width, "│ ", MUTED));
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
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    render_block(child, &mut inner, depth, width);
                }
            }
            for line in inner {
                out.extend(crate::render::wrap_with_bar(&line, width, "┃ ", MUTED));
            }
        }
        "table" => render_table(node, out, width),
        _ => {
            // generic container: descend if possible
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    render_block(child, out, depth, width);
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
    width: usize,
) {
    let content = match item.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return,
    };
    let mut first = true;
    for child in content {
        let ty = child.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty == "bulletList" || ty == "orderedList" || ty == "taskList" {
            render_block(child, out, depth + 1, width);
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
