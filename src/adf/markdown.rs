//! ADF document -> Markdown, for the round-trip edit flow.
//!
//! Produces clean Markdown (not the styled terminal view) so a user can edit an
//! issue's description in `$EDITOR` and have it recompiled back to ADF.

use serde_json::Value;

/// Serialise an ADF document to Markdown.
pub fn to_markdown(doc: &Value) -> String {
    let mut blocks: Vec<String> = Vec::new();
    if let Some(content) = doc.get("content").and_then(|c| c.as_array()) {
        for node in content {
            if let Some(text) = block_to_md(node, 0) {
                blocks.push(text);
            }
        }
    }
    let mut out = blocks.join("\n\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn block_to_md(node: &Value, depth: usize) -> Option<String> {
    let ty = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(3) as usize;
            Some(format!(
                "{} {}",
                "#".repeat(level.clamp(1, 6)),
                inline_to_md(node.get("content"))
            ))
        }
        "paragraph" => Some(inline_to_md(node.get("content"))),
        "bulletList" | "orderedList" => {
            let ordered = ty == "orderedList";
            let mut lines = Vec::new();
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for (i, item) in items.iter().enumerate() {
                    let marker = if ordered {
                        format!("{}. ", i + 1)
                    } else {
                        "- ".to_string()
                    };
                    list_item_to_md(item, depth, &marker, &mut lines);
                }
            }
            Some(lines.join("\n"))
        }
        "taskList" => {
            let mut lines = Vec::new();
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for item in items {
                    let done = item
                        .get("attrs")
                        .and_then(|a| a.get("state"))
                        .and_then(|s| s.as_str())
                        == Some("DONE");
                    let box_ = if done { "[x]" } else { "[ ]" };
                    lines.push(format!(
                        "{}- {} {}",
                        indent(depth),
                        box_,
                        inline_to_md(item.get("content"))
                    ));
                }
            }
            Some(lines.join("\n"))
        }
        "codeBlock" => {
            let lang = node
                .get("attrs")
                .and_then(|a| a.get("language"))
                .and_then(|l| l.as_str())
                .unwrap_or("");
            let body = collect_text(node.get("content"));
            Some(format!("```{lang}\n{body}\n```"))
        }
        "rule" => Some("---".to_string()),
        "blockquote" => {
            let mut inner = Vec::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(t) = block_to_md(child, depth) {
                        inner.push(t);
                    }
                }
            }
            Some(
                inner
                    .join("\n")
                    .lines()
                    .map(|l| format!("> {l}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        _ => {
            let text = inline_to_md(node.get("content"));
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
    }
}

fn list_item_to_md(item: &Value, depth: usize, marker: &str, out: &mut Vec<String>) {
    let content = match item.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return,
    };
    let mut first = true;
    for child in content {
        let ty = child.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty == "bulletList" || ty == "orderedList" || ty == "taskList" {
            if let Some(nested) = block_to_md(child, depth + 1) {
                out.push(nested);
            }
            continue;
        }
        let text = inline_to_md(child.get("content"));
        if first {
            out.push(format!("{}{}{}", indent(depth), marker, text));
            first = false;
        } else {
            out.push(format!("{}  {}", indent(depth), text));
        }
    }
}

fn inline_to_md(content: Option<&Value>) -> String {
    let mut out = String::new();
    let arr = match content.and_then(|c| c.as_array()) {
        Some(a) => a,
        None => return out,
    };
    for node in arr {
        match node.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                let mut text = node
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let marks = node.get("marks").and_then(|m| m.as_array());
                let has = |name: &str| {
                    marks
                        .map(|ms| {
                            ms.iter()
                                .any(|m| m.get("type").and_then(|t| t.as_str()) == Some(name))
                        })
                        .unwrap_or(false)
                };
                if has("code") {
                    text = format!("`{text}`");
                } else {
                    if has("strong") {
                        text = format!("**{text}**");
                    }
                    if has("em") {
                        text = format!("*{text}*");
                    }
                    if has("link") {
                        let href = marks
                            .and_then(|ms| {
                                ms.iter().find(|m| {
                                    m.get("type").and_then(|t| t.as_str()) == Some("link")
                                })
                            })
                            .and_then(|m| m.get("attrs"))
                            .and_then(|a| a.get("href"))
                            .and_then(|h| h.as_str())
                            .unwrap_or("");
                        text = format!("[{text}]({href})");
                    }
                }
                out.push_str(&text);
            }
            Some("hardBreak") => out.push('\n'),
            Some("emoji") => {
                if let Some(t) = node
                    .get("attrs")
                    .and_then(|a| a.get("text"))
                    .and_then(|t| t.as_str())
                {
                    out.push_str(t);
                }
            }
            Some("mention") => {
                if let Some(t) = node
                    .get("attrs")
                    .and_then(|a| a.get("text"))
                    .and_then(|t| t.as_str())
                {
                    out.push_str(t);
                }
            }
            _ => {}
        }
    }
    out
}

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
