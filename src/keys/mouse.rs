//! Mouse input: a distinct input modality from the keyboard, with its own
//! opt-in toggle and its own "pointer, not keyboard focus" scroll routing
//! (see `scroll_at`'s doc comment). Right-click stands in for a mouse
//! back/forward button — crossterm's `MouseButton` has no such variant at
//! all (checked upstream, including its unreleased `master` branch), so a
//! real back/forward button press can't be told apart from any other input
//! this app receives.

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
    // A modal/overlay captures keyboard input exclusively (see the flag
    // checks at the top of `handle_key`) — mouse input must be swallowed
    // the same way, or a click could mutate `app.screen` (or list/quick-view
    // state) while the modal's own flag stays set, orphaning it over
    // whatever screen is now showing underneath.
    if app.show_help || app.picker_open || app.view_picker_open || app.assignee_picker_open {
        return;
    }
    // Any button other than a continuing Left-button drag cancels an
    // in-flight one — e.g. right-click navigating away mid-drag, or a
    // dropped button-release — so a stale `selecting` can't keep painting
    // the drag's inverted highlight over whatever's now on screen.
    if !matches!(
        me.kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Drag(MouseButton::Left)
            | MouseEventKind::Up(MouseButton::Left)
    ) {
        app.mouse.selecting = false;
    }
    match me.kind {
        MouseEventKind::ScrollUp => scroll_at(app, me.column, me.row, -1),
        MouseEventKind::ScrollDown => scroll_at(app, me.column, me.row, 1),
        MouseEventKind::Down(MouseButton::Left) => app.mouse_down(me.column, me.row),
        MouseEventKind::Drag(MouseButton::Left) => app.mouse_drag(me.column, me.row),
        MouseEventKind::Up(MouseButton::Left) => app.mouse_up(me.column, me.row),
        // Middle-click mirrors the `v` key: toggle the quick-view panel.
        // Scoped to Home/List, matching `v`'s own screen guard — Board and
        // the other screens have no quick-view panel to toggle.
        MouseEventKind::Down(MouseButton::Middle)
            if matches!(app.screen, Screen::Home | Screen::List) =>
        {
            app.toggle_quick_view();
        }
        // Right-click steps back (see this module's doc comment for why
        // right-click, not an actual back button) — shares `go_back_or_out`
        // with the `←` key so the two stay in lockstep by construction.
        // Deliberately excludes Home: `back_or_quit` quits the app there,
        // and a stray right-click shouldn't be able to do that the way a
        // deliberate `Esc` keypress can.
        MouseEventKind::Down(MouseButton::Right)
            if matches!(app.screen, Screen::Detail | Screen::List | Screen::Board) =>
        {
            super::go_back_or_out(app);
        }
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
        app.status =
            "mouse mode on · click to open · drag to copy · middle-click = quick view · right-click = back · shift-drag = native"
                .into();
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

    /// Middle-click mirrors the `v` key: toggle the quick-view panel on
    /// Home/List. crossterm has no Back/Forward `MouseButton` variant at
    /// all (checked upstream — even its unreleased `master` branch only
    /// defines Left/Right/Middle), so middle-click is the only extra mouse
    /// button this app can support.
    #[test]
    fn middle_click_toggles_quick_view_on_home_and_list() {
        let mut app = demo_app();
        assert!(!app.quick_view);

        let middle_down = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Middle),
            column: 10,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse(&mut app, middle_down);
        assert!(app.quick_view, "middle-click should open quick view");

        handle_mouse(&mut app, middle_down);
        assert!(!app.quick_view, "middle-click should close quick view");
    }

    #[test]
    fn middle_click_does_nothing_on_board() {
        let mut app = demo_app();
        app.open_board();

        let middle_down = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Middle),
            column: 10,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse(&mut app, middle_down);
        assert!(!app.quick_view, "Board has no quick-view panel to toggle");
    }

    fn right_click(row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Right),
            column: 10,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    /// Right-click on Detail steps through in-body link history first,
    /// exactly like `←` does — see `app::history`.
    #[test]
    fn right_click_on_detail_steps_back_through_link_history() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        let first = app.detail.as_ref().unwrap().key.clone();
        app.open_by_key("DS-9001");
        assert!(app.can_go_back());

        handle_mouse(&mut app, right_click(5));

        assert_eq!(app.screen, Screen::Detail, "history exists, so stay put");
        assert_eq!(app.detail.as_ref().unwrap().key, first);
    }

    /// Right-click on Detail with no history left falls through to leaving
    /// the screen, matching `←`'s own fallback once history is exhausted.
    #[test]
    fn right_click_on_detail_with_no_history_leaves_the_screen() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        assert!(!app.can_go_back());

        handle_mouse(&mut app, right_click(5));

        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn right_click_on_list_and_board_returns_to_home() {
        let mut app = demo_app();
        app.screen = Screen::List;
        handle_mouse(&mut app, right_click(5));
        assert_eq!(app.screen, Screen::Home);

        let mut app = demo_app();
        app.open_board();
        handle_mouse(&mut app, right_click(5));
        assert_eq!(app.screen, Screen::Home);
    }

    /// Deliberately excluded: `back_or_quit` quits the app on Home, and a
    /// stray right-click shouldn't be able to do that.
    #[test]
    fn right_click_on_home_does_nothing() {
        let mut app = demo_app();
        handle_mouse(&mut app, right_click(5));
        assert_eq!(app.screen, Screen::Home);
        assert!(!app.should_quit);
    }

    /// A modal/overlay captures keyboard input exclusively (`handle_key`
    /// checks these flags before anything else) — mouse input must be
    /// swallowed the same way, or a click could navigate away while the
    /// modal's own flag stays set, orphaning it over whatever's now shown.
    #[test]
    fn mouse_input_is_swallowed_while_a_modal_is_open() {
        let mut app = demo_app();
        app.screen = Screen::Detail;
        app.picker_open = true;

        handle_mouse(&mut app, right_click(5));

        assert_eq!(app.screen, Screen::Detail, "picker must stay in front");
        assert!(app.picker_open, "picker must not be silently dismissed");
    }

    /// A stale in-flight drag (e.g. right-click navigating away mid-drag,
    /// or a dropped button-release) must not keep painting its inverted
    /// highlight over whatever screen is now shown — any non-Left-button
    /// event cancels it.
    #[test]
    fn a_non_left_button_event_cancels_a_stale_drag() {
        let mut app = demo_app();
        app.mouse.selecting = true;

        handle_mouse(&mut app, right_click(5));

        assert!(
            !app.mouse.selecting,
            "right-click should cancel a stale left-drag"
        );
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
