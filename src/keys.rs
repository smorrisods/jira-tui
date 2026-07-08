//! Keyboard and mouse event handling: translating input into `App` state
//! changes. Each screen with bespoke navigation (Welcome, the transition
//! picker, Preview, Edit, Search, Board) has its own key-handling block;
//! everything else falls through to the shared `handle_key` match.

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyModifiers, MouseButton,
    MouseEvent, MouseEventKind,
};
use crossterm::execute;

use jira_tui::app::{self, App, Screen};
use jira_tui::infra;

pub(crate) fn handle_key(app: &mut App, key: KeyEvent) {
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

    // The Search / go-to-issue screen captures typing.
    if app.screen == Screen::Search {
        match key.code {
            KeyCode::Esc => app.close_search(),
            KeyCode::Enter => app.confirm_search(),
            KeyCode::Up => app.search_move(-1),
            KeyCode::Down => app.search_move(1),
            KeyCode::Backspace => app.search_backspace(),
            KeyCode::Char(c) => app.search_input_char(c),
            _ => {}
        }
        return;
    }

    // The field-mapping screen: type to search custom fields, pick one to
    // map "Acceptance Criteria" to (or the leading "none" entry to clear it).
    if app.screen == Screen::FieldMapping {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.close_field_mapping(),
            KeyCode::Enter => app.confirm_field_mapping(),
            KeyCode::Up => app.field_mapping_move(-1),
            KeyCode::Down => app.field_mapping_move(1),
            KeyCode::Backspace => app.field_mapping_backspace(),
            KeyCode::Char(c) => app.field_mapping_input_char(c),
            _ => {}
        }
        return;
    }

    // The swimlane board has its own 2D navigation (card / column / lane).
    if app.screen == Screen::Board {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.board_move_card(-1),
            KeyCode::Down | KeyCode::Char('j') => app.board_move_card(1),
            KeyCode::Left | KeyCode::Char('h') => app.board_move_col(-1),
            KeyCode::Right | KeyCode::Char('l') => app.board_move_col(1),
            KeyCode::PageUp => app.board_move_lane(-1),
            KeyCode::PageDown => app.board_move_lane(1),
            KeyCode::Enter => app.board_open(),
            KeyCode::Char('/') => app.open_search(),
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => back_or_quit(app),
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
        KeyCode::Char('b') if matches!(app.screen, Screen::Home | Screen::List) => app.open_board(),
        KeyCode::Char('/')
            if matches!(app.screen, Screen::Home | Screen::List | Screen::Detail) =>
        {
            app.open_search()
        }
        KeyCode::Tab if matches!(app.screen, Screen::Home | Screen::List) => {
            app.toggle_list_focus()
        }
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
            if !app.quick_view {
                app.list_focus = app::ListFocus::List;
            }
        }
        KeyCode::Char('F') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.open_field_mapping();
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

pub(crate) fn handle_mouse(app: &mut App, me: MouseEvent) {
    // Hold Shift to bypass the app and use the terminal's native selection.
    if me.modifiers.contains(KeyModifiers::SHIFT) {
        return;
    }
    match me.kind {
        MouseEventKind::ScrollUp => scroll_at(app, me.column, me.row, -1),
        MouseEventKind::ScrollDown => scroll_at(app, me.column, me.row, 1),
        MouseEventKind::Down(MouseButton::Left) => app.mouse_down(me.row),
        MouseEventKind::Drag(MouseButton::Left) => app.mouse_drag(me.row),
        MouseEventKind::Up(MouseButton::Left) => app.mouse_up(me.row),
        _ => {}
    }
}

/// Scroll whichever panel the pointer is physically over. This is deliberately
/// independent of keyboard `Tab` focus: the mouse always follows the pointer,
/// while `Tab` + arrow keys is a separate, keyboard-only focus model.
fn scroll_at(app: &mut App, x: u16, y: u16, delta: isize) {
    if app.point_in_quick_view(x, y) {
        app.quick_view_scroll_by(delta);
        return;
    }
    match app.screen {
        Screen::Home | Screen::List => app.move_selection(delta),
        Screen::Detail | Screen::Preview => {
            let new = app.detail_scroll as isize + delta;
            app.detail_scroll = new.max(0) as u16;
        }
        Screen::Board => app.board_scroll_by(delta),
        _ => {}
    }
}

fn toggle_mouse(app: &mut App) {
    app.mouse_enabled = !app.mouse_enabled;
    app.selecting = false;
    if app.mouse_enabled {
        let _ = execute!(std::io::stdout(), EnableMouseCapture);
        app.status = "mouse mode on · click to open · drag to copy · shift-drag = native".into();
    } else {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
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
        Screen::Home | Screen::List => {
            // Tab moves keyboard focus between the list and the quick-view
            // panel; while quick view has focus, arrows/PageUp/PageDown
            // scroll it instead of moving the list selection.
            if app.quick_view && app.list_focus == app::ListFocus::QuickView {
                app.quick_view_scroll_by(delta);
            } else {
                app.move_selection(delta);
            }
        }
        Screen::About
        | Screen::Welcome
        | Screen::Edit
        | Screen::Search
        | Screen::Board
        | Screen::FieldMapping => {}
    }
}

fn back_or_quit(app: &mut App) {
    match app.screen {
        Screen::Home | Screen::Welcome => app.should_quit = true,
        Screen::Preview | Screen::Edit => app.cancel_edit(),
        Screen::Search => app.close_search(),
        Screen::FieldMapping => app.close_field_mapping(),
        Screen::List | Screen::Detail | Screen::About | Screen::Board => app.screen = Screen::Home,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jira_tui::app::ListFocus;
    use ratatui::layout::Rect;

    fn demo_app() -> App {
        let mut app = App::new(true);
        app.screen = Screen::Home;
        app
    }

    /// Regression test: the mouse wheel must always follow the pointer
    /// position, never the keyboard `Tab` focus. Hovering the list should
    /// move the list selection even while quick view has keyboard focus.
    #[test]
    fn wheel_over_list_ignores_keyboard_focus() {
        let mut app = demo_app();
        app.quick_view = true;
        app.list_focus = ListFocus::QuickView; // keyboard focus is on quick view
        app.quick_view_area.set(Rect::new(0, 30, 100, 10)); // panel lives elsewhere
        app.selected = 0;
        app.quick_view_scroll = 0;

        // Scroll over the list area (row 5), well outside the quick-view rect.
        scroll_at(&mut app, 10, 5, 1);

        assert_eq!(app.selected, 1, "wheel over the list should move selection");
        assert_eq!(
            app.quick_view_scroll, 0,
            "quick view must not scroll when the pointer is over the list"
        );
    }

    #[test]
    fn wheel_over_quick_view_scrolls_it_regardless_of_focus() {
        let mut app = demo_app();
        app.quick_view = true;
        app.list_focus = ListFocus::List; // keyboard focus is on the list
        app.quick_view_area.set(Rect::new(0, 30, 100, 10));
        app.selected = 0;

        scroll_at(&mut app, 10, 32, 1); // inside the quick-view rect

        assert_eq!(
            app.selected, 0,
            "list selection must not move when the pointer is over quick view"
        );
        assert_eq!(app.quick_view_scroll, 1);
    }

    #[test]
    fn wheel_scrolls_board_when_no_quick_view() {
        let mut app = demo_app();
        app.open_board();
        app.board_scroll = 0;
        scroll_at(&mut app, 5, 5, 1);
        assert_eq!(app.board_scroll, 1);
    }
}
