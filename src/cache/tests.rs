use std::path::PathBuf;

use crate::domain::{IssueSummary, Priority};

use super::Cache;

fn sample_issues() -> Vec<IssueSummary> {
    vec![
        IssueSummary {
            key: "DS-1".into(),
            summary: "Fix the thing".into(),
            issue_type: "Bug".into(),
            status: "In Progress".into(),
            priority: Priority::High,
            assignee: Some("scott".into()),
            blocked: false,
            updated: "1h ago".into(),
            epic: None,
        },
        IssueSummary {
            key: "DS-2".into(),
            summary: "Ship the feature".into(),
            issue_type: "Story".into(),
            status: "To Do".into(),
            priority: Priority::Medium,
            assignee: None,
            blocked: true,
            updated: "2d ago".into(),
            epic: Some("DS-100".into()),
        },
    ]
}

// A tiny hand-rolled temp-dir helper so this module doesn't need a
// `tempfile` dev-dependency just for a handful of tests.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "jira-tui-cache-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open_temp() -> (Cache, TempDir) {
    let dir = TempDir::new();
    let cache = Cache::open_at(&dir.path().join("cache.db")).unwrap();
    (cache, dir)
}

#[test]
fn migrate_is_idempotent() {
    let (cache, _dir) = open_temp();
    // Opening again (which re-runs `migrate`) must not error or duplicate schema.
    cache.migrate().unwrap();
    cache.migrate().unwrap();
}

#[test]
fn cache_db_file_is_0600() {
    let (_cache, dir) = open_temp();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("cache.db"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn site_id_upserts_and_is_stable() {
    let (cache, _dir) = open_temp();
    let id1 = cache.site_id("https://a.atlassian.net").unwrap();
    let id2 = cache.site_id("https://a.atlassian.net").unwrap();
    assert_eq!(id1, id2);

    let id3 = cache.site_id("https://b.atlassian.net").unwrap();
    assert_ne!(id1, id3);
}

#[test]
fn save_and_load_view_round_trips_issues_in_order() {
    let (mut cache, _dir) = open_temp();
    let site = cache.site_id("https://a.atlassian.net").unwrap();
    let issues = sample_issues();

    cache
        .save_view(
            site,
            "my_work",
            "My Work",
            "assignee = currentUser()",
            &issues,
        )
        .unwrap();

    let loaded = cache.load_view(site, "my_work").unwrap().unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].key, "DS-1");
    assert_eq!(loaded[1].key, "DS-2");
    assert_eq!(loaded[1].priority, Priority::Medium);
    assert_eq!(loaded[1].epic.as_deref(), Some("DS-100"));
}

#[test]
fn save_view_overwrites_previous_contents() {
    let (mut cache, _dir) = open_temp();
    let site = cache.site_id("https://a.atlassian.net").unwrap();
    cache
        .save_view(
            site,
            "my_work",
            "My Work",
            "assignee = currentUser()",
            &sample_issues(),
        )
        .unwrap();

    // Re-save with just one issue -- the stale second row must be gone.
    let mut trimmed = sample_issues();
    trimmed.truncate(1);
    cache
        .save_view(
            site,
            "my_work",
            "My Work",
            "assignee = currentUser()",
            &trimmed,
        )
        .unwrap();

    let loaded = cache.load_view(site, "my_work").unwrap().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].key, "DS-1");
}

#[test]
fn different_sites_do_not_leak_into_each_other() {
    let (mut cache, _dir) = open_temp();
    let site_a = cache.site_id("https://a.atlassian.net").unwrap();
    let site_b = cache.site_id("https://b.atlassian.net").unwrap();

    cache
        .save_view(
            site_a,
            "my_work",
            "My Work",
            "assignee = currentUser()",
            &sample_issues(),
        )
        .unwrap();

    let loaded_b = cache.load_view(site_b, "my_work").unwrap();
    assert!(
        loaded_b.is_none(),
        "site B must not see site A's cached issues"
    );
}

#[test]
fn load_view_returns_none_when_nothing_cached() {
    let (cache, _dir) = open_temp();
    let site = cache.site_id("https://a.atlassian.net").unwrap();
    assert!(cache.load_view(site, "my_work").unwrap().is_none());
}

