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

    app.detail_main_area.set(Rect::new(0, 1, 80, 20));
    app.detail_scroll = 0;
    let x = target.start as u16;
    let y = 1 + target.line as u16;

    app.mouse_down(y);
    app.mouse_up(x, y);
    assert_eq!(app.detail.as_ref().unwrap().key, key);
}
