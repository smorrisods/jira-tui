//! Stable wire-shape types — independent of Jira's API surface. Sample data
//! for offline/demo mode lives in `domain::demo`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Jira priority. Some variants are only constructed in live mode.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Highest,
    High,
    Medium,
    Low,
    Lowest,
}

impl Priority {
    pub fn glyph(&self) -> &'static str {
        match self {
            Priority::Highest => "⏫",
            Priority::High => "🔺",
            Priority::Medium => "▪",
            Priority::Low => "🔻",
            Priority::Lowest => "⏬",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Priority::Highest => "Highest",
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
            Priority::Lowest => "Lowest",
        }
    }

    /// Sort rank, highest priority first.
    pub fn rank(&self) -> u8 {
        match self {
            Priority::Highest => 0,
            Priority::High => 1,
            Priority::Medium => 2,
            Priority::Low => 3,
            Priority::Lowest => 4,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IssueSummary {
    pub key: String,
    pub summary: String,
    pub issue_type: String,
    pub status: String,
    pub priority: Priority,
    pub assignee: Option<String>,
    pub blocked: bool,
    pub updated: String,
    /// Parent (usually Epic) key, used to group issues into board swimlanes.
    pub epic: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IssueDetail {
    pub key: String,
    pub summary: String,
    pub issue_type: String,
    pub status: String,
    pub priority: Priority,
    pub assignee: Option<String>,
    pub reporter: Option<String>,
    pub labels: Vec<String>,
    pub components: Vec<String>,
    pub parent: Option<String>,
    pub links: Vec<IssueLink>,
    /// This issue's children: an Epic's child stories/tasks, or a
    /// Story/Task's sub-tasks. The reverse of `parent` — Jira has no single
    /// field for it (see `jira::live::fetch_detail`).
    pub children: Vec<ChildIssue>,
    /// Raw ADF description document.
    pub description: Value,
    /// Raw ADF acceptance criteria, fetched from a configurable custom field
    /// (see `acceptance_criteria_field` in `config.toml`). `None` if the
    /// field isn't configured or the issue has no value for it.
    pub acceptance_criteria: Option<Value>,
    pub transitions: Vec<Transition>,
    /// All comments, oldest first. Fetched in full via pagination in live
    /// mode (see `jira::live::fetch_comments`); a handful of canned entries
    /// in demo mode.
    pub comments: Vec<Comment>,
}

/// A single issue comment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub author: String,
    /// Display string, formatted the same way as `IssueSummary::updated`.
    pub created: String,
    /// Raw ADF comment body, rendered the same way as the description.
    pub body: Value,
}

/// A workflow transition available from the current status.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transition {
    pub id: String,
    pub name: String,
    /// The status this transition leads to (falls back to `name`).
    pub to: String,
}

#[derive(Clone, Debug)]
pub struct IssueLink {
    pub relation: String,
    pub key: String,
    pub summary: String,
}

/// One of `IssueDetail::children` — an Epic's child story/task, or a
/// Story/Task's sub-task. Lighter than `IssueSummary`: just enough to render
/// and navigate to the child issue.
#[derive(Clone, Debug, Serialize)]
pub struct ChildIssue {
    pub key: String,
    pub issue_type: String,
    pub summary: String,
    pub status: String,
}

/// A user assignable to issues in the configured project — carries the
/// `accountId` Jira's assign endpoint requires alongside the display name
/// shown everywhere else (`IssueSummary`/`IssueDetail::assignee` stay
/// display-name-only strings; this is the one place jira-tui needs the
/// account id at all, so it isn't threaded through the rest of the domain
/// model). See `jira::live::assignable_users` / `app::assign`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignableUser {
    pub account_id: String,
    pub display_name: String,
}

/// Where the data on screen came from, shown in the header's sync pill.
/// `Live`/`Cache` are only constructed when the `live` feature is enabled.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Source {
    Demo,
    Cache { user: String },
    Live { site: String, user: String },
}

