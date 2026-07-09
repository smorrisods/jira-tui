use super::*;
use ratatui::layout::Rect;

fn demo_app() -> App {
    let mut app = App::new(true);
    app.screen = Screen::Home;
    app
}

#[test]
fn move_selection_clamps_to_bounds() {
    let mut app = demo_app();
    app.selected = 0;
    app.move_selection(-5);
    assert_eq!(app.selected, 0);
    app.move_selection(1000);
    assert_eq!(app.selected, app.issues.len() - 1);
}

#[test]
fn list_index_at_maps_rows_to_issues() {
    let app = demo_app();
    app.list_area.set(Rect::new(0, 4, 80, 8));
    app.list_start.set(0);
    assert_eq!(app.list_index_at(4), Some(0));
    assert_eq!(app.list_index_at(6), Some(2));
    // Above the list area.
    assert_eq!(app.list_index_at(0), None);
    // Below the populated rows.
    assert_eq!(app.list_index_at(200), None);
}

#[test]
fn click_opens_detail() {
    let mut app = demo_app();
    app.list_area.set(Rect::new(0, 4, 80, 8));
    app.list_start.set(0);
    app.mouse_down(5);
    app.mouse_up(5);
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.detail.is_some());
    assert_eq!(app.selected, 1);
}

#[test]
fn drag_sets_a_pending_copy_range() {
    let mut app = demo_app();
    app.list_area.set(Rect::new(0, 4, 80, 8));
    app.mouse_down(5);
    assert_eq!(app.selection_range(), Some((5, 5)));
    app.mouse_drag(8);
    assert_eq!(app.selection_range(), Some((5, 8)));
    app.mouse_up(8);
    assert_eq!(app.mouse.pending_copy, Some((5, 8)));
    assert!(!app.mouse.selecting);
    assert_eq!(app.screen, Screen::Home, "a drag must not open detail");
}

#[test]
fn credential_form_edits_focused_field() {
    let mut app = demo_app();
    app.onboarding.focus = Field::Email;
    app.input_char('a');
    app.input_char('b');
    assert_eq!(app.onboarding.field_email, "ab");
    app.input_backspace();
    assert_eq!(app.onboarding.field_email, "a");
    app.focus_next();
    assert_eq!(app.onboarding.focus, Field::Token);
    app.focus_prev();
    assert_eq!(app.onboarding.focus, Field::Email);
}

#[test]
fn submit_with_empty_fields_reports_and_does_not_panic() {
    let mut app = demo_app();
    app.onboarding.field_site.clear();
    app.onboarding.field_email.clear();
    app.onboarding.field_token.clear();
    app.submit_credentials();
    assert!(!app.onboarding.setup_msg.is_empty());
}

#[test]
fn selected_issue_url_is_a_browse_link() {
    let app = demo_app();
    let url = app.selected_issue_url().unwrap();
    assert!(url.contains("/browse/DS-"));
}

#[test]
fn confirm_transition_updates_status_locally() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    // Pick the "Done" transition (index 3 in the demo model).
    app.open_transitions();
    app.picker_index = 3;
    app.confirm_transition();
    assert!(!app.picker_open);
    assert_eq!(app.detail.as_ref().unwrap().status, "Done");
    // The summary list reflects it too.
    let key = &app.detail.as_ref().unwrap().key;
    assert_eq!(
        app.issues.iter().find(|i| &i.key == key).unwrap().status,
        "Done"
    );
}

#[test]
fn edit_flow_previews_then_applies() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let md = app.description_markdown().unwrap();
    assert!(md.contains("Problem"));

    app.finish_edit("## Edited\n\nBrand new body.");
    assert_eq!(app.screen, Screen::Preview);
    assert!(app.pending_edit.is_some());

    app.apply_edit();
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.pending_edit.is_none());
    // The new ADF is now the description.
    let desc = &app.detail.as_ref().unwrap().description;
    let text = crate::adf::to_markdown(desc);
    assert!(text.contains("Edited"));
    assert!(text.contains("Brand new body"));
}

#[test]
fn cancel_edit_discards_pending() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.finish_edit("## Nope");
    app.cancel_edit();
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.pending_edit.is_none());
}