// Serialized against other tests that mutate the same process-global
// XDG_CACHE_HOME env var — see `crate::test_support::lock_env`.
#[test]
fn migrate_legacy_json_imports_once_then_deletes_file() {
    let _guard = crate::test_support::lock_env();
    let base = std::env::temp_dir().join(format!(
        "jira-tui-legacy-migrate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let xdg_cache = base.join("cache");
    std::env::set_var("XDG_CACHE_HOME", &xdg_cache);

    let legacy_dir = xdg_cache.join("jira-tui");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy_path = legacy_dir.join("my-work.json");
    std::fs::write(
        &legacy_path,
        serde_json::to_string(&sample_issues()).unwrap(),
    )
    .unwrap();

    let (mut cache, _dir) = open_temp();
    let site = cache.site_id("https://a.atlassian.net").unwrap();

    cache.migrate_legacy_json(site, "assignee = currentUser()");

    let loaded = cache.load_view(site, "my_work").unwrap().unwrap();
    assert_eq!(loaded.len(), 2);
    assert!(
        !legacy_path.exists(),
        "legacy file should be removed after import"
    );

    // Running again must be a harmless no-op (file already gone).
    cache.migrate_legacy_json(site, "assignee = currentUser()");
    let loaded_again = cache.load_view(site, "my_work").unwrap().unwrap();
    assert_eq!(loaded_again.len(), 2);

    std::env::remove_var("XDG_CACHE_HOME");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn migrate_legacy_json_does_not_overwrite_a_freshly_cached_view() {
    let _guard = crate::test_support::lock_env();
    let base = std::env::temp_dir().join(format!(
        "jira-tui-legacy-noop-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let xdg_cache = base.join("cache");
    std::env::set_var("XDG_CACHE_HOME", &xdg_cache);

    let legacy_dir = xdg_cache.join("jira-tui");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy_path = legacy_dir.join("my-work.json");
    std::fs::write(
        &legacy_path,
        serde_json::to_string(&sample_issues()).unwrap(),
    )
    .unwrap();

    let (mut cache, _dir) = open_temp();
    let site = cache.site_id("https://a.atlassian.net").unwrap();
    let mut fresh = sample_issues();
    fresh.truncate(1);
    cache
        .save_view(
            site,
            "my_work",
            "My Work",
            "assignee = currentUser()",
            &fresh,
        )
        .unwrap();

    cache.migrate_legacy_json(site, "assignee = currentUser()");

    let loaded = cache.load_view(site, "my_work").unwrap().unwrap();
    assert_eq!(
        loaded.len(),
        1,
        "existing fresh data must not be overwritten by a legacy import"
    );
    assert!(
        !legacy_path.exists(),
        "legacy file should still be cleaned up even when not imported"
    );

    std::env::remove_var("XDG_CACHE_HOME");
    let _ = std::fs::remove_dir_all(&base);
}

// Coverage gap noticed while splitting this file: migrate_legacy_json's own
// doc comment promises "best-effort — any read/parse failure just means we
// fall through to a fresh live fetch," but nothing exercised the corrupt-JSON
// path — only "file present and valid" and "file present and already
// migrated" were covered. This runs on every startup, so it must degrade
// gracefully rather than panic.
#[test]
fn migrate_legacy_json_handles_corrupt_json_without_panicking() {
    let _guard = crate::test_support::lock_env();
    let base = std::env::temp_dir().join(format!(
        "jira-tui-legacy-corrupt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let xdg_cache = base.join("cache");
    std::env::set_var("XDG_CACHE_HOME", &xdg_cache);

    let legacy_dir = xdg_cache.join("jira-tui");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy_path = legacy_dir.join("my-work.json");
    std::fs::write(&legacy_path, "{ this is not valid json").unwrap();

    let (mut cache, _dir) = open_temp();
    let site = cache.site_id("https://a.atlassian.net").unwrap();

    // Must not panic despite the malformed file.
    cache.migrate_legacy_json(site, "assignee = currentUser()");

    assert!(
        cache.load_view(site, "my_work").unwrap().is_none(),
        "nothing should have been imported from the corrupt file"
    );
    assert!(
        !legacy_path.exists(),
        "the corrupt legacy file should still be cleaned up so this doesn't retry forever"
    );

    std::env::remove_var("XDG_CACHE_HOME");
    let _ = std::fs::remove_dir_all(&base);
}
