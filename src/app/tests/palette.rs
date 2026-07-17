//! Command palette tests (SPEC.md §8).

use super::super::*;
use super::support::*;

fn has_action(app: &App, matches: impl Fn(&PaletteAction) -> bool) -> bool {
    app.palette.all_rows.iter().any(|r| matches(&r.action))
}

#[test]
fn palette_context_resolves_from_detail() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let key = app.detail.as_ref().unwrap().key.clone();
    let (ctx_key, ctx_detail) = app.palette_context();
    assert_eq!(ctx_key, Some(key));
    assert!(
        ctx_detail.is_some(),
        "Detail should resolve full IssueDetail"
    );
}

#[test]
fn palette_context_resolves_from_quick_view_once_loaded() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.selected = 0;
    app.quick_view = true;
    app.ensure_quick_view_loaded();
    let key = app.selected_issue().unwrap().key.clone();
    let (ctx_key, ctx_detail) = app.palette_context();
    assert_eq!(ctx_key, Some(key));
    assert!(
        ctx_detail.is_some(),
        "a loaded quick view should resolve full IssueDetail"
    );
}

#[test]
fn palette_context_resolves_a_bare_key_from_a_plain_list_selection() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.selected = 0;
    let key = app.selected_issue().unwrap().key.clone();
    let (ctx_key, ctx_detail) = app.palette_context();
    assert_eq!(
        ctx_key,
        Some(key),
        "a bare List selection should still resolve a key"
    );
    assert!(
        ctx_detail.is_none(),
        "no IssueDetail has been fetched, so detail-dependent rows shouldn't be offered"
    );
}

#[test]
fn palette_context_resolves_from_the_board_selected_card() {
    let mut app = demo_app();
    app.open_board();
    let expected = app.board_selected_issue().map(|i| i.key.clone());
    assert!(expected.is_some(), "demo board should have a selected card");
    let (ctx_key, ctx_detail) = app.palette_context();
    assert_eq!(ctx_key, expected);
    assert!(
        ctx_detail.is_none(),
        "Board never has a fetched IssueDetail for its selected card"
    );
}

#[test]
fn build_palette_rows_includes_assign_comment_transitions_only_with_detail() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_palette();
    assert!(has_action(&app, |a| matches!(a, PaletteAction::Assign)));
    assert!(has_action(&app, |a| matches!(a, PaletteAction::Comment)));
    assert!(has_action(&app, |a| matches!(
        a,
        PaletteAction::Transition(_)
    )));

    let mut bare = demo_app();
    bare.screen = Screen::List;
    bare.selected = 0;
    bare.open_palette();
    assert!(
        !has_action(&bare, |a| matches!(a, PaletteAction::Assign)),
        "assign shouldn't be offered without a fetched IssueDetail"
    );
    assert!(
        !has_action(&bare, |a| matches!(a, PaletteAction::Comment)),
        "comment shouldn't be offered without a fetched IssueDetail"
    );
    assert!(
        !has_action(&bare, |a| matches!(a, PaletteAction::Transition(_))),
        "transitions shouldn't be offered without a fetched IssueDetail"
    );
    // Copy/open-in-browser only need a resolved key, not a fetched detail.
    assert!(has_action(&bare, |a| matches!(a, PaletteAction::CopyKey)));
    assert!(has_action(&bare, |a| matches!(a, PaletteAction::CopyUrl)));
    assert!(has_action(&bare, |a| matches!(
        a,
        PaletteAction::OpenInBrowser
    )));
}

#[test]
fn build_palette_rows_view_and_app_groups_are_always_present() {
    let mut app = demo_app();
    app.screen = Screen::Board; // no on-key context beyond a bare card
    app.open_palette();
    for action in [
        PaletteAction::FlipView,
        PaletteAction::CycleSort,
        PaletteAction::CycleFilter,
        PaletteAction::ToggleTree,
        PaletteAction::ToggleQuickView,
        PaletteAction::OpenBoard,
        PaletteAction::Refresh,
        PaletteAction::ToggleMouse,
        PaletteAction::ToggleJax,
        PaletteAction::OpenFieldMapping,
        PaletteAction::OpenAbout,
        PaletteAction::OpenHelp,
    ] {
        assert!(
            app.palette.all_rows.iter().any(|r| r.action == action),
            "{action:?} should always be offered regardless of context"
        );
    }
}

#[test]
fn open_palette_filters_as_the_query_is_typed_and_resets_selection() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_palette();
    let all = app.palette.visible.len();
    assert!(all > 0);

    app.palette_input_char('a');
    app.palette_input_char('b');
    app.palette_input_char('o');
    app.palette_input_char('u');
    app.palette_input_char('t');
    assert!(
        app.palette.visible.len() < all,
        "typing a specific query should narrow the visible rows"
    );
    assert!(app.palette.visible.iter().all(|&i| app.palette.all_rows[i]
        .label
        .to_lowercase()
        .contains("about")));
    assert_eq!(app.palette.selected, 0);

    app.palette_backspace();
    assert!(!app.palette.visible.is_empty());
}

#[test]
fn palette_move_wraps_around_the_visible_list() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_palette();
    let len = app.palette.visible.len();
    assert!(len > 1, "test needs more than one visible row");

    app.palette_move(-1);
    assert_eq!(
        app.palette.selected,
        len - 1,
        "moving up from 0 should wrap to the last row"
    );

    app.palette_move(1);
    assert_eq!(
        app.palette.selected, 0,
        "moving down from the last row should wrap to 0"
    );
}

#[test]
fn palette_selected_action_matches_the_row_at_the_selected_index() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_palette();
    let idx = app.palette.visible[app.palette.selected];
    let expected = app.palette.all_rows[idx].action.clone();
    assert_eq!(app.palette_selected_action(), Some(&expected));
}
