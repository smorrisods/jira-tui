//! Suspending the TUI to hand the terminal to an external `$EDITOR`, and
//! reading rendered terminal buffer text back out (used for drag-to-copy).

use std::io;

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::buffer::Buffer;

use jira_tui::app::App;

use crate::Term;

/// Reconstruct the plain text of an inclusive, character-precise selection
/// span from the last rendered frame's buffer (used for drag-to-copy) —
/// mirrors `ui::draw`'s highlight rendering exactly: the first and last row
/// are trimmed to their own start/end column, and only rows genuinely in
/// between (a multi-row drag) are read in full. Takes the buffer directly
/// rather than `Term` — `Terminal::draw` swaps its double buffer before
/// returning, so `terminal.current_buffer_mut()` called after the fact
/// would hand back the blank buffer being prepared for the *next* frame,
/// not the content that's actually on screen. The caller clones
/// `CompletedFrame::buffer` right after `draw()`, while it still refers to
/// the frame just rendered.
pub(crate) fn read_span(buf: &Buffer, start: (u16, u16), end: (u16, u16)) -> String {
    let area = *buf.area();
    let (y0, x0) = start;
    let (y1, x1) = end;
    let x_max = area.width.saturating_sub(1);
    let y_max = area.height.saturating_sub(1);
    let y1c = y1.min(y_max);
    let mut out = String::new();
    for y in y0.min(y_max)..=y1c {
        let (row_x0, row_x1) = if y0 == y1 {
            (x0, x1)
        } else if y == y0 {
            (x0, x_max)
        } else if y == y1c {
            (0, x1)
        } else {
            (0, x_max)
        };
        let row_x1 = row_x1.min(x_max);
        let mut line = String::new();
        if row_x0 <= row_x1 {
            for x in row_x0..=row_x1 {
                if let Some(cell) = buf.cell((x, y)) {
                    line.push_str(cell.symbol());
                }
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Suspend the TUI, open the issue description in `$EDITOR`, then resume and
/// hand the edited Markdown to the app for compilation + preview.
pub(crate) fn edit_in_editor(terminal: &mut Term, app: &mut App) -> Result<()> {
    let Some(markdown) = app.description_markdown() else {
        return Ok(());
    };
    let key = app
        .detail
        .as_ref()
        .map(|d| d.key.clone())
        .unwrap_or_else(|| "issue".into());
    let path = std::env::temp_dir().join(format!("jira-tui-{key}.md"));
    std::fs::write(&path, &markdown)?;

    // Leave the alternate screen and hand the terminal to the editor.
    if app.mouse.enabled {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    // Support editors invoked with arguments, e.g. `code --wait`.
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let status = std::process::Command::new(program)
        .args(parts)
        .arg(&path)
        .status();

    // Resume the TUI.
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    if app.mouse.enabled {
        let _ = execute!(io::stdout(), EnableMouseCapture);
    }
    terminal.clear()?;

    match status {
        Ok(s) if s.success() => {
            let edited = std::fs::read_to_string(&path)?;
            let _ = std::fs::remove_file(&path);
            if edited.trim() == markdown.trim() {
                app.status = "no changes".into();
            } else {
                app.finish_edit(&edited);
            }
        }
        Ok(_) => app.status = "editor exited with an error".into(),
        Err(e) => app.status = format!("could not launch editor '{editor}': {e}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::widgets::Paragraph;
    use ratatui::Terminal;

    fn render_text(text: &str, width: u16, height: u16) -> Buffer {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| f.render_widget(Paragraph::new(text.to_string()), f.area()))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn read_span_extracts_only_the_selected_columns_on_a_single_row() {
        let buf = render_text("hello world", 20, 1);
        assert_eq!(read_span(&buf, (0, 0), (0, 4)), "hello\n");
    }

    #[test]
    fn read_span_trims_first_and_last_rows_but_reads_middle_rows_in_full() {
        let buf = render_text("aaaaaaaaaa\nbbbbbbbbbb\ncccccccccc", 10, 3);
        // Start at row 0, column 5; end at row 2, column 2 (inclusive, so
        // columns 0..=2 of the last row — 3 characters, "ccc").
        let text = read_span(&buf, (0, 5), (2, 2));
        assert_eq!(text, "aaaaa\nbbbbbbbbbb\nccc\n");
    }

    #[test]
    fn read_span_clamps_a_row_past_the_buffers_height() {
        let buf = render_text("only one row", 20, 1);
        // end row (5) is past the 1-row buffer — must clamp, not panic.
        let text = read_span(&buf, (0, 0), (5, 3));
        assert_eq!(text, "only one row\n");
    }
}
