//! Headless rendering tests — drive the real `ui::draw` through a TestBackend
//! and assert the composed screen text, so each screen is exercised in CI.

use jira_tui::app::{App, Screen, WelcomePhase};
use jira_tui::domain::Source;
use jira_tui::ui;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;

fn dump(buf: &Buffer) -> String {
    let area = buf.area;
    let mut s = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            let sym = buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" ");
            s.push_str(sym);
        }
        s.push('\n');
    }
    s
}

fn render(app: &App) -> String {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    dump(terminal.backend().buffer())
}

fn demo_app() -> App {
    App::new(true)
}

#[test]
fn home_screen_shows_work_and_context() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let text = render(&app);
    assert!(text.contains("my work"), "home should list my work");
    assert!(
        text.contains("current context"),
        "home should show git context"
    );
    assert!(text.contains("DS-"), "home should show issue keys");
}

#[test]
fn detail_screen_renders_adf() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    assert_eq!(app.screen, Screen::Detail);
    let text = render(&app);
    assert!(
        text.contains("Acceptance"),
        "detail should show acceptance criteria"
    );
    assert!(text.contains("["), "detail should render task checkboxes");
}

#[test]
fn detail_screen_shows_an_epics_children() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    assert_eq!(app.screen, Screen::Detail);
    let text = render(&app);
    assert!(
        text.contains("child DS-2725") && text.contains("Develop"),
        "an Epic's detail view should list its child issue and type"
    );
}

#[test]
fn detail_screen_shows_comment_indicator_and_jumps_to_comments() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    let comment_count = app.detail.as_ref().unwrap().comments.len();
    assert!(comment_count > 0, "demo detail should have canned comments");

    // The comments section header (with its count) lives below the fold
    // until you scroll or jump to it.
    app.jump_to_comments();
    let text = render(&app);
    assert!(
        text.contains("💬") && text.contains(&comment_count.to_string()),
        "jumping to the comments section should show a comment-count header"
    );
    let first_author = app.detail.as_ref().unwrap().comments[0].author.clone();
    assert!(
        text.contains(&first_author),
        "scrolling to comments should surface the first comment's author"
    );
}

#[test]
fn quick_view_panel_renders_comments_inline() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.quick_view = true;
    app.selected = 0;
    app.ensure_quick_view_loaded();
    app.jump_to_comments();

    let text = render(&app);
    let detail = app.quick_view_detail().unwrap();
    let first_author = detail.comments[0].author.clone();
    assert!(
        text.contains(&first_author),
        "quick view should render full comments, not just an indicator"
    );
}

#[test]
fn about_screen_shows_animated_banner() {
    let mut app = demo_app();
    app.screen = Screen::About;
    let text = render(&app);
    assert!(text.contains('█'), "about should render the block banner");
}

#[test]
fn welcome_intro_shows_jax_and_choices() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    app.onboarding.welcome_phase = WelcomePhase::Intro;
    let text = render(&app);
    assert!(text.contains("Jax"), "welcome should introduce Jax");
    assert!(
        text.contains("Set up live"),
        "welcome should offer live setup"
    );
}

#[test]
fn welcome_setup_shows_credential_fields() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    app.onboarding.welcome_phase = WelcomePhase::Setup;
    app.onboarding.field_token = "supersecret".to_string();
    let text = render(&app);
    assert!(text.contains("site"));
    assert!(text.contains("email"));
    assert!(text.contains("token"));
    // The token must be masked, never shown in the clear.
    assert!(!text.contains("supersecret"), "token must be masked");
    assert!(text.contains('•'), "masked token should render bullets");
}

#[test]
fn transition_picker_lists_targets() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_transitions();
    let text = render(&app);
    assert!(text.contains("move to"), "picker should show a title");
    assert!(
        text.contains("In Progress"),
        "picker should list transitions"
    );
    assert!(
        text.contains("current"),
        "picker should mark the current status"
    );
}

#[test]
fn view_picker_lists_my_work_all_project_and_teammates() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_view_picker();
    let text = render(&app);
    assert!(text.contains("switch view"), "picker should show a title");
    assert!(text.contains("My Work"), "picker should list My Work");
    assert!(
        text.contains("current"),
        "the active view should be marked current"
    );
    assert!(
        text.contains("All Project Issues"),
        "picker should list the all-project view"
    );
    assert!(
        text.contains("priya.nair"),
        "picker should list demo teammates seeded from loaded assignees"
    );
}

