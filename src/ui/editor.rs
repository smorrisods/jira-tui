//! The built-in multi-line Markdown editor for description edits.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::spellcheck;

use super::{danger, muted, warn};

/// One logical editor line's on-screen row shape — either its ordinary
/// word-wrapped text rows (`Text`, today's only case), or, when the `images`
/// feature is compiled in and `App::editor_image_view` is on, a whole-line
/// `adf-media://` token that's actually ready to paint as an image
/// (`Image`). See `layout_for_line`/`ready_image_for_line`.
enum LineLayout {
    Text(Vec<usize>),
    #[cfg(feature = "images")]
    Image(EditorImageLine),
}

impl LineLayout {
    /// How many terminal rows this logical line occupies — `starts.len()`
    /// for wrapped text, or the image's own reserved row count. Used
    /// uniformly by every pass over the buffer (cursor placement, scroll,
    /// visible-range, and the final paint loop) so all four always agree on
    /// exactly the same row budget per line.
    fn rows(&self) -> usize {
        match self {
            LineLayout::Text(starts) => starts.len(),
            #[cfg(feature = "images")]
            LineLayout::Image(img) => img.rows as usize,
        }
    }
}

/// A logical line's resolved, ready-to-paint image (`images` feature only)
/// — `key` is whichever `InlineImageKey` `App::resolve_editor_media_key`
/// resolved the line's token to, already confirmed decoded and cached
/// (`App::inline_images`); `rows`/`cols` come from `App::editor_image_rows_cols`.
#[cfg(feature = "images")]
#[derive(Clone)]
struct EditorImageLine {
    key: crate::app::InlineImageKey,
    rows: u16,
    cols: u16,
}

/// Decide one logical `line`'s `LineLayout` — an image only when the
/// `images` feature is compiled in, `App::editor_image_view` is on, the line
/// is nothing but a whole-line `adf-media://` token, that token resolves to
/// an `InlineImageKey`, and that key's image is already decoded in
/// `App::inline_images`. Everything else (toggle off, unresolved/still
/// -fetching token, or no token at all) falls back to today's plain
/// word-wrapped text — exactly the "collapse back to text" behaviour the
/// toggle exists for.
#[cfg_attr(not(feature = "images"), allow(unused_variables))]
fn layout_for_line(app: &App, line: &str, text_width: u16, max_image_rows: u16) -> LineLayout {
    #[cfg(feature = "images")]
    if let Some(img) = ready_image_for_line(app, line, text_width, max_image_rows) {
        return LineLayout::Image(img);
    }
    LineLayout::Text(wrap_row_starts(line, text_width as usize))
}

#[cfg(feature = "images")]
fn ready_image_for_line(
    app: &App,
    line: &str,
    text_width: u16,
    max_image_rows: u16,
) -> Option<EditorImageLine> {
    if !app.editor_image_view {
        return None;
    }
    let token = crate::app::whole_line_media_url(line)?;
    let key = app.resolve_editor_media_key(token)?;
    if !app.inline_images.borrow().contains_key(&key) {
        return None;
    }
    let (rows, cols) = app.editor_image_rows_cols(&key, text_width, max_image_rows)?;
    Some(EditorImageLine { key, rows, cols })
}