#[test]
fn filter_narrows_and_clears() {
    let mut app = demo_app();
    let total = app.all_issues.len();
    // Cycle to the first status filter.
    app.cycle_filter();
    assert!(app.filter_status.is_some());
    let filtered = app.filter_status.clone().unwrap();
    assert!(app.issues.iter().all(|i| i.status == filtered));
    assert!(app.issues.len() <= total);
    // Cycle all the way back to "all".
    for _ in 0..20 {
        if app.filter_status.is_none() {
            break;
        }
        app.cycle_filter();
    }
    assert!(app.filter_status.is_none());
    assert_eq!(app.issues.len(), total);
}

#[test]
fn sort_reorders_and_preserves_selection() {
    let mut app = demo_app();
    // Select a known issue, then re-sort; selection should follow the key.
    let key = app.issues[2].key.clone();
    app.selected = 2;
    app.sort_key = SortKey::Key;
    app.sort_asc = true;
    app.recompute_view();
    assert_eq!(app.selected_issue().unwrap().key, key);
    // Ascending by key: keys are non-decreasing.
    let nums: Vec<u64> = app
        .issues
        .iter()
        .map(|i| i.key.rsplit('-').next().unwrap().parse().unwrap())
        .collect();
    assert!(nums.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn quick_view_uses_cached_detail_after_open() {
    let mut app = demo_app();
    app.selected = 0;
    assert!(app.quick_view_detail().is_none());
    app.open_detail();
    // Returning to the list, the opened issue is cached for quick view.
    assert!(app.detail_cache.contains_key(&app.issues[0].key));
}

#[test]
fn in_tui_editor_edits_then_commits_to_preview() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    assert_eq!(app.screen, Screen::Edit);
    assert!(!app.editor.lines.is_empty());
    // Type a heading on a fresh first line.
    app.editor.cx = 0;
    app.editor.cy = 0;
    for c in "X ".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);
    assert!(app.pending_edit.is_some());
}

#[test]
fn editor_newline_and_backspace_merge_lines() {
    let mut ed = EditorState::from_text("ab");
    ed.cx = 1;
    ed.newline();
    assert_eq!(ed.lines, vec!["a".to_string(), "b".to_string()]);
    assert_eq!((ed.cy, ed.cx), (1, 0));
    ed.backspace();
    assert_eq!(ed.lines, vec!["ab".to_string()]);
    assert_eq!((ed.cy, ed.cx), (0, 1));
}

#[test]
fn toggle_list_focus_flips_only_when_quick_view_open() {
    let mut app = demo_app();
    // Quick view closed: toggling is a no-op (always forced to List).
    app.toggle_list_focus();
    assert_eq!(app.list_focus, ListFocus::List);

    app.quick_view = true;
    app.toggle_list_focus();
    assert_eq!(app.list_focus, ListFocus::QuickView);
    app.toggle_list_focus();
    assert_eq!(app.list_focus, ListFocus::List);

    // Closing quick view resets focus even if it was on QuickView.
    app.quick_view = true;
    app.list_focus = ListFocus::QuickView;
    app.quick_view = false;
    app.toggle_list_focus();
    assert_eq!(app.list_focus, ListFocus::List);
}

#[test]
fn point_in_quick_view_respects_recorded_area_and_visibility() {
    let mut app = demo_app();
    app.quick_view_area.set(Rect::new(10, 10, 20, 5));
    // Quick view not open: never true, even inside the recorded rect.
    assert!(!app.point_in_quick_view(12, 12));
    app.quick_view = true;
    assert!(app.point_in_quick_view(12, 12));
    assert!(!app.point_in_quick_view(0, 0));
}

#[test]
fn search_finds_matches_by_key_and_summary() {
    let mut app = demo_app();
    app.open_search();
    for c in "accordion".chars() {
        app.search_input_char(c);
    }
    assert!(app.search.rows.iter().any(|r| matches!(
        r,
        SearchRow::Match(idx) if app.all_issues[*idx].summary.to_lowercase().contains("accordion")
    )));
}

