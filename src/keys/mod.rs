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

    // Global: `F9` toggles mouse mode from anywhere — every screen, every
    // modal, mid-edit, mid-search, and both onboarding phases — with no
    // per-screen carve-out needed, unlike the bare `m` this replaces. `m`
    // used to only toggle mouse mode on the handful of screens whose key
    // map fell through to the shared match at the bottom of this function
    // (Home/List/Detail/About); every screen with its own key-handling
    // block (Welcome, Search, Edit, FieldMapping, Board, Release, NewIssue)
    // — plus every type-to-filter modal (palette, assignee picker) — had no
    // binding for it at all, so the app's own onboarding tip ("press 'm'
    // any time for mouse mode") was simply false everywhere else. A literal
    // `m` can't be made global without breaking typing: several of those
    // same screens use `m` as an ordinary character in free-text fields
    // (New Issue's project/summary, Search's query, the in-TUI editor,
    // field-mapping's filter, the palette/assignee-picker filters, and
    // Welcome's Setup form) — a function key sidesteps that entirely, since
    // no text field can ever produce one. `Ctrl-M` was considered and
    // rejected: at the terminal level it's byte-identical to Enter/CR
    // (0x0D), so binding it would make Enter unreliably double as the
    // mouse toggle in many terminal/tmux configurations. `Alt-M` was also
    // considered and rejected: stock macOS Terminal.app doesn't send the
    // Meta/ESC-prefixed sequence for Option+letter by default (Option+M
    // types "µ" instead) without a manual preference change, and this
    // project ships macOS release artifacts.
    if key.code == KeyCode::F(9) {
        mouse::toggle_mouse(app);
        return;
    }

    // Recover from a stuck mouse drag: some terminals keep tagging pointer
    // *movement* with the last-pressed button's SGR code even after it's
    // actually released, instead of reporting a proper release — crossterm
    // then sees an unending stream of `MouseEventKind::Drag(Left)` with no
    // matching `Up`, so `mouse.selecting` never clears and the drag
    // highlight keeps growing to wherever the pointer wanders next. A real
    // keypress is a reliable signal the user wants control back, so it
    // always cancels an in-flight selection first, regardless of what it
    // otherwise does.
    app.mouse.selecting = false;

    // Help overlay swallows input while open.
    if app.show_help {
        app.show_help = false;
        return;
    }

    // Modal: confirm discarding a non-empty in-TUI editor buffer (raised by
    // `Screen::Edit`'s Esc). `y`/`Y` confirms; anything else dismisses the
    // prompt and resumes editing.
    if app.confirm_discard {
        app.confirm_discard = false;
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            app.cancel_edit();
        }
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

    // Modal: the Fix/Affects Version picker. Arrow/j/k move the cursor,
    // space toggles the highlighted version, tab switches which field is
    // being edited — unlike the assignee/palette pickers, this isn't
    // type-to-filter, so `j`/`k` are free for movement here.
    if app.version_picker_open {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.version_picker_move(-1),
            KeyCode::Down | KeyCode::Char('j') => app.version_picker_move(1),
            KeyCode::Tab => app.version_picker_switch_field(),
            KeyCode::Char(' ') => app.version_picker_toggle(),
            KeyCode::Enter => app.confirm_version_picker(),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => app.close_version_picker(),
            _ => {}
        }
        return;
    }

    // Modal: the spelling-suggestion picker (`F2`, opened from `Screen::Edit`
    // only — see the `KeyCode::F(2)` arm in that screen's own block below).
    if app.spell_suggest_open {
        match key.code {
            KeyCode::Esc => app.close_spell_suggest(),
            KeyCode::Enter => app.confirm_spell_suggest(),
            KeyCode::Up | KeyCode::Char('k') => app.spell_suggest_move(-1),
            KeyCode::Down | KeyCode::Char('j') => app.spell_suggest_move(1),
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
    // can't fire while one of them is already open. Excludes Edit/Preview:
    // both hold in-progress, not-yet-applied edit state (`app.editor`'s
    // typed buffer, `pending_edit`) that only `commit_tui_edit`/`apply_edit`/
    // `cancel_edit` know how to resolve — a palette action changing the
    // screen out from under either would silently orphan that state instead
    // of going through one of those. `NewIssue` is excluded for the same
    // reason: its typed project/type/summary has no restore path if a
    // palette action changes the screen out from under it (unlike `About`,
    // which stashes `about_return_screen`).
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && key.code == KeyCode::Char('k')
        && !matches!(
            app.screen,
            Screen::Edit | Screen::Preview | Screen::NewIssue
        )
    {
        app.open_palette();
        return;
    }

    // The edit preview is a confirm screen. Backing out doesn't discard —
    // it returns to the in-TUI editor with the content restored (see
    // `back_out_of_preview`), unless there's genuinely nothing to keep.
    if app.screen == Screen::Preview {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => app.apply_edit(),
            KeyCode::Esc
            | KeyCode::Char('q')
            | KeyCode::Char('h')
            | KeyCode::Left
            | KeyCode::Backspace => app.back_out_of_preview(),
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
            // A buffer that's actually changed since this session started
            // needs confirmation before Esc throws it away (see the
            // `confirm_discard` modal above); an unchanged one — including
            // a freshly opened description edit, which is non-empty by
            // definition — has nothing new to lose.
            KeyCode::Esc if app.editor.is_dirty() => app.confirm_discard = true,
            KeyCode::Esc => app.cancel_edit(),
            KeyCode::Char('s') if ctrl => app.commit_tui_edit(),
            KeyCode::Enter => app.editor.newline(),
            KeyCode::Backspace => app.editor.backspace(),
            KeyCode::Left => app.editor.left(),
            KeyCode::Right => app.editor.right(),
            KeyCode::Up => app.editor.up(),
            KeyCode::Down => app.editor.down(),
            KeyCode::Home => app.editor.line_start(),
            KeyCode::End => app.editor.line_end(),
            KeyCode::F(2) => app.open_spell_suggest(),
            KeyCode::Tab => {
                app.editor.insert_char(' ');
                app.editor.insert_char(' ');
            }
            KeyCode::Char(c) if !ctrl => app.editor.insert_char(c),
            _ => {}
        }
        return;
    }

    // The Search / go-to-issue screen captures typing. `Tab` toggles bulk
    // selection (only meaningful in the release review screen's bulk-add
    // mode, see `App::open_search_for_release`) — every other printable
    // character always types into the query, so bulk mode can't steal a
    // letter someone's trying to search for.
    if app.screen == Screen::Search {
        match key.code {
            KeyCode::Esc => app.close_search(),
            KeyCode::Enter => app.confirm_search(),
            KeyCode::Tab => app.search_toggle_bulk_selected(),
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
            KeyCode::Char('r') => app.refresh(),
            KeyCode::Char('?') | KeyCode::F(1) => app.show_help = true,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => back_or_quit(app),
            _ => {}
        }
        return;
    }

    // The release review screen: version list ↔ a drilled-in version's
    // issue list (see `app::release`'s doc comment for why this is one
    // screen with internal state, not two `Screen` variants). `Esc` backs
    // out one level at a time — from the issue list to the version list,
    // then (via `back_or_quit`, once `release_back` has nothing left to
    // undo) out of the screen entirely.
    if app.screen == Screen::Release {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.release_move(-1),
            KeyCode::Down | KeyCode::Char('j') => app.release_move(1),
            KeyCode::Enter | KeyCode::Right => app.release_confirm(),
            // Bulk membership: `Space` checks/unchecks an issue for
            // removal, `x` removes whatever's checked (or just the
            // highlighted issue if nothing was explicitly checked), `a`
            // opens Search in bulk-add mode for the drilled version. All
            // three are drill-mode-only in effect (`release_toggle_selected`/
            // `release_remove_selected` no-op on an empty issue list; `a`
            // is guarded here since it needs the drilled version's name).
            KeyCode::Char(' ') => app.release_toggle_selected(),
            KeyCode::Char('x') => app.release_remove_selected(),
            KeyCode::Char('a') if app.release.drilled.is_some() => {
                let version_name = app.release.drilled.as_ref().unwrap().name.clone();
                app.open_search_for_release(version_name);
            }
            KeyCode::Char('r') => app.release_refresh(),
            // Cycle the version list's grouping (split unreleased/released
            // vs. one flat list) — no-op in drill mode, mirroring the work
            // list's own `s` sort-cycle key.
            KeyCode::Char('s') if app.release.drilled.is_none() => app.release_cycle_list_mode(),
            KeyCode::Char('?') | KeyCode::F(1) => app.show_help = true,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Left | KeyCode::Backspace
                if !app.release_back() =>
            {
                back_or_quit(app);
            }
            _ => {}
        }
        return;
    }

    // The new-issue compose form: project / issue-type / summary. Tab/
    // Shift+Tab cycle which field has focus (leaving Project may trigger a
    // fresh issue-type fetch — see `App::new_issue_next_field`); Left/Right
    // (or Up/Down) cycle the issue-type list while that field has focus, no
    // filtering, mirroring the transition picker; typing/Backspace edit
    // whichever text field is focused.
    if app.screen == Screen::NewIssue {
        match key.code {
            KeyCode::Esc => app.cancel_new_issue(),
            KeyCode::Enter => app.confirm_new_issue_form(),
            KeyCode::Tab => app.new_issue_next_field(),
            KeyCode::BackTab => app.new_issue_prev_field(),
            KeyCode::Left | KeyCode::Up if app.new_issue.focus == app::NewIssueField::IssueType => {
                app.new_issue_cycle_issue_type(-1)
            }
            KeyCode::Right | KeyCode::Down
                if app.new_issue.focus == app::NewIssueField::IssueType =>
            {
                app.new_issue_cycle_issue_type(1)
            }
            KeyCode::Backspace => app.new_issue_backspace(),
            KeyCode::Char(c) => app.new_issue_input_char(c),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('?') | KeyCode::F(1) => app.show_help = true,
        // `i` for "Info" — About moved off `a` (freed for the reserved
        // "new issue" entry point on Home/List, see issue #89, and already
        // used for "add issues" in the Release drill-down) since it isn't
        // a primary action and doesn't deserve the primary lowercase slot.
        KeyCode::Char('i') => app.open_about(),
        KeyCode::Char('a') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.open_new_issue()
        }
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
        // Toggles the flat ↔ parent/child tree view — `H` for "hierarchy",
        // not `T`: nothing here relates to `t` (transition), the two used
        // to share a letter by case coincidence only.
        KeyCode::Char('H') if matches!(app.screen, Screen::Home | Screen::List) => {
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

        // Comments: add one via the in-TUI editor (c) or external $EDITOR
        // (C) (Detail or quick-view), jump to the comments section (]) /
        // back to the top ([), and step between individual comments (n/p).
        KeyCode::Char('c')
            if (app.screen == Screen::Detail && app.detail.is_some())
                || (matches!(app.screen, Screen::Home | Screen::List)
                    && app.quick_view
                    && app.quick_view_detail().is_some()) =>
        {
            app.begin_comment();
        }
        KeyCode::Char('C')
            if (app.screen == Screen::Detail && app.detail.is_some())
                || (matches!(app.screen, Screen::Home | Screen::List)
                    && app.quick_view
                    && app.quick_view_detail().is_some()) =>
        {
            app.request_edit = app.begin_external_comment();
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
        // `R` mirrors `r`'s own "act on whatever's focused" shape: with an
        // issue in view (Detail, or quick view showing one) it manages that
        // issue's Fix/Affects Version(s); otherwise it opens the Release
        // review screen. Same target-resolution scope as `A` for the first
        // arm — this used to be `R`'s only meaning, with the release screen
        // on the unrelated `w` (no mnemonic connection to "release" at all).
        KeyCode::Char('R')
            if (app.screen == Screen::Detail && app.detail.is_some())
                || (matches!(app.screen, Screen::Home | Screen::List)
                    && app.quick_view
                    && app.quick_view_detail().is_some()) =>
        {
            app.open_version_picker();
        }
        KeyCode::Char('R') if matches!(app.screen, Screen::Home | Screen::List) => {
            app.open_release_screen()
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
        PaletteAction::Transition { key, transition_id } => {
            // Re-verify the target issue is still what the palette showed —
            // an async detail refresh could have landed while the modal had
            // input captured. `confirm_transition` only ever acts on
            // `self.detail`, matching the direct `t` key's own Detail-only
            // scope (rows for this action are only ever built from that
            // same screen set — see `app::palette::build_palette_rows`).
            let Some(detail) = app.detail.as_ref() else {
                app.status = "issue changed — try again".into();
                return;
            };
            if &detail.key != key {
                app.status = "issue changed — try again".into();
                return;
            }
            let Some(idx) = detail
                .transitions
                .iter()
                .position(|t| &t.id == transition_id)
            else {
                app.status = "issue changed — try again".into();
                return;
            };
            app.picker_index = idx;
            app.confirm_transition();
        }
        PaletteAction::Assign => app.open_assignee_picker(),
        PaletteAction::Comment => app.begin_comment(),
        PaletteAction::CopyKey(key) => app.copy_key_value(key),
        PaletteAction::CopyUrl(key) => app.copy_url_for_key(key),
        PaletteAction::OpenInBrowser(key) => app.open_in_browser_for_key(key),
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
        PaletteAction::NewIssue => app.open_new_issue(),
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
        | Screen::Release
        | Screen::FieldMapping
        | Screen::NewIssue => {}
    }
}

fn back_or_quit(app: &mut App) {
    match app.screen {
        Screen::Home | Screen::Welcome => app.should_quit = true,
        Screen::Preview | Screen::Edit => app.cancel_edit(),
        Screen::Search => app.close_search(),
        Screen::FieldMapping => app.close_field_mapping(),
        Screen::List | Screen::Detail | Screen::Board | Screen::Release => {
            app.screen = Screen::Home
        }
        Screen::About => app.screen = app.about_return_screen,
        Screen::NewIssue => app.cancel_new_issue(),
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

    /// `F1` is a plain alias for `?` — the conventional help key most
    /// software uses, alongside this app's own `?`.
    #[test]
    fn f1_opens_help_same_as_question_mark() {
        for screen in [Screen::Home, Screen::List, Screen::Board] {
            let mut app = demo_app();
            app.screen = screen;
            handle_key(&mut app, KeyEvent::from(KeyCode::F(1)));
            assert!(app.show_help, "F1 should open help from {screen:?}");
        }

        let mut app = demo_app();
        app.open_release_screen();
        handle_key(&mut app, KeyEvent::from(KeyCode::F(1)));
        assert!(app.show_help, "F1 should open help from the release screen");
    }

    /// Regression test: a terminal that misreports pointer movement as a
    /// continued `Drag(Left)` after the button was actually released (some
    /// terminals stamp motion reports with the last-pressed button's SGR
    /// code regardless of whether it's still held) leaves `mouse.selecting`
    /// stuck true forever, since crossterm never delivers the matching
    /// `Up`. Any real keypress must cancel it as a recovery hatch.
    #[test]
    fn any_keypress_cancels_a_stuck_mouse_drag() {
        let mut app = demo_app();
        app.mouse.selecting = true;
        app.mouse.sel_start_y = 3;
        app.mouse.sel_end_y = 30;

        handle_key(&mut app, KeyEvent::from(KeyCode::Char('j')));

        assert!(
            !app.mouse.selecting,
            "a keypress should cancel an in-flight selection, however it got stuck"
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
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.screen, Screen::About);
        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Detail);
    }

    /// Re-opening About while already in About must not overwrite the
    /// remembered return screen with About itself.
    #[test]
    fn about_reopened_from_about_does_not_corrupt_return_screen() {
        let mut app = demo_app();
        app.screen = Screen::Detail;
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('i')));
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('i')));
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
    fn ctrl_k_does_not_open_the_palette_while_editing_or_previewing() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_tui_edit();
        assert_eq!(app.screen, Screen::Edit);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert!(
            !app.palette_open,
            "ctrl-k must not open the palette mid-edit, which could orphan the typed buffer"
        );

        app.commit_tui_edit();
        assert_eq!(app.screen, Screen::Preview);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert!(
            !app.palette_open,
            "ctrl-k must not open the palette on the preview/confirm screen either"
        );
    }

    #[test]
    fn ctrl_k_does_not_open_the_palette_on_the_new_issue_form() {
        let mut app = demo_app();
        app.open_new_issue();
        assert_eq!(app.screen, Screen::NewIssue);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert!(
            !app.palette_open,
            "ctrl-k must not open the palette while composing a new issue, which has no \
             restore path if the screen changes out from under it"
        );
    }

    #[test]
    fn palette_copy_key_dispatch_copies_whichever_key_the_row_carries() {
        // `PaletteAction::CopyKey(key)` carries its own already-resolved
        // key (see `app::palette`'s doc comment on why — Board's selected
        // card isn't reflected in `self.selected`), so confirming it must
        // copy that embedded key, not re-derive one from `selected_issue()`.
        // `app::tests::palette::build_palette_rows_carries_the_board_selected_key_not_selected_issue`
        // covers that the *right* key gets embedded in the first place;
        // this covers that dispatch actually uses what's embedded.
        let mut app = demo_app();
        app.screen = Screen::Home;
        app.selected = 0;
        let key = app.selected_issue().unwrap().key.clone();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        for c in "copy issue key".chars() {
            handle_key(&mut app, KeyEvent::from(KeyCode::Char(c)));
        }
        handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
        assert!(
            app.status.contains(&key),
            "status should report copying {key}, got: {}",
            app.status
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

    /// `R` mirrors `r`'s "act on whatever's focused" shape: with an issue in
    /// view it manages that issue's versions; otherwise it opens the
    /// Release review screen (replacing the old, unrelated `w` binding).
    #[test]
    fn shift_r_opens_the_version_picker_when_an_issue_is_in_view_else_the_release_screen() {
        let mut app = demo_app();
        app.screen = Screen::Home;
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('R')));
        assert_eq!(app.screen, Screen::Release);

        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('R')));
        assert!(app.version_picker_open);
        assert_eq!(
            app.screen,
            Screen::Detail,
            "opening the version picker must not navigate away from Detail"
        );
    }

    #[test]
    fn shift_h_toggles_tree_view_not_shift_t() {
        let mut app = demo_app();
        app.screen = Screen::Home;
        let before = app.list_view_mode;
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('T')));
        assert_eq!(
            app.list_view_mode, before,
            "'T' should no longer be bound to anything"
        );
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('H')));
        assert_ne!(
            app.list_view_mode, before,
            "'H' should toggle the tree view instead"
        );
    }

    /// Regression guard: `a` used to open About from anywhere; it's now
    /// palette-only (see `about_from_detail_returns_to_detail_not_home`)
    /// and reserved on Home/List for a future "new issue" entry point.
    #[test]
    fn lowercase_a_opens_new_issue_i_opens_about() {
        let mut app = demo_app();
        app.screen = Screen::Home;
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('a')));
        assert_eq!(
            app.screen,
            Screen::NewIssue,
            "'a' should open the new-issue form"
        );
        app.screen = Screen::Home;
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.screen, Screen::About, "'i' (Info) should open About");
    }

    #[test]
    fn r_refreshes_the_board() {
        let mut app = demo_app();
        app.open_board();
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('r')));
        assert!(
            app.last_synced.is_some(),
            "refresh should have run (demo mode resolves inline)"
        );
    }

    #[test]
    fn r_refreshes_the_release_screen_in_both_modes() {
        let mut app = demo_app();
        app.open_release_screen();
        let before_versions = app.release.versions.clone();
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('r')));
        assert_eq!(app.release.versions.len(), before_versions.len());

        app.release_confirm(); // drill into the first version
        assert!(app.release.drilled.is_some());
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('r')));
        assert!(
            app.release.drilled.is_some(),
            "refreshing while drilled in should re-fetch, not back out"
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

    #[test]
    fn escaping_a_non_empty_preview_returns_to_the_editor_with_content_intact() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_tui_edit();
        for c in "Still working on this.".chars() {
            app.editor.insert_char(c);
        }
        app.commit_tui_edit();
        assert_eq!(app.screen, Screen::Preview);

        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert_eq!(
            app.screen,
            Screen::Edit,
            "Esc on a non-empty preview should go back to editing, not discard it"
        );
        assert!(app.editor.to_text().contains("Still working on this."));
    }

    #[test]
    fn escaping_an_empty_preview_cancels_outright() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_comment();
        app.commit_tui_edit();
        assert_eq!(app.screen, Screen::Preview);

        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert_eq!(
            app.screen,
            Screen::Detail,
            "nothing to keep, so just cancel"
        );
    }

    #[test]
    fn escaping_a_non_empty_editor_asks_for_confirmation_before_discarding() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_comment();
        app.editor.insert_char('!');

        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(
            app.confirm_discard,
            "Esc on a non-empty buffer should raise the discard-confirm prompt"
        );
        assert_eq!(
            app.screen,
            Screen::Edit,
            "the prompt doesn't leave the editor by itself"
        );

        // Any key other than y/Y dismisses the prompt and keeps editing.
        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(!app.confirm_discard);
        assert_eq!(app.screen, Screen::Edit);
        assert_eq!(app.editor.to_text(), "!");
    }

    #[test]
    fn confirming_discard_with_y_actually_discards() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_comment();
        app.editor.insert_char('!');

        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(app.confirm_discard);

        handle_key(&mut app, KeyEvent::from(KeyCode::Char('y')));
        assert!(!app.confirm_discard);
        assert_eq!(app.screen, Screen::Detail);
    }

    #[test]
    fn escaping_an_empty_editor_cancels_without_a_prompt() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_comment();

        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(
            !app.confirm_discard,
            "an empty buffer has nothing to lose, so no prompt is needed"
        );
        assert_eq!(app.screen, Screen::Detail);
    }

    /// Regression test: a freshly opened description edit is preloaded with
    /// the existing description, so it's non-empty from the very first
    /// frame — Esc there with no changes made must cancel outright, not
    /// raise the discard-confirm prompt (which would previously fire for
    /// the single most common edit entry point: open, look, back out).
    #[test]
    fn escaping_a_freshly_opened_unmodified_description_edit_cancels_without_a_prompt() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_tui_edit();
        assert_eq!(app.screen, Screen::Edit);

        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(
            !app.confirm_discard,
            "an unmodified description edit has nothing new to lose"
        );
        assert_eq!(app.screen, Screen::Detail);
    }

    #[test]
    fn mouse_input_is_swallowed_while_confirm_discard_is_open() {
        use crossterm::event::{MouseEvent, MouseEventKind};

        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_comment();
        app.editor.insert_char('!');
        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(app.confirm_discard);

        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(
            app.confirm_discard,
            "a stray click must not silently dismiss the discard prompt"
        );
        assert_eq!(app.screen, Screen::Edit);
    }

    #[test]
    fn home_and_end_jump_within_the_current_line_in_the_editor() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_tui_edit();
        assert_eq!(app.screen, Screen::Edit);

        app.editor.cy = 0;
        app.editor.cx = 3;
        handle_key(&mut app, KeyEvent::from(KeyCode::Home));
        assert_eq!(app.editor.cx, 0, "Home should jump to the line's start");

        handle_key(&mut app, KeyEvent::from(KeyCode::End));
        assert_eq!(
            app.editor.cx,
            app.editor.lines[0].chars().count(),
            "End should jump to the line's end"
        );
    }

    #[test]
    fn f2_opens_the_spell_suggest_picker_and_swallows_input_while_open() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_tui_edit();
        app.editor.lines = vec!["a mispeled word".into()];
        app.editor.cy = 0;
        app.editor.cx = 2;

        handle_key(&mut app, KeyEvent::from(KeyCode::F(2)));
        assert!(app.spell_suggest_open);
        let before = app.spell_suggest.selected;

        // While the picker is open, typing must not fall through to the
        // editor buffer underneath it.
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(app.editor.lines[0], "a mispeled word");
        assert_eq!(app.spell_suggest.selected, before);

        handle_key(&mut app, KeyEvent::from(KeyCode::Down));
        assert!(app.spell_suggest.selected >= before);

        handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
        assert!(!app.spell_suggest_open);
        assert!(!app.editor.lines[0].contains("mispeled"));
    }

    #[test]
    fn esc_closes_the_spell_suggest_picker_without_changing_the_buffer() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_tui_edit();
        app.editor.lines = vec!["a mispeled word".into()];
        app.editor.cy = 0;
        app.editor.cx = 2;

        handle_key(&mut app, KeyEvent::from(KeyCode::F(2)));
        assert!(app.spell_suggest_open);

        handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
        assert!(!app.spell_suggest_open);
        assert_eq!(app.editor.lines[0], "a mispeled word");
        // Esc should close the picker, not also cancel the whole edit.
        assert_eq!(app.screen, Screen::Edit);
    }

    /// `F9` (not the old bare `m`, which conflicts with typing on several
    /// screens — see `handle_key`'s own doc comment above the check) must
    /// toggle mouse mode identically from literally every screen and every
    /// modal, since it can never collide with typed text. Table-driven so
    /// a screen/modal added later without adding it here is an obvious gap.
    #[test]
    fn f9_toggles_mouse_mode_from_every_screen_and_modal() {
        type Setup = fn(&mut App);
        let cases: &[(&str, Setup)] = &[
            ("Home", |app| app.screen = Screen::Home),
            ("List", |app| app.screen = Screen::List),
            ("Board", |app| app.open_board()),
            ("Detail", |app| {
                app.selected = 0;
                app.open_detail();
            }),
            ("Welcome intro", |app| {
                app.screen = Screen::Welcome;
                app.onboarding.welcome_phase = app::WelcomePhase::Intro;
            }),
            ("Welcome setup", |app| {
                app.screen = Screen::Welcome;
                app.onboarding.welcome_phase = app::WelcomePhase::Setup;
            }),
            ("Search", |app| app.open_search()),
            ("NewIssue", |app| app.open_new_issue()),
            ("FieldMapping", |app| {
                app.open_field_mapping();
            }),
            ("Release", |app| app.open_release_screen()),
            ("show_help overlay", |app| app.show_help = true),
            ("palette_open", |app| app.open_palette()),
            ("assignee_picker_open", |app| {
                app.selected = 0;
                app.open_detail();
                app.open_assignee_picker();
            }),
        ];

        for (name, setup) in cases {
            let mut app = demo_app();
            setup(&mut app);
            assert!(!app.mouse.enabled, "{name}: expected to start disabled");

            handle_key(&mut app, KeyEvent::from(KeyCode::F(9)));
            assert!(app.mouse.enabled, "{name}: F9 should enable mouse mode");

            handle_key(&mut app, KeyEvent::from(KeyCode::F(9)));
            assert!(!app.mouse.enabled, "{name}: F9 should toggle it back off");
        }
    }

    /// `m` must keep working as an ordinary typed character everywhere it
    /// used to (New Issue's summary field here) now that the global toggle
    /// has moved to `F9` — this is the whole reason `F9` was chosen over a
    /// literal `m` in the first place.
    #[test]
    fn m_still_types_a_literal_character_on_text_entry_screens() {
        let mut app = demo_app();
        app.open_new_issue();
        app.new_issue.focus = app::NewIssueField::Summary;

        handle_key(&mut app, KeyEvent::from(KeyCode::Char('m')));

        assert_eq!(app.new_issue.summary, "m");
        assert!(
            !app.mouse.enabled,
            "'m' must not be hijacked as a mouse-mode toggle while typing"
        );
    }

    /// `m` is no longer bound to anything on the screens that used to
    /// fall through to the shared match's old `Char('m')` case either —
    /// pinning that the old binding was fully removed, not just shadowed.
    #[test]
    fn m_no_longer_toggles_mouse_mode_on_home() {
        let mut app = demo_app();
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('m')));
        assert!(!app.mouse.enabled, "'m' should no longer toggle mouse mode");
    }
}
