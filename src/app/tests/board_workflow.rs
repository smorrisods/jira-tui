//! Tests for issue-mutation flows: the swimlane board, comments,
//! transitions, assignment, and description/comment editing.

use super::super::*;
use super::support::*;

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

#[test]
fn assignee_picker_pins_me_first_then_lists_others_alphabetically() {
    let mut app = demo_app();
    app.open_detail();
    app.open_assignee_picker();
    assert!(app.assignee_picker_open);
    let labels: Vec<String> = app
        .assignee_picker
        .rows
        .iter()
        .map(|r| match r {
            AssigneeRow::Unassign => "Unassign".to_string(),
            AssigneeRow::User(u) => u.display_name.clone(),
        })
        .collect();
    assert_eq!(labels[0], "Unassign");
    assert_eq!(labels[1], crate::domain::DEMO_CURRENT_USER);
    // Everyone after "me" is alphabetical.
    let rest = &labels[2..];
    let mut sorted = rest.to_vec();
    sorted.sort();
    assert_eq!(rest, sorted.as_slice());
}

#[test]
fn assignee_picker_filters_by_typed_query() {
    let mut app = demo_app();
    app.open_detail();
    app.open_assignee_picker();
    app.assignee_picker_input_char('p');
    app.assignee_picker_input_char('r');
    app.assignee_picker_input_char('i');
    let labels: Vec<String> = app
        .assignee_picker
        .rows
        .iter()
        .map(|r| match r {
            AssigneeRow::Unassign => "Unassign".to_string(),
            AssigneeRow::User(u) => u.display_name.clone(),
        })
        .collect();
    assert!(labels.iter().any(|l| l.contains("priya")));
    assert!(!labels.contains(&"Unassign".to_string()));
    assert!(!labels.contains(&crate::domain::DEMO_CURRENT_USER.to_string()));

    app.assignee_picker_backspace();
    app.assignee_picker_backspace();
    app.assignee_picker_backspace();
    assert!(app
        .assignee_picker
        .rows
        .iter()
        .any(|r| matches!(r, AssigneeRow::Unassign)));
}

#[test]
fn confirm_assignee_updates_assignee_locally_in_demo_mode() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let key = app.detail.as_ref().unwrap().key.clone();
    app.open_assignee_picker();
    // Row 1 is "me" (row 0 is Unassign).
    app.assignee_picker.selected = 1;
    app.confirm_assignee();

    assert!(!app.assignee_picker_open);
    assert_eq!(
        app.detail.as_ref().unwrap().assignee.as_deref(),
        Some(crate::domain::DEMO_CURRENT_USER)
    );
    assert_eq!(
        app.issues
            .iter()
            .find(|i| i.key == key)
            .unwrap()
            .assignee
            .as_deref(),
        Some(crate::domain::DEMO_CURRENT_USER)
    );
}

#[test]
fn confirm_assignee_unassign_clears_assignee_locally() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_assignee_picker();
    app.assignee_picker.selected = 0; // Unassign
    app.confirm_assignee();

    assert!(app.detail.as_ref().unwrap().assignee.is_none());
}

#[test]
fn assignee_picker_move_clamps_to_bounds() {
    let mut app = demo_app();
    app.open_detail();
    app.open_assignee_picker();
    let len = app.assignee_picker.rows.len();
    app.assignee_picker_move(-5);
    assert_eq!(app.assignee_picker.selected, 0);
    app.assignee_picker_move(1000);
    assert_eq!(app.assignee_picker.selected, len - 1);
}

#[tokio::test]
async fn confirm_assignee_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.screen = Screen::Detail;
    // Simulate the startup teammate-discovery fetch having already
    // populated the assignable-users cache (see
    // `async_ops::dispatch_teammate_discovery`) — `live_app()`'s user is
    // "me", so that's the display name to pin first in the picker.
    app.assignable_users = vec![
        crate::domain::AssignableUser {
            account_id: "acc-me".into(),
            display_name: "me".into(),
        },
        crate::domain::AssignableUser {
            account_id: "acc-other".into(),
            display_name: "Other Person".into(),
        },
    ];
    app.open_assignee_picker();
    app.assignee_picker.selected = 1; // "me", pinned right after Unassign

    app.confirm_assignee();
    assert!(app.loading);
    assert!(!app.assignee_picker_open);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.detail.as_ref().unwrap().assignee.as_deref(), Some("me"));
    assert_eq!(
        app.issues
            .iter()
            .find(|i| i.key == key)
            .unwrap()
            .assignee
            .as_deref(),
        Some("me")
    );
}