#[test]
fn search_key_candidate_detects_issue_keys_only() {
    let mut app = demo_app();
    app.search.query = "DS-2603".to_string();
    assert_eq!(app.search_key_candidate(), Some("DS-2603".to_string()));
    app.search.query = "ds-2603".to_string();
    assert_eq!(app.search_key_candidate(), Some("DS-2603".to_string()));
    app.search.query = "accordion".to_string();
    assert_eq!(app.search_key_candidate(), None);
    app.search.query = "DS-".to_string();
    assert_eq!(app.search_key_candidate(), None);
}

#[test]
fn confirm_search_goto_opens_issue_directly_even_if_unfiltered() {
    let mut app = demo_app();
    app.open_search();
    for c in "DS-2603".chars() {
        app.search_input_char(c);
    }
    // The Goto row should be first.
    assert!(matches!(app.search.rows.first(), Some(SearchRow::Goto(k)) if k == "DS-2603"));
    app.search.selected = 0;
    app.confirm_search();
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-2603");
}

#[test]
fn confirm_search_match_opens_that_issue() {
    let mut app = demo_app();
    let target_key = app.all_issues[1].key.clone();
    app.open_search();
    for c in target_key.chars() {
        app.search_input_char(c);
    }
    // Find the Match row for our target and select it.
    let pos = app
        .search
        .rows
        .iter()
        .position(|r| matches!(r, SearchRow::Match(idx) if app.all_issues[*idx].key == target_key))
        .unwrap();
    app.search.selected = pos;
    app.confirm_search();
    assert_eq!(app.detail.as_ref().unwrap().key, target_key);
}

#[test]
fn close_search_returns_to_prior_screen() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.open_search();
    assert_eq!(app.screen, Screen::Search);
    app.close_search();
    assert_eq!(app.screen, Screen::List);
}

#[test]
fn demo_detail_unknown_key_is_clearly_labelled_not_found() {
    let detail = crate::domain::demo_detail("DS-99999");
    assert_eq!(detail.key, "DS-99999", "must preserve the requested key");
    assert!(detail.summary.to_lowercase().contains("not found"));
}

#[test]
fn open_by_key_syncs_selection_when_present_in_view() {
    let mut app = demo_app();
    let key = app.issues[2].key.clone();
    app.selected = 0;
    app.open_by_key(&key);
    assert_eq!(app.selected, 2);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}

#[test]
fn board_columns_follow_workflow_order() {
    let app = demo_app();
    let cols = app.board_columns();
    // Demo data spans Backlog, To Do, In Progress, In Review, Done.
    let positions: Vec<usize> = ["Backlog", "To Do", "In Progress", "In Review", "Done"]
        .iter()
        .filter_map(|s| cols.iter().position(|c| c == s))
        .collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "columns should follow workflow order, got {cols:?}"
    );
}

#[test]
fn board_lanes_group_by_epic_with_no_epic_bucket() {
    let app = demo_app();
    let lanes = app.board_lanes();
    assert!(lanes.contains(&None), "a 'no epic' lane should exist");
    assert!(
        lanes.iter().any(|l| l.as_deref() == Some("DS-2602")),
        "an epic-grouped lane should exist, got {lanes:?}"
    );
}

#[test]
fn board_cell_only_contains_matching_lane_and_status() {
    let app = demo_app();
    let lane = Some("DS-2602".to_string());
    let cell = app.board_cell(&lane, "To Do");
    assert!(!cell.is_empty());
    assert!(cell.iter().all(|i| i.epic == lane && i.status == "To Do"));
}

#[test]
fn board_navigation_moves_within_bounds() {
    let mut app = demo_app();
    app.open_board();
    // Column navigation clamps at the edges.
    let cols_len = app.board_columns().len();
    for _ in 0..(cols_len + 5) {
        app.board_move_col(1);
    }
    assert_eq!(app.board_sel.col, cols_len - 1);
    for _ in 0..(cols_len + 5) {
        app.board_move_col(-1);
    }
    assert_eq!(app.board_sel.col, 0);

    // Lane navigation clamps too.
    let lanes_len = app.board_lanes().len();
    for _ in 0..(lanes_len + 5) {
        app.board_move_lane(1);
    }
    assert_eq!(app.board_sel.lane, lanes_len - 1);
}

