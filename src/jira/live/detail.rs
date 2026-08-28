//! Full issue detail — the most interconnected file in this module: it
//! calls into `mutations` (transitions), `comments`, and `search` (an
//! Epic's child stories) to assemble one `IssueDetail`.

use serde_json::Value;

use super::super::config::Config;
use super::attachments::parse_attachments;
use super::comments::fetch_comments;
use super::mutations::fetch_transitions;
use super::search::search_issues;
use super::sprint::{current_sprint, parse_sprint_field};
use super::support::{priority_from, str_field};
use crate::domain::{ChildIssue, IssueDetail, IssueLink};

pub fn fetch_detail(cfg: &Config, key: &str) -> anyhow::Result<IssueDetail> {
    let mut fields = "summary,status,issuetype,priority,assignee,reporter,labels,\
        components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment"
        .to_string();
    if let Some(ac_field) = &cfg.acceptance_criteria_field {
        fields.push(',');
        fields.push_str(ac_field);
    }
    if let Some(sprint_field) = &cfg.sprint_field {
        fields.push(',');
        fields.push_str(sprint_field);
    }
    let path = format!("/rest/api/3/issue/{key}?fields={fields}");
    let issue = super::support::get(cfg, &path)?;
    let f = issue.get("fields").cloned().unwrap_or(Value::Null);

    let labels = f
        .get("labels")
        .and_then(|l| l.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let components = f
        .get("components")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let fix_versions = version_names(&f, "fixVersions");
    let affects_versions = version_names(&f, "versions");
    let links = f
        .get("issuelinks")
        .and_then(|l| l.as_array())
        .map(|a| parse_links(a))
        .unwrap_or_default();
    let issue_type = str_field(&f, &["issuetype", "name"]).unwrap_or_else(|| "Task".into());
    // Jira has no single "children" field: a Story/Task's sub-tasks come
    // back inline via `subtasks` on this same response, but an Epic's child
    // stories don't — they only show up by searching `parent = <key>`, one
    // extra request, only paid for Epics.
    let children = if issue_type == "Epic" {
        children_of(cfg, key).unwrap_or_default()
    } else {
        f.get("subtasks")
            .and_then(|s| s.as_array())
            .map(|a| parse_subtasks(a))
            .unwrap_or_default()
    };

    Ok(IssueDetail {
        key: key.to_string(),
        summary: str_field(&f, &["summary"]).unwrap_or_default(),
        issue_type,
        status: str_field(&f, &["status", "name"]).unwrap_or_else(|| "Unknown".into()),
        priority: priority_from(&str_field(&f, &["priority", "name"]).unwrap_or_default()),
        assignee: str_field(&f, &["assignee", "displayName"]),
        reporter: str_field(&f, &["reporter", "displayName"]),
        labels,
        components,
        fix_versions,
        affects_versions,
        parent: str_field(&f, &["parent", "key"]),
        links,
        children,
        description: f.get("description").cloned().unwrap_or(Value::Null),
        acceptance_criteria: cfg
            .acceptance_criteria_field
            .as_ref()
            .and_then(|field| f.get(field).cloned()),
        sprint: cfg.sprint_field.as_ref().and_then(|field| {
            let sprints = parse_sprint_field(f.get(field)?);
            current_sprint(&sprints)
        }),
        transitions: fetch_transitions(cfg, key).unwrap_or_default(),
        comments: fetch_comments(cfg, key).unwrap_or_default(),
        attachments: parse_attachments(&f),
    })
}

/// Extract version names from a `fixVersions`/`versions` array field —
/// shared by both since Jira shapes them identically (`[{"name": ...}, ...]`).
fn version_names(fields: &Value, field: &str) -> Vec<String> {
    fields
        .get(field)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_links(arr: &[Value]) -> Vec<IssueLink> {
    let mut out = Vec::new();
    for link in arr {
        let ty = link.get("type").cloned().unwrap_or(Value::Null);
        if let Some(inward) = link.get("inwardIssue") {
            out.push(IssueLink {
                relation: ty
                    .get("inward")
                    .and_then(|v| v.as_str())
                    .unwrap_or("relates to")
                    .to_string(),
                key: inward
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                summary: inward
                    .get("fields")
                    .and_then(|f| f.get("summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
        if let Some(outward) = link.get("outwardIssue") {
            out.push(IssueLink {
                relation: ty
                    .get("outward")
                    .and_then(|v| v.as_str())
                    .unwrap_or("relates to")
                    .to_string(),
                key: outward
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                summary: outward
                    .get("fields")
                    .and_then(|f| f.get("summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    out
}

/// Parse the `subtasks` field inlined on an issue's own `GET` response —
/// each entry is a lightweight `{key, fields: {summary, status, issuetype}}`,
/// cheap enough that it doesn't need a follow-up request.
fn parse_subtasks(arr: &[Value]) -> Vec<ChildIssue> {
    arr.iter()
        .map(|s| ChildIssue {
            key: str_field(s, &["key"]).unwrap_or_else(|| "?".into()),
            issue_type: str_field(s, &["fields", "issuetype", "name"])
                .unwrap_or_else(|| "Sub-task".into()),
            summary: str_field(s, &["fields", "summary"]).unwrap_or_default(),
            status: str_field(s, &["fields", "status", "name"]).unwrap_or_else(|| "Unknown".into()),
        })
        .collect()
}

/// An Epic's child stories/tasks — unlike sub-tasks, these aren't inlined on
/// the Epic's own response, so this issues a `parent = <key>` JQL search
/// through the same machinery as the other views.
fn children_of(cfg: &Config, key: &str) -> anyhow::Result<Vec<ChildIssue>> {
    // Issue keys shouldn't contain quotes, but a JQL string literal is
    // still a string literal.
    let escaped = super::search::escape_jql_string(key);
    let jql = format!("parent = \"{escaped}\" ORDER BY key ASC");
    Ok(search_issues(cfg, &jql)?
        .into_iter()
        .map(|s| ChildIssue {
            key: s.key,
            issue_type: s.issue_type,
            summary: s.summary,
            status: s.status,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::super::support::test_config;
    use super::*;

    #[test]
    fn fetch_detail_includes_the_configured_acceptance_criteria_field() {
        let mut server = mockito::Server::new();
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment,customfield_10001",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "fields": {
                        "summary": "Fix the thing",
                        "issuetype": {"name": "Bug"},
                        "status": {"name": "In Progress"},
                        "priority": {"name": "Low"},
                        "customfield_10001": {"type": "doc", "content": []}
                    }
                }"#,
            )
            .create();
        // fetch_detail also fetches transitions; a 404 here just means the
        // detail still comes back with an empty transitions list.
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let mut cfg = test_config(server.url());
        cfg.acceptance_criteria_field = Some("customfield_10001".into());

        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        assert_eq!(detail.summary, "Fix the thing");
        assert!(detail.acceptance_criteria.is_some());
        assert!(detail.transitions.is_empty());
    }

    #[test]
    fn fetch_detail_omits_acceptance_criteria_when_not_configured() {
        let mut server = mockito::Server::new();
        // No acceptance_criteria_field configured, so the request must not
        // ask for any customfield_* — this mock only matches that exact
        // fields list (no trailing custom field).
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"fields": {"summary": "No AC here"}}"#)
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        assert_eq!(detail.acceptance_criteria, None);
    }

    #[test]
    fn fetch_detail_includes_the_configured_sprint_field_and_picks_the_active_sprint() {
        let mut server = mockito::Server::new();
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment,customfield_10020",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "fields": {
                        "summary": "Sprint-tracked issue",
                        "customfield_10020": [
                            {"id": 5375, "name": "Design Sprint 10", "state": "closed", "goal": ""},
                            {"id": 6415, "name": "Design Sprint 81", "state": "active", "boardId": 843, "goal": ""}
                        ]
                    }
                }"#,
            )
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let mut cfg = test_config(server.url());
        cfg.sprint_field = Some("customfield_10020".into());

        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        let sprint = detail.sprint.expect("an active sprint should be picked");
        assert_eq!(sprint.id, "6415");
        assert_eq!(sprint.name, "Design Sprint 81");
        assert_eq!(sprint.state, "active");
    }

    #[test]
    fn fetch_detail_omits_sprint_when_not_configured() {
        let mut server = mockito::Server::new();
        // No sprint_field configured, so the request must not ask for any
        // customfield_* at all — this mock only matches that exact fields
        // list (no trailing custom field), even though the response itself
        // happens to carry a sprint-shaped field Jira wasn't asked for.
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"fields": {"summary": "No sprint tracking here"}}"#)
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        assert_eq!(detail.sprint, None);
    }

    #[test]
    fn fetch_detail_sprint_is_none_when_configured_but_issue_has_no_current_sprint() {
        let mut server = mockito::Server::new();
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment,customfield_10020",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "fields": {
                        "summary": "Only sprint history, nothing current",
                        "customfield_10020": [
                            {"id": 1, "name": "Old Sprint", "state": "closed", "goal": ""}
                        ]
                    }
                }"#,
            )
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let mut cfg = test_config(server.url());
        cfg.sprint_field = Some("customfield_10020".into());

        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        assert_eq!(
            detail.sprint, None,
            "sprint_field is configured, but only closed history exists — no current sprint"
        );
    }

    #[test]
    fn fetch_detail_parses_fix_and_affects_versions() {
        let mut server = mockito::Server::new();
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "fields": {
                        "summary": "Ships in the next release",
                        "fixVersions": [{"name": "v1.0.0"}, {"name": "v1.1.0"}],
                        "versions": [{"name": "v0.9.0"}]
                    }
                }"#,
            )
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        assert_eq!(detail.fix_versions, vec!["v1.0.0", "v1.1.0"]);
        assert_eq!(detail.affects_versions, vec!["v0.9.0"]);
    }

    #[test]
    fn fetch_detail_defaults_to_empty_versions_when_absent() {
        let mut server = mockito::Server::new();
        server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"fields": {"summary": "No versions here"}}"#)
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        assert!(detail.fix_versions.is_empty());
        assert!(detail.affects_versions.is_empty());
    }

    #[test]
    fn fetch_detail_parses_attachments() {
        let mut server = mockito::Server::new();
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "fields": {
                        "summary": "Has an attachment",
                        "attachment": [
                            {
                                "id": "10001",
                                "filename": "accordion-mockup.png",
                                "size": 245760,
                                "mimeType": "image/png",
                                "created": "2026-07-08T09:00:00.000-0400",
                                "content": "https://demo.atlassian.net/secure/attachment/10001/accordion-mockup.png"
                            }
                        ]
                    }
                }"#,
            )
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        assert_eq!(detail.attachments.len(), 1);
        assert_eq!(detail.attachments[0].id, "10001");
        assert_eq!(detail.attachments[0].filename, "accordion-mockup.png");
        assert_eq!(detail.attachments[0].size, 245_760);
        assert_eq!(detail.attachments[0].mime_type, "image/png");
        assert_eq!(detail.attachments[0].created, "2026-07-08");
        assert_eq!(
            detail.attachments[0].content_url,
            "https://demo.atlassian.net/secure/attachment/10001/accordion-mockup.png"
        );
    }

    #[test]
    fn fetch_detail_defaults_to_empty_attachments_when_absent() {
        let mut server = mockito::Server::new();
        server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"fields": {"summary": "No attachments here"}}"#)
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        assert!(detail.attachments.is_empty());
    }

    #[test]
    fn fetch_detail_parses_inline_subtasks_for_non_epic_issues() {
        let mut server = mockito::Server::new();
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "fields": {
                        "summary": "Parent story",
                        "issuetype": {"name": "Story"},
                        "subtasks": [
                            {
                                "key": "DS-2",
                                "fields": {
                                    "summary": "Sub-task one",
                                    "status": {"name": "Done"},
                                    "issuetype": {"name": "Sub-task"}
                                }
                            }
                        ]
                    }
                }"#,
            )
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        assert_eq!(detail.children.len(), 1);
        assert_eq!(detail.children[0].key, "DS-2");
        assert_eq!(detail.children[0].issue_type, "Sub-task");
        assert_eq!(detail.children[0].summary, "Sub-task one");
        assert_eq!(detail.children[0].status, "Done");
    }

    #[test]
    fn fetch_detail_fetches_epic_children_via_jql_instead_of_subtasks() {
        let mut server = mockito::Server::new();
        // Epics don't inline their child stories under `subtasks`, so
        // fetch_detail must fall back to a `parent = <key>` JQL search.
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,fixVersions,versions,parent,issuelinks,description,subtasks,attachment",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"fields": {"summary": "Ship the epic", "issuetype": {"name": "Epic"}}}"#)
            .create();
        server
            .mock("GET", "/rest/api/3/issue/DS-1/transitions")
            .with_status(404)
            .create();
        let children_mock = server
            .mock(
                "GET",
                "/rest/api/3/search/jql?jql=parent%20%3D%20%22DS-1%22%20ORDER%20BY%20key%20ASC&maxResults=50&fields=summary,status,issuetype,priority,assignee,updated,issuelinks,parent",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "issues": [
                        {
                            "key": "DS-2",
                            "fields": {
                                "summary": "Child story",
                                "issuetype": {"name": "Story"},
                                "status": {"name": "To Do"}
                            }
                        }
                    ],
                    "isLast": true
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let detail = fetch_detail(&cfg, "DS-1").unwrap();

        issue_mock.assert();
        children_mock.assert();
        assert_eq!(detail.children.len(), 1);
        assert_eq!(detail.children[0].key, "DS-2");
        assert_eq!(detail.children[0].issue_type, "Story");
        assert_eq!(detail.children[0].summary, "Child story");
        assert_eq!(detail.children[0].status, "To Do");
    }

    #[test]
    fn children_of_escapes_quotes_and_backslashes_in_the_key() {
        let mut server = mockito::Server::new();
        // A key isn't attacker-controlled in practice, but children_of must
        // still escape it the same way jql_for's Teammate arm does — a
        // stray `"` in the interpolated value would otherwise terminate the
        // JQL string literal early and let the rest of the key inject
        // arbitrary JQL.
        let mock = server
            .mock("GET", "/rest/api/3/search/jql")
            .match_query(mockito::Matcher::AllOf(vec![mockito::Matcher::UrlEncoded(
                "jql".into(),
                "parent = \"DS-1\\\" OR 1=1 --\" ORDER BY key ASC".into(),
            )]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"issues": [], "isLast": true}"#)
            .create();

        let cfg = test_config(server.url());
        let children = children_of(&cfg, "DS-1\" OR 1=1 --").unwrap();

        mock.assert();
        assert!(children.is_empty());
    }
}