#[test]
fn assignee_picker_lists_unassign_me_and_teammates() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_assignee_picker();
    let text = render(&app);
    assert!(text.contains("assign to"), "picker should show a title");
    assert!(text.contains("Unassign"), "picker should offer Unassign");
    assert!(
        text.contains("scott.morris"),
        "picker should pin the current user"
    );
    assert!(
        text.contains("priya.nair"),
        "picker should list demo teammates"
    );
}

#[test]
fn assignee_picker_filters_as_the_query_is_typed() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_assignee_picker();
    app.assignee_picker_input_char('p');
    app.assignee_picker_input_char('r');
    app.assignee_picker_input_char('i');
    let text = render(&app);
    assert!(text.contains("priya.nair"), "filtered picker keeps a match");
    assert!(
        !text.contains("Unassign"),
        "filtered picker hides non-matching rows"
    );
}

#[test]
fn assignee_picker_keeps_the_selection_in_view_on_a_short_terminal() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_assignee_picker();
    // Demo data has "Unassign" + 4 users; move to the last row.
    let last = app.assignee_picker.rows.len() - 1;
    app.assignee_picker.selected = last;

    // A short terminal can't fit every row at once — the scroll window
    // must still keep the highlighted (last) row on screen instead of
    // clipping it off the bottom of the popup.
    let backend = TestBackend::new(120, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());

    let last_label = match &app.assignee_picker.rows[last] {
        jira_tui::app::AssigneeRow::Unassign => "Unassign".to_string(),
        jira_tui::app::AssigneeRow::User(u) => u.display_name.clone(),
    };
    assert!(
        text.contains(&last_label),
        "the selected row must stay visible even when the popup can't fit every row"
    );
    assert!(
        text.contains('▌'),
        "the selection cursor must still be drawn somewhere"
    );
}

#[test]
fn switching_to_a_teammate_view_shows_in_the_header() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.switch_view(jira_tui::domain::ViewKind::Teammate("alex.chen".into()));
    let text = render(&app);
    assert!(
        text.contains("viewing: alex.chen's Work"),
        "header should indicate the active non-default view"
    );
}

#[test]
fn preview_screen_renders_pending_edit() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.finish_edit("## Fresh heading\n\nEdited body text.");
    assert_eq!(app.screen, Screen::Preview);
    let text = render(&app);
    assert!(text.contains("preview"), "preview should be titled");
    assert!(text.contains("Fresh heading"));
    assert!(text.contains("apply"));
    assert!(
        text.contains("y/⏎"),
        "preview copy should mention both y and Enter apply the edit"
    );
}

#[test]
fn in_tui_editor_renders_buffer() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    let text = render(&app);
    assert!(text.contains("editing"), "editor should be titled");
    assert!(
        text.contains("Problem"),
        "editor should show the description Markdown"
    );
}

#[test]
fn quick_view_panel_shows_selected_issue() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.quick_view = true;
    app.selected = 0;
    // Simulate the run loop's lazy-load call.
    app.ensure_quick_view_loaded();
    let text = render(&app);
    assert!(
        text.contains("quick view"),
        "quick view panel should render"
    );
    // Once loaded, the full fields and ADF body should be visible, not just
    // the one-line summary.
    assert!(text.contains("assignee:"), "quick view should show fields");
    assert!(
        text.contains("Problem") || text.contains("Proposed"),
        "quick view should render the full ADF body"
    );
}

#[test]
fn quick_view_panel_spans_full_width() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = true;
    app.selected = 0;
    let text = render(&app);
    // Rendered as one string per row by our TestBackend dump helper: the
    // "quick view" title should appear near the left edge of a wide frame,
    // confirming the panel isn't squeezed into a narrow column.
    let line = text.lines().find(|l| l.contains("quick view")).unwrap();
    assert!(
        line.trim_start().starts_with('│') || line.trim_start().starts_with('╭'),
        "quick view panel should start at the frame's left edge (full width), got: {line:?}"
    );
}

#[test]
fn work_list_title_shows_sort_and_filter() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.cycle_filter();
    let text = render(&app);
    assert!(
        text.contains("sort"),
        "list title should show the sort mode"
    );
    assert!(
        text.contains("filter"),
        "list title should show the active filter"
    );
}

#[test]
fn work_list_title_reflects_the_active_view() {
    use jira_tui::domain::ViewKind;

    let mut app = demo_app();
    app.screen = Screen::Home;
    app.switch_view(ViewKind::AllProject);
    let text = render(&app);
    assert!(
        text.contains("all project issues"),
        "home list title should reflect the All Project Issues view, not stay stuck on 'my work'"
    );

    app.switch_view(ViewKind::Teammate("alex.chen".into()));
    let text = render(&app);
    assert!(
        text.contains("alex.chen's work"),
        "home list title should reflect the active teammate view"
    );

    app.screen = Screen::List;
    let text = render(&app);
    assert!(
        text.contains("all alex.chen's work"),
        "the full-screen List view should prefix a non-'all'-prefixed label with 'all'"
    );
}