#[test]
fn board_navigation_scrolls_the_selection_into_view() {
    // Regression test: moving the card/lane selection with the keyboard
    // must scroll the board viewport to follow it — previously only the
    // mouse wheel (`board_scroll_by`) touched `board_scroll`, so moving
    // down with the keyboard past the visible window left the highlighted
    // card scrolled off-screen with no way to see (or keep navigating to)
    // it.
    let mut app = demo_app();
    app.open_board();
    // Simulate a short viewport — a handful of rows, definitely shorter
    // than the demo data's full lane list.
    app.board_area.set(Rect::new(0, 0, 80, 6));
    assert_eq!(app.board_scroll, 0);

    let lanes_len = app.board_lanes().len();
    assert!(
        lanes_len > 1,
        "test needs more than one lane to be meaningful"
    );

    // Step through every lane; the scroll offset must grow to keep the
    // selected lane's row within the 6-row window, and must never leave
    // the selection above the top of the window either.
    for _ in 0..lanes_len - 1 {
        app.board_move_lane(1);
        let selected_line = app.board_selected_line();
        let scroll = app.board_scroll as usize;
        let height = app.board_area.get().height as usize;
        assert!(
            selected_line >= scroll && selected_line < scroll + height,
            "selected line {selected_line} not within visible window [{scroll}, {})",
            scroll + height
        );
    }

    // And scrolling back up to the first lane must bring the offset back
    // down so the selection is visible again (not stuck scrolled down).
    for _ in 0..lanes_len - 1 {
        app.board_move_lane(-1);
    }
    assert_eq!(app.board_sel.lane, 0);
    let selected_line = app.board_selected_line();
    let scroll = app.board_scroll as usize;
    assert!(selected_line >= scroll, "scrolled past the first lane");
}

#[test]
fn board_open_loads_the_selected_card() {
    let mut app = demo_app();
    app.open_board();
    // Find a lane/column with at least one card and select it directly.
    let lanes = app.board_lanes();
    let cols = app.board_columns();
    let mut found = false;
    'outer: for (li, lane) in lanes.iter().enumerate() {
        for (ci, status) in cols.iter().enumerate() {
            if !app.board_cell(lane, status).is_empty() {
                app.board_sel.lane = li;
                app.board_sel.col = ci;
                app.board_sel.card = 0;
                found = true;
                break 'outer;
            }
        }
    }
    assert!(found, "expected at least one non-empty cell");
    app.board_open();
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.detail.is_some());
}

#[test]
fn board_scroll_by_never_goes_negative() {
    let mut app = demo_app();
    app.board_scroll = 0;
    app.board_scroll_by(-5);
    assert_eq!(app.board_scroll, 0);
    app.board_scroll_by(3);
    assert_eq!(app.board_scroll, 3);
}

#[test]
fn open_board_clamps_stale_selection() {
    let mut app = demo_app();
    app.board_sel = BoardSelection {
        lane: 999,
        col: 999,
        card: 999,
    };
    app.open_board();
    assert_eq!(app.screen, Screen::Board);
    assert!(app.board_sel.lane < app.board_lanes().len());
    assert!(app.board_sel.col < app.board_columns().len());
}

#[test]
fn begin_comment_from_detail_composes_and_apply_appends_it() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let before = app.detail.as_ref().unwrap().comments.len();

    app.begin_comment();
    assert_eq!(app.screen, Screen::Edit);
    assert_eq!(app.edit_target, EditTarget::Comment);
    assert!(app.editor.lines.iter().all(|l| l.is_empty()));

    for c in "Looks good to me.".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);
    assert!(app.pending_edit.is_some());

    app.apply_edit();
    assert_eq!(app.screen, Screen::Detail);
    let comments = &app.detail.as_ref().unwrap().comments;
    assert_eq!(comments.len(), before + 1);
    let newest = comments.last().unwrap();
    assert_eq!(
        crate::adf::to_markdown(&newest.body).trim(),
        "Looks good to me."
    );
}

#[test]
fn begin_comment_from_quick_view_returns_to_list_and_updates_cache() {
    let mut app = demo_app();
    app.selected = 0;
    app.quick_view = true;
    app.ensure_quick_view_loaded();
    assert_eq!(app.screen, Screen::Home);

    app.begin_comment();
    assert_eq!(app.screen, Screen::Edit);
    assert_eq!(app.edit_target, EditTarget::Comment);

    app.editor.insert_char('!');
    app.commit_tui_edit();
    app.apply_edit();

    // Composing from quick-view returns to the screen it was opened from.
    assert_eq!(app.screen, Screen::Home);
    let key = app.issues[0].key.clone();
    let cached = app.detail_cache.get(&key).unwrap();
    assert!(!cached.comments.is_empty());
}

