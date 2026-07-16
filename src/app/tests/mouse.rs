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
    app.mouse_down(5);
    app.mouse_up(0, 5);
    assert_eq!(app.screen, Screen::Detail);
    assert!(app.detail.is_some());
    assert_eq!(app.selected, 0);
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
