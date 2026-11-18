//! Markdown -> ADF compiler for the round-trip edit flow.
//!
//! Mirrors the mapping rules used by the `jira-ds-skill` pipeline so edits made
//! in `$EDITOR` recompile to proper ADF: headings, paragraphs, bullet/ordered/
//! task lists, fenced code blocks, GFM tables, and inline `code`/**bold**/
//! *italic*/links.

use serde_json::{json, Value};

use super::media;

/// Per-compile counter for the `localId` attribute Jira requires on
/// `taskList` and `taskItem` nodes. Ids are UUID-shaped but deterministic,
/// and unique within one `compile()` call.
struct LocalIds(u64);

impl LocalIds {
    fn new() -> Self {
        Self(0)
    }

    fn next(&mut self) -> String {
        self.0 += 1;
        format!("00000000-0000-4000-8000-{:012x}", self.0)
    }
}

/// Compile Markdown text into an ADF `doc`.
pub fn compile(md: &str) -> Value {
    let mut local_ids = LocalIds::new();
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
        if trimmed.starts_with("```") {
            flush_paragraph!();
            blocks.push(parse_fence(&lines, &mut i, 0));
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

        // Task, bullet, and ordered lists (nesting handled by `parse_list`).
        if let Some(kind) = list_kind(trimmed) {
            flush_paragraph!();
            blocks.push(parse_list(&lines, &mut i, kind, None, &mut local_ids));
            continue;
        }

        // GFM table: a header line followed by a delimiter row.
        if is_table_start(&lines, i) {
            flush_paragraph!();
            blocks.push(parse_table(&lines, &mut i));
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Task,
    Bullet,
    Ordered,
}

fn list_kind(trimmed: &str) -> Option<ListKind> {
    if is_task_item(trimmed) {
        Some(ListKind::Task)
    } else if is_bullet_item(trimmed) {
        Some(ListKind::Bullet)
    } else if ordered_prefix(trimmed).is_some() {
        Some(ListKind::Ordered)
    } else {
        None
    }
}

/// ADF only allows a task list inside a task list, and bullet/ordered lists
/// inside bullet/ordered list items.
fn can_nest(parent: ListKind, child: ListKind) -> bool {
    (parent == ListKind::Task) == (child == ListKind::Task)
}

/// Width of a line's leading whitespace in columns (a tab counts as four).
fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

fn marker_text(kind: ListKind, trimmed: &str) -> &str {
    match kind {
        ListKind::Task => trimmed[5..].trim_start(),
        ListKind::Bullet => &trimmed[2..],
        ListKind::Ordered => trimmed.split_once(". ").map(|x| x.1).unwrap_or(""),
    }
}

/// Lines that start a block of their own even when indented under a list item.
fn starts_own_block(trimmed: &str) -> bool {
    heading_level(trimmed).is_some()
        || trimmed == "---"
        || trimmed == "***"
        || whole_line_image(trimmed.trim_end())
            .is_some_and(|(_, url)| media::is_adf_media_url(&url) && media::decode(&url).is_some())
}

fn strip_indent(line: &str, columns: usize) -> &str {
    let lead = line.chars().take_while(|&c| c == ' ').count().min(columns);
    &line[lead..]
}

/// Parse a fenced code block starting at the opening fence, leaving `i` after
/// the closing fence. Up to `strip` columns of indentation are removed from
/// each body line.
fn parse_fence(lines: &[&str], i: &mut usize, strip: usize) -> Value {
    let language = lines[*i]
        .trim_start()
        .strip_prefix("```")
        .unwrap_or("")
        .trim()
        .to_string();
    let mut body: Vec<&str> = Vec::new();
    *i += 1;
    while *i < lines.len() && !lines[*i].trim_start().starts_with("```") {
        body.push(strip_indent(lines[*i], strip));
        *i += 1;
    }
    *i += 1; // skip closing fence
    let mut node = json!({
        "type": "codeBlock",
        "content": [ { "type": "text", "text": body.join("\n") } ]
    });
    if !language.is_empty() {
        node["attrs"] = json!({ "language": language });
    }
    node
}

/// Parse one list of `kind` starting at the marker line `lines[*i]`.
///
/// A marker indented further than its item's own marker nests inside that
/// item; a line no deeper than `parent_indent` belongs to an enclosing list.
/// Nesting that ADF doesn't allow (a task item under a bullet, or the reverse)
/// ends the list instead, so the caller starts a fresh one.
fn parse_list(
    lines: &[&str],
    i: &mut usize,
    kind: ListKind,
    parent_indent: Option<usize>,
    local_ids: &mut LocalIds,
) -> Value {
    let list_id = (kind == ListKind::Task).then(|| local_ids.next());
    let mut items: Vec<Value> = Vec::new();

    while *i < lines.len() {
        let line = lines[*i];
        let trimmed = line.trim_start();
        let item_indent = indent_of(line);
        if trimmed.is_empty()
            || parent_indent.is_some_and(|p| item_indent <= p)
            || list_kind(trimmed) != Some(kind)
        {
            break;
        }

        let done = trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]");
        let item_id = (kind == ListKind::Task).then(|| local_ids.next());
        let mut text = marker_text(kind, trimmed).to_string();
        let mut children: Vec<Value> = Vec::new();
        *i += 1;

        while *i < lines.len() {
            let line = lines[*i];
            let trimmed = line.trim_start();
            let indent = indent_of(line);
            if trimmed.is_empty() || indent <= item_indent {
                break;
            }
            if is_table_start(lines, *i) {
                // Jira doesn't allow a table inside a list item.
                break;
            }
            if let Some(nested_kind) = list_kind(trimmed) {
                if !can_nest(kind, nested_kind) {
                    break;
                }
                children.push(parse_list(
                    lines,
                    i,
                    nested_kind,
                    Some(item_indent),
                    local_ids,
                ));
            } else if trimmed.starts_with("```") {
                if kind == ListKind::Task {
                    break;
                }
                children.push(parse_fence(lines, i, indent));
            } else if children.is_empty() && !starts_own_block(trimmed) {
                let cont = trimmed.trim_end();
                if text.is_empty() {
                    text = cont.to_string();
                } else {
                    text.push(' ');
                    text.push_str(cont);
                }
                *i += 1;
            } else {
                break;
            }
        }

        if let Some(id) = item_id {
            items.push(json!({
                "type": "taskItem",
                "attrs": {
                    "localId": id,
                    "state": if done { "DONE" } else { "TODO" }
                },
                "content": parse_inline(&text)
            }));
            // A nested task list sits beside its parent item, not inside it.
            items.extend(children);
        } else {
            let mut content = vec![json!({ "type": "paragraph", "content": parse_inline(&text) })];
            content.extend(children);
            items.push(json!({ "type": "listItem", "content": content }));
        }
    }

    match kind {
        ListKind::Task => json!({
            "type": "taskList",
            "attrs": { "localId": list_id },
            "content": items
        }),
        ListKind::Bullet => json!({ "type": "bulletList", "content": items }),
        ListKind::Ordered => json!({ "type": "orderedList", "content": items }),
    }
}

/// Split one table row into cells on unescaped `|`, dropping the optional
/// leading and trailing pipe and turning `\|` into a literal `|`.
fn split_cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut ended_with_pipe = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        ended_with_pipe = false;
        if c == '\\' && chars.peek() == Some(&'|') {
            cell.push('|');
            chars.next();
        } else if c == '|' {
            cells.push(cell.trim().to_string());
            cell = String::new();
            ended_with_pipe = true;
        } else {
            cell.push(c);
        }
    }
    if !ended_with_pipe {
        cells.push(cell.trim().to_string());
    }
    cells
}

