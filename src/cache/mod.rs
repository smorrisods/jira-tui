//! The on-disk issue cache: a small SQLite database (feature: `live`) at
//! `$XDG_CACHE_HOME/jira-tui/cache.db`, replacing the flat `my-work.json`
//! blob used previously.
//!
//! Scoped per Jira site (`sites` table) so switching Jira instances can
//! never show stale data from a different org/account, and scoped per
//! "view" (`views`/`view_issues`) so future work (teammate views, an
//! all-issues view) can cache more than just the fixed "my work" list
//! without inventing a new file per view. The database file gets the same
//! `0600` permissions as the token file, since cached issue summaries are
//! real (if lower-sensitivity) Jira content.
//!
//! `issue_details`/`comments` tables are deliberately not part of the
//! schema yet — only summaries are cached today, matching the previous
//! `my-work.json` behaviour. Add them later only if a feature actually
//! needs to cache full issue detail.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::domain::{IssueSummary, Priority};

/// Current schema version. Bump this and add a branch in `migrate` when the
/// schema changes.
const SCHEMA_VERSION: i64 = 1;

/// `$XDG_CACHE_HOME/jira-tui/cache.db` (or `~/.cache/jira-tui/cache.db`).
pub fn cache_db_path() -> Option<PathBuf> {
    crate::config::cache_dir().map(|d| d.join("cache.db"))
}

/// A handle to the on-disk cache. Opening it creates the database file (with
/// `0600` permissions) and runs any pending migrations.
pub struct Cache {
    conn: Connection,
}

impl Cache {
    /// Open (creating if needed) the cache database at its standard XDG path.
    pub fn open() -> Result<Self> {
        let path = cache_db_path().context("could not resolve a cache directory")?;
        Self::open_at(&path)
    }

    /// Open (creating if needed) the cache database at an explicit path —
    /// used directly by tests so they don't depend on `$XDG_CACHE_HOME`.
    pub fn open_at(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path).context("opening cache database")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        let cache = Cache { conn };
        cache.migrate()?;
        Ok(cache)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;

