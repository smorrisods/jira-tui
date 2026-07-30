//! The built-in multi-line Markdown editor for description edits.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::spellcheck;

use super::{danger, muted, warn};

pub(crate) fn draw_editor(f: &mut Frame, app: &App, area: Rect) {
    // A new issue has no key yet — title on its project instead.
    let title = if app.edit_target == crate::app::EditTarget::NewIssue {
        format!("  new issue in {} · Markdown  ", app.new_issue.project)
    } else {
        let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
        format!("  editing {key} · Markdown  ")
    };
    let border_colour = if app.confirm_discard {
        danger()
    } else {
        warn()
    };
    let bottom_hint = if app.confirm_discard {
        "  discard this edit? y = discard, any other key = keep editing  "
    } else {
        "  ^S preview · esc cancel  "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(border_colour))
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_colour)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            bottom_hint,
            Style::default().fg(if app.confirm_discard {
                danger()
            } else {
                muted()
            }),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let ed = &app.editor;
    let height = inner.height.max(1) as usize;
    let gutter_w = 4u16;
    let text_width = inner.width.saturating_sub(gutter_w).max(1) as usize;

    // Word-wrap every logical line to the pane width, and locate the
    // cursor's visual row/column within its line's wrapped rows.
    let row_starts: Vec<Vec<usize>> = ed
        .lines
        .iter()
        .map(|line| wrap_row_starts(line, text_width))
        .collect();

    let mut cursor_row = 0usize;
    let mut cursor_col = 0usize;
    let mut visual_row = 0usize;
    for (i, starts) in row_starts.iter().enumerate() {
        if i == ed.cy {
            let local = starts.partition_point(|&s| s <= ed.cx).saturating_sub(1);
            cursor_row = visual_row + local;
            cursor_col = ed.cx - starts[local];
        }
        visual_row += starts.len();
    }

    let scroll = if cursor_row >= height {
        cursor_row - height + 1
    } else {
        0
    };

    // Which logical lines actually produce a visible wrapped row — so only
    // those get checked against the dictionary on every frame, not the
    // whole buffer (the fence state `misspelled_spans_in_range` derives for
    // lines before `first_visible` is a cheap marker-count prescan, no
    // dictionary lookups).
    let mut first_visible = 0usize;
    let mut last_visible = 0usize;
    {
        let mut vr = 0usize;
        let mut found_first = false;
        for (i, starts) in row_starts.iter().enumerate() {
            if !found_first && vr + starts.len() > scroll {
                first_visible = i;
                found_first = true;
            }
            if found_first {
                last_visible = i;
            }
            vr += starts.len();
            if vr >= scroll + height {
                break;
            }
        }
    }
    let misspellings = spellcheck::misspelled_spans_in_range(
        &ed.lines,
        first_visible,
        last_visible - first_visible + 1,
    );

    let mut lines: Vec<Line> = Vec::new();
    let mut visual_row: usize = row_starts[..first_visible].iter().map(Vec::len).sum();
    'outer: for i in first_visible..=last_visible {
        let line = &ed.lines[i];
        let byte_of: Vec<usize> = line
            .char_indices()
            .map(|(b, _)| b)
            .chain(std::iter::once(line.len()))
            .collect();
        let misspelled_bytes = &misspellings[i - first_visible];
        let starts = &row_starts[i];
        for (r, &start) in starts.iter().enumerate() {
            if visual_row >= scroll {
                let end = starts.get(r + 1).copied().unwrap_or(byte_of.len() - 1);
                let byte_start = byte_of[start];
                let byte_end = byte_of[end];
                let segment = &line[byte_start..byte_end];
                // Clip this logical line's misspelled byte-spans to the
                // segment's own byte range, and shift them relative to it.
                let segment_spans: Vec<(usize, usize)> = misspelled_bytes
                    .iter()
                    .filter(|&&(s, e)| s >= byte_start && e <= byte_end)
                    .map(|&(s, e)| (s - byte_start, e - byte_start))
                    .collect();
                let gutter = if r == 0 {
                    Span::styled(format!("{:>3} ", i + 1), Style::default().fg(muted()))
                } else {
                    Span::raw("    ")
                };
                let mut spans = vec![gutter];
                spans.extend(spans_with_misspellings(segment, &segment_spans));
                lines.push(Line::from(spans));
                if lines.len() == height {
                    break 'outer;
                }
            }
            visual_row += 1;
        }
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);

    // Place the real terminal cursor.
    if cursor_row >= scroll {
        let cx = inner.x + gutter_w + cursor_col as u16;
        let cy = inner.y + (cursor_row - scroll) as u16;
        if cx < inner.x + inner.width && cy < inner.y + inner.height {
            f.set_cursor_position((cx, cy));
        }
    }
}

