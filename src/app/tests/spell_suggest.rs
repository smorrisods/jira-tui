//! The spelling-suggestion picker (`F2`): finding the misspelled word under
//! the cursor and applying a chosen replacement.

use super::support::*;

#[test]
fn open_spell_suggest_finds_the_word_the_cursor_is_inside() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["This has a mispeled word.".into()];
    app.editor.cy = 0;
    app.editor.cx = "This has a mis".chars().count(); // inside "mispeled"

    app.open_spell_suggest();
    assert!(app.spell_suggest_open);
    assert!(
        !app.spell_suggest.suggestions.is_empty(),
        "expected at least one suggestion for \"mispeled\""
    );
    let (start, end) = (app.spell_suggest.start, app.spell_suggest.end);
    assert_eq!(&app.editor.lines[0][start..end], "mispeled");
}

#[test]
fn open_spell_suggest_does_nothing_on_a_correctly_spelled_word() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["This is fine.".into()];
    app.editor.cy = 0;
    app.editor.cx = 2; // inside "This"

    app.open_spell_suggest();
    assert!(!app.spell_suggest_open);
    assert_eq!(app.status, "no misspelled word here");
}

#[test]
fn confirm_spell_suggest_replaces_the_word_and_closes_the_picker() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["This has a mispeled word.".into()];
    app.editor.cy = 0;
    app.editor.cx = "This has a mis".chars().count();

    app.open_spell_suggest();
    assert!(app.spell_suggest_open);
    let chosen = app.spell_suggest.suggestions[0].clone();

    app.confirm_spell_suggest();
    assert!(!app.spell_suggest_open);
    assert!(
        app.editor.lines[0].contains(&chosen),
        "expected the buffer to contain the chosen replacement {chosen:?}, got {:?}",
        app.editor.lines[0]
    );
    assert!(!app.editor.lines[0].contains("mispeled"));
}

#[test]
fn spell_suggest_move_clamps_to_the_suggestion_list_bounds() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["a mispeled word".into()];
    app.editor.cy = 0;
    app.editor.cx = 2;
    app.open_spell_suggest();
    let len = app.spell_suggest.suggestions.len();
    assert!(
        len > 1,
        "test needs more than one suggestion to be meaningful"
    );

    app.spell_suggest_move(-1);
    assert_eq!(app.spell_suggest.selected, 0, "must not go below zero");

    app.spell_suggest_move(len as isize + 5);
    assert_eq!(
        app.spell_suggest.selected,
        len - 1,
        "must not go past the last suggestion"
    );
}

#[test]
fn open_spell_suggest_matches_at_the_words_exact_start_and_end_bytes() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["a mispeled word".into()];

    // Cursor at the char index right at the word's first letter.
    app.editor.cy = 0;
    app.editor.cx = "a ".chars().count();
    app.open_spell_suggest();
    assert!(app.spell_suggest_open, "should match at the word's start");
    app.close_spell_suggest();

    // Cursor at the char index right after the word's last letter.
    app.editor.cx = "a mispeled".chars().count();
    app.open_spell_suggest();
    assert!(app.spell_suggest_open, "should match at the word's end");
}

#[test]
fn open_spell_suggest_on_the_second_of_two_adjacent_misspelled_words() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["wrold mispeled".into()];
    app.editor.cy = 0;
    app.editor.cx = "wrold mis".chars().count(); // inside the second word

    app.open_spell_suggest();
    assert!(app.spell_suggest_open);
    let (start, end) = (app.spell_suggest.start, app.spell_suggest.end);
    assert_eq!(
        &app.editor.lines[0][start..end],
        "mispeled",
        "must match the word the cursor is actually in, not the earlier one"
    );
}

#[test]
fn close_spell_suggest_leaves_the_buffer_untouched() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["a mispeled word".into()];
    app.editor.cy = 0;
    app.editor.cx = 2;
    app.open_spell_suggest();

    app.close_spell_suggest();
    assert!(!app.spell_suggest_open);
    assert_eq!(app.editor.lines[0], "a mispeled word");
}
