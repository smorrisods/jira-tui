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

/// Reconstruct the plain text of an inclusive screen-row range from the last
/// rendered frame's buffer (used for drag-to-copy). Takes the buffer
/// directly rather than `Term` — `Terminal::draw` swaps its double buffer
/// before returning, so `terminal.current_buffer_mut()` called after the
/// fact would hand back the blank buffer being prepared for the *next*
/// frame, not the content that's actually on screen. The caller clones
/// `CompletedFrame::buffer` right after `draw()`, while it still refers to
/// the frame just rendered.
pub(crate) fn read_rows(buf: &Buffer, y0: u16, y1: u16) -> String {
    let area = *buf.area();
    let mut out = String::new();
    let last = y1.min(area.height.saturating_sub(1));
    for y in y0..=last {
        let mut line = String::new();
        for x in 0..area.width {
            if let Some(cell) = buf.cell((x, y)) {
                line.push_str(cell.symbol());
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
