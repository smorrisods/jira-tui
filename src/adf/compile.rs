//! Markdown -> ADF compiler for the round-trip edit flow.
//!
//! Mirrors the mapping rules used by the `jira-ds-skill` pipeline so edits made
//! in `$EDITOR` recompile to proper ADF: headings, paragraphs, bullet/ordered/
//! task lists, fenced code blocks, and inline `code`/**bold**/*italic*/links.

use serde_json::{json, Value};

/// Compile Markdown text into an ADF `doc`.
pub fn compile(md: &str) -> Value {
    let mut blocks: Vec<Value> = Vec::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    let mut paragraph: Vec<&str> = Vec::new();

    macro_rules! flush_paragraph {
        () => {
            if !paragraph.is_empty() {
                let text = paragraph.join(" ");
                blocks.push(json!({ "type": "paragraph", "content": parse_inline(&text) }));
                paragraph.clear();
            }
        };
    }

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // Fenced code block.
        if let Some(lang) = trimmed.strip_prefix("```") {
            flush_paragraph!();
            let language = lang.trim().to_string();
            let mut body: Vec<&str> = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                body.push(lines[i]);
                i += 1;
            }
            i += 1; // skip closing fence
            let mut node = json!({
                "type": "codeBlock",
                "content": [ { "type": "text", "text": body.join("\n") } ]
            });
            if !language.is_empty() {
                node["attrs"] = json!({ "language": language });
            }
            blocks.push(node);
            continue;
        }

        // Blank line ends a paragraph.
        if trimmed.is_empty() {
            flush_paragraph!();
            i += 1;
            continue;
        }

        // Heading.
        if let Some(level) = heading_level(trimmed) {
            flush_paragraph!();
            let text = trimmed[level..].trim_start();
            blocks.push(json!({
                "type": "heading",
                "attrs": { "level": level.min(6) },
                "content": parse_inline(text)
            }));
            i += 1;
            continue;
        }

        // Horizontal rule.
        if trimmed == "---" || trimmed == "***" {
            flush_paragraph!();
            blocks.push(json!({ "type": "rule" }));
            i += 1;
            continue;
        }

        // Task list.
        if is_task_item(trimmed) {
            flush_paragraph!();
            let mut items: Vec<Value> = Vec::new();
            while i < lines.len() && is_task_item(lines[i].trim_start()) {
                let t = lines[i].trim_start();
                let done = t.starts_with("- [x]") || t.starts_with("- [X]");
                let text = &t[5..].trim_start();
                items.push(json!({
                    "type": "taskItem",
                    "attrs": { "state": if done { "DONE" } else { "TODO" } },
                    "content": parse_inline(text)
                }));
                i += 1;
            }
            blocks.push(json!({ "type": "taskList", "content": items }));
            continue;
        }

        // Bullet list.
        if is_bullet_item(trimmed) {
            flush_paragraph!();
            let mut items: Vec<Value> = Vec::new();
            while i < lines.len()
                && is_bullet_item(lines[i].trim_start())
                && !is_task_item(lines[i].trim_start())
            {
                let t = lines[i].trim_start();
                let text = &t[2..];
                items.push(list_item(text));
                i += 1;
            }
            blocks.push(json!({ "type": "bulletList", "content": items }));
            continue;
        }

        // Ordered list.
        if let Some(_n) = ordered_prefix(trimmed) {
            flush_paragraph!();
            let mut items: Vec<Value> = Vec::new();
            while i < lines.len() && ordered_prefix(lines[i].trim_start()).is_some() {
                let t = lines[i].trim_start();
                let text = t.split_once(". ").map(|x| x.1).unwrap_or("");
                items.push(list_item(text));
                i += 1;
            }
            blocks.push(json!({ "type": "orderedList", "content": items }));
            continue;
        }

        // Otherwise, accumulate into a paragraph.
        paragraph.push(line.trim());
        i += 1;
    }
    flush_paragraph!();

    json!({ "type": "doc", "version": 1, "content": blocks })
}

fn list_item(text: &str) -> Value {
    json!({
        "type": "listItem",
        "content": [ { "type": "paragraph", "content": parse_inline(text) } ]
    })
}

fn heading_level(s: &str) -> Option<usize> {
    let hashes = s.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && s.chars().nth(hashes) == Some(' ') {
        Some(hashes)
    } else {
        None
    }
}

fn is_task_item(s: &str) -> bool {
    s.starts_with("- [ ]") || s.starts_with("- [x]") || s.starts_with("- [X]")
}

fn is_bullet_item(s: &str) -> bool {
    (s.starts_with("- ") || s.starts_with("* ")) && !is_task_item(s)
}

fn ordered_prefix(s: &str) -> Option<usize> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &s[digits.len()..];
    if rest.starts_with(". ") {
        digits.parse().ok()
    } else {
        None
    }
}

