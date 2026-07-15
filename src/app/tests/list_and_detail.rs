//! Tests for the Home/List surface: mouse/list-focus, sort/filter,
//! quick-view, search, detail loading, in-body links, and the small
//! cross-cutting `App` query helpers (window title, selected-issue URL,
//! move_selection).

use super::super::*;
use super::support::*;
use ratatui::layout::Rect;

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
    app.mouse_up(0, 5);
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
    app.mouse_up(0, 8);
    assert_eq!(app.mouse.pending_copy, Some((5, 8)));
    assert!(!app.mouse.selecting);
    assert_eq!(app.screen, Screen::Home, "a drag must not open detail");
}

#[test]
fn selected_issue_url_is_a_browse_link() {
    let app = demo_app();
    let url = app.selected_issue_url().unwrap();
    assert!(url.contains("/browse/DS-"));
}

// Coverage gap noticed while splitting app/mod.rs into loader.rs/query.rs:
// `assigned_to_me`/`blocked`/`active_flash` were only ever exercised
// indirectly (via `ui/home.rs`'s tile counts and `ui/mod.rs`'s footer toast
// render), never with a direct test of their own return value. Each builds
// a small controlled `all_issues` fixture rather than depending on the
// demo dataset's exact composition, so these stay correct if the demo data
// changes later.

fn issue(key: &str, assignee: Option<&str>, status: &str, blocked: bool) -> IssueSummary {
    IssueSummary {
        status: status.into(),
        assignee: assignee.map(String::from),
        blocked,
        ..crate::test_support::sample_issue(key)
    }
}

#[test]
fn assigned_to_me_excludes_unassigned_and_done_issues() {
    let mut app = demo_app();
    app.all_issues = vec![
        issue("DS-1", Some("scott.morris"), "In Progress", false),
        issue("DS-2", None, "To Do", false),
        issue("DS-3", Some("scott.morris"), "Done", false),
    ];
    let assigned = app.assigned_to_me();
    assert_eq!(
        assigned.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
        vec!["DS-1"]
    );
}

#[test]
fn blocked_returns_only_blocked_issues() {
    let mut app = demo_app();
    app.all_issues = vec![
        issue("DS-1", None, "To Do", true),
        issue("DS-2", None, "To Do", false),
    ];
    let blocked = app.blocked();
    assert_eq!(
        blocked.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
        vec!["DS-1"]
    );
}

#[test]
fn active_flash_shows_the_message_until_it_expires() {
    let mut app = demo_app();
    assert_eq!(
        app.active_flash(),
        None,
        "no toast before flash() is called"
    );

    app.flash("✓ done");
    assert_eq!(app.active_flash(), Some("✓ done"));

    app.tick = app.flash_until;
    assert_eq!(
        app.active_flash(),
        None,
        "the toast must not still show once tick reaches flash_until"
    );
}

#[test]
fn window_title_is_plain_outside_issue_screens() {
    let app = demo_app();
    assert_eq!(app.window_title(), "jira-tui");
}

#[test]
fn window_title_reflects_open_detail() {
    let mut app = demo_app();
    app.open_detail();
    let key = app.detail.as_ref().unwrap().key.clone();
    let summary = app.detail.as_ref().unwrap().summary.clone();
    assert_eq!(app.window_title(), format!("{key}: {summary} — jira-tui"));
}

#[test]
fn window_title_reflects_quick_view_selection() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = true;
    let issue = app.selected_issue().unwrap().clone();
    assert_eq!(
        app.window_title(),
        format!("{}: {} — jira-tui", issue.key, issue.summary)
    );
}

#[test]
fn window_title_ignores_quick_view_when_closed() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = false;
    assert_eq!(app.window_title(), "jira-tui");
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

#[test]
fn next_and_prev_link_cycle_and_wrap() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let links = app.active_links();
    assert!(
        links.len() >= 2,
        "demo detail should have at least a parent and a linked issue"
    );

    app.link_index = 0;
    app.next_link();
    assert_eq!(app.link_index, 1);
    // A full lap (one more step per remaining link) returns to the start.
    for _ in 0..(links.len() - 1) {
        app.next_link();
    }
    assert_eq!(app.link_index, 0);

    // Stepping back from the start wraps to the last link.
    app.prev_link();
    assert_eq!(app.link_index, links.len() - 1);
    app.prev_link();
    assert_eq!(app.link_index, links.len() - 2);
}

#[test]
fn open_highlighted_link_jumps_to_the_issue_key_target() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let links = app.active_links();
    let (idx, key) = links
        .iter()
        .enumerate()
        .find_map(|(i, t)| match &t.kind {
            crate::render::LinkKind::Issue(k) => Some((i, k.clone())),
            _ => None,
        })
        .expect("demo detail should link to another issue");

    app.link_index = idx;
    app.open_highlighted_link();
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}

#[test]
fn has_links_is_false_with_no_detail_loaded() {
    let app = demo_app();
    assert!(!app.has_links());
}

#[test]
fn click_on_a_detail_link_opens_it() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let links = app.active_links();
    let (idx, key) = links
        .iter()
        .enumerate()
        .find_map(|(i, t)| match &t.kind {
            crate::render::LinkKind::Issue(k) => Some((i, k.clone())),
            _ => None,
        })
        .expect("demo detail should link to another issue");
    let target = &links[idx];

    app.detail_area.set(Rect::new(0, 1, 80, 20));
    app.detail_scroll = 0;
    let x = target.start as u16;
    let y = 1 + target.line as u16;

    app.mouse_down(y);
    app.mouse_up(x, y);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}

#[test]
fn refresh_detail_reloads_the_open_issue_without_touching_history() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_by_key("DS-9001"); // build up some link-navigation history
    assert!(app.can_go_back());
    assert!(!app.can_go_forward());

    app.detail_scroll = 7;
    app.refresh_detail();

    // Same issue is still shown, and the back/forward stacks — which a
    // real navigation would touch — are untouched by a refresh.
    assert_eq!(app.detail.as_ref().unwrap().key, "DS-9001");
    assert!(app.can_go_back());
    assert!(!app.can_go_forward());
    assert!(app.detail_cache.contains_key("DS-9001"));
}

#[test]
fn refresh_detail_does_nothing_with_no_issue_open() {
    let mut app = demo_app();
    app.refresh_detail();
    assert!(app.detail.is_none());
}

#[test]
fn refresh_detail_refreshes_the_focused_quick_view_issue_from_the_list() {
    let mut app = demo_app();
    app.quick_view = true;
    app.list_focus = ListFocus::QuickView;
    app.selected = 0;
    let key = app.issues[0].key.clone();
    app.ensure_quick_view_loaded();
    assert!(app.detail_cache.contains_key(&key));

    app.refresh_detail();
    // Detail screen was never entered, but the quick-view cache entry for
    // the selected issue is refreshed in place.
    assert_eq!(app.screen, Screen::Home);
    assert!(app.detail_cache.contains_key(&key));
}

#[tokio::test]
async fn refresh_detail_against_a_live_source_updates_the_open_issue_once_loaded() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.open_by_key(&key);
    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert_eq!(app.detail.as_ref().unwrap().key, key);

    app.detail_scroll = 5;
    app.refresh_detail();
    assert!(app.loading);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.screen, Screen::Detail);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}