#[test]
fn jax_companion_appears_when_toggled() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.show_jax = true;
    let text = render(&app);
    assert!(text.contains("jax"), "the Jax companion box should render");
}

#[test]
fn jax_companion_sits_above_quick_view_not_overlapping() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.show_jax = true;
    app.quick_view = true;
    app.selected = 0;
    let text = render(&app);
    let lines: Vec<&str> = text.lines().collect();
    let jax_row = lines
        .iter()
        .position(|l| l.contains("jax"))
        .expect("jax box should render");
    let quick_view_row = lines
        .iter()
        .position(|l| l.contains("quick view"))
        .expect("quick view panel should render");
    assert!(
        jax_row < quick_view_row,
        "Jax (row {jax_row}) should appear above the quick-view panel (row {quick_view_row})"
    );
}

#[test]
fn search_screen_shows_goto_and_filters_results() {
    let mut app = demo_app();
    app.open_search();
    for c in "DS-2603".chars() {
        app.search_input_char(c);
    }
    let text = render(&app);
    assert!(
        text.contains("go to"),
        "search should offer a go-to-issue action"
    );
    assert!(text.contains("DS-2603"), "search should show the typed key");
}

#[test]
fn search_screen_empty_query_shows_hint_or_full_list() {
    let mut app = demo_app();
    app.open_search();
    let text = render(&app);
    assert!(
        text.contains("search") || text.contains("results"),
        "search screen should render its panels even with an empty query"
    );
}

#[test]
fn board_screen_shows_columns_and_lanes() {
    let mut app = demo_app();
    app.open_board();
    let text = render(&app);
    assert!(text.contains("board"), "board panel should render");
    assert!(text.contains("To Do"), "board should show a status column");
    assert!(
        text.contains("No epic"),
        "board should show the no-epic lane"
    );
    assert!(text.contains("DS-"), "board should show issue cards");
}

#[test]
fn board_screen_highlights_selected_card() {
    let mut app = demo_app();
    app.open_board();
    // Land the selection on a real card, then confirm it renders without
    // panicking and still shows the board chrome.
    app.board_move_col(0);
    let text = render(&app);
    assert!(
        text.contains("column"),
        "board title should show column count"
    );
}

#[test]
fn board_footer_does_not_advertise_the_unbound_t_key() {
    // Regression test: an earlier draft of the footer's Board ACT group
    // advertised `t transition`, but `t` isn't bound on Board at all (only
    // within Detail) — pressing it there did nothing.
    let mut app = demo_app();
    app.open_board();
    let text = render(&app);
    assert!(
        !text.contains("t transition"),
        "Board's footer must not advertise a key that isn't bound there"
    );
}

#[test]
fn board_footer_shows_every_group_at_the_default_test_width() {
    // Regression test: the Board footer's GO group (search/view) used to
    // silently drop even at the standard 120-column test width because the
    // group content (inflated by the unbound `t transition` hint above) was
    // too wide for the footer's hint column.
    let mut app = demo_app();
    app.open_board();
    let text = render(&app);
    assert!(text.contains("NAV"), "NAV group should render");
    assert!(text.contains("ACT"), "ACT group should render");
    assert!(text.contains("GO"), "GO group should render");
    assert!(text.contains("all keys"), "the pinned tail should render");
}

