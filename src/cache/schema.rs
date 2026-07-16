//! Connection lifecycle and schema migrations.

use anyhow::{Context, Result};
use rusqlite::params;

use super::{cache_db_path, Cache};

/// Current schema version. Bump this and add a branch in `migrate` when the
/// schema changes.
const SCHEMA_VERSION: i64 = 2;

impl Cache {
    /// Open (creating if needed) the cache database at its standard XDG path.
    pub fn open() -> Result<Self> {
        let path = cache_db_path().context("could not resolve a cache directory")?;
        Self::open_at(&path)
    }

    /// Open (creating if needed) the cache database at an explicit path —
    /// used directly by tests so they don't depend on `$XDG_CACHE_HOME`.
    pub fn open_at(path: &std::path::Path) -> Result<Self> {
        let conn = rusqlite::Connection::open(path).context("opening cache database")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        let cache = Cache { conn };
        cache.migrate()?;
        Ok(cache)
    }

    pub(super) fn migrate(&self) -> Result<()> {
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

        if current_version < 2 {
            self.conn
                .execute_batch("ALTER TABLE issues ADD COLUMN updated_at TEXT;")?;
        }

        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![SCHEMA_VERSION.to_string()],
        )?;

        Ok(())
    }
}
