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
    let key = app.detail.as_ref().map(|d| d.key.as_str()).unwrap_or("");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(warn()))
        .title(Span::styled(
            format!("  editing {key} · Markdown  "),
            Style::default().fg(warn()).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            "  ^S preview · esc cancel  ",
            Style::default().fg(muted()),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let ed = &app.editor;
    let height = inner.height.max(1) as usize;
    let scroll = if ed.cy >= height {
        ed.cy - height + 1
    } else {
        0
    };

    // Only the visible window is checked against the dictionary on every
    // frame — the fence state carried in from off-screen lines is a cheap
    // marker-count prescan, no dictionary lookups (see
    // `spellcheck::misspelled_spans_in_range`).
    let misspellings = spellcheck::misspelled_spans_in_range(&ed.lines, scroll, height);
    let gutter_w = 4u16;
    let mut lines: Vec<Line> = Vec::new();
    for ((i, line), spans_for_line) in ed
        .lines
        .iter()
        .enumerate()
        .skip(scroll)
        .take(height)
        .zip(misspellings.iter())
    {
        let mut spans = vec![Span::styled(
            format!("{:>3} ", i + 1),
            Style::default().fg(muted()),
        )];
        spans.extend(spans_with_misspellings(line, spans_for_line));
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);

    // Place the real terminal cursor.
    let cx = inner.x + gutter_w + ed.cx as u16;
    let cy = inner.y + (ed.cy - scroll) as u16;
    if cx < inner.x + inner.width && cy < inner.y + inner.height {
        f.set_cursor_position((cx, cy));
    }
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