fn is_delimiter_cell(cell: &str) -> bool {
    let cell = cell.trim();
    let cell = cell.strip_prefix(':').unwrap_or(cell);
    let cell = cell.strip_suffix(':').unwrap_or(cell);
    !cell.is_empty() && cell.chars().all(|c| c == '-')
}

/// A table delimiter row such as `| --- | :---: | ---: |`.
fn is_delimiter_row(line: &str) -> bool {
    line.contains('|') && split_cells(line).iter().all(|c| is_delimiter_cell(c))
}

/// A table starts at a line containing `|` that is directly followed by a
/// delimiter row; a lone `|` line stays an ordinary paragraph line.
fn is_table_start(lines: &[&str], i: usize) -> bool {
    lines[i].contains('|') && lines.get(i + 1).is_some_and(|next| is_delimiter_row(next))
}

/// Split a cell on `<br>` tags into inline nodes joined by `hardBreak`s.
fn parse_cell(text: &str) -> Vec<Value> {
    let lower = text.to_ascii_lowercase();
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut idx = 0;
    while idx < lower.len() {
        if lower.as_bytes()[idx] == b'<' {
            let tag = ["<br>", "<br/>", "<br />"]
                .iter()
                .find(|t| lower[idx..].starts_with(**t));
            if let Some(tag) = tag {
                parts.push(&text[start..idx]);
                idx += tag.len();
                start = idx;
                continue;
            }
        }
        idx += 1;
    }
    parts.push(&text[start..]);

    let mut nodes: Vec<Value> = Vec::new();
    for part in parts.into_iter().map(str::trim).filter(|p| !p.is_empty()) {
        if !nodes.is_empty() {
            nodes.push(json!({ "type": "hardBreak" }));
        }
        nodes.extend(parse_inline(part));
    }
    nodes
}

