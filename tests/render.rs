//! Headless rendering tests — drive the real `ui::draw` through a TestBackend
//! and assert the composed screen text, so each screen is exercised in CI.

use jira_tui::app::{App, Screen, WelcomePhase};
use jira_tui::ui;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;

fn dump(buf: &Buffer) -> String {
    let area = buf.area;
    let mut s = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            let sym = buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" ");
            s.push_str(sym);
        }
        s.push('\n');
    }
    s
}

fn render(app: &App) -> String {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    dump(terminal.backend().buffer())
}

fn demo_app() -> App {
    App::new(true)
}

#[test]
fn home_screen_shows_work_and_context() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let text = render(&app);
    assert!(text.contains("my work"), "home should list my work");
    assert!(
        text.contains("current context"),
        "home should show git context"
    );
    assert!(text.contains("DS-"), "home should show issue keys");
}

#[test]
fn detail_screen_renders_adf() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    assert_eq!(app.screen, Screen::Detail);
    let text = render(&app);
    assert!(
        text.contains("Acceptance"),
        "detail should show acceptance criteria"
    );
    assert!(text.contains("["), "detail should render task checkboxes");
}

#[test]
fn about_screen_shows_animated_banner() {
    let mut app = demo_app();
    app.screen = Screen::About;
    let text = render(&app);
    assert!(text.contains('█'), "about should render the block banner");
}

#[test]
fn welcome_intro_shows_jax_and_choices() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    app.welcome_phase = WelcomePhase::Intro;
    let text = render(&app);
    assert!(text.contains("Jax"), "welcome should introduce Jax");
    assert!(
        text.contains("Set up live"),
        "welcome should offer live setup"
    );
}

#[test]
fn welcome_setup_shows_credential_fields() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    app.welcome_phase = WelcomePhase::Setup;
    app.field_token = "supersecret".to_string();
    let text = render(&app);
    assert!(text.contains("site"));
    assert!(text.contains("email"));
    assert!(text.contains("token"));
    // The token must be masked, never shown in the clear.
    assert!(!text.contains("supersecret"), "token must be masked");
    assert!(text.contains('•'), "masked token should render bullets");
}

#[test]
fn transition_picker_lists_targets() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_transitions();
    let text = render(&app);
    assert!(text.contains("move to"), "picker should show a title");
    assert!(
        text.contains("In Progress"),
        "picker should list transitions"
    );
    assert!(
        text.contains("current"),
        "picker should mark the current status"
    );
}

#[test]
fn preview_screen_renders_pending_edit() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.finish_edit("## Fresh heading\n\nEdited body text.");
    assert_eq!(app.screen, Screen::Preview);
    let text = render(&app);
    assert!(text.contains("preview"), "preview should be titled");
    assert!(text.contains("Fresh heading"));
    assert!(text.contains("apply"));
}

#[test]
fn in_tui_editor_renders_buffer() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    let text = render(&app);
    assert!(text.contains("editing"), "editor should be titled");
    assert!(
        text.contains("Problem"),
        "editor should show the description Markdown"
    );
}

#[test]
fn quick_view_panel_shows_selected_issue() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.quick_view = true;
    app.selected = 0;
    // Simulate the run loop's lazy-load call.
    app.ensure_quick_view_loaded();
    let text = render(&app);
    assert!(
        text.contains("quick view"),
        "quick view panel should render"
    );
    // Once loaded, the full fields and ADF body should be visible, not just
    // the one-line summary.
    assert!(text.contains("assignee:"), "quick view should show fields");
    assert!(
        text.contains("Problem") || text.contains("Proposed"),
        "quick view should render the full ADF body"
    );
}

#[test]
fn quick_view_panel_spans_full_width() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = true;
    app.selected = 0;
    let text = render(&app);
    // Rendered as one string per row by our TestBackend dump helper: the
    // "quick view" title should appear near the left edge of a wide frame,
    // confirming the panel isn't squeezed into a narrow column.
    let line = text.lines().find(|l| l.contains("quick view")).unwrap();
    assert!(
        line.trim_start().starts_with('│') || line.trim_start().starts_with('╭'),
        "quick view panel should start at the frame's left edge (full width), got: {line:?}"
    );
}

#[test]
fn work_list_title_shows_sort_and_filter() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.cycle_filter();
    let text = render(&app);
    assert!(
        text.contains("sort"),
        "list title should show the sort mode"
    );
    assert!(
        text.contains("filter"),
        "list title should show the active filter"
    );
}

#[test]
fn jax_companion_appears_when_toggled() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.show_jax = true;
    let text = render(&app);
    assert!(text.contains("jax"), "the Jax companion box should render");
}
