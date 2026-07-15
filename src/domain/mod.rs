//! Domain models — stable internal shapes independent of Jira's API surface.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

/// Where the data on screen came from, shown in the footer.
/// `Live`/`Cache` are only constructed when the `live` feature is enabled.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Source {
    Demo,
    Cache { user: String },
    Live { site: String, user: String },
}

impl Source {
    pub fn label(&self) -> String {
        match self {
            Source::Demo => "demo · offline sample data".into(),
            Source::Cache { user } => format!("cache · {user} · offline"),
            Source::Live { site, user } => format!("live · {site} · {user}"),
        }
    }
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

/// Baked-in sample issues so the TUI is fully explorable with zero network.
/// Flavoured after the real DS design-system project this toolkit grew from.
///
/// The implicit "you" in demo mode — offline `Source::Demo` carries no real
/// username, but the view switcher's teammate list still needs to exclude
/// whichever assignee stands in for "my work" so it isn't offered back as a
/// redundant pseudo-teammate.
pub const DEMO_CURRENT_USER: &str = "scott.morris";

pub fn demo_issues() -> Vec<IssueSummary> {
    vec![
        IssueSummary {
            key: "DS-2722".into(),
            summary: "Make accordion content searchable with hidden=\"until-found\"".into(),
            issue_type: "Epic".into(),
            status: "In Progress".into(),
            priority: Priority::High,
            assignee: Some(DEMO_CURRENT_USER.into()),
            blocked: false,
            updated: "2h ago".into(),
            epic: None,
        },
        IssueSummary {
            key: "DS-2725".into(),
            summary: "Update web component accordion to support until-found".into(),
            issue_type: "Develop".into(),
            status: "In Progress".into(),
            priority: Priority::High,
            assignee: Some(DEMO_CURRENT_USER.into()),
            blocked: false,
            updated: "31m ago".into(),
            epic: Some("DS-2722".into()),
        },
        IssueSummary {
            key: "DS-2603".into(),
            summary: "Ship precompiled CSS export for React package".into(),
            issue_type: "Develop".into(),
            status: "To Do".into(),
            priority: Priority::High,
            assignee: Some(DEMO_CURRENT_USER.into()),
            blocked: true,
            updated: "1d ago".into(),
            epic: Some("DS-2602".into()),
        },
        IssueSummary {
            key: "DS-2604".into(),
            summary: "Surface Next.js integration guide in README and npm".into(),
            issue_type: "Content".into(),
            status: "To Do".into(),
            priority: Priority::Medium,
            assignee: None,
            blocked: false,
            updated: "1d ago".into(),
            epic: Some("DS-2602".into()),
        },
        IssueSummary {
            key: "DS-2610".into(),
            summary: "Primitive design-token layer: colour ramp generator".into(),
            issue_type: "Develop".into(),
            status: "In Review".into(),
            priority: Priority::Medium,
            assignee: Some(DEMO_CURRENT_USER.into()),
            blocked: false,
            updated: "3d ago".into(),
            epic: Some("DS-2600".into()),
        },
        IssueSummary {
            key: "DS-2648".into(),
            summary: "Angular wrapper publishes stale nested package.json".into(),
            issue_type: "Bug".into(),
            status: "To Do".into(),
            priority: Priority::Highest,
            assignee: Some(DEMO_CURRENT_USER.into()),
            blocked: false,
            updated: "5h ago".into(),
            epic: None,
        },
        IssueSummary {
            key: "DS-2661".into(),
            summary: "pnpm version mismatch breaks sync-docs pipeline".into(),
            issue_type: "Bug".into(),
            status: "Done".into(),
            priority: Priority::High,
            assignee: Some(DEMO_CURRENT_USER.into()),
            blocked: false,
            updated: "6d ago".into(),
            epic: None,
        },
        IssueSummary {
            key: "DS-2599".into(),
            summary: "Add controlled value API to OntarioDateInput".into(),
            issue_type: "Develop".into(),
            status: "Backlog".into(),
            priority: Priority::Low,
            assignee: None,
            blocked: false,
            updated: "2w ago".into(),
            epic: Some("DS-2600".into()),
        },
        // A couple of teammate-assigned issues so the teammate-view switcher
        // (see the "switch view" feature) has something to show offline.
        IssueSummary {
            key: "DS-2631".into(),
            summary: "Audit focus order across the multi-step form wizard".into(),
            issue_type: "Develop".into(),
            status: "In Progress".into(),
            priority: Priority::Medium,
            assignee: Some("priya.nair".into()),
            blocked: false,
            updated: "4h ago".into(),
            epic: Some("DS-2600".into()),
        },
        IssueSummary {
            key: "DS-2640".into(),
            summary: "Investigate flaky visual-regression snapshots on CI".into(),
            issue_type: "Bug".into(),
            status: "To Do".into(),
            priority: Priority::Medium,
            assignee: Some("alex.chen".into()),
            blocked: false,
            updated: "9h ago".into(),
            epic: None,
        },
    ]
}

/// Offline stand-in for `jira::live::assignable_users`, so the assignee
/// picker (`A`) is fully explorable in demo mode and in a cache-only
/// session with no live client to ask — see `App::assignable_users_source`.
/// Account ids are obviously fake; only the display names need to line up
/// with `demo_issues`' existing assignees (`DEMO_CURRENT_USER`,
/// `priya.nair`, `alex.chen`) so "assign to me" and reassigning to a
/// teammate already visible in the demo data both work as expected.
pub fn demo_assignable_users() -> Vec<AssignableUser> {
    vec![
        AssignableUser {
            account_id: "demo-scott-morris".into(),
            display_name: DEMO_CURRENT_USER.into(),
        },
        AssignableUser {
            account_id: "demo-priya-nair".into(),
            display_name: "priya.nair".into(),
        },
        AssignableUser {
            account_id: "demo-alex-chen".into(),
            display_name: "alex.chen".into(),
        },
        AssignableUser {
            account_id: "demo-jane-reporter".into(),
            display_name: "jane.reporter".into(),
        },
    ]
}

/// A detailed view for a demo issue key, with a rich ADF description so the
/// ADF renderer is genuinely exercised offline.
///
/// Unknown keys (e.g. from "go to issue" on a key outside the demo set) return
/// a clearly-labelled placeholder that preserves the requested key, rather
/// than silently substituting a different issue.
pub fn demo_detail(key: &str) -> IssueDetail {
    let issues = demo_issues();
    let Some(base) = issues.iter().find(|i| i.key == key).cloned() else {
        return demo_detail_not_found(key);
    };

    let description = json!({
        "type": "doc",
        "version": 1,
        "content": [
            { "type": "heading", "attrs": { "level": 3 },
              "content": [ { "type": "text", "text": "Problem / Context" } ] },
            { "type": "paragraph", "content": [
                { "type": "text", "text": "Accordion panels hide their content with " },
                { "type": "text", "text": "display: none", "marks": [ { "type": "code" } ] },
                { "type": "text", "text": ", so in-page find (Ctrl+F) cannot reach collapsed copy." }
            ] },
            { "type": "heading", "attrs": { "level": 3 },
              "content": [ { "type": "text", "text": "Proposed Solution" } ] },
            { "type": "bulletList", "content": [
                { "type": "listItem", "content": [ { "type": "paragraph", "content": [
                    { "type": "text", "text": "Adopt " },
                    { "type": "text", "text": "hidden=\"until-found\"", "marks": [ { "type": "code" } ] },
                    { "type": "text", "text": " for collapsed panels." } ] } ] },
                { "type": "listItem", "content": [ { "type": "paragraph", "content": [
                    { "type": "text", "text": "Listen for the " },
                    { "type": "text", "text": "beforematch", "marks": [ { "type": "code" } ] },
                    { "type": "text", "text": " event to auto-expand on find." } ] } ] }
            ] },
            { "type": "codeBlock", "attrs": { "language": "ts" }, "content": [
                { "type": "text", "text": "panel.addEventListener('beforematch', () => this.open(panel));" }
            ] },
            { "type": "heading", "attrs": { "level": 3 },
              "content": [ { "type": "text", "text": "Definition of Done" } ] },
            { "type": "taskList", "content": [
                { "type": "taskItem", "attrs": { "state": "DONE" },
                  "content": [ { "type": "text", "text": "Feature-detect until-found support" } ] },
                { "type": "taskItem", "attrs": { "state": "TODO" },
                  "content": [ { "type": "text", "text": "Fallback to display:none where unsupported" } ] },
                { "type": "taskItem", "attrs": { "state": "TODO" },
                  "content": [ { "type": "text", "text": "Docs updated with the new behaviour" } ] }
            ] }
        ]
    });

    let acceptance = json!({
        "type": "doc",
        "version": 1,
        "content": [
            { "type": "taskList", "content": [
                { "type": "taskItem", "attrs": { "state": "TODO" },
                  "content": [ { "type": "text", "text": "Collapsed content is reachable via Ctrl+F" } ] },
                { "type": "taskItem", "attrs": { "state": "TODO" },
                  "content": [ { "type": "text", "text": "Matched panel auto-expands and scrolls into view" } ] },
                { "type": "taskItem", "attrs": { "state": "DONE" },
                  "content": [ { "type": "text", "text": "No regression in browsers lacking support" } ] }
            ] }
        ]
    });

    IssueDetail {
        key: base.key.clone(),
        summary: base.summary.clone(),
        issue_type: base.issue_type.clone(),
        status: base.status.clone(),
        priority: base.priority.clone(),
        assignee: base.assignee.clone(),
        reporter: Some("jane.reporter".into()),
        labels: vec!["accordion".into(), "web-components".into(), "a11y".into()],
        components: vec!["Accordion".into(), "Web Components".into()],
        parent: if base.issue_type == "Epic" {
            None
        } else {
            Some("DS-2722".into())
        },
        links: vec![IssueLink {
            relation: "is blocked by".into(),
            key: "DS-2603".into(),
            summary: "Ship precompiled CSS export for React package".into(),
        }],
        children: issues
            .into_iter()
            .filter(|i| i.epic.as_deref() == Some(key))
            .map(|i| ChildIssue {
                key: i.key,
                issue_type: i.issue_type,
                summary: i.summary,
                status: i.status,
            })
            .collect(),
        description,
        acceptance_criteria: Some(acceptance),
        transitions: ["To Do", "In Progress", "In Review", "Done"]
            .iter()
            .enumerate()
            .map(|(i, name)| Transition {
                id: (i + 1).to_string(),
                name: name.to_string(),
                to: name.to_string(),
            })
            .collect(),
        comments: demo_comments(),
    }
}

/// A few canned comments so the comment-reading UI is fully explorable
/// offline, without any live Jira connection.
fn demo_comments() -> Vec<Comment> {
    vec![
        Comment {
            id: "1".into(),
            author: "jane.reporter".into(),
            created: "3d ago".into(),
            body: json!({
                "type": "doc",
                "version": 1,
                "content": [
                    { "type": "paragraph", "content": [
                        { "type": "text", "text": "Confirmed on Safari 17 — collapsed accordion copy is invisible to Ctrl+F. Blocking a client accessibility audit." }
                    ] }
                ]
            }),
        },
        Comment {
            id: "2".into(),
            author: "scott.morris".into(),
            created: "2d ago".into(),
            body: json!({
                "type": "doc",
                "version": 1,
                "content": [
                    { "type": "paragraph", "content": [
                        { "type": "text", "text": "Prototype using " },
                        { "type": "text", "text": "hidden=\"until-found\"", "marks": [ { "type": "code" } ] },
                        { "type": "text", "text": " looks promising — pushing a branch for review shortly." }
                    ] }
                ]
            }),
        },
        Comment {
            id: "3".into(),
            author: "jane.reporter".into(),
            created: "5h ago".into(),
            body: json!({
                "type": "doc",
                "version": 1,
                "content": [
                    { "type": "paragraph", "content": [
                        { "type": "text", "text": "👍 tested the branch locally, works great in Chrome/Firefox. Let's confirm the fallback path before merging." }
                    ] }
                ]
            }),
        },
    ]
}

/// Placeholder detail for a key that isn't part of the demo dataset, used by
/// "go to issue" when offline. Keeps the requested key intact instead of
/// silently showing an unrelated issue.
fn demo_detail_not_found(key: &str) -> IssueDetail {
    let description = json!({
        "type": "doc",
        "version": 1,
        "content": [
            { "type": "paragraph", "content": [
                { "type": "text", "text": "This key isn't part of the offline demo dataset. " },
                { "type": "text", "text": "Connect to live Jira to look it up for real." }
            ] }
        ]
    });
    IssueDetail {
        key: key.to_string(),
        summary: "Not found in demo data".to_string(),
        issue_type: "Unknown".to_string(),
        status: "Unknown".to_string(),
        priority: Priority::Medium,
        assignee: None,
        reporter: None,
        labels: Vec::new(),
        components: Vec::new(),
        parent: None,
        links: Vec::new(),
        children: Vec::new(),
        description,
        acceptance_criteria: None,
        transitions: Vec::new(),
        comments: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_issues_are_present_and_unique() {
        let issues = demo_issues();
        assert!(issues.len() >= 5);
        let mut keys: Vec<&str> = issues.iter().map(|i| i.key.as_str()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), issues.len(), "issue keys must be unique");
    }

    #[test]
    fn demo_issues_include_a_blocked_one() {
        assert!(demo_issues().iter().any(|i| i.blocked));
    }

    #[test]
    fn demo_detail_matches_requested_key() {
        let d = demo_detail("DS-2725");
        assert_eq!(d.key, "DS-2725");
        assert_eq!(
            d.description.get("type").and_then(|t| t.as_str()),
            Some("doc")
        );
        assert!(d.acceptance_criteria.is_some());
    }

    #[test]
    fn demo_detail_falls_back_for_unknown_key() {
        // Unknown keys should not panic; they fall back to a sensible default.
        let d = demo_detail("DS-0000");
        assert!(!d.summary.is_empty());
    }

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
    fn source_labels_render() {
        assert!(Source::Demo.label().contains("demo"));
        assert!(Source::Cache { user: "me".into() }
            .label()
            .contains("cache"));
        assert!(Source::Live {
            site: "x.atlassian.net".into(),
            user: "me".into()
        }
        .label()
        .contains("live"));
    }

    #[test]
    fn issue_summary_round_trips_through_json() {
        let issues = demo_issues();
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
