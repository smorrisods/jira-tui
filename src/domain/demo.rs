//! Baked-in sample data so the TUI is fully explorable with zero network.
//! Flavoured after the real DS design-system project this toolkit grew from.

use serde_json::json;

use super::types::{
    AssignableUser, ChildIssue, Comment, IssueDetail, IssueLink, IssueSummary, Priority, Transition,
};

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
        // Unknown keys should not panic; they fall back to a sensible
        // default that preserves the requested key rather than silently
        // substituting a different issue (see demo_detail_not_found's doc
        // comment) — the original test only checked the summary was
        // non-empty, missing the key-preservation guarantee.
        let d = demo_detail("DS-0000");
        assert_eq!(d.key, "DS-0000");
        assert!(!d.summary.is_empty());
    }

    #[test]
    fn demo_assignable_users_names_match_demo_issue_assignees() {
        // Coverage gap: demo_assignable_users had no direct test at all,
        // despite its own doc comment stating the display names must line
        // up with demo_issues' assignees for "assign to me"/reassignment to
        // work in demo mode.
        let users = demo_assignable_users();
        let assignable: Vec<&str> = users.iter().map(|u| u.display_name.as_str()).collect();
        assert!(assignable.contains(&DEMO_CURRENT_USER));
        for issue in demo_issues() {
            if let Some(assignee) = issue.assignee {
                assert!(
                    assignable.contains(&assignee.as_str()),
                    "{assignee} is assigned to a demo issue but missing from demo_assignable_users"
                );
            }
        }
    }
}
