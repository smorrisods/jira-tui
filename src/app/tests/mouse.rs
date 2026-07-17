//! Mouse / list-focus tests.

use super::super::*;
use super::support::*;
use ratatui::layout::Rect;

#[test]
fn list_index_at_maps_rows_to_issues() {
    let app = demo_app();
    app.list_area.set(Rect::new(0, 4, 80, 8));
    app.list_start.set(0);
    // The area's first row is the column header line, not a data row.
    assert_eq!(app.list_index_at(4), None);
    assert_eq!(app.list_index_at(5), Some(0));
    assert_eq!(app.list_index_at(7), Some(2));
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
    app.mouse_down(0, 5);
    app.mouse_up(0, 5);
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.detail.is_some());
    assert_eq!(app.selected, 0);
}

#[test]
fn drag_sets_a_pending_copy_range() {
    let mut app = demo_app();
    app.list_area.set(Rect::new(0, 4, 80, 8));
    app.mouse_down(0, 5);
    assert_eq!(app.selection_range(), Some(((5, 0), (5, 0))));
    app.mouse_drag(0, 8);
    assert_eq!(app.selection_range(), Some(((5, 0), (8, 0))));
    app.mouse_up(0, 8);
    assert_eq!(app.mouse.pending_copy, Some(((5, 0), (8, 0))));
    assert!(!app.mouse.selecting);
    assert_eq!(app.screen, Screen::Home, "a drag must not open detail");
}

/// Regression coverage for character-precise selection: dragging up-and-
/// left must still normalize to (earlier point, later point) in reading
/// order, comparing `(y, x)` as a single tuple rather than each axis
/// independently (which would otherwise produce a nonsensical "bounding
/// box" pairing for a diagonal drag).
#[test]
fn selection_range_normalizes_a_backward_drag_into_reading_order() {
    let mut app = demo_app();
    app.mouse_down(20, 8);
    app.mouse_drag(5, 3);
    assert_eq!(app.selection_range(), Some(((3, 5), (8, 20))));
}

#[test]
fn a_single_point_click_is_not_a_selection_even_with_pixel_perfect_repeats() {
    let mut app = demo_app();
    app.list_area.set(Rect::new(0, 4, 80, 8));
    app.mouse_down(10, 5);
    app.mouse_drag(10, 5);
    app.mouse_up(10, 5);
    assert_eq!(
        app.mouse.pending_copy, None,
        "an exact-same-point down/drag/up is a click, not a drag-select"
    );
}

#[test]
fn toggle_quick_view_opens_and_closes_resetting_focus() {
    let mut app = demo_app();
    assert!(!app.quick_view);

    app.toggle_quick_view();
    assert!(app.quick_view);

    // Give keyboard focus to the quick-view panel, then close it — focus
    // must be forced back to the list, matching `toggle_list_focus`'s own
    // rule, so arrow keys never end up stuck scrolling a hidden panel.
    app.list_focus = ListFocus::QuickView;
    app.toggle_quick_view();
    assert!(!app.quick_view);
    assert_eq!(app.list_focus, ListFocus::List);
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
fn point_in_jax_mini_respects_jax_popped_and_hidden_screens() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.jax_mini_area.set(Rect::new(60, 10, 14, 1));

    // Not popped, visible screen: the recorded area is live.
    assert!(app.point_in_jax_mini(62, 10));
    assert!(!app.point_in_jax_mini(0, 0));

    // Popped out (full box showing instead): a stale mini area must not
    // still resolve.
    app.jax_popped = true;
    assert!(!app.point_in_jax_mini(62, 10));
    app.jax_popped = false;

    // A screen where Jax is hidden entirely: same stale-area guard.
    for screen in [Screen::Welcome, Screen::Edit, Screen::About] {
        app.screen = screen;
        assert!(!app.point_in_jax_mini(62, 10));
    }
}

#[test]
fn clicking_the_jax_mini_dock_toggles_jax_popped() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.jax_mini_area.set(Rect::new(60, 10, 14, 1));
    assert!(!app.jax_popped);

    app.mouse_down(62, 10);
    app.mouse_up(62, 10);
    assert!(
        app.jax_popped,
        "clicking the mini dock should pop the full box out"
    );

    // Clicking again while popped shouldn't hit the (now stale) mini area —
    // `J`/a real click target would be needed to toggle it back off, but a
    // click at the same coordinates must not misfire against a leftover
    // mini `Rect` while the full box is what's actually showing.
    app.mouse_down(62, 10);
    app.mouse_up(62, 10);
    assert!(
        app.jax_popped,
        "a click at the mini dock's old coordinates must not affect the popped-out full box"
    );
}

#[test]
fn link_at_ignores_the_wide_quick_views_meta_column() {
    // Regression test: quick view's wide layout (SPEC.md §4) puts the
    // description in a left column and a compact meta grid in a right
    // column, each independently linkified — but `link_at` used to compute
    // (line, col) against the whole panel with no column split at all, so a
    // click landing in the meta column could coincidentally resolve to a
    // description-pane link at that same raw column offset.
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = true;
    app.selected = 0;
    app.ensure_quick_view_loaded();

    // Demo descriptions don't naturally mention another issue key; swap in
    // one that does, so the description (main) pane has a real link to
    // click, distinct from the meta grid's own "parent" link (if any).
    let key = app.issues[0].key.clone();
    let mut detail = app.quick_view_detail().unwrap().clone();
    detail.description = serde_json::json!({
        "type": "doc", "version": 1,
        "content": [{"type": "paragraph", "content": [
            {"type": "text", "text": "See DS-9999 for context."}
        ]}]
    });
    app.detail_cache.insert(key, detail);

    // Comfortably above the wide-layout threshold.
    let area = Rect::new(0, 0, 120, 20);
    app.quick_view_area.set(area);

    let links = app.active_links();
    let first = links
        .iter()
        .find(|t| t.pane == crate::render::DetailPane::Main)
        .expect("the synthetic description should have a navigable link");
    let x = area.x + first.start as u16;
    let y = area.y + first.line as u16;
    assert_eq!(
        app.link_at(x, y),
        Some(0),
        "a click on the description column's own link should resolve"
    );

    // Same row, but far enough right to land in the meta column instead —
    // must not resolve to anything, regardless of what raw column math
    // would otherwise say.
    let meta_x = area.x + area.width - 2;
    assert_eq!(
        app.link_at(meta_x, y),
        None,
        "a click in the meta column must not resolve to a description-pane link"
    );
}
