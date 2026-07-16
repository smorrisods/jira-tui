//! Small cross-cutting `App` query helper tests (selection, window title,
//! toasts, at-a-glance counts).

use super::super::*;
use super::support::*;

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
fn in_review_returns_only_in_review_issues() {
    let mut app = demo_app();
    app.all_issues = vec![
        issue("DS-1", None, "In Review", false),
        issue("DS-2", None, "To Do", false),
    ];
    let in_review = app.in_review();
    assert_eq!(
        in_review.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
        vec!["DS-1"]
    );
}

#[test]
fn done_this_week_excludes_stale_done_issues_and_issues_with_no_timestamp() {
    let mut app = demo_app();
    let now = chrono::Utc::now();
    app.all_issues = vec![
        IssueSummary {
            status: "Done".into(),
            updated_at: Some(now - chrono::Duration::days(2)),
            ..crate::test_support::sample_issue("DS-1")
        },
        IssueSummary {
            status: "Done".into(),
            updated_at: Some(now - chrono::Duration::days(10)),
            ..crate::test_support::sample_issue("DS-2")
        },
        IssueSummary {
            status: "Done".into(),
            updated_at: None,
            ..crate::test_support::sample_issue("DS-3")
        },
        IssueSummary {
            status: "In Progress".into(),
            updated_at: Some(now - chrono::Duration::hours(1)),
            ..crate::test_support::sample_issue("DS-4")
        },
    ];
    let done = app.done_this_week();
    assert_eq!(
        done.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
        vec!["DS-1"],
        "only the Done issue updated within the last 7 days should count"
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