/// Which issue list is currently loaded into `App.all_issues` — "my work" is
/// the long-standing default; `AllProject` and `Teammate` are alternate JQL
/// queries through the same generic `search_issues` primitive, switched via
/// the view picker (`V`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ViewKind {
    #[default]
    MyWork,
    AllProject,
    /// A teammate's assigned work, matched by Jira display name (see the
    /// caveat in issue #6: fragile against renames/duplicate names, but
    /// avoids an extra `accountId` lookup for v1).
    Teammate(String),
}

impl ViewKind {
    /// Short label shown in the header/status when this view is active.
    pub fn label(&self) -> String {
        match self {
            ViewKind::MyWork => "My Work".into(),
            ViewKind::AllProject => "All Project Issues".into(),
            ViewKind::Teammate(name) => format!("{name}'s Work"),
        }
    }

    /// The cache `kind` this view persists under (used as the SQLite
    /// `views.kind` column). Every view gets a durable on-disk cache entry;
    /// each teammate gets their own `kind` (`teammate:<name>`) so switching
    /// between teammates doesn't clobber or shadow another teammate's last
    /// cached fetch — `Cache::load_view` picks the most recently fetched
    /// view *for a given kind*, so a shared "teammate" kind would otherwise
    /// return whichever teammate happened to be fetched most recently.
    pub fn cache_kind(&self) -> String {
        match self {
            ViewKind::MyWork => "my_work".to_string(),
            ViewKind::AllProject => "all_project".to_string(),
            ViewKind::Teammate(name) => format!("teammate:{name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_glyph_and_label_are_nonempty() {
        for p in [
            Priority::Highest,
            Priority::High,
            Priority::Medium,
            Priority::Low,
            Priority::Lowest,
        ] {
            assert!(!p.glyph().is_empty());
            assert!(!p.label().is_empty());
        }
    }

    #[test]
    fn priority_rank_orders_highest_first() {
        // Coverage gap noticed while splitting this file: glyph/label were
        // tested per-variant but rank() — the actual sort key used to order
        // the work list — had no test at all.
        let ranks = [
            Priority::Highest,
            Priority::High,
            Priority::Medium,
            Priority::Low,
            Priority::Lowest,
        ]
        .map(|p| p.rank());
        let mut sorted = ranks;
        sorted.sort();
        assert_eq!(
            ranks, sorted,
            "rank() should already be in ascending order, highest priority first"
        );
        let unique: std::collections::HashSet<_> = ranks.iter().collect();
        assert_eq!(
            unique.len(),
            ranks.len(),
            "every priority should have a distinct rank"
        );
    }

    #[test]
    fn view_kind_labels_render() {
        // Coverage gap: cache_kind() had a dedicated test but label() (the
        // string actually shown in the header) did not.
        assert_eq!(ViewKind::MyWork.label(), "My Work");
        assert_eq!(ViewKind::AllProject.label(), "All Project Issues");
        assert_eq!(ViewKind::Teammate("Amy".into()).label(), "Amy's Work");
    }

    #[test]
    fn issue_summary_round_trips_through_json() {
        let issues = crate::domain::demo_issues();
        let json = serde_json::to_string(&issues).unwrap();
        let back: Vec<IssueSummary> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), issues.len());
        assert_eq!(back[0].key, issues[0].key);
    }

    #[test]
    fn view_kind_cache_kinds_are_distinct_per_view_including_each_teammate() {
        // Every view gets its own durable cache entry now — in particular,
        // each teammate must get a distinct `kind` so switching between
        // teammates doesn't clobber or shadow another teammate's last
        // cached fetch (`Cache::load_view` picks the most recent row for a
        // given `kind`, regardless of which JQL produced it).
        assert_eq!(ViewKind::MyWork.cache_kind(), "my_work");
        assert_eq!(ViewKind::AllProject.cache_kind(), "all_project");

        let amy = ViewKind::Teammate("Amy".into()).cache_kind();
        let bob = ViewKind::Teammate("Bob".into()).cache_kind();
        assert_ne!(amy, bob);
        assert_ne!(amy, ViewKind::MyWork.cache_kind());
        assert_ne!(amy, ViewKind::AllProject.cache_kind());
    }
}
