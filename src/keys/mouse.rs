//! Mouse input: a distinct input modality from the keyboard, with its own
//! opt-in toggle and its own "pointer, not keyboard focus" scroll routing
//! (see `scroll_at`'s doc comment).

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;

use jira_tui::app::{App, Screen};

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
        MouseEventKind::Up(MouseButton::Left) => app.mouse_up(me.column, me.row),
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

pub(super) fn toggle_mouse(app: &mut App) {
    app.mouse.enabled = !app.mouse.enabled;
    app.mouse.selecting = false;
    if app.mouse.enabled {
        let _ = execute!(std::io::stdout(), EnableMouseCapture);
        app.status = "mouse mode on · click to open · drag to copy · shift-drag = native".into();
    } else {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        app.status = "mouse mode off · terminal selection restored".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::MouseEventKind;
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

    /// CLAUDE.md "what to keep true": "Mouse mode is opt-in: Shift-drag must
    /// fall through to the terminal's native selection." Coverage gap
    /// noticed while splitting this file — the Shift guard itself (the one
    /// line implementing that rule) had no direct test.
    #[test]
    fn shift_held_bypasses_the_app_entirely() {
        let mut app = demo_app();

        let me = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 10,
            row: 5,
            modifiers: KeyModifiers::SHIFT,
        };
        handle_mouse(&mut app, me);

        // `mouse_down` unconditionally sets `mouse.selecting = true` — if
        // this is still false, `handle_mouse` returned before ever calling
        // into app state, proving the Shift-held click was bypassed rather
        // than merely producing a no-op selection.
        assert!(
            !app.mouse.selecting,
            "a Shift-held click must not be interpreted by the app at all"
        );
    }
}
