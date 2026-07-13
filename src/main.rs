//! jira-tui — a keyboard-driven Jira terminal UI with a little bit of soul.

use jira_tui::{app, config, ui};

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio_stream::StreamExt;

use app::{App, Screen};
use cli::Cli;

mod cli;
mod editor_launch;
mod keys;

/// Frame cadence — also the animation tick rate for the About panel.
const TICK: Duration = Duration::from_millis(90);

type Term = Terminal<CrosstermBackend<Stdout>>;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.init {
        return init_config();
    }

    if cli.no_cache {
        // SAFETY: single-threaded at this point in startup, before any
        // other code (including Settings::load(), called from App::new)
        // reads env vars.
        unsafe {
            std::env::set_var("JIRA_NO_CACHE", "1");
        }
    }

    let mut app = App::new(cli.demo);
    if cli.onboard {
        app.screen = Screen::Welcome;
        app.onboarding.welcome_phase = app::WelcomePhase::Intro;
    }
    if cli.about {
        app.screen = Screen::About;
    }

    let mut terminal = setup_terminal()?;
    install_panic_hook();
    if app.mouse.enabled {
        let _ = execute!(io::stdout(), EnableMouseCapture);
    }
    let result = run(&mut terminal, &mut app).await;
    let _ = execute!(io::stdout(), DisableMouseCapture);
    // Drop any issue-specific title set while running rather than leaving it
    // stuck in the shell's tab/window after we hand the terminal back.
    let _ = execute!(io::stdout(), SetTitle("jira-tui"));
    restore_terminal(&mut terminal)?;
    result
}

/// The async run loop. Input arrives over a `crossterm::EventStream`, the
/// animation cadence over a `tokio::time::interval`, and completed
/// background fetches (a `refresh`/`switch_view` against live Jira; see
/// `app::async_ops`) over `app.events_rx` — all three raced with
/// `tokio::select!` so none of them starves the others.
async fn run(terminal: &mut Term, app: &mut App) -> Result<()> {
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    // `Burst` (tokio's default) replays every tick missed during a stall
    // back-to-back once polling resumes — e.g. after the genuinely
    // blocking `$EDITOR` handoff in `editor_launch::edit_in_editor`, which
    // freezes this whole task for the editor's lifetime. `Delay` instead
    // just pushes the next tick out by one `TICK`, matching the old
    // `event::poll(TICK)` loop's behaviour and avoiding a redraw/animation
    // burst right after the editor closes.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick fires immediately; that's fine, it just draws once.

    // Empty so the very first loop iteration always issues a `SetTitle`,
    // even if the initial state resolves to the plain "jira-tui" title.
    let mut window_title = String::new();

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Reflect the issue currently being viewed (full detail, its
        // preview/edit flow, or the quick-view panel) in the window title,
        // only touching the terminal when it actually changes.
        let title = app.window_title();
        if title != window_title {
            let _ = execute!(terminal.backend_mut(), SetTitle(&title));
            window_title = title;
        }

        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        keys::handle_key(app, key);
                    }
                    Some(Ok(Event::Mouse(me))) => keys::handle_mouse(app, me),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => return Ok(()),
                }
            }
            _ = ticker.tick() => {}
            // A background refresh/switch_view fetch (see `app::async_ops`)
            // completed; `app.events_tx` never drops (it's a field on
            // `App`), so this branch just stays pending between fetches.
            Some(ev) = app.events_rx.recv() => {
                app.apply_event(ev);
            }
        }

        // Fulfil a drag-select copy using the frame we just rendered.
        if let Some((y0, y1)) = app.mouse.pending_copy.take() {
            let text = editor_launch::read_rows(terminal, y0, y1);
            let n = text.lines().filter(|l| !l.trim().is_empty()).count();
            let _ = jira_tui::infra::osc52_copy(&text);
            app.status = format!("copied {n} line(s) to clipboard");
            app.flash(format!("✓ copied {n} line(s)"));
        }

        // Launch $EDITOR for a round-trip description edit.
        if app.request_edit {
            app.request_edit = false;
            if let Err(e) = editor_launch::edit_in_editor(terminal, app) {
                app.status = format!("edit failed: {e}");
            }
        }

        // Populate the quick-view panel lazily (cheap no-op once cached).
        app.ensure_quick_view_loaded();

        app.tick = app.tick.wrapping_add(1);

        if app.should_quit {
            return Ok(());
        }
    }
}

// ── Config init ──────────────────────────────────────────────────────────────
fn init_config() -> Result<()> {
    let (path, created) = config::write_default_config()?;
    if created {
        println!("Wrote default config to {}", path.display());
    } else {
        println!(
            "Config already exists at {} (left unchanged)",
            path.display()
        );
    }
    if let Some(cache) = config::cache_dir() {
        println!("Cache directory: {}", cache.display());
    }
    Ok(())
}

// ── Terminal lifecycle ───────────────────────────────────────────────────────
fn setup_terminal() -> Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Term) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Ensure the terminal is restored even if a panic unwinds out of the draw loop,
/// so a crash never leaves the user in a corrupted (raw, alt-screen) shell.
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        original(info);
    }));
}
