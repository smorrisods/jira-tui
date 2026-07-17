//! Keyboard and mouse event handling: translating input into `App` state
//! changes. Each screen with bespoke navigation (Welcome, the transition
//! picker, Preview, Edit, Search, Board) has its own key-handling block;
//! everything else falls through to the shared `handle_key` match. Split
//! into `welcome` (the onboarding key map) and `mouse` (pointer input) —
//! `handle_key`'s own match stays whole here since it's one connected
//! dispatch table over screen/modal state.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use jira_tui::app::{self, App, PaletteAction, Screen};

mod mouse;
mod welcome;

pub(crate) use mouse::handle_mouse;

pub(crate) fn handle_key(app: &mut App, key: KeyEvent) {
    // Global: Ctrl-C always quits.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    // Global: Ctrl-Z suspends to the shell, same as any other job-control-
    // aware terminal program; the run loop picks this up and hands off to
    // `crate::suspend`.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('z') {
        app.request_suspend = true;
        return;
    }

    // Help overlay swallows input while open.
    if app.show_help {
        app.show_help = false;
        return;
    }

    // Onboarding has its own key map (including a text-entry form).
    if app.screen == Screen::Welcome {
        welcome::handle_welcome_key(app, key);
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

    // Modal: the view switcher (My Work / All Project Issues / a teammate).
    if app.view_picker_open {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.view_picker_move(-1),
            KeyCode::Down | KeyCode::Char('j') => app.view_picker_move(1),
            KeyCode::Enter => app.confirm_view_switch(),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Left | KeyCode::Backspace => {
                app.close_view_picker()
            }
            _ => {}
        }
        return;
    }

    // Modal: the assignee picker. Type-to-filter like Search, so j/k aren't
    // bound to movement (they're typeable filter characters) — only the
    // arrow keys move the highlight.
    if app.assignee_picker_open {
        match key.code {
            KeyCode::Esc => app.close_assignee_picker(),
            KeyCode::Enter => app.confirm_assignee(),
            KeyCode::Up => app.assignee_picker_move(-1),
            KeyCode::Down => app.assignee_picker_move(1),
            KeyCode::Backspace => app.assignee_picker_backspace(),
            KeyCode::Char(c) => app.assignee_picker_input_char(c),
            _ => {}
        }
        return;
    }

    // Modal: the command palette (SPEC.md §8). Type-to-filter like the
    // assignee picker above.
    if app.palette_open {
        match key.code {
            KeyCode::Esc => app.close_palette(),
            KeyCode::Enter => {
                if let Some(action) = app.palette_selected_action().cloned() {
                    run_palette_action(app, &action);
                }
                app.close_palette();
            }
            KeyCode::Up => app.palette_move(-1),
            KeyCode::Down => app.palette_move(1),
            KeyCode::Backspace => app.palette_backspace(),
            KeyCode::Char(c) => app.palette_input_char(c),
            _ => {}
        }
        return;
    }

    // Global: `ctrl-k` opens the command palette from any screen (SPEC.md
    // §8) — placed after every other modal's own early-return above, so it
    // can't fire while one of them is already open.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
        app.open_palette();
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
            KeyCode::Char('V') => app.open_view_picker(),
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => back_or_quit(app),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('a') => app.open_about(),
        KeyCode::Char('g') => app.screen = Screen::Home,
        // `r` refreshes whatever's actually being looked at: the open
        // issue in Detail, or the quick-view panel once it has keyboard
        // focus (mirroring the `Enter`-opens-link guard below); otherwise
        // it refreshes the issue list, same as always.
        KeyCode::Char('r')
            if app.screen == Screen::Detail
                || (matches!(app.screen, Screen::Home | Screen::List)
                    && app.quick_view
                    && app.list_focus == app::ListFocus::QuickView) =>
        {
            app.refresh_detail();
        }
        KeyCode::Char('r') => app.refresh(),
        KeyCode::Char('m') => mouse::toggle_mouse(app),
        KeyCode::Char('b') if matches!(app.screen, Screen::Home | Screen::List) => app.open_board(),
        KeyCode::Char('/')
            if matches!(app.screen, Screen::Home | Screen::List | Screen::Detail) =>
        {
            app.open_search()
        }
        KeyCode::Tab if matches!(app.screen, Screen::Home | Screen::List) => {
            app.toggle_list_focus()
        }
        KeyCode::Char('J') => app.toggle_jax(),
        KeyCode::Char('y') => app.copy_key(),
        KeyCode::Char('Y') => app.copy_url(),
        KeyCode::Char('q') => back_or_quit(app),

        // Detail issue-navigation history: `←` steps back through issues
        // followed via in-body links (see `app::history`), falling through
        // to its prior meaning — exit Detail — once there's nothing left to
        // step through; see `go_back_or_out` (shared with right-click).
        KeyCode::Left => go_back_or_out(app),
        KeyCode::Right if app.screen == Screen::Detail && app.can_go_forward() => app.go_forward(),

        KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace => back_or_quit(app),

        // Sort + filter on the work list.
        KeyCode::Char('s') if matches!(app.screen, Screen::Home | Screen::List) => app.cycle_sort(),
        KeyCode::Char('S') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.toggle_sort_dir()
        }
        KeyCode::Char('f') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.cycle_filter()
        }
        KeyCode::Char('v') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.toggle_quick_view();
        }
        KeyCode::Char('T') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.toggle_list_view_mode();
        }
        KeyCode::Char('F') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.open_field_mapping();
        }
        KeyCode::Char('V') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.open_view_picker();
        }
        KeyCode::Char('<') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.cycle_view(-1);
        }
        KeyCode::Char('>') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.cycle_view(1);
        }

        KeyCode::Char('l') if app.screen != Screen::Detail => app.screen = Screen::List,

        KeyCode::Char('t') if app.screen == Screen::Detail => app.open_transitions(),
        // In-TUI editor (default) and external $EDITOR (E).
        KeyCode::Char('e') if app.screen == Screen::Detail && app.detail.is_some() => {
            app.begin_tui_edit();
        }
        KeyCode::Char('E') if app.screen == Screen::Detail && app.detail.is_some() => {
            app.request_edit = app.begin_external_edit();
        }

        // Comments: add one (Detail or quick-view), jump to the comments
        // section (]) / back to the top ([), and step between individual
        // comments (n/p).
        KeyCode::Char('c')
            if (app.screen == Screen::Detail && app.detail.is_some())
                || (matches!(app.screen, Screen::Home | Screen::List)
                    && app.quick_view
                    && app.quick_view_detail().is_some()) =>
        {
            app.begin_comment();
        }
        // Assignee picker: reassign or unassign the viewed issue (Detail or
        // quick-view). Deliberately not gated on `list_focus` — like `c`
        // above, opening a modal picker captures all subsequent input
        // anyway, so there's no ambiguity about which issue it targets.
        KeyCode::Char('A')
            if (app.screen == Screen::Detail && app.detail.is_some())
                || (matches!(app.screen, Screen::Home | Screen::List)
                    && app.quick_view
                    && app.quick_view_detail().is_some()) =>
        {
            app.open_assignee_picker();
        }
        KeyCode::Char(']')
            if app.screen == Screen::Detail
                || (matches!(app.screen, Screen::Home | Screen::List) && app.quick_view) =>
        {
            app.jump_to_comments();
        }
        KeyCode::Char('[')
            if app.screen == Screen::Detail
                || (matches!(app.screen, Screen::Home | Screen::List) && app.quick_view) =>
        {
            app.jump_to_top();
        }
        KeyCode::Char('n')
            if app.screen == Screen::Detail
                || (matches!(app.screen, Screen::Home | Screen::List) && app.quick_view) =>
        {
            app.next_comment();
        }
        KeyCode::Char('p')
            if app.screen == Screen::Detail
                || (matches!(app.screen, Screen::Home | Screen::List) && app.quick_view) =>
        {
            app.prev_comment();
        }
        // Fold/unfold the narrow Detail layout's facts panel (SPEC.md §6).
        // Unconditionally on-screen rather than width-gated — a no-op in
        // the wide layout, matching this codebase's existing Screen-only
        // gating style (e.g. `t`/`e` above).
        KeyCode::Char('x') if app.screen == Screen::Detail => app.toggle_facts_folded(),

        // In-body link navigation: issue keys and URLs mentioned in the
        // description/comments/parent/links fields are underlined; `{`/`}`
        // cycle which one is highlighted, `Enter` opens it (jumps to the
        // issue, or opens the URL in the system browser).
        KeyCode::Char('}')
            if app.screen == Screen::Detail
                || (matches!(app.screen, Screen::Home | Screen::List) && app.quick_view) =>
        {
            app.next_link();
        }
        KeyCode::Char('{')
            if app.screen == Screen::Detail
                || (matches!(app.screen, Screen::Home | Screen::List) && app.quick_view) =>
        {
            app.prev_link();
        }
        KeyCode::Enter if app.screen == Screen::Detail && app.has_links() => {
            app.open_highlighted_link();
        }
        KeyCode::Enter
            if matches!(app.screen, Screen::Home | Screen::List)
                && app.quick_view
                && app.list_focus == app::ListFocus::QuickView
                && app.has_links() =>
        {
            app.open_highlighted_link();
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

/// Runs a confirmed command-palette row (SPEC.md §8) — matches each
/// `PaletteAction` to the exact same call its direct key makes. Lives here,
/// not in `app::palette`, because `ToggleMouse` needs real terminal I/O
/// (`mouse::toggle_mouse`'s `crossterm::execute!`) that only this binary
/// crate can perform — keeping every action's dispatch in one match instead
/// of splitting it across the app/binary boundary.
fn run_palette_action(app: &mut App, action: &PaletteAction) {
    match action {
        PaletteAction::Transition(id) => {
            let Some(detail) = app.detail.as_ref() else {
                return;
            };
            let Some(idx) = detail.transitions.iter().position(|t| &t.id == id) else {
                return;
            };
            app.picker_index = idx;
            app.confirm_transition();
        }
        PaletteAction::Assign => app.open_assignee_picker(),
        PaletteAction::Comment => app.begin_comment(),
        PaletteAction::CopyKey => app.copy_key(),
        PaletteAction::CopyUrl => app.copy_url(),
        PaletteAction::OpenInBrowser => app.open_selected_in_browser(),
        PaletteAction::FlipView => app.cycle_view(1),
        PaletteAction::CycleSort => app.cycle_sort(),
        PaletteAction::CycleFilter => app.cycle_filter(),
        PaletteAction::ToggleTree => app.toggle_list_view_mode(),
        PaletteAction::ToggleQuickView => app.toggle_quick_view(),
        PaletteAction::OpenBoard => app.open_board(),
        PaletteAction::Refresh => app.refresh(),
        PaletteAction::ToggleMouse => mouse::toggle_mouse(app),
        PaletteAction::ToggleJax => app.toggle_jax(),
        PaletteAction::OpenFieldMapping => {
            app.open_field_mapping();
        }
        PaletteAction::OpenAbout => app.open_about(),
        PaletteAction::OpenHelp => app.show_help = true,
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
        Screen::List | Screen::Detail | Screen::Board => app.screen = Screen::Home,
        Screen::About => app.screen = app.about_return_screen,
    }
}

/// `←` on Detail steps back through in-body-link history first, falling
/// through to `back_or_quit` once there's nothing left to step through.
/// Shared with the mouse's right-click "back" gesture (`keys::mouse`) so
/// the two stay in lockstep by construction rather than by comment.
fn go_back_or_out(app: &mut App) {
    if app.screen == Screen::Detail && app.can_go_back() {
        app.go_back();
    } else {
        back_or_quit(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_app() -> App {
        let mut app = App::new(true);
        app.screen = Screen::Home;
        app
    }

    /// CLAUDE.md "what to keep true": help toggles on `?`, and any key
    /// closes the overlay again rather than being forwarded to the
    /// underlying screen — coverage gap noticed while splitting this file,
    /// the overlay's own tests only lived in `tests/render.rs` (what it
    /// draws), never here (that it actually swallows the next keypress).
    #[test]
    fn help_overlay_swallows_the_first_keypress_then_closes() {
        let mut app = demo_app();
        app.show_help = true;
        app.selected = 0;

        // A key that would otherwise move the selection must be swallowed.
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('j')));

        assert!(!app.show_help, "help overlay should close on any keypress");
        assert_eq!(
            app.selected, 0,
            "the swallowed keypress must not also move the selection"
        );
    }

    #[test]
    fn esc_from_list_goes_home() {
        let mut app = demo_app();
        app.screen = Screen::List;
        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Home);
        assert!(!app.should_quit);
    }

    #[test]
    fn esc_from_home_quits() {
        let mut app = demo_app();
        app.screen = Screen::Home;
        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(app.should_quit);
    }

    /// Regression test for #38: About used to always back out to Home,
    /// discarding whatever screen it was opened from.
    #[test]
    fn about_from_detail_returns_to_detail_not_home() {
        let mut app = demo_app();
        app.screen = Screen::Detail;
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('a')));
        assert_eq!(app.screen, Screen::About);
        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Detail);
    }

    /// Re-pressing `a` while already in About must not overwrite the
    /// remembered return screen with About itself.
    #[test]
    fn about_reopened_from_about_does_not_corrupt_return_screen() {
        let mut app = demo_app();
        app.screen = Screen::Detail;
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('a')));
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('a')));
        assert_eq!(app.screen, Screen::About);
        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Detail);
    }

    #[test]
    fn ctrl_k_opens_the_palette_from_any_screen() {
        for screen in [Screen::Home, Screen::List, Screen::Detail, Screen::Board] {
            let mut app = demo_app();
            app.screen = screen;
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            );
            assert!(
                app.palette_open,
                "ctrl-k should open the palette from {screen:?}"
            );
        }
    }

    #[test]
    fn palette_esc_closes_without_side_effects() {
        let mut app = demo_app();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert!(app.palette_open);
        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(!app.palette_open);
        assert_eq!(app.screen, Screen::Home, "Esc must not run any action");
    }

    #[test]
    fn palette_confirm_runs_the_selected_action_and_closes() {
        let mut app = demo_app();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        for c in "about".chars() {
            handle_key(&mut app, KeyEvent::from(KeyCode::Char(c)));
        }
        handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
        assert!(!app.palette_open, "confirming should close the palette");
        assert_eq!(
            app.screen,
            Screen::About,
            "should dispatch the same open_about() 'a' calls"
        );
    }

    #[test]
    fn palette_transition_dispatch_uses_confirm_transition() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        assert_ne!(
            app.detail.as_ref().unwrap().status,
            "Done",
            "test needs an issue not already Done"
        );
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        // Filter to the "Transition {key} → Done" row specifically, so this
        // exercises a real status change rather than a same-status one
        // (the demo transitions list includes a "→ {current status}" entry,
        // which would be a false-negative no-op for this assertion).
        for c in "→ done".chars() {
            handle_key(&mut app, KeyEvent::from(KeyCode::Char(c)));
        }
        handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
        assert!(!app.palette_open);
        assert_eq!(
            app.detail.as_ref().unwrap().status,
            "Done",
            "confirming a Transition row should actually run confirm_transition"
        );
    }

    #[test]
    fn ctrl_k_does_not_open_the_palette_while_another_modal_owns_input() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.open_assignee_picker();
        assert!(app.assignee_picker_open);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert!(
            !app.palette_open,
            "ctrl-k should be swallowed as filter input by the already-open assignee picker"
        );
    }

    #[test]
    fn view_flip_keys_cycle_on_home_and_list() {
        let options = demo_app().view_options();
        for screen in [Screen::Home, Screen::List] {
            let mut app = demo_app();
            app.screen = screen;
            handle_key(&mut app, KeyEvent::from(KeyCode::Char('>')));
            assert_eq!(app.current_view, options[1], "'>' should advance the view");
            handle_key(&mut app, KeyEvent::from(KeyCode::Char('<')));
            assert_eq!(
                app.current_view, options[0],
                "'<' should step back to the previous view"
            );
        }
    }

    #[test]
    fn view_flip_keys_do_nothing_on_board() {
        let mut app = demo_app();
        app.open_board();
        let before = app.current_view.clone();
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('>')));
        assert_eq!(
            app.current_view, before,
            "view-flipping is scoped to Home/List, not Board"
        );
    }

    #[test]
    fn x_toggles_facts_folded_on_detail_only() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        assert!(!app.facts_folded);

        handle_key(&mut app, KeyEvent::from(KeyCode::Char('x')));
        assert!(app.facts_folded, "'x' should fold the facts panel");
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('x')));
        assert!(!app.facts_folded, "'x' again should unfold it");

        app.screen = Screen::Home;
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('x')));
        assert!(
            !app.facts_folded,
            "'x' is scoped to Detail, not Home/List/Board"
        );
    }
}
