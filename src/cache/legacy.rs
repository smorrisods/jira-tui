//! One-time upgrade path from the legacy flat-JSON cache. Safe to delete
//! once enough time has passed that no user is still on a pre-SQLite build.

use crate::domain::IssueSummary;

use super::Cache;

impl Cache {
    /// One-time migration from the legacy flat-JSON cache (`my-work.json`):
    /// if it exists and this site has no cached `my_work` view yet, import
    /// it as the initial one. Best-effort — any read/parse failure just
    /// means we fall through to a fresh live fetch, exactly like a
    /// corrupt/missing cache always has.
    pub fn migrate_legacy_json(&mut self, site_id: i64, jql: &str) {
        let Some(legacy_path) = crate::config::legacy_cache_file() else {
            return;
        };
        if !legacy_path.exists() {
            return;
        }
        match self.load_view(site_id, "my_work") {
            Ok(None) => {
                // Nothing cached for this site yet -- try the import below.
            }
            Ok(Some(_)) => {
                // Already migrated, or a live fetch already populated this
                // site's cache — nothing left to import; clean up the file
                // so this check only ever runs once per upgrade.
                let _ = std::fs::remove_file(&legacy_path);
                return;
            }
            Err(_) => {
                // Couldn't tell whether this site already has a view (a
                // transient error, not "definitely empty") — leave the
                // legacy file alone and try again next launch rather than
                // risk silently discarding an importable cache.
                return;
            }
        }
        if let Ok(content) = std::fs::read_to_string(&legacy_path) {
            if let Ok(issues) = serde_json::from_str::<Vec<IssueSummary>>(&content) {
                let _ = self.save_view(site_id, "my_work", "My Work", jql, &issues);
            }
        }
        let _ = std::fs::remove_file(&legacy_path);
    }
}
