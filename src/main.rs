//! jira-tui — a keyboard-driven Jira terminal UI with a little bit of soul.

use jira_tui::{app, config, infra, ui};

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, Screen};

/// Frame cadence — also the animation tick rate for the About panel.
const TICK: Duration = Duration::from_millis(90);

type Term = Terminal<CrosstermBackend<Stdout>>;

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
    if args.iter().any(|a| a == "--init") {
        return init_config();
    }
    let force_demo = args.iter().any(|a| a == "--demo");
    let start_about = args.iter().any(|a| a == "--about");
    let force_onboard = args.iter().any(|a| a == "--onboard");

    let mut app = App::new(force_demo);
    if force_onboard {
        app.screen = Screen::Welcome;
        app.welcome_phase = app::WelcomePhase::Intro;
    }
    if start_about {
        app.screen = Screen::About;
    }

    let mut terminal = setup_terminal()?;
    install_panic_hook();
    if app.mouse_enabled {
        let _ = execute!(io::stdout(), EnableMouseCapture);
    }
    let result = run(&mut terminal, &mut app);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    restore_terminal(&mut terminal)?;
    result
}

fn run(terminal: &mut Term, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for input, but wake on TICK so animations keep moving.
        if event::poll(TICK)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key),
                Event::Mouse(me) => handle_mouse(app, me),
                _ => {}
            }
        }

        // Fulfil a drag-select copy using the frame we just rendered.
        if let Some((y0, y1)) = app.pending_copy.take() {
            let text = read_rows(terminal, y0, y1);
            let n = text.lines().filter(|l| !l.trim().is_empty()).count();
            let _ = infra::osc52_copy(&text);
            app.status = format!("copied {n} line(s) to clipboard");
            app.flash(format!("✓ copied {n} line(s)"));
        }

        // Launch $EDITOR for a round-trip description edit.
        if app.request_edit {
            app.request_edit = false;
            if let Err(e) = edit_in_editor(terminal, app) {
                app.status = format!("edit failed: {e}");
            }
        }

        app.tick = app.tick.wrapping_add(1);

        if app.should_quit {
            return Ok(());
        }
    }
}

/// Reconstruct the plain text of an inclusive screen-row range from the last
/// rendered frame's buffer (used for drag-to-copy).
fn read_rows(terminal: &mut Term, y0: u16, y1: u16) -> String {
    let buf = terminal.current_buffer_mut();
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
fn edit_in_editor(terminal: &mut Term, app: &mut App) -> Result<()> {
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
    if app.mouse_enabled {
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
    if app.mouse_enabled {
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

    // Onboarding has its own key map (including a text-entry form).
    if app.screen == Screen::Welcome {
        handle_welcome_key(app, key);
        return;
    }

    // Modal: the transition picker captures navigation while open.
    if app.picker_open {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.picker_move(-1),
            KeyCode::Down | KeyCode::Char('j') => app.picker_move(1),
            KeyCode::Enter => app.confirm_transition(),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Left | KeyCode::Backspace => {
                app.close_picker()
            }
            _ => {}
        }
        return;
    }

    // The edit preview is a confirm screen.
    if app.screen == Screen::Preview {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => app.apply_edit(),
            KeyCode::Esc
            | KeyCode::Char('q')
            | KeyCode::Char('h')
            | KeyCode::Left
            | KeyCode::Backspace => app.cancel_edit(),
            KeyCode::Up | KeyCode::Char('k') => nav(app, -1),
            KeyCode::Down | KeyCode::Char('j') => nav(app, 1),
            KeyCode::PageUp => nav(app, -8),
            KeyCode::PageDown => nav(app, 8),
            _ => {}
        }
        return;
    }

    // The in-TUI Markdown editor captures typing.
    if app.screen == Screen::Edit {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => app.cancel_edit(),
            KeyCode::Char('s') if ctrl => app.commit_tui_edit(),
            KeyCode::Enter => app.editor.newline(),
            KeyCode::Backspace => app.editor.backspace(),
            KeyCode::Left => app.editor.left(),
            KeyCode::Right => app.editor.right(),
            KeyCode::Up => app.editor.up(),
            KeyCode::Down => app.editor.down(),
            KeyCode::Tab => {
                app.editor.insert_char(' ');
                app.editor.insert_char(' ');
            }
            KeyCode::Char(c) if !ctrl => app.editor.insert_char(c),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('a') => app.screen = Screen::About,
        KeyCode::Char('g') => app.screen = Screen::Home,
        KeyCode::Char('r') => app.refresh(),
        KeyCode::Char('m') => toggle_mouse(app),
        KeyCode::Char('J') => {
            app.show_jax = !app.show_jax;
            app.status = if app.show_jax {
                "Jax is here to keep you company 🦦".into()
            } else {
                "Jax went for a nap 😴".into()
            };
        }
        KeyCode::Char('y') => yank_key(app),
        KeyCode::Char('Y') => yank_url(app),
        KeyCode::Char('q') => back_or_quit(app),
        KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => back_or_quit(app),

        // Sort + filter on the work list.
        KeyCode::Char('s') if matches!(app.screen, Screen::Home | Screen::List) => app.cycle_sort(),
        KeyCode::Char('S') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.toggle_sort_dir()
        }
        KeyCode::Char('f') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.cycle_filter()
        }
        KeyCode::Char('v') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.quick_view = !app.quick_view;
        }

        KeyCode::Char('l') if app.screen != Screen::Detail => app.screen = Screen::List,

        KeyCode::Char('t') if app.screen == Screen::Detail => app.open_transitions(),
        // In-TUI editor (default) and external $EDITOR (E).
        KeyCode::Char('e') if app.screen == Screen::Detail && app.detail.is_some() => {
            app.begin_tui_edit();
        }
        KeyCode::Char('E') if app.screen == Screen::Detail && app.detail.is_some() => {
            app.request_edit = true;
        }

        KeyCode::Up | KeyCode::Char('k') => nav(app, -1),
        KeyCode::Down | KeyCode::Char('j') => nav(app, 1),
        KeyCode::PageUp => nav(app, -8),
        KeyCode::PageDown => nav(app, 8),

        // Right or Enter opens the selected issue.
        KeyCode::Enter | KeyCode::Right if matches!(app.screen, Screen::Home | Screen::List) => {
            app.open_detail()
        }
        _ => {}
    }
}