/// Regression test: a comment session's `edit_target`/`edit_key`/
/// `edit_return_screen` must not leak into a later, unrelated external
/// `$EDITOR` description edit on a different issue — otherwise the second
/// issue's edited description would be silently posted as a comment on the
/// first issue instead of updating its own description.
#[test]
fn external_edit_after_comment_session_targets_the_right_issue() {
    let mut app = demo_app();

    // First, compose (and apply) a comment on issue A from quick-view.
    app.selected = 0;
    app.quick_view = true;
    app.ensure_quick_view_loaded();
    let issue_a = app.issues[0].key.clone();
    app.begin_comment();
    assert_eq!(app.edit_target, EditTarget::Comment);
    app.editor.insert_char('!');
    app.commit_tui_edit();
    app.apply_edit();
    assert_eq!(app.screen, Screen::Home);
    let a_comments_after_first_session = app.detail_cache.get(&issue_a).unwrap().comments.len();

    // Now open a different issue B in the full Detail screen and simulate
    // the external $EDITOR round-trip (`E`): prime the target, then finish
    // the edit as `editor_launch` would after the process exits.
    app.selected = 1;
    app.open_detail();
    let issue_b = app.detail.as_ref().unwrap().key.clone();
    assert_ne!(issue_a, issue_b, "test needs two distinct issues");
    let comments_on_b_before = app.detail.as_ref().unwrap().comments.len();

    app.begin_external_edit();
    assert_eq!(app.edit_target, EditTarget::Description);
    app.finish_edit("Updated description for B.");
    assert_eq!(app.screen, Screen::Preview);

    app.apply_edit();

    // B's description was updated, not appended as a comment...
    assert_eq!(app.screen, Screen::Detail);
    let b = app.detail.as_ref().unwrap();
    assert_eq!(b.key, issue_b);
    assert!(crate::adf::to_markdown(&b.description).contains("Updated description for B."));
    assert_eq!(b.comments.len(), comments_on_b_before);

    // ...and A's comments are untouched by this second session.
    let a_cached = app.detail_cache.get(&issue_a).unwrap();
    assert_eq!(a_cached.comments.len(), a_comments_after_first_session);
}

#[test]
fn cancel_comment_discards_pending_and_returns_to_detail() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let before = app.detail.as_ref().unwrap().comments.len();

    app.begin_comment();
    app.editor.insert_char('x');
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);

    app.cancel_edit();
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.pending_edit.is_none());
    assert_eq!(app.detail.as_ref().unwrap().comments.len(), before);
}

#[test]
fn jump_to_comments_and_back_moves_scroll() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    assert!(
        !app.detail.as_ref().unwrap().comments.is_empty(),
        "demo detail should have canned comments"
    );

    app.detail_scroll = 0;
    app.jump_to_comments();
    assert!(app.detail_scroll > 0);

    app.jump_to_top();
    assert_eq!(app.detail_scroll, 0);
}

#[test]
fn next_and_prev_comment_step_through_and_clamp() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let comment_count = app.detail.as_ref().unwrap().comments.len();
    assert!(comment_count >= 2, "test needs at least 2 demo comments");

    app.detail_scroll = 0;
    let mut positions = Vec::new();
    for _ in 0..comment_count {
        app.next_comment();
        positions.push(app.detail_scroll);
    }
    // Each step should move further down than the last.
    assert!(positions.windows(2).all(|w| w[0] < w[1]));

    // Stepping past the last comment clamps at the last position.
    let last = *positions.last().unwrap();
    app.next_comment();
    assert_eq!(app.detail_scroll, last);

    // Stepping back through should retrace, clamping at the first.
    for _ in 0..comment_count {
        app.prev_comment();
    }
    assert_eq!(app.detail_scroll, positions[0]);
    app.prev_comment();
    assert_eq!(app.detail_scroll, positions[0]);
}

