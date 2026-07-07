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
    assert_eq!(app.pending_copy, Some((5, 8)));
    assert!(!app.selecting);
    assert_eq!(app.screen, Screen::Home, "a drag must not open detail");
}

#[test]
fn credential_form_edits_focused_field() {
    let mut app = demo_app();
    app.focus = Field::Email;
    app.input_char('a');
    app.input_char('b');
    assert_eq!(app.field_email, "ab");
    app.input_backspace();
    assert_eq!(app.field_email, "a");
    app.focus_next();
    assert_eq!(app.focus, Field::Token);
    app.focus_prev();
    assert_eq!(app.focus, Field::Email);
}

#[test]
fn submit_with_empty_fields_reports_and_does_not_panic() {
    let mut app = demo_app();
    app.field_site.clear();
    app.field_email.clear();
    app.field_token.clear();
    app.submit_credentials();
    assert!(!app.setup_msg.is_empty());
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
    assert!(app.search_rows.iter().any(|r| matches!(
        r,
        SearchRow::Match(idx) if app.all_issues[*idx].summary.to_lowercase().contains("accordion")
    )));
}

#[test]
fn search_key_candidate_detects_issue_keys_only() {
    let mut app = demo_app();
    app.search_query = "DS-2603".to_string();
    assert_eq!(app.search_key_candidate(), Some("DS-2603".to_string()));
    app.search_query = "ds-2603".to_string();
    assert_eq!(app.search_key_candidate(), Some("DS-2603".to_string()));
    app.search_query = "accordion".to_string();
    assert_eq!(app.search_key_candidate(), None);
    app.search_query = "DS-".to_string();
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
    assert!(matches!(app.search_rows.first(), Some(SearchRow::Goto(k)) if k == "DS-2603"));
    app.search_selected = 0;
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
        .search_rows
        .iter()
        .position(|r| matches!(r, SearchRow::Match(idx) if app.all_issues[*idx].key == target_key))
        .unwrap();
    app.search_selected = pos;
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
