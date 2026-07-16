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
    // it. `board_scroll` is now a lane index (SPEC.md §7's card-grid
    // rewrite), not a text-row offset, so this checks the lane-granularity
    // invariant directly instead of replicating render-time row math.
    let mut app = demo_app();
    app.open_board();
    // Wide layout (>=90 cols), short enough height that only ~1 lane fits
    // at a time, forcing the selection to scroll as it moves.
    app.board_area.set(Rect::new(0, 0, 120, 8));
    assert_eq!(app.board_scroll, 0);

    let lanes_len = app.board_lanes().len();
    assert!(
        lanes_len > 1,
        "test needs more than one lane to be meaningful"
    );

    // Step through every lane; the scroll offset must never leave the
    // selection above the top of the visible window.
    for _ in 0..lanes_len - 1 {
        app.board_move_lane(1);
        assert!(
            app.board_sel.lane as u16 >= app.board_scroll,
            "lane {} scrolled above the visible window (scroll={})",
            app.board_sel.lane,
            app.board_scroll
        );
    }
    assert!(
        app.board_scroll > 0,
        "stepping through every lane in a short viewport should have scrolled"
    );

    // And scrolling back up to the first lane must bring the offset back
    // down so the selection is visible again (not stuck scrolled down).
    for _ in 0..lanes_len - 1 {
        app.board_move_lane(-1);
    }
    assert_eq!(app.board_sel.lane, 0);
    assert_eq!(
        app.board_scroll, 0,
        "scrolling back to the first lane should reset the offset"
    );
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

fn issue(key: &str, epic: Option<&str>, status: &str) -> IssueSummary {
    IssueSummary {
        status: status.into(),
        epic: epic.map(String::from),
        ..crate::test_support::sample_issue(key)
    }
}

#[test]
fn board_wide_lanes_collapses_a_fully_done_lane_but_not_a_mixed_one() {
    // Demo data has no fully-done lane (SPEC.md §7's "fully-done lanes
    // collapse behind pgdn" needs a dedicated fixture to exercise at all).
    let mut app = demo_app();
    app.issues = vec![
        issue("DS-1", Some("EPIC-A"), "Done"),
        issue("DS-2", Some("EPIC-A"), "Done"),
        issue("DS-3", Some("EPIC-B"), "To Do"),
        issue("DS-4", Some("EPIC-B"), "Done"),
    ];
    // EPIC-B (first-seen via DS-3) is selected, so EPIC-A is the only one
    // eligible to collapse.
    app.board_sel.lane = app
        .board_lanes()
        .iter()
        .position(|l| l.as_deref() == Some("EPIC-B"))
        .unwrap();
    let (visible, hidden) = app.board_wide_lanes();
    assert!(
        !visible.iter().any(|l| l.as_deref() == Some("EPIC-A")),
        "the fully-done lane should collapse"
    );
    assert!(
        visible.iter().any(|l| l.as_deref() == Some("EPIC-B")),
        "the mixed lane should stay visible"
    );
    assert_eq!(hidden, 1);
}

/// Regression test: `board_columns` appends any non-preferred status
/// alphabetically *after* every preferred one (including "Done"), so
/// `cols.last()` is only "Done" when the workflow has no custom statuses —
/// a workflow with a custom terminal status (e.g. "Won't Do", "Zzz-custom")
/// would otherwise make "fully done" mean the wrong column entirely.
#[test]
fn board_wide_lanes_prefers_the_literal_done_column_over_the_positional_last_one() {
    let mut app = demo_app();
    app.issues = vec![
        issue("DS-1", Some("EPIC-A"), "Done"),
        issue("DS-2", Some("EPIC-A"), "Done"),
        issue("DS-3", Some("EPIC-B"), "Zzz-custom"),
    ];
    let cols = app.board_columns();
    assert_eq!(
        cols.last().map(String::as_str),
        Some("Zzz-custom"),
        "test fixture needs a custom status sorting after \"Done\""
    );
    app.board_sel.lane = app
        .board_lanes()
        .iter()
        .position(|l| l.as_deref() == Some("EPIC-B"))
        .unwrap();
    let (visible, hidden) = app.board_wide_lanes();
    assert!(
        !visible.iter().any(|l| l.as_deref() == Some("EPIC-A")),
        "EPIC-A is fully Done and should collapse, even though \"Done\" isn't the last column"
    );
    assert_eq!(hidden, 1);
}

