//! jira-tui — a keyboard-driven Jira terminal UI with a little bit of soul.

use jira_tui::{app, config, ui};

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    EventStream, KeyEventKind,
};
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
mod suspend;

/// Frame cadence — also the animation tick rate for the About panel.
const TICK: Duration = Duration::from_millis(90);

type Term = Terminal<CrosstermBackend<Stdout>>;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    jira_tui::debug::init_from_env();

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
        app.about_return_screen = app.screen;
        app.screen = Screen::About;
    }

    let mut terminal = setup_terminal()?;
    // Before anything that could panic and leave the terminal in raw
    // mode/alt-screen with no restoration — including the image-capability
    // probe below.
    install_panic_hook();
    // Strictly before `run()`'s `crossterm::EventStream` starts polling
    // stdin: `Picker::from_query_stdio` writes an escape-sequence query and
    // synchronously reads stdin for the terminal's response, which a
    // concurrently polling event stream would otherwise consume as a stray
    // input event, losing the response.
    #[cfg(feature = "images")]
    {
        app.image_picker = detect_image_picker();
    }
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

    // Whether *any* modal/overlay (`App::any_modal_open`) was open on the
    // *previous* iteration's render — every one of them, not just the
    // largest (help/nerd-info), turned out to leave a Sixel/iTerm2 image's
    // pixels ghosted once closed (confirmed against a real terminal this
    // session; even the command palette, which barely overlaps the image
    // area, was enough). See `erase_image_prone_areas`'s own doc comment
    // for why a plain ratatui redraw doesn't already handle this.
    #[cfg(feature = "images")]
    let mut any_modal_was_open = false;

    loop {
        // A Sixel/iTerm2 image's pixels aren't reliably cleared by a normal
        // ratatui redraw once something else has been drawn over them and
        // removed again (see `erase_image_prone_areas`) — do the scoped
        // erase for whichever pane last held one, right before the frame
        // that's about to render the overlay-free content again, on every
        // open<->close transition of *any* modal that can cover one.
        #[cfg(feature = "images")]
        {
            let any_modal_is_open = app.any_modal_open();
            if any_modal_is_open != any_modal_was_open {
                erase_image_prone_areas(terminal, app);
            }
            any_modal_was_open = any_modal_is_open;
        }

        // Cloned immediately: `Terminal::draw` swaps its double buffer
        // before returning, so `terminal.current_buffer_mut()` queried any
        // later in this loop iteration would hand back the blank buffer
        // being prepared for the *next* frame rather than what's actually
        // on screen — this is the only point where the just-rendered
        // content is still reachable, for the drag-to-copy read below.
        let last_frame = terminal.draw(|f| ui::draw(f, app))?.buffer.clone();

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
                    Some(Ok(Event::Paste(text))) => app.handle_paste(text),
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
        if let Some(span) = app.mouse.pending_copy.take() {
            let text = editor_launch::read_span(&last_frame, &span);
            let n = text.lines().filter(|l| !l.trim().is_empty()).count();
            let _ = jira_tui::infra::osc52_copy(&text);
            app.status = format!("copied {n} line(s) to clipboard");
            app.flash(format!("✓ copied {n} line(s)"));
        }

        // Launch $EDITOR for a round-trip description or comment edit.
        if app.request_edit {
            app.request_edit = false;
            if let Err(e) = editor_launch::edit_in_editor(terminal, app) {
                app.status = format!("edit failed: {e}");
            }
        }

        // Suspend to the shell on Ctrl+Z, resuming once the shell brings us
        // back to the foreground.
        if app.request_suspend {
            app.request_suspend = false;
            if let Err(e) = suspend::suspend(terminal, app) {
                app.status = format!("suspend failed: {e}");
            }
        }

        // Populate the quick-view panel lazily (cheap no-op once cached).
        app.ensure_quick_view_loaded();

        // Fire the Search screen's debounced live text search, if one's due.
        app.ensure_search_dispatched();

        // Fire the attachment picker's debounced preview fetch, if one's due.
        #[cfg(feature = "images")]
        app.ensure_attachment_preview_dispatched();

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

/// Query the terminal for image-graphics support (Kitty/Sixel/iTerm2, with a
/// Unicode half-block fallback) — see issue #130. `None` whenever there's no
/// point asking: not a real interactive tty (a pipe, CI, `cargo test`'s
/// captured stdio), or the query itself failed. Every other code path treats
/// a `None` picker exactly like the `images` feature being absent: fall back
/// to the `[image: alt]` placeholder.
#[cfg(feature = "images")]
fn detect_image_picker() -> Option<ratatui_image::picker::Picker> {
    use std::io::IsTerminal;
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        return None;
    }
    ratatui_image::picker::Picker::from_query_stdio().ok()
}