/// Parse inline Markdown into ADF text nodes with marks.
#[allow(unused_assignments)]
pub fn parse_inline(text: &str) -> Vec<Value> {
    let chars: Vec<char> = text.chars().collect();
    let mut nodes: Vec<Value> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                nodes.push(json!({ "type": "text", "text": buf }));
                buf = String::new();
            }
        };
    }

    while i < chars.len() {
        // Inline code: `...`
        if chars[i] == '`' {
            if let Some(end) = find_char(&chars, i + 1, '`') {
                flush!();
                let inner: String = chars[i + 1..end].iter().collect();
                nodes.push(json!({
                    "type": "text", "text": inner,
                    "marks": [ { "type": "code" } ]
                }));
                i = end + 1;
                continue;
            }
        }
        // Bold: **...**
        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            if let Some(end) = find_seq(&chars, i + 2, &['*', '*']) {
                flush!();
                let inner: String = chars[i + 2..end].iter().collect();
                nodes.push(json!({
                    "type": "text", "text": inner,
                    "marks": [ { "type": "strong" } ]
                }));
                i = end + 2;
                continue;
            }
        }
        // Italic: *...*
        if chars[i] == '*' {
            if let Some(end) = find_char(&chars, i + 1, '*') {
                flush!();
                let inner: String = chars[i + 1..end].iter().collect();
                nodes.push(json!({
                    "type": "text", "text": inner,
                    "marks": [ { "type": "em" } ]
                }));
                i = end + 1;
                continue;
            }
        }
        // Link: [text](href)
        if chars[i] == '[' {
            if let Some((label, href, next)) = parse_link(&chars, i) {
                flush!();
                nodes.push(json!({
                    "type": "text", "text": label,
                    "marks": [ { "type": "link", "attrs": { "href": href } } ]
                }));
                i = next;
                continue;
            }
        }
        buf.push(chars[i]);
        i += 1;
    }
    flush!();

    if nodes.is_empty() {
        nodes.push(json!({ "type": "text", "text": "" }));
    }
    nodes
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    (start..chars.len()).find(|&j| chars[j] == target)
}

fn find_seq(chars: &[char], start: usize, seq: &[char]) -> Option<usize> {
    let mut j = start;
    while j + seq.len() <= chars.len() {
        if chars[j..j + seq.len()] == *seq {
            return Some(j);
        }
        j += 1;
    }
    None
}

fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let close = find_char(chars, start + 1, ']')?;
    if close + 1 >= chars.len() || chars[close + 1] != '(' {
        return None;
    }
    let paren = find_char(chars, close + 2, ')')?;
    let label: String = chars[start + 1..close].iter().collect();
    let href: String = chars[close + 2..paren].iter().collect();
    Some((label, href, paren + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adf::to_markdown;

    fn text_of(doc: &Value) -> String {
        // flatten all text nodes
        fn walk(v: &Value, out: &mut String) {
            if v.get("type").and_then(|t| t.as_str()) == Some("text") {
                out.push_str(v.get("text").and_then(|t| t.as_str()).unwrap_or(""));
            }
            if let Some(a) = v.get("content").and_then(|c| c.as_array()) {
                for c in a {
                    walk(c, out);
                }
            }
        }
        let mut s = String::new();
        walk(doc, &mut s);
        s
    }

    fn block_types(doc: &Value) -> Vec<String> {
        doc.get("content")
            .and_then(|c| c.as_array())
            .map(|a| {
                a.iter()
                    .map(|n| {
                        n.get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn compiles_headings_and_paragraphs() {
        let doc = compile("## Title\n\nHello world");
        assert_eq!(block_types(&doc), vec!["heading", "paragraph"]);
        assert_eq!(doc["content"][0]["attrs"]["level"].as_u64(), Some(2));
        assert!(text_of(&doc).contains("Title"));
        assert!(text_of(&doc).contains("Hello world"));
    }

    #[test]
    fn compiles_task_and_bullet_lists() {
        let doc = compile("- [x] done\n- [ ] todo\n\n- one\n- two");
        let types = block_types(&doc);
        assert!(types.contains(&"taskList".to_string()));
        assert!(types.contains(&"bulletList".to_string()));
        let tasks = &doc["content"][0]["content"];
        assert_eq!(tasks[0]["attrs"]["state"].as_str(), Some("DONE"));
        assert_eq!(tasks[1]["attrs"]["state"].as_str(), Some("TODO"));
    }

    #[test]
    fn compiles_ordered_list_and_code_block() {
        let doc = compile("1. first\n2. second\n\n```rust\nlet x = 1;\n```");
        let types = block_types(&doc);
        assert!(types.contains(&"orderedList".to_string()));
        assert!(types.contains(&"codeBlock".to_string()));
        let code = doc["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["type"] == "codeBlock")
            .unwrap();
        assert_eq!(code["attrs"]["language"].as_str(), Some("rust"));
        assert!(text_of(code).contains("let x = 1;"));
    }

    #[test]
    fn parses_inline_marks() {
        let nodes = parse_inline("a `code` **bold** *em* [x](http://y)");
        let has_mark = |name: &str| {
            nodes.iter().any(|n| {
                n.get("marks")
                    .and_then(|m| m.as_array())
                    .map(|ms| ms.iter().any(|m| m["type"] == name))
                    .unwrap_or(false)
            })
        };
        assert!(has_mark("code"));
        assert!(has_mark("strong"));
        assert!(has_mark("em"));
        assert!(has_mark("link"));
    }

    #[test]
    fn round_trip_preserves_structure() {
        let original = crate::domain::demo_detail("DS-2725").description;
        let md = to_markdown(&original);
        let recompiled = compile(&md);
        // The important structural nodes survive a round trip.
        let types = block_types(&recompiled);
        assert!(types.contains(&"heading".to_string()));
        assert!(types.contains(&"bulletList".to_string()));
        assert!(types.contains(&"codeBlock".to_string()));
        assert!(types.contains(&"taskList".to_string()));
        // And the code content is intact.
        assert!(text_of(&recompiled).contains("beforematch"));
    }

    #[test]
    fn empty_input_yields_empty_doc() {
        let doc = compile("");
        assert_eq!(doc["type"], "doc");
        assert_eq!(doc["content"].as_array().unwrap().len(), 0);
    }
}
