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
    /// Raw ADF description document.
    pub description: Value,
    /// Raw ADF acceptance criteria (customfield_10309).
    pub acceptance_criteria: Option<Value>,
    pub transitions: Vec<Transition>,
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

/// Baked-in sample issues so the TUI is fully explorable with zero network.
/// Flavoured after the real DS design-system project this toolkit grew from.
pub fn demo_issues() -> Vec<IssueSummary> {
    vec![
        IssueSummary {
            key: "DS-2722".into(),
            summary: "Make accordion content searchable with hidden=\"until-found\"".into(),
            issue_type: "Epic".into(),
            status: "In Progress".into(),
            priority: Priority::High,
            assignee: Some("scott.morris".into()),
            blocked: false,
            updated: "2h ago".into(),
        },
        IssueSummary {
            key: "DS-2725".into(),
            summary: "Update web component accordion to support until-found".into(),
            issue_type: "Develop".into(),
            status: "In Progress".into(),
            priority: Priority::High,
            assignee: Some("scott.morris".into()),
            blocked: false,
            updated: "31m ago".into(),
        },
        IssueSummary {
            key: "DS-2603".into(),
            summary: "Ship precompiled CSS export for React package".into(),
            issue_type: "Develop".into(),
            status: "To Do".into(),
            priority: Priority::High,
            assignee: Some("scott.morris".into()),
            blocked: true,
            updated: "1d ago".into(),
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
        },
        IssueSummary {
            key: "DS-2610".into(),
            summary: "Primitive design-token layer: colour ramp generator".into(),
            issue_type: "Develop".into(),
            status: "In Review".into(),
            priority: Priority::Medium,
            assignee: Some("scott.morris".into()),
            blocked: false,
            updated: "3d ago".into(),
        },
        IssueSummary {
            key: "DS-2648".into(),
            summary: "Angular wrapper publishes stale nested package.json".into(),
            issue_type: "Bug".into(),
            status: "To Do".into(),
            priority: Priority::Highest,
            assignee: Some("scott.morris".into()),
            blocked: false,
            updated: "5h ago".into(),
        },
        IssueSummary {
            key: "DS-2661".into(),
            summary: "pnpm version mismatch breaks sync-docs pipeline".into(),
            issue_type: "Bug".into(),
            status: "Done".into(),
            priority: Priority::High,
            assignee: Some("scott.morris".into()),
            blocked: false,
            updated: "6d ago".into(),
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
        },
    ]
}

/// A detailed view for a demo issue key, with a rich ADF description so the
/// ADF renderer is genuinely exercised offline.
pub fn demo_detail(key: &str) -> IssueDetail {
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

    let base = demo_issues()
        .into_iter()
        .find(|i| i.key == key)
        .unwrap_or_else(|| demo_issues()[0].clone());

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
}
