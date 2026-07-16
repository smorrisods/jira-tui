//! In-body-link (issue key / URL) navigation tests.

use super::super::*;
use super::support::*;

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

/// Simulates a mouse click on `target`, as if `app.detail_main_area` were
/// the Rect it was rendered into starting at row 1 — shared by every
/// click-to-open-a-link test below.
fn click_link(app: &mut App, target: &crate::render::LinkTarget) {
    app.detail_main_area.set(Rect::new(0, 1, 80, 20));
    app.detail_scroll = 0;
    let x = target.start as u16;
    let y = 1 + target.line as u16;
    app.mouse_down(y);
    app.mouse_up(x, y);
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
    let target = links[idx].clone();

    click_link(&mut app, &target);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}

/// Regression coverage for the Wide Detail layout's main-column mouse-click
/// path (SPEC.md §6): the test above only ever exercises the Narrow layout,
/// since it never sets `app.detail_area` to a Wide (>=90 col) width. Demo
/// data's canned description/comments don't mention any issue key, so the
/// description is overridden here with one that does, giving the Main pane
/// something to click on.
#[test]
fn click_on_a_detail_link_opens_it_in_the_wide_layout() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();

    let mut detail = app.detail.clone().unwrap();
    detail.description = serde_json::json!({
        "type": "doc",
        "version": 1,
        "content": [{
            "type": "paragraph",
            "content": [{ "type": "text", "text": "See DS-2603 for details." }]
        }]
    });
    app.detail = Some(detail);

    app.detail_area.set(Rect::new(0, 0, 120, 40));
    let links = app.active_links();
    let (idx, key) = links
        .iter()
        .enumerate()
        .find_map(|(i, t)| match (&t.kind, t.pane) {
            (crate::render::LinkKind::Issue(k), crate::render::DetailPane::Main) => {
                Some((i, k.clone()))
            }
            _ => None,
        })
        .expect("the overridden description should link to DS-2603 in the Main pane");
    let target = links[idx].clone();

    click_link(&mut app, &target);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}
