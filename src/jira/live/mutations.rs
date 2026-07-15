//! Write-only endpoints with no rich return type beyond success/failure (or,
//! for `create_issue`, the new key): transitions, assignment, and
//! description/summary updates.

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use super::super::config::Config;
use super::support::{auth_header, get, send, url_encode};
use crate::domain::AssignableUser;

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

/// Update an issue's summary.
pub fn update_summary(cfg: &Config, key: &str, summary: &str) -> Result<()> {
    send(
        cfg,
        "PUT",
        &format!("/rest/api/3/issue/{key}"),
        serde_json::json!({ "fields": { "summary": summary } }),
    )
}

/// Every user assignable to issues in `project` — used to discover
/// teammates for the view picker and to populate the assignee picker
/// (`A`), without touching any issue data at all (`GET
/// /rest/api/3/user/assignable/search`). Far cheaper than deriving
/// assignees from a full "All Project Issues" search: no issue payloads to
/// page through, and most projects have far fewer members than issues.
/// The endpoint returns a flat JSON array (no `total`/`isLast`), so a page
/// shorter than `PAGE_SIZE` signals the last page. Carries `accountId`
/// alongside the display name — the view picker only ever needed the name,
/// but assigning an issue (`assign_issue`) requires the account id, so this
/// is the one place both are captured together.
pub fn assignable_users(cfg: &Config, project: &str) -> Result<Vec<AssignableUser>> {
    let project = url_encode(project);
    let mut out = Vec::new();
    let mut start_at: u64 = 0;
    const PAGE_SIZE: u64 = 100;
    loop {
        let path = format!(
            "/rest/api/3/user/assignable/search?project={project}&startAt={start_at}&maxResults={PAGE_SIZE}"
        );
        let page = get(cfg, &path)?;
        let users = page.as_array().cloned().unwrap_or_default();
        let got = users.len() as u64;
        out.extend(users.iter().filter_map(|u| {
            let account_id = u.get("accountId").and_then(|v| v.as_str())?.to_string();
            let display_name = u.get("displayName").and_then(|v| v.as_str())?.to_string();
            Some(AssignableUser {
                account_id,
                display_name,
            })
        }));
        if got < PAGE_SIZE {
            break;
        }
        start_at += got;
    }
    Ok(out)
}

/// Assign (or, with `account_id: None`, unassign) an issue
/// (`PUT /issue/{key}/assignee`). Jira accepts `null` for `accountId` to
/// clear the assignee entirely.
pub fn assign_issue(cfg: &Config, key: &str, account_id: Option<&str>) -> Result<()> {
    send(
        cfg,
        "PUT",
        &format!("/rest/api/3/issue/{key}/assignee"),
        serde_json::json!({ "accountId": account_id }),
    )
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

#[cfg(test)]
mod tests {
    use super::super::support::test_config;
    use super::*;
    use crate::domain::AssignableUser;

    #[test]
    fn assignable_users_returns_display_names_from_a_single_page() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/user/assignable/search")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("project".into(), "PROJ".into()),
                mockito::Matcher::UrlEncoded("startAt".into(), "0".into()),
                mockito::Matcher::UrlEncoded("maxResults".into(), "100".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {"accountId": "1", "displayName": "Priya Nair"},
                    {"accountId": "2", "displayName": "Alex Chen"}
                ]"#,
            )
            .create();

        let cfg = test_config(server.url());
        let users = assignable_users(&cfg, "PROJ").unwrap();

        mock.assert();
        assert_eq!(
            users,
            vec![
                AssignableUser {
                    account_id: "1".into(),
                    display_name: "Priya Nair".into()
                },
                AssignableUser {
                    account_id: "2".into(),
                    display_name: "Alex Chen".into()
                },
            ]
        );
    }

    #[test]
    fn assignable_users_pages_until_a_short_page() {
        let mut server = mockito::Server::new();
        // A full page (== PAGE_SIZE, 100 users) means there might be more.
        let full_page: String = (0..100)
            .map(|i| format!(r#"{{"accountId": "{i}", "displayName": "User {i}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let first = server
            .mock("GET", "/rest/api/3/user/assignable/search")
            .match_query(mockito::Matcher::AllOf(vec![mockito::Matcher::UrlEncoded(
                "startAt".into(),
                "0".into(),
            )]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!("[{full_page}]"))
            .create();
        let second = server
            .mock("GET", "/rest/api/3/user/assignable/search")
            .match_query(mockito::Matcher::AllOf(vec![mockito::Matcher::UrlEncoded(
                "startAt".into(),
                "100".into(),
            )]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"accountId": "100", "displayName": "User 100"}]"#)
            .create();

        let cfg = test_config(server.url());
        let users = assignable_users(&cfg, "PROJ").unwrap();

        first.assert();
        second.assert();
        assert_eq!(users.len(), 101);
        assert_eq!(users[100].display_name, "User 100");
        assert_eq!(users[100].account_id, "100");
    }

    #[test]
    fn assignable_users_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rest/api/3/user/assignable/search")
            .with_status(401)
            .create();

        let cfg = test_config(server.url());
        assert!(assignable_users(&cfg, "PROJ").is_err());
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
    fn assign_issue_sends_the_account_id() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("PUT", "/rest/api/3/issue/DS-1/assignee")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "accountId": "abc123"
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        assign_issue(&cfg, "DS-1", Some("abc123")).unwrap();

        mock.assert();
    }

    #[test]
    fn assign_issue_with_none_unassigns() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("PUT", "/rest/api/3/issue/DS-1/assignee")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "accountId": null
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        assign_issue(&cfg, "DS-1", None).unwrap();

        mock.assert();
    }

    #[test]
    fn assign_issue_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("PUT", "/rest/api/3/issue/DS-1/assignee")
            .with_status(403)
            .create();

        let cfg = test_config(server.url());
        assert!(assign_issue(&cfg, "DS-1", Some("abc123")).is_err());
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

    // Coverage gap noticed while splitting this file: create_issue and
    // update_summary/update_description each only had a happy-path test —
    // unlike assign_issue/fetch_comments/search_issues, which each also
    // exercise their error path.

    #[test]
    fn create_issue_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/rest/api/3/issue")
            .with_status(400)
            .create();

        let cfg = test_config(server.url());
        assert!(create_issue(&cfg, "New issue", "Task", None).is_err());
    }

    #[test]
    fn create_issue_errors_when_response_is_missing_a_key() {
        // Exercises the `ok_or_else` branch: a 2xx response that Jira
        // somehow returns without a `key` field must still be treated as a
        // failure, not silently produce an empty/garbage issue key.
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/rest/api/3/issue")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id": "10042"}"#)
            .create();

        let cfg = test_config(server.url());
        assert!(create_issue(&cfg, "New issue", "Task", None).is_err());
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

    #[test]
    fn update_summary_and_description_surface_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("PUT", "/rest/api/3/issue/DS-1")
            .with_status(500)
            .create();

        let cfg = test_config(server.url());
        assert!(update_summary(&cfg, "DS-1", "New summary").is_err());

        let desc = serde_json::json!({"type": "doc", "content": []});
        assert!(update_description(&cfg, "DS-1", &desc).is_err());
    }
}
