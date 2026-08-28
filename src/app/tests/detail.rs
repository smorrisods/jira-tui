//! Detail-loading tests.

use super::super::*;
use super::support::*;

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

#[test]
fn refresh_detail_reloads_the_open_issue_without_touching_history() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.follow_link("DS-9001"); // build up some link-navigation history
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

/// `Tab` on the Detail screen should do nothing in the narrow layout — no
/// rail exists there to focus.
#[test]
fn cycle_rail_focus_is_a_no_op_in_the_narrow_layout() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.detail_area.set(Rect::new(0, 0, 80, 40)); // < 90 cols: narrow

    app.cycle_rail_focus(true);
    assert_eq!(app.rail_focus, None);
}

/// With no panel overflowing (`rail_overflow` all-false, its default before
/// any render has run), `Tab` should leave focus on the main column rather
/// than get stuck cycling a panel with nothing to scroll.
#[test]
fn cycle_rail_focus_skips_panels_that_do_not_overflow() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.detail_area.set(Rect::new(0, 0, 120, 40)); // wide

    app.cycle_rail_focus(true);
    assert_eq!(
        app.rail_focus, None,
        "nothing overflows, so Tab should have nothing to focus"
    );
}

/// The core of the bug fix: `Tab` cycles forward through only the
/// overflowing panels, wrapping back to `None` (main column) rather than
/// straight to the first panel again — and `Shift+Tab` reverses it.
#[test]
fn cycle_rail_focus_cycles_only_overflowing_panels_and_wraps_through_none() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.detail_area.set(Rect::new(0, 0, 120, 40)); // wide
    let mut overflow = [false; 5];
    overflow[RailPanel::Meta.index()] = true;
    overflow[RailPanel::Children.index()] = true;
    app.rail_overflow.set(overflow);

    app.cycle_rail_focus(true);
    assert_eq!(app.rail_focus, Some(RailPanel::Meta));
    app.cycle_rail_focus(true);
    assert_eq!(app.rail_focus, Some(RailPanel::Children));
    app.cycle_rail_focus(true);
    assert_eq!(
        app.rail_focus, None,
        "Tab past the last overflowing panel returns focus to the main column"
    );
    app.cycle_rail_focus(true);
    assert_eq!(
        app.rail_focus,
        Some(RailPanel::Meta),
        "and wraps back around"
    );

    app.cycle_rail_focus(false);
    assert_eq!(
        app.rail_focus, None,
        "Shift+Tab from the first overflowing panel goes back to the main column"
    );
    app.cycle_rail_focus(false);
    assert_eq!(
        app.rail_focus,
        Some(RailPanel::Children),
        "Shift+Tab from the main column wraps to the last overflowing panel"
    );
}

/// Scrolling only ever touches the currently focused panel's own slot in
/// `rail_scroll`, and clamps at 0 the same way `detail_scroll` does. Uses
/// the workflow panel specifically — it never links to another issue, so
/// this exercises the plain-row-scroll fallback in `scroll_rail_panel_by`
/// regardless of layout width; link-stepping is covered separately below.
#[test]
fn scroll_rail_by_only_moves_the_focused_panel_and_clamps_at_zero() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.rail_focus = Some(RailPanel::Workflow);

    app.scroll_rail_by(1);
    app.scroll_rail_by(1);
    assert_eq!(app.rail_scroll[RailPanel::Workflow.index()], 2);
    assert_eq!(
        app.rail_scroll[RailPanel::Meta.index()],
        0,
        "an unfocused panel's scroll must be untouched"
    );

    app.scroll_rail_by(-8);
    assert_eq!(app.rail_scroll[RailPanel::Workflow.index()], 0);

    app.rail_focus = None;
    app.scroll_rail_by(1);
    assert_eq!(
        app.rail_scroll[RailPanel::Workflow.index()],
        0,
        "no panel focused: arrow keys shouldn't scroll anything on the rail"
    );
}