        let current_version: i64 = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        if current_version < 1 {
            self.conn.execute_batch(
                "CREATE TABLE sites (
                    id           INTEGER PRIMARY KEY,
                    base_url     TEXT NOT NULL UNIQUE,
                    account_id   TEXT,
                    display_name TEXT,
                    last_used_at TEXT NOT NULL
                );

                CREATE TABLE views (
                    id         INTEGER PRIMARY KEY,
                    site_id    INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
                    kind       TEXT NOT NULL,
                    label      TEXT NOT NULL,
                    jql        TEXT NOT NULL,
                    fetched_at TEXT NOT NULL,
                    UNIQUE(site_id, kind, jql)
                );

                CREATE TABLE issues (
                    id                  INTEGER PRIMARY KEY,
                    site_id             INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
                    key                 TEXT NOT NULL,
                    summary             TEXT NOT NULL,
                    issue_type          TEXT NOT NULL,
                    status              TEXT NOT NULL,
                    priority            TEXT NOT NULL,
                    assignee            TEXT,
                    blocked             INTEGER NOT NULL DEFAULT 0,
                    updated             TEXT NOT NULL,
                    epic                TEXT,
                    UNIQUE(site_id, key)
                );

                CREATE TABLE view_issues (
                    view_id  INTEGER NOT NULL REFERENCES views(id) ON DELETE CASCADE,
                    issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
                    position INTEGER NOT NULL,
                    PRIMARY KEY (view_id, issue_id)
                );",
            )?;
        }

        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![SCHEMA_VERSION.to_string()],
        )?;

        Ok(())
    }

    /// Find or create the `sites` row for `base_url`, updating
    /// `last_used_at` and returning its id.
    pub fn site_id(&self, base_url: &str) -> Result<i64> {
        let now = now_iso8601();
        self.conn.execute(
            "INSERT INTO sites (base_url, last_used_at) VALUES (?1, ?2)
             ON CONFLICT(base_url) DO UPDATE SET last_used_at = ?2",
            params![base_url, now],
        )?;
        let id = self.conn.query_row(
            "SELECT id FROM sites WHERE base_url = ?1",
            params![base_url],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Replace whatever is cached for `(site_id, kind)` with `issues`,
    /// upserting each issue's own row along the way. Overwrite semantics
    /// (not incremental merge) — matches the previous `my-work.json`
    /// behaviour of "the last successful fetch wins".
    pub fn save_view(
        &self,
        site_id: i64,
        kind: &str,
        label: &str,
        jql: &str,
        issues: &[IssueSummary],
    ) -> Result<()> {
        let now = now_iso8601();
        self.conn.execute(
            "INSERT INTO views (site_id, kind, label, jql, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(site_id, kind, jql) DO UPDATE SET label = ?3, fetched_at = ?5",
            params![site_id, kind, label, jql, now],
        )?;
        let view_id: i64 = self.conn.query_row(
            "SELECT id FROM views WHERE site_id = ?1 AND kind = ?2 AND jql = ?3",
            params![site_id, kind, jql],
            |row| row.get(0),
        )?;

        self.conn.execute(
            "DELETE FROM view_issues WHERE view_id = ?1",
            params![view_id],
        )?;

        for (position, issue) in issues.iter().enumerate() {
            self.conn.execute(
                "INSERT INTO issues (site_id, key, summary, issue_type, status, priority, assignee, blocked, updated, epic)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(site_id, key) DO UPDATE SET
                    summary = ?3, issue_type = ?4, status = ?5, priority = ?6,
                    assignee = ?7, blocked = ?8, updated = ?9, epic = ?10",
                params![
                    site_id,
                    issue.key,
                    issue.summary,
                    issue.issue_type,
                    issue.status,
                    issue.priority.label(),
                    issue.assignee,
                    issue.blocked,
                    issue.updated,
                    issue.epic,
                ],
            )?;
            let issue_id: i64 = self.conn.query_row(
                "SELECT id FROM issues WHERE site_id = ?1 AND key = ?2",
                params![site_id, issue.key],
                |row| row.get(0),
            )?;
            self.conn.execute(
                "INSERT INTO view_issues (view_id, issue_id, position) VALUES (?1, ?2, ?3)",
                params![view_id, issue_id, position as i64],
            )?;
        }

        Ok(())
    }

    /// Load the cached issues for `(site_id, kind)`, in their original
    /// order, if any view of that kind has ever been cached for this site.
    /// When more than one `jql` has been cached under the same `kind` (e.g.
    /// a future teammate view cached under different JQL per teammate), the
    /// most recently fetched one wins.
    pub fn load_view(&self, site_id: i64, kind: &str) -> Result<Option<Vec<IssueSummary>>> {
        let view_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM views WHERE site_id = ?1 AND kind = ?2 ORDER BY fetched_at DESC LIMIT 1",
                params![site_id, kind],
                |row| row.get(0),
            )
            .ok();
        let Some(view_id) = view_id else {
            return Ok(None);
        };

        let mut stmt = self.conn.prepare(
            "SELECT i.key, i.summary, i.issue_type, i.status, i.priority, i.assignee, i.blocked, i.updated, i.epic
             FROM view_issues vi
             JOIN issues i ON i.id = vi.issue_id
             WHERE vi.view_id = ?1
             ORDER BY vi.position",
        )?;
        let rows = stmt.query_map(params![view_id], |row| {
            let priority_label: String = row.get(4)?;
            Ok(IssueSummary {
                key: row.get(0)?,
                summary: row.get(1)?,
                issue_type: row.get(2)?,
                status: row.get(3)?,
                priority: priority_from_label(&priority_label),
                assignee: row.get(5)?,
                blocked: row.get(6)?,
                updated: row.get(7)?,
                epic: row.get(8)?,
            })
        })?;

        let issues: Vec<IssueSummary> = rows.collect::<rusqlite::Result<_>>()?;
        if issues.is_empty() {
            Ok(None)
        } else {
            Ok(Some(issues))
        }
    }
}

fn priority_from_label(label: &str) -> Priority {
    match label {
        "Highest" => Priority::Highest,
        "High" => Priority::High,
        "Low" => Priority::Low,
        "Lowest" => Priority::Lowest,
        _ => Priority::Medium,
    }
}

fn now_iso8601() -> String {
    // Avoid pulling in a datetime crate just for a cache timestamp: seconds
    // since the epoch, as a string, is sortable and sufficient here.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (cache, _dir) = open_temp();
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
        let (cache, _dir) = open_temp();
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
        let (cache, _dir) = open_temp();
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
}
