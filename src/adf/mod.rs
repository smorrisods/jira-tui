//! ADF (Atlassian Document Format) -> styled terminal text.
//!
//! A faithful-enough port of the Python `_render_issue.py` renderer so the TUI
//! shows Jira rich text the way it is actually stored: headings as headings,
//! task lists as checkboxes, code as code. Display only — never round-tripped
//! back to Jira from here.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use serde_json::Value;

const HEADING: Color = Color::Cyan;
const CODE_FG: Color = Color::LightGreen;
const MUTED: Color = Color::DarkGray;

/// Render an ADF document into styled lines.
pub fn render(doc: &Value) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(content) = doc.get("content").and_then(|c| c.as_array()) {
        for (i, node) in content.iter().enumerate() {
            render_block(node, &mut lines, 0);
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

fn render_block(node: &Value, out: &mut Vec<Line<'static>>, depth: usize) {
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
                    render_list_item(item, out, depth, &marker);
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
                out.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(MUTED)),
                    Span::styled(raw.to_string(), Style::default().fg(CODE_FG)),
                ]));
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
                    render_block(child, &mut inner, depth);
                }
            }
            for line in inner {
                let mut spans = vec![Span::styled("┃ ", Style::default().fg(MUTED))];
                spans.extend(line.spans);
                out.push(Line::from(spans));
            }
        }
        "table" => render_table(node, out),
        _ => {
            // generic container: descend if possible
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    render_block(child, out, depth);
                }
            }
        }
    }
}

fn render_list_item(item: &Value, out: &mut Vec<Line<'static>>, depth: usize, marker: &str) {
    let content = match item.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return,
    };
    let mut first = true;
    for child in content {
        let ty = child.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty == "bulletList" || ty == "orderedList" || ty == "taskList" {
            render_block(child, out, depth + 1);
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

fn render_table(node: &Value, out: &mut Vec<Line<'static>>) {
    let rows = match node.get("content").and_then(|c| c.as_array()) {
        Some(r) => r,
        None => return,
    };
    for row in rows {
        let cells = match row.get("content").and_then(|c| c.as_array()) {
            Some(c) => c,
            None => continue,
        };
        let mut spans: Vec<Span<'static>> = vec![Span::styled("│ ", Style::default().fg(MUTED))];
        for cell in cells {
            let is_header = cell.get("type").and_then(|t| t.as_str()) == Some("tableHeader");
            let txt = collect_text(cell.get("content"));
            let style = if is_header {
                Style::default().add_modifier(Modifier::BOLD).fg(HEADING)
            } else {
                Style::default()
            };
            spans.push(Span::styled(format!("{:<16}", txt), style));
            spans.push(Span::styled("│ ", Style::default().fg(MUTED)));
        }
        out.push(Line::from(spans));
    }
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
        let text = render(&doc);
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
