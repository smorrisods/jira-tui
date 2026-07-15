//! Swimlane board tests.

use super::super::*;
use super::support::*;

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
