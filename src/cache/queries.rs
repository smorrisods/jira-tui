//! Per-view read/write queries: the actual cache API consumers call.

use anyhow::Result;
use rusqlite::params;

use crate::domain::{IssueSummary, Priority};

use super::Cache;

impl Cache {
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
    /// behaviour of "the last successful fetch wins". Runs as a single
    /// transaction so a mid-write failure (or process kill) can never leave
    /// `view_issues` partially deleted/repopulated.
    pub fn save_view(
        &mut self,
        site_id: i64,
        kind: &str,
        label: &str,
        jql: &str,
        issues: &[IssueSummary],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let now = now_iso8601();
        tx.execute(
            "INSERT INTO views (site_id, kind, label, jql, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(site_id, kind, jql) DO UPDATE SET label = ?3, fetched_at = ?5",
            params![site_id, kind, label, jql, now],
        )?;
        let view_id: i64 = tx.query_row(
            "SELECT id FROM views WHERE site_id = ?1 AND kind = ?2 AND jql = ?3",
            params![site_id, kind, jql],
            |row| row.get(0),
        )?;

        tx.execute(
            "DELETE FROM view_issues WHERE view_id = ?1",
            params![view_id],
        )?;

        for (position, issue) in issues.iter().enumerate() {
            tx.execute(
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
            let issue_id: i64 = tx.query_row(
                "SELECT id FROM issues WHERE site_id = ?1 AND key = ?2",
                params![site_id, issue.key],
                |row| row.get(0),
            )?;
            tx.execute(
                "INSERT INTO view_issues (view_id, issue_id, position) VALUES (?1, ?2, ?3)",
                params![view_id, issue_id, position as i64],
            )?;
        }

        tx.commit()?;
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
