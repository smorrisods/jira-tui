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
    render_at(app, 120, 40)
}

fn render_at(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
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
fn detail_screen_wide_layout_shows_side_rail_panels() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    let text = render_at(&app, 120, 34);
    assert!(
        text.contains("workflow"),
        "wide Detail should show a workflow rail panel"
    );
    assert!(
        text.contains("children"),
        "wide Detail should show a children rail panel"
    );
    assert!(
        text.contains("child DS-2725"),
        "the children panel should list the Epic's child"
    );
    assert!(
        text.contains("Acceptance"),
        "the main column should still show acceptance criteria"
    );
    // Regression test: the workflow/meta rail panels used to be sized from
    // their logical (unwrapped) line count, so a wrapped chip strip or a
    // long components/labels line would silently push the panel's own
    // trailing content (the "t to change" hint, the "updated:" line) off
    // the bottom of its allotted height.
    assert!(
        text.contains("t to change"),
        "the workflow panel's hint line must not be clipped by a wrapped chip strip"
    );
    assert!(
        text.contains("updated:"),
        "the people & meta panel's trailing 'updated:' line must not be clipped"
    );
}

#[test]
fn detail_screen_narrow_layout_shows_facts_and_linked_panels() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    let text = render_at(&app, 84, 46);
    assert!(
        text.contains("facts"),
        "narrow Detail should show a facts panel"
    );
    assert!(
        text.contains("linked"),
        "narrow Detail should show a linked panel"
    );
    assert!(
        text.contains("Acceptance"),
        "the description should still show acceptance criteria"
    );
    let first_author = app.detail.as_ref().unwrap().comments[0].author.clone();
    assert!(
        text.contains(&first_author),
        "activity should still show comments"
    );
}