/// The char offsets in `line` where each word-wrapped visual row begins,
/// greedily packing whole words (with their trailing space) into rows no
/// wider than `width`. A word longer than `width` on its own is hard-broken
/// at the width boundary. Always returns at least one entry (`0`), even for
/// an empty line.
fn wrap_row_starts(line: &str, width: usize) -> Vec<usize> {
    let width = width.max(1);
    if line.is_empty() {
        return vec![0];
    }
    let mut starts = vec![0usize];
    let mut row_start = 0usize;
    for (tok_start, tok_end) in tokenize(line) {
        if tok_end - row_start <= width {
            continue;
        }
        if tok_start > row_start {
            starts.push(tok_start);
            row_start = tok_start;
        }
        while tok_end - row_start > width {
            row_start += width;
            starts.push(row_start);
        }
    }
    starts
}

/// Splits `line` into tokens of (a run of non-space chars) + (any spaces
/// immediately following it), as char-index ranges, so concatenating the
/// tokens back together reproduces the original line exactly.
fn tokenize(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        while i < chars.len() && chars[i] != ' ' {
            i += 1;
        }
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        tokens.push((start, i));
    }
    tokens
}

/// Splits `line` into spans, styling the byte ranges in `misspelled`
/// (already sorted, non-overlapping — see `spellcheck::misspelled_spans`)
/// with an underline so they stand out without changing the surrounding
/// text's own colour.
fn spans_with_misspellings<'a>(line: &'a str, misspelled: &[(usize, usize)]) -> Vec<Span<'a>> {
    debug_assert!(
        misspelled.windows(2).all(|w| w[0].1 <= w[1].0),
        "misspelled spans must be sorted and non-overlapping: {misspelled:?}"
    );
    debug_assert!(
        misspelled.last().is_none_or(|&(_, end)| end <= line.len()),
        "a misspelled span must not run past the end of its line"
    );
    let mut spans = Vec::new();
    let mut pos = 0usize;
    for &(start, end) in misspelled {
        if start > pos {
            spans.push(Span::raw(&line[pos..start]));
        }
        spans.push(Span::styled(
            &line[start..end],
            Style::default()
                .fg(danger())
                .add_modifier(Modifier::UNDERLINED),
        ));
        pos = end;
    }
    if pos < line.len() {
        spans.push(Span::raw(&line[pos..]));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_line_has_one_row() {
        assert_eq!(wrap_row_starts("", 10), vec![0]);
    }

    #[test]
    fn short_line_fits_one_row() {
        assert_eq!(wrap_row_starts("hello world", 20), vec![0]);
    }

    #[test]
    fn wraps_on_word_boundaries() {
        let line = "hello world this is a test of wrapping logic";
        assert_eq!(wrap_row_starts(line, 10), vec![0, 6, 12, 22, 30, 39]);
    }

    #[test]
    fn hard_breaks_a_word_longer_than_the_width() {
        // A single 12-char "word" with no spaces, width 5.
        assert_eq!(wrap_row_starts("abcdefghijkl", 5), vec![0, 5, 10]);
    }

    #[test]
    fn hard_break_mid_document_after_a_short_row() {
        let line = "hi supercalifragilisticexpialidocious";
        // "hi " fits (3 <= 5); the long word can't, so it hard-breaks from
        // its own start rather than from the empty remainder of row 0.
        assert_eq!(wrap_row_starts(line, 5), vec![0, 3, 8, 13, 18, 23, 28, 33]);
    }

    #[test]
    fn tokens_reconstruct_the_original_line() {
        let line = "  a  bb   ccc";
        let toks = tokenize(line);
        let rebuilt: String = toks
            .iter()
            .map(|&(s, e)| line.chars().skip(s).take(e - s).collect::<String>())
            .collect();
        assert_eq!(rebuilt, line);
    }
}