pub(crate) fn draw_editor(f: &mut Frame, app: &App, area: Rect) {
    // A new issue has no key yet — title on its project instead.
    let title = if app.edit_target == crate::app::EditTarget::NewIssue {
        format!("  new issue in {} · Markdown  ", app.new_issue.project)
    } else {
        // `edit_key`, not `app.detail` — a comment composed from the
        // quick-view panel (Home/List) targets the quick-viewed issue, which
        // can differ from whatever `app.detail` last held from a previous
        // Detail-screen visit. `edit_key` is what `apply_comment`/
        // `apply_description_edit` actually post to, so the title must
        // agree with it or the label lies about which issue is being edited.
        let key = app.edit_key.as_deref().unwrap_or("");
        format!("  editing {key} · Markdown  ")
    };
    // `pending_image_embed` (a staged image awaiting an upload-and-embed
    // confirmation, see `App::begin_image_embed`) and `confirm_discard` are
    // mutually exclusive in ordinary play (`keys::handle_key` swallows every
    // keypress while either modal is showing, before the other could ever be
    // raised), but `pending_image_embed` still wins the tie-break here if
    // both were somehow set — a stray unconfirmed upload is the more
    // consequential of the two to lose track of visually.
    let modal_active = app.pending_image_embed.is_some() || app.confirm_discard;
    let border_colour = if modal_active { danger() } else { warn() };
    let bottom_hint = if let Some(path) = app.pending_image_embed.as_ref() {
        format!(
            "  insert {} as inline image? y/⏎ confirm · esc = plain text  ",
            path.display()
        )
    } else if app.confirm_discard {
        "  discard this edit? y = discard, any other key = keep editing  ".to_string()
    } else {
        "  ^S preview · esc cancel  ".to_string()
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
            Style::default().fg(if modal_active { danger() } else { muted() }),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let ed = &app.editor;
    let height = inner.height.max(1) as usize;
    let gutter_w = 4u16;
    let text_width = inner.width.saturating_sub(gutter_w).max(1);
    // Half the pane's height, so a single embedded image can never swallow
    // the whole editor — clamped into the same row band Detail/quick view's
    // own inline images use (see `inline_images::MIN_INLINE_IMAGE_ROWS`'s
    // own doc comment), which is the whole point of a collapsible toggle: a
    // weirdly-shaped or oversized image shouldn't dominate a small terminal.
    #[cfg(feature = "images")]
    let max_image_rows = (inner.height / 2).clamp(
        crate::app::inline_images::MIN_INLINE_IMAGE_ROWS,
        crate::app::inline_images::MAX_INLINE_IMAGE_ROWS,
    );
    #[cfg(not(feature = "images"))]
    let max_image_rows = 0u16;

    // Word-wrap every logical line to the pane width (or, when the toggle
    // and cache line up, size it as an image instead — see
    // `layout_for_line`), and locate the cursor's visual row/column within
    // its line's rows.
    let layouts: Vec<LineLayout> = ed
        .lines
        .iter()
        .map(|line| layout_for_line(app, line, text_width, max_image_rows))
        .collect();

    let mut cursor_row = 0usize;
    let mut cursor_col = 0usize;
    let mut visual_row = 0usize;
    for (i, layout) in layouts.iter().enumerate() {
        if i == ed.cy {
            match layout {
                LineLayout::Text(starts) => {
                    let local = starts.partition_point(|&s| s <= ed.cx).saturating_sub(1);
                    cursor_row = visual_row + local;
                    cursor_col = ed.cx - starts[local];
                }
                // An image line has no meaningful text column to place a
                // cursor within — park it at the top-left of the reserved
                // block; arrow keys/backspace still operate on the
                // underlying raw text regardless of where the cursor glyph
                // visually sits.
                #[cfg(feature = "images")]
                LineLayout::Image(_) => {
                    cursor_row = visual_row;
                    cursor_col = 0;
                }
            }
        }
        visual_row += layout.rows();
    }

    let scroll = if cursor_row >= height {
        cursor_row - height + 1
    } else {
        0
    };

    // Which logical lines actually produce a visible row — so only those
    // get checked against the dictionary on every frame, not the whole
    // buffer (the fence state `misspelled_spans_in_range` derives for lines
    // before `first_visible` is a cheap marker-count prescan, no dictionary
    // lookups).
    let mut first_visible = 0usize;
    let mut last_visible = 0usize;
    {
        let mut vr = 0usize;
        let mut found_first = false;
        for (i, layout) in layouts.iter().enumerate() {
            if !found_first && vr + layout.rows() > scroll {
                first_visible = i;
                found_first = true;
            }
            if found_first {
                last_visible = i;
            }
            vr += layout.rows();
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
    #[cfg(feature = "images")]
    let mut pending_images: Vec<(i32, EditorImageLine)> = Vec::new();
    let mut visual_row: usize = layouts[..first_visible].iter().map(LineLayout::rows).sum();
    'outer: for i in first_visible..=last_visible {
        let line = &ed.lines[i];
        match &layouts[i] {
            LineLayout::Text(starts) => {
                let byte_of: Vec<usize> = line
                    .char_indices()
                    .map(|(b, _)| b)
                    .chain(std::iter::once(line.len()))
                    .collect();
                let misspelled_bytes = &misspellings[i - first_visible];
                for (r, &start) in starts.iter().enumerate() {
                    if visual_row >= scroll {
                        let end = starts.get(r + 1).copied().unwrap_or(byte_of.len() - 1);
                        let byte_start = byte_of[start];
                        let byte_end = byte_of[end];
                        let segment = &line[byte_start..byte_end];
                        // Clip this logical line's misspelled byte-spans to
                        // the segment's own byte range, and shift them
                        // relative to it.
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
            // Reserve `img.rows` blank rows in the scrolled text (so the
            // Paragraph below doesn't render the raw token text underneath
            // it) and record where it landed for the paint pass just after
            // — mirrors `ui::detail::paint_inline_images`'s own
            // scroll-relative `y` (which can go negative for a
            // partially-scrolled-above placement; `SlicedImage` clips that
            // on its own, so this doesn't need to pre-clip the row count).
            #[cfg(feature = "images")]
            LineLayout::Image(img) => {
                let y = visual_row as i32 - scroll as i32;
                pending_images.push((y, img.clone()));
                for _ in 0..img.rows {
                    if visual_row >= scroll {
                        lines.push(Line::default());
                        if lines.len() == height {
                            break 'outer;
                        }
                    }
                    visual_row += 1;
                }
            }
        }
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);

    #[cfg(feature = "images")]
    for (y, img) in pending_images {
        if y + i32::from(img.rows) <= 0 || y >= i32::from(inner.height) {
            continue;
        }
        let Some(protocol) = app.inline_image_protocol_for_key(
            &img.key,
            ratatui::layout::Size::new(img.cols, img.rows),
        ) else {
            continue;
        };
        let y = y.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let position = ratatui_image::sliced::SignedPosition::from((gutter_w as i16, y));
        f.render_widget(
            ratatui_image::sliced::SlicedImage::new(&protocol, position),
            inner,
        );
    }

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