/// Erase `App::detail_main_area`/`quick_view_area` (whichever are non-empty
/// — a plain `Rect::default()` from a screen/session that's never rendered
/// either pane is skipped) at the terminal level, bypassing ratatui's usual
/// buffer diff entirely. Only worth calling around an `App::any_modal_open`
/// transition — see the run loop's own call site.
///
/// Why this exists: `ratatui-image`'s Sixel/iTerm2 protocols paint pixels
/// directly to the terminal, outside ratatui's normal character-cell model.
/// The crate's own `SlicedProtocol` already re-issues a proper erase (ECH,
/// not a plain space/blank-character overwrite — the crate's own docs note
/// blanks alone don't reliably clear graphics-protocol pixels on every
/// terminal) for its own believed image rect on every single render call —
/// but confirmed against a real terminal this session, that alone wasn't
/// enough: opening so much as the command palette over an image and closing
/// it again still left visible ghosting, meaning either the ghosting
/// extends past the image's own exact cell rect (font-size/pixel rounding
/// slop, or an overlay's footprint larger than the image's) or this
/// particular terminal's Sixel implementation doesn't fully honour the
/// crate's own erase in every case. Either way, explicitly erasing the
/// *whole* pane (not just the image's tight rect) at the moment an overlay
/// is about to stop covering it directly addresses the actual observed
/// symptom, independent of exactly which of those is the underlying cause.
///
/// Deliberately writes straight to `terminal.backend_mut()` rather than
/// going through a `ratatui::widgets::Clear` widget in the next `Frame` —
/// `Clear` only marks cells as blank *within ratatui's own buffer model*,
/// which is exactly the "overwriting with blank characters" case the
/// crate's docs say isn't sufficient for Sixel; this needs the same
/// terminal-level ECH primitive the crate itself uses, issued once, right
/// before the next `terminal.draw` call composes the real content.
///
/// Experimental: erasing the same terminal-level way (ECH + explicit
/// per-row cursor positioning) as the crate's own `clear_area` should be
/// safe for ratatui's cursor-position bookkeeping — every backend
/// implementation re-issues an explicit absolute move for the first cell of
/// any diffed run, rather than trusting a cached "cursor is already here"
/// assumption across an external write it didn't itself perform — but this
/// hasn't been verified against ratatui's crossterm backend source beyond
/// that general expectation. If a stray misplaced character ever shows up
/// immediately after a modal closes on an `images` build, that assumption
/// is the first thing to revisit.
///
/// A functional regression this same mechanism caused, found live: writing
/// straight to the backend bypasses ratatui's own diff buffer entirely —
/// `Terminal::draw`'s `flush()` only ever sends the backend cells that
/// *differ* from what its internal "previous frame" buffer still believes
/// is on screen, so an external erase these lines don't know about leaves
/// that buffer thinking the description text is still there. If the
/// upcoming frame's content happens to be identical (the common case: a
/// modal opening/closing doesn't itself change the Detail screen
/// underneath), the diff sees "no change" and skips rewriting those exact
/// cells — leaving the whole erased pane blank instead of repainted, not
/// just the image. `Terminal::clear()` (below) is the only public API that
/// resets that internal "previous frame" tracking, forcing every cell to
/// be treated as dirty on the very next `draw`; it also does its own
/// terminal-level `ClearType::All`, but that alone isn't a substitute for
/// the ECH writes above — `ClearType::All` maps to a plain `ESC[2J`, the
/// same "doesn't reliably clear Sixel" primitive this function's own ECH
/// approach exists to work around (confirmed against `ratatui-crossterm`'s
/// own source, not just the crate's compatibility notes). Both are needed:
/// ECH for the graphics pixels, `clear()` for ratatui's own bookkeeping.
#[cfg(feature = "images")]
fn erase_image_prone_areas(terminal: &mut Term, app: &App) {
    use std::fmt::Write as _;
    use std::io::Write as _;

    let mut seq = String::new();
    for rect in [app.detail_main_area.get(), app.quick_view_area.get()] {
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        for row in 0..rect.height {
            // CUP (absolute cursor position, 1-indexed) then ECH (erase
            // `width` characters from the cursor without moving it) — the
            // same two primitives `ratatui-image`'s own `clear_area` uses
            // internally, just looped here per row via absolute
            // positioning rather than relative cursor-down movements,
            // since there's no existing cursor position worth preserving
            // around this call.
            let _ = write!(
                seq,
                "\x1b[{};{}H\x1b[{}X",
                rect.y + row + 1,
                rect.x + 1,
                rect.width
            );
        }
    }
    if seq.is_empty() {
        return;
    }
    let backend = terminal.backend_mut();
    let _ = backend.write_all(seq.as_bytes());
    let _ = backend.flush();
    // Forces the next `terminal.draw` to rewrite every cell rather than
    // skipping ones its diff thinks are unchanged — see this function's
    // own doc comment for why the ECH writes above aren't enough on their
    // own.
    let _ = terminal.clear();
}

// ── Terminal lifecycle ───────────────────────────────────────────────────────
fn setup_terminal() -> Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Term) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Ensure the terminal is restored even if a panic unwinds out of the draw loop,
/// so a crash never leaves the user in a corrupted (raw, alt-screen) shell.
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        original(info);
    }));
}