#[test]
fn open_view_picker_lists_my_work_all_project_and_teammates() {
    let mut app = demo_app();
    app.open_view_picker();
    assert!(app.view_picker_open);
    assert_eq!(app.view_picker_options[0], ViewKind::MyWork);
    assert_eq!(app.view_picker_options[1], ViewKind::AllProject);
    // Teammates distinct from the demo "current user" should show up,
    // deduped and sorted; the demo current user itself must not appear as a
    // redundant pseudo-teammate (it's already covered by "My Work").
    let teammates: Vec<&ViewKind> = app.view_picker_options[2..].iter().collect();
    assert!(teammates.contains(&&ViewKind::Teammate("priya.nair".into())));
    assert!(teammates.contains(&&ViewKind::Teammate("alex.chen".into())));
    assert!(!teammates.contains(&&ViewKind::Teammate(
        crate::domain::DEMO_CURRENT_USER.into()
    )));
}

#[test]
fn view_picker_move_clamps_to_bounds() {
    let mut app = demo_app();
    app.open_view_picker();
    let len = app.view_picker_options.len();
    app.view_picker_move(-10);
    assert_eq!(app.view_picker_index, 0);
    app.view_picker_move(1000);
    assert_eq!(app.view_picker_index, len - 1);
}

#[test]
fn confirm_view_switch_to_teammate_filters_by_assignee() {
    let mut app = demo_app();
    app.open_view_picker();
    let idx = app
        .view_picker_options
        .iter()
        .position(|v| *v == ViewKind::Teammate("priya.nair".into()))
        .expect("priya.nair should be a demo teammate");
    app.view_picker_index = idx;
    app.confirm_view_switch();

    assert!(!app.view_picker_open);
    assert_eq!(app.current_view, ViewKind::Teammate("priya.nair".into()));
    assert!(!app.all_issues.is_empty());
    assert!(app
        .all_issues
        .iter()
        .all(|i| i.assignee.as_deref() == Some("priya.nair")));
}

#[test]
fn switch_view_to_all_project_then_back_to_my_work_round_trips() {
    let mut app = demo_app();
    let my_work_count = app.all_issues.len();

    app.switch_view(ViewKind::AllProject);
    assert_eq!(app.current_view, ViewKind::AllProject);
    assert!(!app.all_issues.is_empty());

    app.switch_view(ViewKind::MyWork);
    assert_eq!(app.current_view, ViewKind::MyWork);
    assert_eq!(app.all_issues.len(), my_work_count);
}

#[test]
fn refresh_preserves_the_current_view() {
    let mut app = demo_app();
    app.switch_view(ViewKind::Teammate("alex.chen".into()));
    app.refresh();
    assert_eq!(app.current_view, ViewKind::Teammate("alex.chen".into()));
    assert!(app
        .all_issues
        .iter()
        .all(|i| i.assignee.as_deref() == Some("alex.chen")));
}

/// Drain a completed fetch off `app.events_rx`, with a generous timeout —
/// these tests have no real network to wait on, only `spawn_blocking`
/// scheduling, so this should resolve almost immediately.
async fn next_event(app: &mut App) -> AppEvent {
    tokio::time::timeout(std::time::Duration::from_secs(5), app.events_rx.recv())
        .await
        .expect("fetch task did not complete in time")
        .expect("events_tx dropped unexpectedly")
}

