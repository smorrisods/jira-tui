//! Markdown -> ADF compiler for the round-trip edit flow.
//!
//! Mirrors the mapping rules used by the `jira-ds-skill` pipeline so edits made
//! in `$EDITOR` recompile to proper ADF: headings, paragraphs, bullet/ordered/
//! task lists, fenced code blocks, and inline `code`/**bold**/*italic*/links.

use serde_json::{json, Value};

use super::media;

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

        // Standalone image reference (nothing else on the line): matches
        // how Jira's own editor treats a lone image as its own media
        // block rather than inline text (see the `mediaSingle`/`mediaGroup`
        // render arms in `src/adf/mod.rs`). Only one of our own
        // `adf-media://` round-trip tokens (see `media`) gets reconstructed
        // into a real `media` node here; anything else (a plain filename or
        // a real URL a human typed) falls through to the paragraph path,
        // where `parse_inline`'s image branch neutralizes it as inert text
        // — `compile()` has no attachment-lookup capability to turn that
        // into a real embed.
        if let Some((_alt, url)) = whole_line_image(trimmed.trim_end()) {
            if media::is_adf_media_url(&url) {
                if let Some(decoded) = media::decode(&url) {
                    flush_paragraph!();
                    match decoded.wrapper {
                        media::DecodedWrapper::Group => {
                            let mut children =
                                vec![json!({ "type": "media", "attrs": decoded.media_attrs })];
                            i += 1;
                            while i < lines.len() {
                                let t = lines[i].trim();
                                let next = whole_line_image(t).and_then(|(_, u)| {
                                    if media::is_adf_media_url(&u) {
                                        media::decode(&u)
                                    } else {
                                        None
                                    }
                                });
                                match next {
                                    Some(d)
                                        if matches!(d.wrapper, media::DecodedWrapper::Group) =>
                                    {
                                        children.push(
                                            json!({ "type": "media", "attrs": d.media_attrs }),
                                        );
                                        i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            blocks.push(json!({ "type": "mediaGroup", "content": children }));
                        }
                        media::DecodedWrapper::Single(ms_attrs) => {
                            let mut node = json!({
                                "type": "mediaSingle",
                                "content": [ { "type": "media", "attrs": decoded.media_attrs } ]
                            });
                            if ms_attrs.as_object().is_some_and(|o| !o.is_empty()) {
                                node["attrs"] = ms_attrs;
                            }
                            blocks.push(node);
                            i += 1;
                        }
                        media::DecodedWrapper::None => {
                            blocks.push(json!({ "type": "media", "attrs": decoded.media_attrs }));
                            i += 1;
                        }
                    }
                    continue;
                }
            }
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

/// Recognizes a line that is *only* an image reference (`![alt](url)`,
/// with nothing else before or after it), the same way `is_bullet_item`/
/// `heading_level` recognize other block-starting syntax. Reuses
/// `parse_link`'s `[label](href)` scanner (an image is just `!` + that
/// shape) rather than a separate hand-rolled matcher.
fn whole_line_image(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('!')?;
    let chars: Vec<char> = rest.chars().collect();
    if chars.first() != Some(&'[') {
        return None;
    }
    let (label, href, next) = parse_link(&chars, 0)?;
    if next == chars.len() {
        Some((label, href))
    } else {
        None
    }
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
        // Image: ![alt](url) — tried before the link branch below, since
        // otherwise `!` lands in the plain-text buffer and the following
        // `[alt](url)` matches the link branch on its own, fabricating a
        // bogus hyperlink literally named "alt" (see issue #122). A
        // standalone image on its own line is reconstructed into a real
        // `media` node earlier, at the block level in `compile()` (see
        // `whole_line_image`), before `parse_inline` ever runs on it; if
        // one shows up here instead (inline, mixed with other text, or a
        // plain filename/URL that isn't one of this crate's own
        // `adf-media://` tokens), there's no attachment-lookup capability
        // at this layer to turn it into a real embed, so it collapses to
        // inert text rather than a link.
        if chars[i] == '!' && i + 1 < chars.len() && chars[i + 1] == '[' {
            if let Some((label, href, next)) = parse_link(&chars, i + 1) {
                flush!();
                nodes.push(json!({ "type": "text", "text": format!("![{label}]({href})") }));
                i = next;
                continue;
            }
        }
        // Mention: [~accountid:XXXX] (Jira wiki-markup convention) — tried
        // before the link syntax below, since a real `[text](href)` link
        // never starts with `~accountid:` and so falls through untouched.
        if chars[i] == '[' {
            if let Some((account_id, next)) = parse_mention(&chars, i) {
                flush!();
                nodes.push(json!({
                    "type": "mention",
                    "attrs": { "id": account_id, "text": "@mention" }
                }));
                i = next;
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

/// Match the Jira wiki-markup mention convention `[~accountid:XXXXX]` — no
/// trailing `(href)`, which is what distinguishes it from a real link.
fn parse_mention(chars: &[char], start: usize) -> Option<(String, usize)> {
    // chars[start] == '['
    let prefix: Vec<char> = "~accountid:".chars().collect();
    let body_start = start + 1;
    if body_start + prefix.len() > chars.len()
        || chars[body_start..body_start + prefix.len()] != prefix[..]
    {
        return None;
    }
    let close = find_char(chars, body_start + prefix.len(), ']')?;
    let id: String = chars[body_start + prefix.len()..close].iter().collect();
    if id.trim().is_empty() {
        return None;
    }
    Some((id, close + 1))
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
    fn parses_mention_token() {
        let nodes = parse_inline("hey [~accountid:abc123] check this");
        let mention = nodes
            .iter()
            .find(|n| n["type"] == "mention")
            .expect("expected a mention node");
        assert_eq!(mention["attrs"]["id"].as_str(), Some("abc123"));

        let text: String = nodes
            .iter()
            .filter(|n| n["type"] == "text")
            .filter_map(|n| n["text"].as_str())
            .collect();
        assert!(text.contains("hey"));
        assert!(text.contains("check this"));
    }

    #[test]
    fn malformed_mention_falls_through_to_link_or_text() {
        // Empty accountId: parse_mention must decline, so this either
        // parses as a normal link (it isn't one, so it stays literal text)
        // rather than emitting a bogus mention node.
        let nodes = parse_inline("[~accountid:] not a mention");
        assert!(nodes.iter().all(|n| n["type"] != "mention"));
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

    /// True if a "link" mark/node type shows up anywhere in the tree —
    /// used to prove the image-syntax fix (issue #122) never fabricates a
    /// link out of `![alt](url)`, regardless of where in the JSON shape
    /// such a mark would land.
    fn contains_link_mark(v: &Value) -> bool {
        if v.get("type").and_then(|t| t.as_str()) == Some("link") {
            return true;
        }
        match v {
            Value::Array(a) => a.iter().any(contains_link_mark),
            Value::Object(o) => o.values().any(contains_link_mark),
            _ => false,
        }
    }

    #[test]
    fn media_single_round_trips_through_markdown() {
        // A realistic mediaSingle > media node (the shape a real Jira
        // description carries for an inline image), run through the full
        // to_markdown -> compile round trip. This is the core regression
        // test for the data-loss bug (#122): before the fix, `to_markdown`
        // silently dropped media nodes entirely.
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "mediaSingle",
                    "attrs": { "layout": "center", "width": 800, "widthType": "pixel" },
                    "content": [
                        {
                            "type": "media",
                            "attrs": {
                                "type": "file",
                                "id": "5f2d9c3a-1234-4abc-9def-abcdef012345",
                                "collection": "",
                                "width": 800,
                                "height": 381,
                                "alt": "screenshot.png"
                            }
                        }
                    ]
                }
            ]
        });

        let md = to_markdown(&original);
        let recompiled = compile(&md);

        let orig_node = &original["content"][0];
        let got_node = &recompiled["content"][0];
        assert_eq!(got_node["type"], orig_node["type"]);
        assert_eq!(got_node["attrs"], orig_node["attrs"]);
        assert_eq!(
            got_node["content"][0]["type"],
            orig_node["content"][0]["type"]
        );
        assert_eq!(
            got_node["content"][0]["attrs"],
            orig_node["content"][0]["attrs"]
        );
    }

    #[test]
    fn media_group_round_trips_with_order_preserved() {
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "mediaGroup",
                    "content": [
                        {
                            "type": "media",
                            "attrs": { "type": "file", "id": "id-one", "collection": "", "alt": "first" }
                        },
                        {
                            "type": "media",
                            "attrs": { "type": "file", "id": "id-two", "collection": "", "alt": "second" }
                        }
                    ]
                }
            ]
        });

        let md = to_markdown(&original);
        let recompiled = compile(&md);

        let got_node = &recompiled["content"][0];
        assert_eq!(got_node["type"], "mediaGroup");
        let got_children = got_node["content"].as_array().expect("mediaGroup content");
        let orig_children = original["content"][0]["content"].as_array().unwrap();
        assert_eq!(got_children.len(), orig_children.len());
        for (got, orig) in got_children.iter().zip(orig_children.iter()) {
            assert_eq!(got["type"], orig["type"]);
            assert_eq!(got["attrs"], orig["attrs"]);
        }
    }

    #[test]
    fn plain_image_reference_compiles_to_inert_text_not_link() {
        // A plain filename (not one of this crate's own `adf-media://`
        // tokens) must never produce a bogus link styled with the alt text
        // — the core regression test for the bogus-link bug (#122).
        let doc = compile("![alt](some-filename.png)");
        assert!(!contains_link_mark(&doc));
        assert!(text_of(&doc).contains("![alt](some-filename.png)"));
    }

    #[test]
    fn plain_external_image_url_compiles_to_inert_text_not_link() {
        let doc = compile("See ![a diagram](https://example.com/diagram.png) above.");
        assert!(!contains_link_mark(&doc));
        assert!(text_of(&doc).contains("![a diagram](https://example.com/diagram.png)"));
    }

    #[test]
    fn adf_media_token_round_trips_special_characters() {
        // `&`, spaces, and unicode in `alt`, plus `&`/`=` in `collection`,
        // all need percent-encoding to survive the query string intact.
        let tricky_alt = "weird & spacey näme (v2).png";
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "media",
                    "attrs": {
                        "type": "file",
                        "id": "abc-123",
                        "collection": "proj=DS&team=x",
                        "alt": tricky_alt
                    }
                }
            ]
        });

        let md = to_markdown(&original);
        let recompiled = compile(&md);
        assert_eq!(
            recompiled["content"][0]["attrs"],
            original["content"][0]["attrs"]
        );
    }

    #[test]
    fn adf_media_token_round_trips_when_alt_contains_a_closing_bracket() {
        // `alt` containing a literal `]` would otherwise close the
        // Markdown bracket early and make the token unrecognizable —
        // `media::encode` sanitizes only the cosmetic bracket text, while
        // the query param (authoritative on decode) keeps the exact
        // original string.
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "media",
                    "attrs": { "type": "file", "id": "abc-123", "alt": "img].png" }
                }
            ]
        });

        let md = to_markdown(&original);
        let recompiled = compile(&md);
        assert_eq!(recompiled["content"][0]["type"], "media");
        assert_eq!(
            recompiled["content"][0]["attrs"],
            original["content"][0]["attrs"]
        );
    }

    #[test]
    fn adf_media_token_survives_trailing_whitespace_on_its_line() {
        // A standalone image line with trailing whitespace (plausible after
        // an `$EDITOR` save) must still be recognized as a media block, not
        // silently degrade into an inert-text paragraph.
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [
                { "type": "media", "attrs": { "type": "file", "id": "abc-123" } }
            ]
        });
        let md = to_markdown(&original);
        let padded = format!("{}   ", md.trim_end());
        let recompiled = compile(&padded);
        assert_eq!(recompiled["content"][0]["type"], "media");
        assert_eq!(
            recompiled["content"][0]["attrs"],
            original["content"][0]["attrs"]
        );
    }

    #[test]
    fn adf_media_token_preserves_whole_number_float_dimensions() {
        // A width/height that arrives as a whole-number JSON float (e.g.
        // `800.0`) must not silently become an integer `800` on the way
        // back — the two `Number` representations aren't `==`, and Jira
        // would receive a different wire value than it sent.
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "media",
                    "attrs": {
                        "type": "file",
                        "id": "abc-123",
                        "width": 800.0,
                        "height": 600.0
                    }
                }
            ]
        });
        let md = to_markdown(&original);
        let recompiled = compile(&md);
        assert_eq!(
            recompiled["content"][0]["attrs"],
            original["content"][0]["attrs"]
        );
    }
}