#[test]
fn board_wide_lanes_never_collapses_the_selected_lane() {
    let mut app = demo_app();
    app.issues = vec![
        issue("DS-1", Some("EPIC-A"), "Done"),
        issue("DS-2", Some("EPIC-A"), "Done"),
        issue("DS-3", Some("EPIC-B"), "To Do"),
    ];
    app.board_sel.lane = app
        .board_lanes()
        .iter()
        .position(|l| l.as_deref() == Some("EPIC-A"))
        .unwrap();
    let (visible, hidden) = app.board_wide_lanes();
    assert!(
        visible.iter().any(|l| l.as_deref() == Some("EPIC-A")),
        "the selected lane must not collapse even though it's fully done"
    );
    assert_eq!(hidden, 0);
}

#[test]
fn board_narrow_lanes_collapses_a_lane_empty_in_the_current_column() {
    let mut app = demo_app();
    app.issues = vec![
        issue("DS-1", Some("EPIC-A"), "To Do"),
        issue("DS-2", Some("EPIC-B"), "In Progress"),
    ];
    app.board_sel.lane = app
        .board_lanes()
        .iter()
        .position(|l| l.as_deref() == Some("EPIC-A"))
        .unwrap();
    let (visible, hidden) = app.board_narrow_lanes("To Do");
    assert!(visible.iter().any(|l| l.as_deref() == Some("EPIC-A")));
    assert!(
        !visible.iter().any(|l| l.as_deref() == Some("EPIC-B")),
        "EPIC-B has nothing in To Do and isn't selected, so it should collapse"
    );
    assert_eq!(hidden, 1);
}

#[test]
fn board_lane_counts_reports_here_and_total() {
    let app_issues = vec![
        issue("DS-1", Some("EPIC-A"), "To Do"),
        issue("DS-2", Some("EPIC-A"), "Done"),
        issue("DS-3", Some("EPIC-A"), "Done"),
    ];
    let mut app = demo_app();
    app.issues = app_issues;
    let lane = Some("EPIC-A".to_string());
    assert_eq!(app.board_lane_counts(&lane, "Done"), (2, 3));
    assert_eq!(app.board_lane_counts(&lane, "To Do"), (1, 3));
}

#[test]
fn board_neighbour_counts_omits_missing_sides_at_the_edges() {
    let mut app = demo_app();
    app.issues = vec![
        issue("DS-1", Some("EPIC-A"), "To Do"),
        issue("DS-2", Some("EPIC-A"), "In Progress"),
        issue("DS-3", Some("EPIC-A"), "Done"),
    ];
    let lane = Some("EPIC-A".to_string());
    let cols = app.board_columns();
    assert!(cols.len() >= 3, "test needs at least 3 columns");

    app.board_sel.col = 0;
    let (prev, next) = app.board_neighbour_counts(&lane);
    assert!(prev.is_none(), "no previous column at the first column");
    assert!(next.is_some());

    app.board_sel.col = cols.len() - 1;
    let (prev, next) = app.board_neighbour_counts(&lane);
    assert!(prev.is_some());
    assert!(next.is_none(), "no next column at the last column");

    app.board_sel.col = 1;
    let (prev, next) = app.board_neighbour_counts(&lane);
    assert!(prev.is_some() && next.is_some());
}

#[test]
fn board_ensure_visible_keeps_selection_reachable_across_a_width_change() {
    let mut app = demo_app();
    app.open_board();
    app.board_area.set(Rect::new(0, 0, 120, 10));
    let lanes_len = app.board_lanes().len();
    for _ in 0..lanes_len - 1 {
        app.board_move_lane(1);
    }
    // Resize narrower (Wide -> Narrow) at the same short height, then
    // re-trigger `board_ensure_visible` (a delta-0 lane move is a
    // position-preserving no-op that still re-derives scroll for the
    // current area) — the selection must still be within the new layout's
    // visible window, not left stranded by stale Wide-layout scroll math.
    app.board_area.set(Rect::new(0, 0, 80, 10));
    app.board_move_lane(0);
    assert!(
        app.board_sel.lane as u16 >= app.board_scroll,
        "selection should still be reachable after a layout change"
    );
}