/// The bug-report follow-up: while a panel that actually links to other
/// issues (Children here) has `Tab` focus, arrows step `link_index` between
/// that panel's own links one at a time — wrapping within just that pane,
/// not the full cross-pane `{`/`}` list — and scroll it into view, rather
/// than just scrolling raw text.
#[test]
fn scroll_rail_by_steps_between_a_linked_panels_own_links() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_by_key("DS-2722");
    let mut detail = app.detail.clone().unwrap();
    detail.children = vec![
        crate::domain::ChildIssue {
            key: "DS-9000".into(),
            issue_type: "Sub-task".into(),
            summary: "first".into(),
            status: "To Do".into(),
        },
        crate::domain::ChildIssue {
            key: "DS-9001".into(),
            issue_type: "Sub-task".into(),
            summary: "second".into(),
            status: "To Do".into(),
        },
        crate::domain::ChildIssue {
            key: "DS-9002".into(),
            issue_type: "Sub-task".into(),
            summary: "third".into(),
            status: "To Do".into(),
        },
    ];
    app.detail = Some(detail);
    app.detail_area.set(Rect::new(0, 0, 120, 40)); // wide: Children is its own pane
    app.rail_visible_rows.set([20; 5]); // plenty of room — isolates stepping from clamping
    app.rail_focus = Some(RailPanel::Children);

    app.scroll_rail_by(1);
    let key_at = |app: &App| match &app.active_links()[app.link_index].kind {
        crate::render::LinkKind::Issue(k) => k.clone(),
        crate::render::LinkKind::Url(_) => panic!("children links are always issue keys"),
    };
    assert_eq!(
        key_at(&app),
        "DS-9000",
        "first step lands on the first child"
    );
    assert_eq!(
        app.active_links()[app.link_index].pane,
        crate::render::DetailPane::Children
    );

    app.scroll_rail_by(1);
    assert_eq!(key_at(&app), "DS-9001");
    app.scroll_rail_by(1);
    assert_eq!(key_at(&app), "DS-9002");
    app.scroll_rail_by(1);
    assert_eq!(
        key_at(&app),
        "DS-9000",
        "steps wrap within the panel's own links"
    );

    app.scroll_rail_by(-1);
    assert_eq!(key_at(&app), "DS-9002", "and reverse (Shift+Tab's arrows)");
}

/// Stepping to a link that's off-screen must scroll the panel to bring it
/// into view — otherwise `Enter` would open an issue the user can't
/// actually see highlighted.
#[test]
fn scroll_rail_by_auto_scrolls_the_panel_to_keep_the_stepped_link_in_view() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_by_key("DS-2722");
    let mut detail = app.detail.clone().unwrap();
    detail.children = (0..3)
        .map(|i| crate::domain::ChildIssue {
            key: format!("DS-900{i}"),
            issue_type: "Sub-task".into(),
            summary: format!("child {i}"),
            status: "To Do".into(),
        })
        .collect();
    app.detail = Some(detail);
    app.detail_area.set(Rect::new(0, 0, 120, 40));
    // Deliberately tiny — smaller than a single wrapped child entry — so
    // every step past the first is guaranteed to need a scroll.
    app.rail_visible_rows.set([2; 5]);
    app.rail_focus = Some(RailPanel::Children);

    app.scroll_rail_by(1);
    assert_eq!(
        app.rail_scroll[RailPanel::Children.index()],
        0,
        "the first child is at the top — no scroll needed yet"
    );

    app.scroll_rail_by(1);
    let after_second = app.rail_scroll[RailPanel::Children.index()];
    assert!(
        after_second > 0,
        "stepping to the second child should scroll it into view"
    );

    app.scroll_rail_by(1);
    assert!(
        app.rail_scroll[RailPanel::Children.index()] > after_second,
        "stepping further should scroll further"
    );
}

/// Navigating to a fresh issue must reset both rail focus and every panel's
/// scroll offset — otherwise, e.g., a scrolled-down children panel on one
/// issue would carry over and look broken on the next issue opened, which
/// might not even have an overflowing children panel.
#[test]
fn opening_a_new_issue_resets_rail_focus_and_scroll() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.rail_focus = Some(RailPanel::Children);
    app.rail_scroll[RailPanel::Children.index()] = 5;

    let other_key = app.issues[1].key.clone();
    app.open_by_key(&other_key);

    assert_eq!(app.rail_focus, None);
    assert_eq!(app.rail_scroll, [0; 5]);
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
