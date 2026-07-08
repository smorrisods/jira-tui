//! The live Jira REST client (`ureq`-based): reads, workflow transitions,
//! description/summary writes, comments, and issue creation.

use super::config::Config;
use crate::domain::{IssueDetail, IssueLink, IssueSummary, Priority};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value;

fn auth_header(cfg: &Config) -> String {
    let raw = format!("{}:{}", cfg.email, cfg.token);
    format!("Basic {}", STANDARD.encode(raw.as_bytes()))
}

fn get(cfg: &Config, path: &str) -> Result<Value> {
    let url = format!("{}{}", cfg.base_url, path);
    let resp = ureq::get(&url)
        .set("Authorization", &auth_header(cfg))
        .set("Accept", "application/json")
        .call()
        .map_err(|e| anyhow!("Jira request failed: {e}"))?;
    let value: Value = resp.into_json().context("decoding Jira JSON")?;
    Ok(value)
}

fn send(cfg: &Config, method: &str, path: &str, body: Value) -> Result<()> {
    let url = format!("{}{}", cfg.base_url, path);
    let req = match method {
        "POST" => ureq::post(&url),
        "PUT" => ureq::put(&url),
        other => return Err(anyhow!("unsupported method {other}")),
    };
    req.set("Authorization", &auth_header(cfg))
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| anyhow!("Jira write failed: {e}"))?;
    Ok(())
}

