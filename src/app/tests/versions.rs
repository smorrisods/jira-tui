//! Fix/Affects Version picker tests.

use super::super::*;
use super::support::*;

#[test]
fn open_version_picker_seeds_selections_from_the_issues_current_versions() {
    let mut app = demo_app();
    // DS-2648 carries both a fix and an affects version in demo data (see
    // `domain::demo::demo_versions_for`).
    app.open_by_key("DS-2648");
    app.open_version_picker();
    assert!(app.version_picker_open);
    assert_eq!(
        app.version_picker.fix_selected,
        ["v3.5.0".to_string()].into_iter().collect()
    );
    assert_eq!(
        app.version_picker.affects_selected,
        ["v3.4.0".to_string()].into_iter().collect()
    );
    assert_eq!(app.version_picker.field, VersionField::Fix);
    assert!(!app.version_picker.versions.is_empty());
}

#[test]
fn version_picker_switch_field_toggles_between_fix_and_affects() {
    let mut app = demo_app();
    app.open_by_key("DS-2648");
    app.open_version_picker();
    assert_eq!(app.version_picker.field, VersionField::Fix);
    app.version_picker_switch_field();
    assert_eq!(app.version_picker.field, VersionField::Affects);
    app.version_picker_switch_field();
    assert_eq!(app.version_picker.field, VersionField::Fix);
}

#[test]
fn version_picker_toggle_adds_and_removes_from_the_active_field_only() {
    let mut app = demo_app();
    app.open_by_key("DS-2599"); // no versions in demo data
    app.open_version_picker();
    let v36_idx = app
        .version_picker
        .versions
        .iter()
        .position(|v| v.name == "v3.6.0")
        .unwrap();
    app.version_picker.cursor = v36_idx;

    app.version_picker_toggle();
    assert!(app.version_picker.fix_selected.contains("v3.6.0"));
    assert!(
        !app.version_picker.affects_selected.contains("v3.6.0"),
        "toggling Fix must not also affect the Affects selection"
    );

    app.version_picker_toggle();
    assert!(!app.version_picker.fix_selected.contains("v3.6.0"));
}

#[test]
fn version_picker_move_clamps_to_bounds() {
    let mut app = demo_app();
    app.open_by_key("DS-2599");
    app.open_version_picker();
    let len = app.version_picker.versions.len();
    app.version_picker_move(-5);
    assert_eq!(app.version_picker.cursor, 0);
    app.version_picker_move(1000);
    assert_eq!(app.version_picker.cursor, len - 1);
}

#[test]
fn confirm_version_picker_updates_fix_versions_locally_in_demo_mode() {
    let mut app = demo_app();
    app.open_by_key("DS-2599"); // starts with no versions
    app.open_version_picker();
    let v35_idx = app
        .version_picker
        .versions
        .iter()
        .position(|v| v.name == "v3.5.0")
        .unwrap();
    app.version_picker.cursor = v35_idx;
    app.version_picker_toggle();
    app.confirm_version_picker();

    assert!(!app.version_picker_open);
    assert_eq!(
        app.detail.as_ref().unwrap().fix_versions,
        vec!["v3.5.0".to_string()]
    );
    assert!(
        app.detail.as_ref().unwrap().affects_versions.is_empty(),
        "affects versions were never touched, so must stay untouched"
    );
}

#[test]
fn confirm_version_picker_with_no_changes_is_a_no_op() {
    let mut app = demo_app();
    app.open_by_key("DS-2599");
    app.open_version_picker();
    let before = app.status.clone();
    app.confirm_version_picker();
    assert!(!app.version_picker_open);
    assert_eq!(
        app.status, before,
        "no toggles means nothing to apply, so status shouldn't change"
    );
}

#[test]
fn confirm_version_picker_can_apply_both_fields_in_one_go() {
    let mut app = demo_app();
    app.open_by_key("DS-2599");
    app.open_version_picker();
    let v35_idx = app
        .version_picker
        .versions
        .iter()
        .position(|v| v.name == "v3.5.0")
        .unwrap();
    app.version_picker.cursor = v35_idx;
    app.version_picker_toggle(); // fix += v3.5.0

    app.version_picker_switch_field();
    let v34_idx = app
        .version_picker
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.version_picker.cursor = v34_idx;
    app.version_picker_toggle(); // affects += v3.4.0

    app.confirm_version_picker();

    assert_eq!(
        app.detail.as_ref().unwrap().fix_versions,
        vec!["v3.5.0".to_string()]
    );
    assert_eq!(
        app.detail.as_ref().unwrap().affects_versions,
        vec!["v3.4.0".to_string()]
    );
}

#[tokio::test]
async fn confirm_version_picker_against_a_live_source_dispatches_and_applies_on_completion() {
    let _guard = crate::test_support::lock_env_async().await;
    let mut app = live_app();
    let key = app.issues[0].key.clone();
    app.detail = Some(crate::domain::demo_detail(&key));
    app.detail.as_mut().unwrap().fix_versions.clear();
    app.screen = Screen::Detail;
    app.project_versions = crate::domain::demo_versions();
    app.open_version_picker();
    app.version_picker.cursor = 0;
    app.version_picker_toggle();

    app.confirm_version_picker();
    assert!(app.loading);
    assert!(!app.version_picker_open);

    let event = next_event(&mut app).await;
    app.apply_event(event);
    assert!(!app.loading);
    assert_eq!(app.detail.as_ref().unwrap().fix_versions.len(), 1);
}