fn handle_welcome_key(app: &mut App, key: KeyEvent) {
    use app::WelcomePhase;
    match app.welcome_phase {
        WelcomePhase::Intro => match key.code {
            KeyCode::Char('s') => {
                app.welcome_phase = WelcomePhase::Setup;
                app.setup_msg.clear();
            }
            KeyCode::Char('d') | KeyCode::Enter => app.finish_onboarding(),
            KeyCode::Char('w') => app.write_config_from_welcome(),
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Char('q') | KeyCode::Esc => app.finish_onboarding(),
            _ => {}
        },
        WelcomePhase::Setup => match key.code {
            KeyCode::Esc => {
                app.welcome_phase = WelcomePhase::Intro;
                app.setup_msg.clear();
            }
            KeyCode::Enter => app.submit_credentials(),
            KeyCode::Tab | KeyCode::Down => app.focus_next(),
            KeyCode::BackTab | KeyCode::Up => app.focus_prev(),
            KeyCode::Backspace => app.input_backspace(),
            KeyCode::Char(c) => app.input_char(c),
            _ => {}
        },
    }
}

fn handle_mouse(app: &mut App, me: MouseEvent) {
    // Hold Shift to bypass the app and use the terminal's native selection.
    if me.modifiers.contains(KeyModifiers::SHIFT) {
        return;
    }
    match me.kind {
        MouseEventKind::ScrollUp => nav(app, -1),
        MouseEventKind::ScrollDown => nav(app, 1),
        MouseEventKind::Down(MouseButton::Left) => app.mouse_down(me.row),
        MouseEventKind::Drag(MouseButton::Left) => app.mouse_drag(me.row),
        MouseEventKind::Up(MouseButton::Left) => app.mouse_up(me.row),
        _ => {}
    }
}

fn toggle_mouse(app: &mut App) {
    app.mouse_enabled = !app.mouse_enabled;
    app.selecting = false;
    if app.mouse_enabled {
        let _ = execute!(io::stdout(), EnableMouseCapture);
        app.status = "mouse mode on · click to open · drag to copy · shift-drag = native".into();
    } else {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        app.status = "mouse mode off · terminal selection restored".into();
    }
}

fn yank_key(app: &mut App) {
    if let Some(issue) = app.selected_issue() {
        let key = issue.key.clone();
        let _ = infra::osc52_copy(&key);
        app.status = format!("copied {key} to clipboard");
        app.flash(format!("✓ copied {key}"));
    }
}

fn yank_url(app: &mut App) {
    if let Some(url) = app.selected_issue_url() {
        let _ = infra::osc52_copy(&url);
        app.status = format!("copied {url} to clipboard");
        app.flash("✓ copied issue URL");
    }
}

fn nav(app: &mut App, delta: isize) {
    match app.screen {
        Screen::Detail | Screen::Preview => {
            let new = app.detail_scroll as isize + delta.signum() * delta.abs().max(1);
            app.detail_scroll = new.max(0) as u16;
        }
        Screen::Home | Screen::List => app.move_selection(delta),
        Screen::About | Screen::Welcome | Screen::Edit => {}
    }
}

fn back_or_quit(app: &mut App) {
    match app.screen {
        Screen::Home | Screen::Welcome => app.should_quit = true,
        Screen::Preview | Screen::Edit => app.cancel_edit(),
        Screen::List | Screen::Detail | Screen::About => app.screen = Screen::Home,
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
    --onboard     Re-run the first-run welcome / live setup\n\
    --init        Write a default config to ~/.config/jira-tui/config.toml\n\
    -V, --version Print version\n\
    -h, --help    Print this help\n\
\n\
MOUSE:\n\
    Press 'm' to toggle mouse mode (click to open, wheel to scroll, drag to\n\
    copy via OSC 52). Hold Shift while dragging to use your terminal's native\n\
    selection instead.\n\
\n\
EDITING:\n\
    In an issue, press 't' to change status and 'e' to edit the description in\n\
    $EDITOR (VISUAL/EDITOR, falling back to vi). Edits are recompiled to ADF and\n\
    previewed before anything is sent to Jira.\n\
\n\
LIVE MODE:\n\
    Set JIRA_EMAIL and JIRA_API_TOKEN (or a token.txt file), and optionally\n\
    JIRA_BASE_URL / JIRA_PROJECT, to load your real assigned work. Without\n\
    them, jira-tui runs against built-in sample data (or the last cached list).\n",
        env!("CARGO_PKG_VERSION")
    );
}