/// Fetch the workflow transitions available from the current status.
pub fn fetch_transitions(cfg: &Config, key: &str) -> Result<Vec<crate::domain::Transition>> {
    let data = get(cfg, &format!("/rest/api/3/issue/{key}/transitions"))?;
    let arr = data
        .get("transitions")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(arr
        .iter()
        .map(|t| {
            let name = t
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let to = t
                .get("to")
                .and_then(|to| to.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or(&name)
                .to_string();
            crate::domain::Transition {
                id: t
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                name,
                to,
            }
        })
        .collect())
}

/// Apply a transition by id.
pub fn apply_transition(cfg: &Config, key: &str, transition_id: &str) -> Result<()> {
    send(
        cfg,
        "POST",
        &format!("/rest/api/3/issue/{key}/transitions"),
        serde_json::json!({ "transition": { "id": transition_id } }),
    )
}

/// Replace an issue's ADF description.
pub fn update_description(cfg: &Config, key: &str, description: &Value) -> Result<()> {
    send(
        cfg,
        "PUT",
        &format!("/rest/api/3/issue/{key}"),
        serde_json::json!({ "fields": { "description": description } }),
    )
}

pub fn whoami(cfg: &Config) -> Result<String> {
    let me = get(cfg, "/rest/api/3/myself")?;
    Ok(me
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or("me")
        .to_string())
}

fn priority_from(name: &str) -> Priority {
    match name {
        "Highest" => Priority::Highest,
        "High" => Priority::High,
        "Low" => Priority::Low,
        "Lowest" => Priority::Lowest,
        _ => Priority::Medium,
    }
}

fn str_field(fields: &Value, path: &[&str]) -> Option<String> {
    let mut cur = fields;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().map(|s| s.to_string())
}

fn is_blocked(fields: &Value) -> bool {
    fields
        .get("issuelinks")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter().any(|link| {
                // An inward "is blocked by" link means this issue is blocked.
                link.get("inwardIssue").is_some()
                    && link
                        .get("type")
                        .and_then(|t| t.get("inward"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_lowercase().contains("block"))
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn summary_from(issue: &Value) -> IssueSummary {
    let key = issue
        .get("key")
        .and_then(|k| k.as_str())
        .unwrap_or("?")
        .to_string();
    let f = issue.get("fields").cloned().unwrap_or(Value::Null);
    IssueSummary {
        key,
        summary: str_field(&f, &["summary"]).unwrap_or_default(),
        issue_type: str_field(&f, &["issuetype", "name"]).unwrap_or_else(|| "Task".into()),
        status: str_field(&f, &["status", "name"]).unwrap_or_else(|| "Unknown".into()),
        priority: priority_from(&str_field(&f, &["priority", "name"]).unwrap_or_default()),
        assignee: str_field(&f, &["assignee", "displayName"]),
        blocked: is_blocked(&f),
        updated: str_field(&f, &["updated"])
            .map(|s| s.chars().take(10).collect())
            .unwrap_or_default(),
        // Used to group issues into board swimlanes. Usually an Epic, but
        // whatever Jira reports as the parent for issues without one.
        epic: str_field(&f, &["parent", "key"]),
    }
}

pub fn fetch_my_work(cfg: &Config) -> Result<Vec<IssueSummary>> {
    search_issues(
        cfg,
        "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC",
    )
}

/// Run an arbitrary JQL query and return matching issue summaries.
/// Used both for "my work" (a fixed JQL) and the MCP server's free-form
/// search tool.
pub fn search_issues(cfg: &Config, jql: &str) -> Result<Vec<IssueSummary>> {
    let encoded = url_encode(jql);
    // Enhanced JQL search endpoint (`/search/jql`); the classic `/search`
    // endpoint has been sunset on Jira Cloud.
    let path = format!(
        "/rest/api/3/search/jql?jql={encoded}&maxResults=50&fields=summary,status,issuetype,priority,assignee,updated,issuelinks,parent"
    );
    let data = get(cfg, &path)?;
    let issues = data
        .get("issues")
        .and_then(|i| i.as_array())
        .ok_or_else(|| anyhow!("no issues array in response"))?;
    Ok(issues.iter().map(summary_from).collect())
}

/// Create a new issue and return its key. `description` is a full ADF
/// document (build it with `crate::adf::compile` from Markdown).
pub fn create_issue(
    cfg: &Config,
    summary: &str,
    issue_type: &str,
    description: Option<&Value>,
) -> Result<String> {
    let mut fields = serde_json::json!({
        "project": { "key": cfg.project },
        "summary": summary,
        "issuetype": { "name": issue_type },
    });
    if let Some(desc) = description {
        fields["description"] = desc.clone();
    }
    let url = format!("{}/rest/api/3/issue", cfg.base_url);
    let resp = ureq::post(&url)
        .set("Authorization", &auth_header(cfg))
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({ "fields": fields }))
        .map_err(|e| anyhow!("Jira create failed: {e}"))?;
    let value: Value = resp.into_json().context("decoding Jira JSON")?;
    value
        .get("key")
        .and_then(|k| k.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Jira create response missing key"))
}

/// Update an issue's summary.
pub fn update_summary(cfg: &Config, key: &str, summary: &str) -> Result<()> {
    send(
        cfg,
        "PUT",
        &format!("/rest/api/3/issue/{key}"),
        serde_json::json!({ "fields": { "summary": summary } }),
    )
}

/// Add a comment. `body` is a full ADF document (build it with
/// `crate::adf::compile` from Markdown).
pub fn add_comment(cfg: &Config, key: &str, body: &Value) -> Result<()> {
    send(
        cfg,
        "POST",
        &format!("/rest/api/3/issue/{key}/comment"),
        serde_json::json!({ "body": body }),
    )
}

/// A Jira field's ID and human-readable name, as returned by
/// `GET /rest/api/3/field`.
#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub id: String,
    pub name: String,
}

/// List the site's custom fields (id + name), sorted by name. Used by
/// the field-mapping screen to let you find, e.g., "Acceptance Criteria"
/// without knowing its instance-specific `customfield_NNNNN` ID up front.
pub fn list_fields(cfg: &Config) -> Result<Vec<FieldInfo>> {
    let value = get(cfg, "/rest/api/3/field")?;
    let arr = value
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow!("unexpected /field response shape"))?;

    let mut fields: Vec<FieldInfo> = arr
        .into_iter()
        .filter_map(|f| {
            let id = f.get("id")?.as_str()?.to_string();
            // Built-in fields (summary, status, ...) are already handled
            // by name; only custom fields have instance-specific IDs
            // worth mapping here.
            if !id.starts_with("customfield_") {
                return None;
            }
            let name = f.get("name")?.as_str()?.to_string();
            Some(FieldInfo { id, name })
        })
        .collect();
    fields.sort_by_key(|a| a.name.to_lowercase());
    Ok(fields)
}

pub fn fetch_detail(cfg: &Config, key: &str) -> Result<IssueDetail> {
    let mut fields = "summary,status,issuetype,priority,assignee,reporter,labels,\
        components,parent,issuelinks,description"
        .to_string();
    if let Some(ac_field) = &cfg.acceptance_criteria_field {
        fields.push(',');
        fields.push_str(ac_field);
    }
    let path = format!("/rest/api/3/issue/{key}?fields={fields}");
    let issue = get(cfg, &path)?;
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
    let links = f
        .get("issuelinks")
        .and_then(|l| l.as_array())
        .map(|a| parse_links(a))
        .unwrap_or_default();

    Ok(IssueDetail {
        key: key.to_string(),
        summary: str_field(&f, &["summary"]).unwrap_or_default(),
        issue_type: str_field(&f, &["issuetype", "name"]).unwrap_or_else(|| "Task".into()),
        status: str_field(&f, &["status", "name"]).unwrap_or_else(|| "Unknown".into()),
        priority: priority_from(&str_field(&f, &["priority", "name"]).unwrap_or_default()),
        assignee: str_field(&f, &["assignee", "displayName"]),
        reporter: str_field(&f, &["reporter", "displayName"]),
        labels,
        components,
        parent: str_field(&f, &["parent", "key"]),
        links,
        description: f.get("description").cloned().unwrap_or(Value::Null),
        acceptance_criteria: cfg
            .acceptance_criteria_field
            .as_ref()
            .and_then(|field| f.get(field).cloned()),
        transitions: fetch_transitions(cfg, key).unwrap_or_default(),
    })
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

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(base_url: String) -> Config {
        Config {
            base_url,
            email: "me@example.com".into(),
            token: "secret-token".into(),
            project: "PROJ".into(),
            acceptance_criteria_field: None,
        }
    }

    #[test]
    fn whoami_returns_display_name() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/myself")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"displayName": "Ada Lovelace"}"#)
            .create();

        let cfg = test_config(server.url());
        let name = whoami(&cfg).unwrap();

        mock.assert();
        assert_eq!(name, "Ada Lovelace");
    }

    #[test]
    fn whoami_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rest/api/3/myself")
            .with_status(401)
            .create();

        let cfg = test_config(server.url());
        assert!(whoami(&cfg).is_err());
    }

    #[test]
    fn fetch_my_work_sends_the_expected_jql_and_parses_issues() {
        let mut server = mockito::Server::new();
        let expected_path = "/rest/api/3/search/jql?jql=assignee%20%3D%20currentUser%28%29%20AND%20statusCategory%20%21%3D%20Done%20ORDER%20BY%20updated%20DESC&maxResults=50&fields=summary,status,issuetype,priority,assignee,updated,issuelinks,parent";
        let mock = server
            .mock("GET", expected_path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "issues": [
                        {
                            "key": "DS-1",
                            "fields": {
                                "summary": "Fix the thing",
                                "issuetype": {"name": "Bug"},
                                "status": {"name": "In Progress"},
                                "priority": {"name": "Highest"},
                                "assignee": {"displayName": "Ada Lovelace"},
                                "updated": "2024-01-02T03:04:05.000+0000",
                                "parent": {"key": "DS-0"},
                                "issuelinks": [
                                    {
                                        "type": {"inward": "is blocked by"},
                                        "inwardIssue": {"key": "DS-9"}
                                    }
                                ]
                            }
                        }
                    ]
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let issues = fetch_my_work(&cfg).unwrap();

        mock.assert();
        assert_eq!(issues.len(), 1);
        let issue = &issues[0];
        assert_eq!(issue.key, "DS-1");
        assert_eq!(issue.summary, "Fix the thing");
        assert_eq!(issue.issue_type, "Bug");
        assert_eq!(issue.status, "In Progress");
        assert_eq!(issue.priority, Priority::Highest);
        assert_eq!(issue.assignee.as_deref(), Some("Ada Lovelace"));
        assert_eq!(issue.updated, "2024-01-02");
        assert_eq!(issue.epic.as_deref(), Some("DS-0"));
        assert!(
            issue.blocked,
            "an inward 'is blocked by' link must set blocked"
        );
    }

    #[test]
    fn search_issues_errors_when_response_has_no_issues_array() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"unexpected": true}"#)
            .create();

        let cfg = test_config(server.url());
        assert!(search_issues(&cfg, "any jql").is_err());
    }

    #[test]
    fn list_fields_filters_to_custom_fields_and_sorts_by_name() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/field")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {"id": "summary", "name": "Summary"},
                    {"id": "customfield_10002", "name": "Story Points"},
                    {"id": "customfield_10001", "name": "Acceptance Criteria"}
                ]"#,
            )
            .create();

        let cfg = test_config(server.url());
        let fields = list_fields(&cfg).unwrap();

        mock.assert();
        // Built-in fields (no `customfield_` prefix) are excluded, and the
        // rest are sorted by name, not by id or response order.
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "Acceptance Criteria");
        assert_eq!(fields[0].id, "customfield_10001");
        assert_eq!(fields[1].name, "Story Points");
    }

    #[test]
    fn fetch_detail_includes_the_configured_acceptance_criteria_field() {
        let mut server = mockito::Server::new();
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,parent,issuelinks,description,customfield_10001",
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
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,parent,issuelinks,description",
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
    fn apply_transition_sends_the_transition_id() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/rest/api/3/issue/DS-1/transitions")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "transition": { "id": "31" }
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        apply_transition(&cfg, "DS-1", "31").unwrap();

        mock.assert();
    }

    #[test]
    fn create_issue_sends_project_summary_type_and_returns_key() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/rest/api/3/issue")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "fields": {
                    "project": { "key": "PROJ" },
                    "summary": "New issue",
                    "issuetype": { "name": "Task" }
                }
            })))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"key": "PROJ-42"}"#)
            .create();

        let cfg = test_config(server.url());
        let key = create_issue(&cfg, "New issue", "Task", None).unwrap();

        mock.assert();
        assert_eq!(key, "PROJ-42");
    }

    #[test]
    fn add_comment_sends_the_adf_body() {
        let mut server = mockito::Server::new();
        let body = serde_json::json!({"type": "doc", "content": []});
        let mock = server
            .mock("POST", "/rest/api/3/issue/DS-1/comment")
            .match_body(mockito::Matcher::Json(serde_json::json!({ "body": body })))
            .with_status(201)
            .create();

        let cfg = test_config(server.url());
        add_comment(&cfg, "DS-1", &body).unwrap();

        mock.assert();
    }

    #[test]
    fn update_summary_and_description_send_expected_bodies() {
        let mut server = mockito::Server::new();
        let summary_mock = server
            .mock("PUT", "/rest/api/3/issue/DS-1")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "fields": { "summary": "New summary" }
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        update_summary(&cfg, "DS-1", "New summary").unwrap();
        summary_mock.assert();

        server.reset();
        let desc = serde_json::json!({"type": "doc", "content": []});
        let desc_mock = server
            .mock("PUT", "/rest/api/3/issue/DS-1")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "fields": { "description": desc }
            })))
            .with_status(204)
            .create();
        update_description(&cfg, "DS-1", &desc).unwrap();
        desc_mock.assert();
    }
}