/// Parse a GFM table starting at its header line, leaving `i` after its last
/// body row. The header row becomes `tableHeader` cells and the rest
/// `tableCell`s; every row is padded to the widest row's column count.
fn parse_table(lines: &[&str], i: &mut usize) -> Value {
    let mut rows: Vec<Vec<String>> = vec![split_cells(lines[*i])];
    *i += 2; // header line and delimiter row
    while *i < lines.len() {
        let trimmed = lines[*i].trim_start();
        if trimmed.is_empty()
            || !trimmed.contains('|')
            || trimmed.starts_with("```")
            || heading_level(trimmed).is_some()
            || list_kind(trimmed).is_some()
        {
            break;
        }
        rows.push(split_cells(lines[*i]));
        *i += 1;
    }

    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let rows: Vec<Value> = rows
        .into_iter()
        .enumerate()
        .map(|(row_idx, mut cells)| {
            cells.resize(width, String::new());
            let cell_type = if row_idx == 0 {
                "tableHeader"
            } else {
                "tableCell"
            };
            let cells: Vec<Value> = cells
                .iter()
                .map(|text| {
                    json!({
                        "type": cell_type,
                        "content": [ { "type": "paragraph", "content": parse_cell(text) } ]
                    })
                })
                .collect();
            json!({ "type": "tableRow", "content": cells })
        })
        .collect();

    json!({
        "type": "table",
        "attrs": { "isNumberColumnEnabled": false, "layout": "default" },
        "content": rows
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
        // Strikethrough: ~~...~~
        if chars[i] == '~' && i + 1 < chars.len() && chars[i + 1] == '~' {
            if let Some(end) = find_seq(&chars, i + 2, &['~', '~']).filter(|&end| end > i + 2) {
                flush!();
                let inner: String = chars[i + 2..end].iter().collect();
                nodes.push(json!({
                    "type": "text", "text": inner,
                    "marks": [ { "type": "strike" } ]
                }));
                i = end + 2;
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

    fn collect_task_local_ids(v: &Value, out: &mut Vec<Value>) {
        if matches!(
            v.get("type").and_then(|t| t.as_str()),
            Some("taskList" | "taskItem")
        ) {
            out.push(v["attrs"]["localId"].clone());
        }
        if let Some(a) = v.get("content").and_then(|c| c.as_array()) {
            for c in a {
                collect_task_local_ids(c, out);
            }
        }
    }

    #[test]
    fn task_nodes_carry_non_empty_local_ids() {
        let doc = compile("- [x] done\n- [ ] todo");
        let mut ids = Vec::new();
        collect_task_local_ids(&doc, &mut ids);
        assert_eq!(ids.len(), 3);
        for id in &ids {
            assert!(id.as_str().is_some_and(|s| !s.is_empty()), "bad id: {id}");
        }
        assert_eq!(doc["content"][0]["content"][0]["attrs"]["state"], "DONE");
    }

    #[test]
    fn task_local_ids_are_unique_across_lists_in_one_document() {
        let doc = compile("- [ ] a\n- [x] b\n\npara\n\n- [ ] c\n- [ ] d");
        let mut ids = Vec::new();
        collect_task_local_ids(&doc, &mut ids);
        assert_eq!(ids.len(), 6);
        let unique: std::collections::HashSet<_> = ids.iter().map(|v| v.to_string()).collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn task_local_ids_are_deterministic() {
        let md = "- [ ] a\n- [x] b\n\n- [ ] c";
        assert_eq!(compile(md), compile(md));
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
    fn strikethrough_compiles_to_a_strike_mark() {
        let nodes = parse_inline("some ~~gone~~ text");
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0]["text"], "some ");
        assert_eq!(nodes[1]["text"], "gone");
        assert_eq!(nodes[1]["marks"], json!([{ "type": "strike" }]));
        assert_eq!(nodes[2]["text"], " text");
    }

    #[test]
    fn unclosed_or_lone_tildes_stay_literal() {
        for input in ["~~never closed", "about ~5 minutes", "a ~ b ~ c", "~~~~"] {
            let nodes = parse_inline(input);
            assert_eq!(nodes.len(), 1, "{input}");
            assert_eq!(nodes[0]["text"], input);
            assert!(nodes[0].get("marks").is_none(), "{input}");
        }
    }

    #[test]
    fn strikethrough_round_trips_through_markdown() {
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [
                    { "type": "text", "text": "keep " },
                    { "type": "text", "text": "gone", "marks": [ { "type": "strike" } ] },
                    { "type": "text", "text": " keep" }
                ]
            }]
        });
        let md = to_markdown(&original);
        assert!(md.contains("keep ~~gone~~ keep"));
        let recompiled = compile(&md);
        let content = recompiled["content"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);
        assert_eq!(content[1]["text"], "gone");
        assert_eq!(content[1]["marks"], json!([{ "type": "strike" }]));
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

    fn types_of(v: &Value) -> Vec<&str> {
        v["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["type"].as_str().unwrap())
            .collect()
    }

    fn item_text(item: &Value) -> String {
        match item["type"].as_str() {
            Some("taskItem") => text_of(item),
            _ => text_of(&item["content"][0]),
        }
    }

    /// Shape of a list tree as `(type, [(text, nested)])`, ignoring `localId`s.
    fn shape(v: &Value) -> Value {
        let mut items = Vec::new();
        for item in v["content"].as_array().unwrap() {
            match item["type"].as_str().unwrap() {
                "taskList" => items.push(json!({ "nested": shape(item) })),
                ty => {
                    let nested: Vec<Value> = if ty == "listItem" {
                        item["content"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .skip(1)
                            .map(|c| match c["type"].as_str().unwrap() {
                                "codeBlock" => json!({ "type": "codeBlock", "text": text_of(c) }),
                                _ => shape(c),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    items.push(json!({
                        "type": ty,
                        "state": item["attrs"]["state"],
                        "text": item_text(item),
                        "nested": nested
                    }));
                }
            }
        }
        json!({ "list": v["type"], "items": items })
    }

    #[test]
    fn nested_bullet_list_compiles_to_nested_bullet_list() {
        let doc = compile("- a\n  - b\n  - c\n- d");
        assert_eq!(block_types(&doc), vec!["bulletList"]);
        let items = doc["content"][0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(item_text(&items[0]), "a");
        assert_eq!(item_text(&items[1]), "d");
        let a = items[0]["content"].as_array().unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0]["type"], "paragraph");
        assert_eq!(a[1]["type"], "bulletList");
        let nested = a[1]["content"].as_array().unwrap();
        assert_eq!(nested.len(), 2);
        assert_eq!(item_text(&nested[0]), "b");
        assert_eq!(item_text(&nested[1]), "c");
        assert_eq!(items[1]["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn ordered_list_nests_inside_bullet_and_the_reverse() {
        let doc = compile("- a\n  1. b\n  2. c\n\n1. x\n   - y\n2. z");
        assert_eq!(block_types(&doc), vec!["bulletList", "orderedList"]);
        let bullet = &doc["content"][0]["content"][0]["content"];
        assert_eq!(bullet[1]["type"], "orderedList");
        assert_eq!(bullet[1]["content"].as_array().unwrap().len(), 2);
        let ordered = &doc["content"][1];
        assert_eq!(ordered["content"].as_array().unwrap().len(), 2);
        assert_eq!(ordered["content"][0]["content"][1]["type"], "bulletList");
        assert_eq!(item_text(&ordered["content"][1]), "z");
    }

    #[test]
    fn ordered_list_nests_inside_ordered_list() {
        let doc = compile("1. a\n  1. b\n2. c");
        assert_eq!(block_types(&doc), vec!["orderedList"]);
        assert_eq!(
            doc["content"][0]["content"][0]["content"][1]["type"],
            "orderedList"
        );
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn lists_nest_three_levels_deep() {
        let doc = compile("- a\n  - b\n    - c\n  - d\n- e");
        assert_eq!(
            shape(&doc["content"][0]),
            json!({ "list": "bulletList", "items": [
                { "type": "listItem", "state": null, "text": "a", "nested": [
                    { "list": "bulletList", "items": [
                        { "type": "listItem", "state": null, "text": "b", "nested": [
                            { "list": "bulletList", "items": [
                                { "type": "listItem", "state": null, "text": "c", "nested": [] }
                            ] }
                        ] },
                        { "type": "listItem", "state": null, "text": "d", "nested": [] }
                    ] }
                ] },
                { "type": "listItem", "state": null, "text": "e", "nested": [] }
            ] })
        );
    }

    #[test]
    fn four_space_indentation_nests() {
        let doc = compile("- a\n    - b\n        - c\n- d");
        assert_eq!(block_types(&doc), vec!["bulletList"]);
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 2);
        let b = &doc["content"][0]["content"][0]["content"][1]["content"][0];
        assert_eq!(item_text(b), "b");
        assert_eq!(b["content"][1]["type"], "bulletList");
    }

    #[test]
    fn changing_marker_kind_at_the_same_depth_starts_a_sibling_list() {
        let doc = compile("- a\n  - b\n  1. c\n- d\n1. e");
        assert_eq!(block_types(&doc), vec!["bulletList", "orderedList"]);
        let a = doc["content"][0]["content"][0]["content"]
            .as_array()
            .unwrap();
        assert_eq!(
            a.iter()
                .map(|n| n["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["paragraph", "bulletList", "orderedList"]
        );
    }

    #[test]
    fn indented_continuation_line_extends_the_item_paragraph() {
        let doc = compile("- first line\n  second line\n- next");
        let items = doc["content"][0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(item_text(&items[0]), "first line second line");
        assert_eq!(items[0]["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unindented_line_after_a_list_still_ends_it() {
        let doc = compile("- a\n  - b\nplain");
        assert_eq!(block_types(&doc), vec!["bulletList", "paragraph"]);
    }

    #[test]
    fn indented_fenced_code_block_nests_inside_the_item() {
        let doc = compile("- a\n  ```rust\n  let x = 1;\n    indented\n  ```\n- b");
        assert_eq!(block_types(&doc), vec!["bulletList"]);
        let items = doc["content"][0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        let code = &items[0]["content"][1];
        assert_eq!(code["type"], "codeBlock");
        assert_eq!(code["attrs"]["language"], "rust");
        assert_eq!(text_of(code), "let x = 1;\n  indented");
    }

    #[test]
    fn nested_task_list_is_a_child_of_the_parent_task_list() {
        let doc = compile("- [ ] a\n  - [x] b\n  - [ ] c\n- [ ] d");
        assert_eq!(block_types(&doc), vec!["taskList"]);
        let list = &doc["content"][0];
        assert_eq!(types_of(list), vec!["taskItem", "taskList", "taskItem"]);
        let nested = &list["content"][1];
        assert_eq!(types_of(nested), vec!["taskItem", "taskItem"]);
        assert_eq!(nested["content"][0]["attrs"]["state"], "DONE");
        assert_eq!(item_text(&list["content"][2]), "d");

        let mut ids = Vec::new();
        collect_task_local_ids(&doc, &mut ids);
        assert_eq!(ids.len(), 6);
        let unique: std::collections::HashSet<_> = ids.iter().map(|v| v.to_string()).collect();
        assert_eq!(unique.len(), ids.len());
        assert!(ids
            .iter()
            .all(|id| id.as_str().is_some_and(|s| !s.is_empty())));
    }

    #[test]
    fn task_item_under_a_bullet_closes_the_list() {
        let doc = compile("- a\n  - [ ] t\n- b");
        assert_eq!(
            block_types(&doc),
            vec!["bulletList", "taskList", "bulletList"]
        );
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn bullet_under_a_task_item_closes_the_list() {
        let doc = compile("- [ ] a\n  - b\n- [ ] c");
        assert_eq!(
            block_types(&doc),
            vec!["taskList", "bulletList", "taskList"]
        );
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn deeply_nested_disallowed_item_closes_every_enclosing_list() {
        let doc = compile("- a\n  - b\n    - [ ] t\n- c");
        assert_eq!(
            block_types(&doc),
            vec!["bulletList", "taskList", "bulletList"]
        );
    }

    #[test]
    fn nested_lists_round_trip_through_markdown() {
        let li = |text: &str, nested: Vec<Value>| {
            let mut content = vec![json!({
                "type": "paragraph",
                "content": [ { "type": "text", "text": text } ]
            })];
            content.extend(nested);
            json!({ "type": "listItem", "content": content })
        };
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [
                { "type": "bulletList", "content": [
                    li("a", vec![
                        json!({ "type": "orderedList", "content": [
                            li("b", vec![
                                json!({ "type": "bulletList", "content": [ li("c", vec![]) ] })
                            ]),
                            li("d", vec![])
                        ] })
                    ]),
                    li("e", vec![])
                ] },
                { "type": "orderedList", "content": [
                    li("one", vec![
                        json!({ "type": "orderedList", "content": [ li("one-a", vec![]) ] })
                    ]),
                    li("two", vec![])
                ] }
            ]
        });
        let md = to_markdown(&original);
        let recompiled = compile(&md);
        assert_eq!(recompiled["content"].as_array().unwrap().len(), 2);
        for (got, want) in recompiled["content"]
            .as_array()
            .unwrap()
            .iter()
            .zip(original["content"].as_array().unwrap())
        {
            assert_eq!(shape(got), shape(want), "markdown was:\n{md}");
        }
    }

    #[test]
    fn nested_task_list_round_trips_through_markdown() {
        let ti = |id: &str, state: &str, text: &str| {
            json!({
                "type": "taskItem",
                "attrs": { "localId": id, "state": state },
                "content": [ { "type": "text", "text": text } ]
            })
        };
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [ {
                "type": "taskList",
                "attrs": { "localId": "l1" },
                "content": [
                    ti("i1", "TODO", "parent"),
                    { "type": "taskList", "attrs": { "localId": "l2" }, "content": [
                        ti("i2", "DONE", "child"),
                        { "type": "taskList", "attrs": { "localId": "l3" }, "content": [
                            ti("i3", "TODO", "grandchild")
                        ] },
                        ti("i4", "TODO", "child two")
                    ] },
                    ti("i5", "DONE", "sibling")
                ]
            } ]
        });
        let md = to_markdown(&original);
        assert_eq!(
            md,
            "- [ ] parent\n  - [x] child\n    - [ ] grandchild\n  - [ ] child two\n- [x] sibling\n"
        );
        let recompiled = compile(&md);
        assert_eq!(
            shape(&recompiled["content"][0]),
            shape(&original["content"][0])
        );
    }

    #[test]
    fn code_block_in_list_item_round_trips_through_markdown() {
        let original = json!({
            "type": "doc",
            "version": 1,
            "content": [ { "type": "bulletList", "content": [ {
                "type": "listItem",
                "content": [
                    { "type": "paragraph", "content": [ { "type": "text", "text": "a" } ] },
                    { "type": "codeBlock", "attrs": { "language": "sh" },
                      "content": [ { "type": "text", "text": "ls\n  -l" } ] }
                ]
            } ] } ]
        });
        let md = to_markdown(&original);
        let recompiled = compile(&md);
        assert_eq!(recompiled, original);
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

    fn table_rows(table: &Value) -> Vec<Vec<String>> {
        table["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                row["content"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(text_of)
                    .collect()
            })
            .collect()
    }

    fn cell_types(table: &Value, row: usize) -> Vec<&str> {
        table["content"][row]["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["type"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn compiles_the_issue_110_table() {
        let md = "| Icon asset | Used for | Matches reported selector? |\n\
                  | --- | --- | --- |\n\
                  | `foo.svg` | Thing A | Yes |\n\
                  | `bar.svg` | Thing B | No |";
        let doc = compile(md);
        assert_eq!(block_types(&doc), vec!["table"]);
        let table = &doc["content"][0];
        assert_eq!(table["attrs"]["isNumberColumnEnabled"], false);
        assert_eq!(table["attrs"]["layout"], "default");
        assert_eq!(table["content"].as_array().unwrap().len(), 3);
        assert_eq!(
            cell_types(table, 0),
            vec!["tableHeader", "tableHeader", "tableHeader"]
        );
        assert_eq!(
            cell_types(table, 2),
            vec!["tableCell", "tableCell", "tableCell"]
        );
        assert_eq!(table_rows(table)[1], vec!["foo.svg", "Thing A", "Yes"]);
        let code = &table["content"][1]["content"][0]["content"][0]["content"][0];
        assert_eq!(code["marks"][0]["type"], "code");
    }

    #[test]
    fn table_delimiter_accepts_alignment_colons() {
        let doc = compile("| a | b | c |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |");
        assert_eq!(block_types(&doc), vec!["table"]);
        assert_eq!(table_rows(&doc["content"][0])[1], vec!["1", "2", "3"]);
    }

    #[test]
    fn table_without_outer_pipes_compiles() {
        let doc = compile("a | b\n--|--\n1 | 2");
        assert_eq!(block_types(&doc), vec!["table"]);
        assert_eq!(table_rows(&doc["content"][0])[1], vec!["1", "2"]);
    }

    #[test]
    fn escaped_pipe_stays_inside_its_cell() {
        let doc = compile("| a | b |\n| --- | --- |\n| x \\| y | z |");
        let rows = table_rows(&doc["content"][0]);
        assert_eq!(rows[1], vec!["x | y", "z"]);
    }

    #[test]
    fn empty_cell_is_a_paragraph_with_empty_content() {
        let doc = compile("| a | b |\n| --- | --- |\n|  | z |");
        let cell = &doc["content"][0]["content"][1]["content"][0];
        assert_eq!(cell["type"], "tableCell");
        assert_eq!(cell["content"][0]["type"], "paragraph");
        assert_eq!(cell["content"][0]["content"], json!([]));
    }

    #[test]
    fn short_and_long_rows_are_padded_to_the_widest_row() {
        let doc = compile("| a | b |\n| --- | --- |\n| 1 |\n| 1 | 2 | 3 |");
        let table = &doc["content"][0];
        let rows = table_rows(table);
        assert_eq!(rows[0], vec!["a", "b", ""]);
        assert_eq!(rows[1], vec!["1", "", ""]);
        assert_eq!(rows[2], vec!["1", "2", "3"]);
        assert_eq!(cell_types(table, 0).len(), 3);
    }

    #[test]
    fn br_in_a_cell_becomes_a_hard_break() {
        let doc = compile("| a |\n| --- |\n| one<br>two |");
        let para = &doc["content"][0]["content"][1]["content"][0]["content"][0];
        let types: Vec<&str> = para["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["type"].as_str().unwrap())
            .collect();
        assert_eq!(types, vec!["text", "hardBreak", "text"]);
    }

    #[test]
    fn pipe_line_without_a_delimiter_row_stays_a_paragraph() {
        let doc = compile("| not | a table |\n| really | no |");
        assert_eq!(block_types(&doc), vec!["paragraph"]);
        assert!(text_of(&doc).contains("| not | a table |"));
    }

    #[test]
    fn table_interrupts_a_paragraph_without_a_blank_line() {
        let doc = compile("intro text\n| a | b |\n| --- | --- |\n| 1 | 2 |");
        assert_eq!(block_types(&doc), vec!["paragraph", "table"]);
        assert_eq!(text_of(&doc["content"][0]), "intro text");
    }

    #[test]
    fn paragraph_after_a_table_ends_it_once_the_pipes_stop() {
        let doc = compile("| a |\n| --- |\n| 1 |\n\nafter");
        assert_eq!(block_types(&doc), vec!["table", "paragraph"]);
        let doc = compile("| a |\n| --- |\n| 1 |\nafter");
        assert_eq!(block_types(&doc), vec!["table", "paragraph"]);
    }

    #[test]
    fn table_directly_after_list_items_ends_the_list() {
        let doc = compile("- one\n- two\n| a | b |\n| --- | --- |\n| 1 | 2 |");
        assert_eq!(block_types(&doc), vec!["bulletList", "table"]);
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn indented_table_under_a_list_item_is_never_nested() {
        let doc = compile("- one\n  | a | b |\n  | --- | --- |\n  | 1 | 2 |");
        assert_eq!(block_types(&doc), vec!["bulletList", "table"]);
        let item = &doc["content"][0]["content"][0];
        assert_eq!(item["content"].as_array().unwrap().len(), 1);
        assert_eq!(table_rows(&doc["content"][1])[1], vec!["1", "2"]);
    }

    #[test]
    fn indented_table_under_a_nested_task_item_ends_every_list() {
        let doc = compile("- [ ] one\n  - [ ] two\n    | a |\n    | --- |\n    | 1 |");
        assert_eq!(block_types(&doc), vec!["taskList", "table"]);
    }

    #[test]
    fn markdown_table_round_trips_through_markdown() {
        let md = "- item\n\n| Icon | Used for |\n| --- | --- |\n| `foo.svg` | a \\| b |\n|  | **bold** |\n";
        let doc = compile(md);
        assert_eq!(block_types(&doc), vec!["bulletList", "table"]);
        let out = to_markdown(&doc);
        assert_eq!(out, md);
        assert_eq!(compile(&out), doc);
    }

    #[test]
    fn adf_table_round_trips_through_markdown_with_structure_intact() {
        let cell = |ty: &str, text: &str| {
            let content = if text.is_empty() {
                json!([])
            } else {
                json!([{ "type": "text", "text": text }])
            };
            json!({ "type": ty, "content": [{ "type": "paragraph", "content": content }] })
        };
        let doc = json!({
            "type": "doc", "version": 1,
            "content": [{
                "type": "table",
                "attrs": { "isNumberColumnEnabled": false, "layout": "default" },
                "content": [
                    { "type": "tableRow", "content": [cell("tableHeader", "h1"), cell("tableHeader", "h|2")] },
                    { "type": "tableRow", "content": [cell("tableCell", "a"), cell("tableCell", "")] }
                ]
            }]
        });
        let back = compile(&to_markdown(&doc));
        assert_eq!(back["content"], doc["content"]);
    }
}