/// A non-demo session (no credentials configured) still exercises the real
/// async dispatch path in `refresh`/`switch_view` — `load_issues_for` falls
/// back to demo data internally, but the point here is the
/// spawn/spawn_blocking/channel plumbing around it, not the fetched data.
/// Points `XDG_CONFIG_HOME` at an empty temp dir and clears the `JIRA_*` env
/// vars so `Config::load()` deterministically finds no credentials — this
/// machine's real `~/.config/jira-tui/config.toml` (if any) must not leak
/// in and trigger an actual network call. See `crate::test_support::lock_env_async`,
/// held for the caller's whole test (including across the `.await` that
/// drains the fetch) so a racing test can't change these back mid-flight.
fn non_demo_app() -> App {
    let base = std::env::temp_dir().join(format!(
        "jira-tui-asynctest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::env::set_var("XDG_CONFIG_HOME", &base);
    for var in [
        "JIRA_BASE_URL",
        "JIRA_EMAIL",
        "JIRA_API_TOKEN",
        "JIRA_TOKEN_FILE",
    ] {
        std::env::remove_var(var);
    }
    let mut app = demo_app();
    app.source = crate::domain::Source::Cache { user: "me".into() };
    app
}

#[tokio::test]
async fn refresh_against_a_non_demo_source_dispatches_and_clears_loading() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app();
    assert!(!app.loading);

    app.refresh();
    assert!(
        app.loading,
        "refresh should flip on the loading flag immediately"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(
        !app.loading,
        "applying the result should clear the loading flag"
    );
}

#[tokio::test]
async fn switch_view_against_a_non_demo_source_dispatches_and_updates_current_view() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app();

    app.switch_view(ViewKind::AllProject);
    assert!(app.loading);
    // current_view/selected only update once the fetch resolves and is applied.
    assert_eq!(app.current_view, ViewKind::MyWork);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.current_view, ViewKind::AllProject);
}

#[tokio::test]
async fn a_superseded_fetch_result_is_dropped_instead_of_clobbering_newer_state() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = non_demo_app();

    app.refresh();
    let stale_generation = app.generation;
    let stale_event = next_event(&mut app).await;

    // A second refresh starts before the first result is applied, bumping
    // the generation counter past the first request's.
    app.refresh();
    assert_ne!(app.generation, stale_generation);

    app.apply_event(stale_event);
    assert!(
        app.loading,
        "the stale result must not clear loading for the newer, still in-flight request"
    );

    let fresh_event = next_event(&mut app).await;
    app.apply_event(fresh_event);
    assert!(!app.loading);
}

/// Like `non_demo_app`, but with a genuine `Source::Live` session — the
/// condition every detail/transition/edit dispatch actually checks (unlike
/// `refresh`/`switch_view`, which also treat `Source::Cache` as needing a
/// live round-trip). Reuses the same `XDG_CONFIG_HOME`/`JIRA_*` isolation so
/// `Config::load()` deterministically finds no credentials and these tests
/// exercise the spawn/spawn_blocking/channel plumbing (falling back to demo
/// detail data) rather than a real network call.
fn live_app() -> App {
    let mut app = non_demo_app();
    app.source = crate::domain::Source::Live {
        site: "demo.atlassian.net".into(),
        user: "me".into(),
    };
    app
}

#[tokio::test]
async fn open_by_key_against_a_live_source_dispatches_and_navigates_once_loaded() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();

    app.open_by_key(&key);
    assert!(app.loading);
    assert_eq!(
        app.screen,
        Screen::Home,
        "must not navigate until the fetch resolves"
    );
    assert!(app.detail.is_none());

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
    assert!(app.detail_cache.contains_key(&key));
}

#[tokio::test]
async fn ensure_quick_view_loaded_against_a_live_source_does_not_duplicate_in_flight_fetches() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.quick_view = true;
    app.selected = 0;
    let key = app.issues[0].key.clone();

    app.ensure_quick_view_loaded();
    assert!(app.loading);
    let first_generation = app.detail_generation;

    // Called again before the first fetch resolves, exactly like the run
    // loop polling every tick — must not dispatch a second fetch for the
    // same key.
    app.ensure_quick_view_loaded();
    assert_eq!(app.detail_generation, first_generation);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(
        app.screen,
        Screen::Home,
        "quick-view load must not navigate"
    );
    assert!(app.detail_cache.contains_key(&key));
}

#[tokio::test]
async fn confirm_transition_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    let initial_status = app.detail.as_ref().unwrap().status.clone();
    app.screen = Screen::Detail;
    app.open_transitions();
    // "Done", per the demo transitions list.
    app.picker_index = 3;

    app.confirm_transition();
    assert!(app.loading);
    assert!(!app.picker_open);
    assert_eq!(
        app.detail.as_ref().unwrap().status,
        initial_status,
        "must not apply until the transition resolves"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.detail.as_ref().unwrap().status, "Done");
    assert_eq!(
        app.issues.iter().find(|i| i.key == key).unwrap().status,
        "Done"
    );
}

