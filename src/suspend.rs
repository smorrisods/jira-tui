//! Suspending the TUI to the shell on Ctrl+Z, matching any well-behaved
//! job-control-aware terminal program (`vim`, `less`, `htop`, …).

use std::io;

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

use jira_tui::app::App;

use crate::Term;

/// Leave the alternate screen and raw mode, stop the process with
/// `SIGTSTP` (handing control back to the shell), then restore the TUI once
/// the shell resumes us with `SIGCONT` after `fg`.
///
/// A no-op on platforms without POSIX job control (i.e. Windows — see
/// `stop_self` below), since there's no shell-level suspend/resume to hook
/// into there.
pub(crate) fn suspend(terminal: &mut Term, app: &mut App) -> Result<()> {
    if app.mouse.enabled {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    stop_self();

    // Resume: re-enter raw mode and the alternate screen exactly as
    // `setup_terminal` did on startup.
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    if app.mouse.enabled {
        let _ = execute!(io::stdout(), EnableMouseCapture);
    }
    // The shell (and whatever else ran while we were stopped) may have
    // scribbled over the alternate screen's saved contents; force a full
    // redraw on the next frame rather than trusting ratatui's diff against
    // a buffer that no longer reflects what's on screen.
    terminal.clear()?;
    Ok(())
}

/// Stop this process with `SIGTSTP`, the same signal a shell sends on
/// Ctrl+Z. `raise` blocks until the shell resumes the process (group) with
/// `SIGCONT` after `fg`/`bg`, at which point execution just continues here.
#[cfg(unix)]
fn stop_self() {
    // SAFETY: raising a signal against our own process has no preconditions
    // beyond the process existing, and the default `SIGTSTP` disposition
    // (stop) is never overridden anywhere in this crate.
    unsafe {
        libc::raise(libc::SIGTSTP);
    }
}

/// No POSIX job control to hook into on non-Unix targets (e.g. Windows);
/// Ctrl+Z simply isn't wired up to anything there.
#[cfg(not(unix))]
fn stop_self() {}
