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
//!
//! Split into `schema` (connection lifecycle + migrations), `queries` (the
//! per-view read/write API), and `legacy` (the one-time flat-JSON upgrade
//! path) — each an `impl Cache` block in its own file, mirroring `app/`'s
//! per-concern split.

use std::path::PathBuf;

use rusqlite::Connection;

mod legacy;
mod queries;
mod schema;

#[cfg(test)]
mod tests;

/// `$XDG_CACHE_HOME/jira-tui/cache.db` (or `~/.cache/jira-tui/cache.db`).
pub fn cache_db_path() -> Option<PathBuf> {
    crate::config::cache_dir().map(|d| d.join("cache.db"))
}

/// A handle to the on-disk cache. Opening it creates the database file (with
/// `0600` permissions) and runs any pending migrations.
pub struct Cache {
    conn: Connection,
}