#[tokio::test]
async fn apply_description_edit_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.begin_description_edit_target();
    app.finish_edit("## Edited\n\nBrand new body.");
    assert_eq!(app.screen, Screen::Preview);

    app.apply_edit();
    assert!(app.loading);
    assert_eq!(
        app.screen,
        Screen::Preview,
        "must stay put until the update resolves"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.screen, Screen::Detail);
    let text = crate::adf::to_markdown(&app.detail.as_ref().unwrap().description);
    assert!(text.contains("Edited"));
    assert!(text.contains("Brand new body"));
}

#[tokio::test]
async fn apply_comment_against_a_live_source_dispatches_and_appends_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    let before = app.detail.as_ref().unwrap().comments.len();
    app.begin_comment();
    app.finish_edit("A brand new comment.");
    assert_eq!(app.screen, Screen::Preview);

    app.apply_edit();
    assert!(app.loading);
    assert_eq!(
        app.detail.as_ref().unwrap().comments.len(),
        before,
        "must not append until the post resolves"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().comments.len(), before + 1);
}

/// Regression test for a code-review finding on PR #20: a cache-only
/// quick-view load already in flight for a key must be "upgraded" to
/// navigate once an explicit open comes in for the same key, rather than
/// the open being silently swallowed by the in-flight dedup check.
#[tokio::test]
async fn an_explicit_open_escalates_an_in_flight_quick_view_load_to_navigate() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    app.quick_view = true;
    app.selected = 0;
    let key = app.issues[0].key.clone();

    // The quick-view panel's per-tick poll dispatches a cache-only load.
    app.ensure_quick_view_loaded();
    assert!(app.loading);
    let generation = app.detail_generation;

    // The user explicitly opens the same issue before that load resolves —
    // must not dispatch a second fetch, but must remember to navigate.
    app.open_by_key(&key);
    assert_eq!(
        app.detail_generation, generation,
        "must not dispatch a duplicate fetch for the same key"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(
        app.screen,
        Screen::Detail,
        "the escalated open must still navigate once the shared fetch resolves"
    );
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}

/// Regression test for a code-review finding on PR #20: without a
/// re-entrancy guard, cancelling out of a pending edit/transition and
/// immediately starting a new one bumps the shared generation counter,
/// silently discarding the first request's result (success or failure)
/// with no user-visible feedback. `open_transitions` now refuses to reopen
/// the picker while a transition is still in flight.
#[tokio::test]
async fn open_transitions_refuses_to_reopen_while_one_is_in_flight() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.open_transitions();
    app.picker_index = 3; // "Done"
    app.confirm_transition();
    assert!(app.loading);
    assert!(app.transition_pending);
    let generation = app.transition_generation;

    // Reopening the picker while the first transition is still resolving
    // must not be possible — that would let a second `confirm_transition`
    // bump the generation counter out from under the first request.
    app.open_transitions();
    assert!(
        !app.picker_open,
        "the picker must not reopen while a transition is in flight"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert!(!app.transition_pending);
    assert_eq!(app.detail.as_ref().unwrap().status, "Done");
    assert_eq!(app.transition_generation, generation);
}

/// Same regression as above, for the edit/comment side: starting a new
/// edit session while a previous description update or comment post is
/// still resolving must be refused, not silently allowed to clobber the
/// shared `edit_generation` counter.
#[tokio::test]
async fn begin_tui_edit_refuses_to_start_while_an_edit_is_in_flight() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    app.begin_description_edit_target();
    app.finish_edit("## First edit\n\nStill in flight.");
    app.apply_edit();
    assert!(app.loading);
    assert!(app.edit_pending);
    let generation = app.edit_generation;

    // Starting a second edit session while the first is still resolving
    // must be refused — it would otherwise take a fresh `pending_edit` and
    // dispatch a second write under a bumped `edit_generation`, discarding
    // whatever the first one eventually returns.
    app.begin_tui_edit();
    assert_eq!(
        app.screen,
        Screen::Preview,
        "must not open a new edit session while one is pending"
    );

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert!(!app.edit_pending);
    let text = crate::adf::to_markdown(&app.detail.as_ref().unwrap().description);
    assert!(text.contains("First edit"));
    assert_eq!(app.edit_generation, generation);
}