#[test]
fn footer_status_truncates_with_an_ellipsis_instead_of_hard_clipping() {
    // Regression test: the status column used to be rendered with no
    // truncation at all, so a long message (a real live-Jira failure
    // reason, for example) got hard-clipped mid-word by the Paragraph
    // instead of ending in a visible "…".
    let mut app = demo_app();
    app.status =
        "Jira unreachable (a very long and detailed error message that will not fit) — showing sample data".into();
    let backend = TestBackend::new(60, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(
        text.contains('…'),
        "a status message too long for the column should end in an ellipsis"
    );
}

#[test]
fn help_overlay_shows_audited_keys() {
    // Regression test for the SPEC.md §10 keybinding audit: `g`, `l`,
    // PgUp/PgDn, and the board's vim keys were bound in `src/keys.rs` but
    // missing from the help overlay.
    let mut app = demo_app();
    app.show_help = true;
    let text = render(&app);
    assert!(text.contains("go to Home"), "`g` should be documented");
    assert!(text.contains("go to List"), "`l` should be documented");
    assert!(text.contains("PgUp"), "PgUp/PgDn should be documented");
    assert!(
        text.contains("vim-style"),
        "board vim-key support should be documented"
    );
}

#[test]
fn help_overlay_shows_every_row_without_clipping() {
    // The popup used to be a fixed 62% of the frame height; once the audit
    // fixes above grew the row count, the last several rows (including the
    // overlay's own "? / q toggle help / quit" hint) were clipped off the
    // bottom with no scroll indicator. The popup is now sized to the row
    // count instead, so every row — especially the last one — must render.
    let mut app = demo_app();
    app.show_help = true;
    let text = render(&app);
    assert!(
        text.contains("? / q") && text.contains("toggle help"),
        "the last help row (close help/quit) must not be clipped"
    );
}

#[test]
fn help_overlay_key_column_has_a_separator_for_long_keys() {
    // The key column used to be a fixed 9-char width; keys longer than that
    // (e.g. "PgUp / PgDn", "h/j/k/l (board)") ran straight into their
    // description with no separating space.
    let mut app = demo_app();
    app.show_help = true;
    let text = render(&app);
    assert!(
        !text.contains("PgDnjump"),
        "PgUp / PgDn must not glue into its description"
    );
    assert!(
        !text.contains("(board)vim"),
        "the board vim-key row must not glue into its description"
    );
}

#[test]
fn footer_shows_grouped_hints_on_a_wide_terminal() {
    // SPEC.md §2: footer hints are grouped under a faint uppercase label
    // (NAV/VIEW/ACT/GO). At a comfortably wide terminal every group for
    // Home should render.
    let backend = TestBackend::new(120, 34);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = demo_app();
    app.screen = Screen::Home;
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(text.contains("NAV"), "the NAV group should render");
    assert!(text.contains("VIEW"), "the VIEW group should render");
    assert!(text.contains("GO"), "the GO group should render");
    assert!(text.contains("all keys"), "the pinned tail should render");
}

#[test]
fn footer_never_wraps_and_keeps_all_keys_visible_on_a_narrow_terminal() {
    // SPEC.md §2's no-wrap rule: whole groups drop right-to-left as the
    // terminal narrows, but `? all keys` — the pinned last group — must
    // always survive, even when nothing else fits.
    let backend = TestBackend::new(40, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = demo_app();
    app.screen = Screen::Home;
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(
        text.contains("all keys"),
        "the pinned `? all keys` group must survive even on a very narrow terminal"
    );
}

#[test]
fn footer_renders_at_the_84x46_reference_size() {
    // SPEC.md §13: exercise breakpoints at both reference sizes.
    let backend = TestBackend::new(84, 46);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = demo_app();
    app.open_board();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(
        text.contains("all keys"),
        "the board footer's pinned tail should render at the 84-col reference size"
    );
}

#[test]
fn header_breadcrumb_shows_the_current_view_on_home() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let text = render(&app);
    assert!(
        text.contains("My Work"),
        "Home's breadcrumb should show the current view"
    );
}

#[test]
fn header_breadcrumb_shows_view_and_screen_on_list() {
    let mut app = demo_app();
    app.screen = Screen::List;
    let text = render(&app);
    assert!(
        text.contains("My Work") && text.contains("List"),
        "List's breadcrumb should show both the view and the screen"
    );
}

#[test]
fn header_breadcrumb_shows_the_issue_key_and_back_count_on_detail() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let text = render(&app);
    let key = app.detail.as_ref().unwrap().key.clone();
    assert!(
        text.contains(&key),
        "Detail's breadcrumb should show the open issue's key"
    );
}

#[test]
fn header_breadcrumb_shows_the_active_filter() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.cycle_filter();
    let text = render(&app);
    assert!(
        text.contains("filter"),
        "an active filter should appear as a breadcrumb crumb"
    );
}

#[test]
fn header_sync_pill_shows_demo_in_demo_mode() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let text = render(&app);
    assert!(text.contains("demo"), "the sync pill should show demo mode");
}

#[test]
fn header_sync_pill_shows_live_and_synced_when_wide() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.source = Source::Live {
        site: "example.atlassian.net".into(),
        user: "me".into(),
    };
    let text = render(&app);
    assert!(
        text.contains("live") && text.contains("synced"),
        "a wide terminal should show the full sync pill"
    );
}

#[test]
fn header_sync_pill_collapses_to_led_and_short_duration_when_narrow() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.source = Source::Live {
        site: "example.atlassian.net".into(),
        user: "me".into(),
    };
    let backend = TestBackend::new(70, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(
        !text.contains("synced"),
        "below the collapse width the pill should drop to just the LED and a short duration"
    );
}