#[test]
fn detail_screen_narrow_facts_panel_folds_with_x() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    // Populate `detail_area` at the narrow breakpoint before folding.
    let _ = render_at(&app, 84, 46);
    let unfolded = render_at(&app, 84, 46);
    assert!(
        unfolded.contains("reporter:"),
        "unfolded facts panel should show every field"
    );

    app.toggle_facts_folded();
    let folded = render_at(&app, 84, 46);
    assert!(
        !folded.contains("reporter:"),
        "folding the facts panel should hide the per-field kv lines"
    );

    // The fold is narrow-only: at the wide breakpoint the rail's people &
    // meta panel shows in full regardless of `facts_folded`.
    let wide = render_at(&app, 120, 34);
    assert!(
        wide.contains("reporter:"),
        "the wide rail's meta panel must not be affected by the narrow fold state"
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

    // `jump_to_comments` picks the Wide/Narrow layout's comment offset from
    // the last-rendered `detail_area`'s width, so it needs a real render
    // first — before this phase there was only one document shape, so call
    // order didn't matter, but now it must mirror real usage (screen always
    // draws before a keypress is handled).
    let _ = render(&app);
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
fn switching_to_a_teammate_view_shows_a_confirmation_toast() {
    // Renamed from "...shows_in_the_header": this asserts on the transient
    // flash toast `switch_view` raises (`App::flash`), not the header —
    // the old header literally said "viewing: X" and this test's name
    // predates the phase-3 breadcrumb rework, which dropped that text in
    // favour of the breadcrumb itself (see the test below).
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.switch_view(jira_tui::domain::ViewKind::Teammate("alex.chen".into()));
    let text = render(&app);
    assert!(
        text.contains("viewing: alex.chen's Work"),
        "switching views should show a confirmation toast"
    );
}

#[test]
fn switching_to_a_teammate_view_shows_in_the_header_breadcrumb() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.switch_view(jira_tui::domain::ViewKind::Teammate("alex.chen".into()));
    // Expire the confirmation toast so only the header's own breadcrumb
    // text is left to satisfy the assertion.
    app.tick = app.flash_until;
    let text = render(&app);
    assert!(
        text.contains("alex.chen's Work"),
        "the header breadcrumb should show the active non-default view"
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
fn list_screen_shows_column_header_and_view_flip_arrows_at_120x34() {
    let mut app = demo_app();
    app.screen = Screen::List;
    let text = render_at(&app, 120, 34);
    assert!(
        text.contains('◂') && text.contains('▸'),
        "title should show view-flip arrows"
    );
    assert!(
        text.contains(" of "),
        "title should show the N of M count format"
    );
    for label in ["KEY", "TYPE", "STATUS", "SUMMARY", "ASSIGNEE", "UPDATED"] {
        assert!(
            text.contains(label),
            "column header should show {label} at a wide terminal"
        );
    }
}

#[test]
fn list_screen_tree_mode_shows_box_drawing_guides() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.toggle_list_view_mode();
    assert_eq!(app.list_view_mode, jira_tui::app::ListViewMode::Tree);
    let text = render_at(&app, 120, 34);
    // DS-2722 (an Epic) has DS-2725 as its only child in the demo data.
    assert!(
        text.contains('▾'),
        "a parent with children should show the expanded-parent marker"
    );
    assert!(
        text.contains("└─") || text.contains("├─"),
        "a child row should show a box-drawing guide"
    );
}

#[test]
fn list_screen_drops_optional_columns_and_shows_two_line_selected_row_at_84x46() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.selected = app
        .issues
        .iter()
        .position(|i| i.key == "DS-2725")
        .expect("DS-2725 should be in the demo data");
    let text = render_at(&app, 84, 46);
    assert!(
        !text.contains("ASSIGNEE") && !text.contains("TYPE"),
        "assignee/type column headers should be dropped below the narrow breakpoint"
    );
    assert!(
        text.contains("↳ DS-2722"),
        "the selected row's second line should show its parent key"
    );
    assert!(
        text.contains("all keys"),
        "the footer's pinned tail should still render alongside the narrower list (regression guard for phase 2)"
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

// The Detail footer's NAV group (which the `x` "fold facts" hint joins) is
// already wide enough to get dropped by `fit_footer_groups`' width-fit at
// typical terminal sizes, even before this phase — a pre-existing footer
// content/width tradeoff, not something to re-litigate here. See
// `ui::footer::tests::detail_nav_group_advertises_fold_facts_only_when_narrow`
// for a unit test against the pre-fit group content instead.

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
    // Deterministic branch name: this test must not depend on whatever
    // branch happens to be checked out when it runs (the real GitContext
    // otherwise reflects the actual repo state) — an unrelated long branch
    // name sharing the header's right column with the sync pill previously
    // pushed "live" off the edge here.
    app.git.branch = Some("main".into());
    let text = render(&app);
    assert!(
        text.contains("live") && text.contains("synced"),
        "a wide terminal should show the full sync pill"
    );
}

#[test]
fn header_sync_pill_shows_cache_and_synced_when_wide() {
    // Every other sync-pill test exercises only Source::Live — this covers
    // the Cache arm (amber LED, username as the detail segment) so a future
    // regression specific to Cache isn't invisible to the whole suite.
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.source = Source::Cache { user: "me".into() };
    app.git.branch = Some("main".into());
    let text = render(&app);
    assert!(
        text.contains("cache") && text.contains("synced"),
        "a wide terminal should show the full sync pill for a cached source"
    );
}

#[test]
fn header_sync_pill_degrades_gracefully_instead_of_clipping_mid_word() {
    // Regression test: the sync pill's site/user detail segment used to be
    // included unconditionally, which combined with a long branch name and
    // a long site hostname overflowed the header's un-wrapped right column
    // and silently clipped words off mid-way (e.g. "syn" instead of
    // "synced"). With a genuinely pathological branch + site name, there's
    // no room for the full pill — the correct outcome is a clean drop to
    // the collapsed LED + short-duration form, never a broken fragment.
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.source = Source::Live {
        site: "an-unusually-long-jira-site-hostname.atlassian.net".into(),
        user: "me".into(),
    };
    app.git.branch = Some("an-unusually-long-feature-branch-name-here".into());
    let text = render(&app);
    // Scoped to the header row specifically — the demo data happens to
    // include an unrelated issue titled "...breaks sync-docs", so a
    // whole-screen substring check would false-positive on that.
    let header_line = text.lines().nth(1).unwrap_or("");
    assert!(
        header_line.contains('●'),
        "the sync pill's LED should still render"
    );
    // If "sync" appears in the header at all, the complete word "synced"
    // must too — a bare "sync"/"syn" fragment would mean the pill got
    // clipped mid-word rather than cleanly dropping to the collapsed form.
    assert!(
        !header_line.contains("sync") || header_line.contains("synced"),
        "the pill must never clip \"synced\" mid-word: {header_line:?}"
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
