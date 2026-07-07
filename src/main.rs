//! jira-tui — a keyboard-driven Jira terminal UI with a little bit of soul.

mod adf;
mod app;
mod domain;
mod git;
mod jira;
mod ui;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, Screen};

/// Frame cadence — also the animation tick rate for the About panel.
const TICK: Duration = Duration::from_millis(90);

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("jira-tui v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let force_demo = args.iter().any(|a| a == "--demo");
    let start_about = args.iter().any(|a| a == "--about");

    let mut app = App::new(force_demo);
    if start_about {
        app.screen = Screen::About;
    }

    let mut terminal = setup_terminal()?;
    install_panic_hook();
    let result = run(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for input, but wake on TICK so animations keep moving.
        if event::poll(TICK)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key);
                }
            }
        }
        app.tick = app.tick.wrapping_add(1);

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    // Global: Ctrl-C always quits.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    // Help overlay swallows input while open.
    if app.show_help {
        app.show_help = false;
        return;
    }

    match key.code {
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('a') => app.screen = Screen::About,
        KeyCode::Char('g') => app.screen = Screen::Home,
        KeyCode::Char('r') => app.refresh(),
        KeyCode::Char('q') => back_or_quit(app),
        KeyCode::Esc | KeyCode::Char('h') => back_or_quit(app),

        KeyCode::Char('l') if app.screen != Screen::Detail => app.screen = Screen::List,

        KeyCode::Up | KeyCode::Char('k') => nav(app, -1),
        KeyCode::Down | KeyCode::Char('j') => nav(app, 1),
        KeyCode::PageUp => nav(app, -8),
        KeyCode::PageDown => nav(app, 8),

        KeyCode::Enter => {
            if app.screen == Screen::Home || app.screen == Screen::List {
                app.open_detail();
            }
        }

        KeyCode::Char('o') => {
            if let Some(issue) = app.selected_issue() {
                app.status = format!("{} · key ready to paste", issue.key);
            }
        }
        _ => {}
    }
}

fn nav(app: &mut App, delta: isize) {
    match app.screen {
        Screen::Detail => {
            let new = app.detail_scroll as isize + delta.signum() * delta.abs().max(1);
            app.detail_scroll = new.max(0) as u16;
        }
        Screen::Home | Screen::List => app.move_selection(delta),
        Screen::About => {}
    }
}

fn back_or_quit(app: &mut App) {
    match app.screen {
        Screen::Home => app.should_quit = true,
        Screen::List | Screen::Detail | Screen::About => app.screen = Screen::Home,
    }
}

// Terminal lifecycle
fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
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
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}

fn print_help() {
    println!(
        "jira-tui v{}\n\
\n\
A developer-first, keyboard-driven Jira terminal UI.\n\
\n\
USAGE:\n\
    jira-tui [OPTIONS]\n\
\n\
OPTIONS:\n\
    --demo        Force offline demo mode (ignore any credentials)\n\
    --about       Open straight to the animated about panel\n\
    -V, --version Print version\n\
    -h, --help    Print this help\n\
\n\
LIVE MODE:\n\
    Set JIRA_EMAIL and JIRA_API_TOKEN (or a token.txt file), and optionally\n\
    JIRA_BASE_URL / JIRA_PROJECT, to load your real assigned work. Without\n\
    them, jira-tui runs against built-in sample data.\n",
        env!("CARGO_PKG_VERSION")
    );
}
