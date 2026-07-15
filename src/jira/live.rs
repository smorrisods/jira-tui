//! The live Jira REST client (`ureq`-based): reads, workflow transitions,
//! description/summary writes, comments, and issue creation.

use super::config::Config;
use crate::domain::{AssignableUser, ChildIssue, IssueDetail, IssueLink, IssueSummary, Priority};
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
    post_or_put(cfg, method, path, body)?;
    Ok(())
}

/// Like `send`, but returns the decoded JSON response body (needed when the
/// caller wants to read back server-assigned fields, e.g. a new comment's id
/// and timestamp).
fn post_or_put(cfg: &Config, method: &str, path: &str, body: Value) -> Result<Value> {
    let url = format!("{}{}", cfg.base_url, path);
    let req = match method {
        "POST" => ureq::post(&url),
        "PUT" => ureq::put(&url),
        other => return Err(anyhow!("unsupported method {other}")),
    };
    let resp = req
        .set("Authorization", &auth_header(cfg))
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| anyhow!("Jira write failed: {e}"))?;
    // PUT responses (e.g. update_description) are often empty bodies; treat
    // decode failure as "no useful body" rather than an error.
    Ok(resp.into_json().unwrap_or(Value::Null))
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

pub fn whoami(cfg: &Config) -> Result<String> {
    let me = get(cfg, "/rest/api/3/myself")?;
    Ok(me
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or("me")
        .to_string())
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

/// The fixed JQL behind "my work" — exposed as a constant (not just inline
/// in `fetch_my_work`) so the cache layer can record exactly which query
/// produced a cached view, without duplicating the string.
pub const MY_WORK_JQL: &str =
    "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";

pub fn fetch_my_work(cfg: &Config) -> Result<Vec<IssueSummary>> {
    search_issues(cfg, MY_WORK_JQL)
}

/// Build the JQL for a given view. `project` is the Jira project key
/// (`Config.project`, already used for issue creation).
pub fn jql_for(view: &crate::domain::ViewKind, project: &str) -> String {
    use crate::domain::ViewKind;
    match view {
        ViewKind::MyWork => MY_WORK_JQL.to_string(),
        ViewKind::AllProject => format!("project = \"{project}\" ORDER BY updated DESC"),
        ViewKind::Teammate(name) => {
            // Escape backslashes *before* quotes — a JQL string literal
            // uses `\` as its escape character, so a name ending in `\`
            // would otherwise absorb the closing quote as an escaped
            // character instead of terminating the string.
            let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
            format!("assignee = \"{escaped}\" AND statusCategory != Done ORDER BY updated DESC")
        }
    }
}

/// Run an arbitrary JQL query and return matching issue summaries, paging
/// through the enhanced JQL search endpoint's `nextPageToken` until Jira
/// reports `isLast`, or until `SEARCH_RESULTS_CAP` is hit — whichever comes
/// first. Used both for "my work"/the view switcher (a handful of fixed JQL
/// variants) and the MCP server's free-form search tool.
///
/// A single page used to be the whole story (`maxResults=50`, no paging),
/// which meant `AllProject`/large-project views silently truncated at the
/// 50 most-recently-updated issues — including the teammate picker, which
/// is seeded from whatever's currently loaded. `SEARCH_RESULTS_CAP` still
/// bounds the total so a very large project can't page forever.
pub fn search_issues(cfg: &Config, jql: &str) -> Result<Vec<IssueSummary>> {
    let mut all = Vec::new();
    let mut next_page_token: Option<String> = None;

    loop {
        let encoded = url_encode(jql);
        // Enhanced JQL search endpoint (`/search/jql`); the classic
        // `/search` endpoint has been sunset on Jira Cloud.
        let mut path = format!(
            "/rest/api/3/search/jql?jql={encoded}&maxResults={SEARCH_PAGE_SIZE}&fields=summary,status,issuetype,priority,assignee,updated,issuelinks,parent"
        );
        if let Some(token) = &next_page_token {
            path.push_str(&format!("&nextPageToken={}", url_encode(token)));
        }

        let data = get(cfg, &path)?;
        let issues = data
            .get("issues")
            .and_then(|i| i.as_array())
            .ok_or_else(|| anyhow!("no issues array in response"))?;
        all.extend(issues.iter().map(summary_from));

        // Missing `isLast` (e.g. an older mock/response) is treated as "this
        // was the only page" rather than looping forever.
        let is_last = data.get("isLast").and_then(|v| v.as_bool()).unwrap_or(true);
        if is_last || all.len() >= SEARCH_RESULTS_CAP {
            break;
        }
        let Some(token) = data
            .get("nextPageToken")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            // Jira said there's more, but didn't give us a token to fetch
            // it with — stop rather than loop on the same page forever.
            break;
        };
        next_page_token = Some(token);
    }

    Ok(all)
}

/// Results per page for `search_issues`'s paging loop.
const SEARCH_PAGE_SIZE: usize = 50;

/// The most `search_issues` will ever return across every page, so a very
/// large project can't page indefinitely. Exposed so callers (the view
/// loader) can tell a genuinely truncated result apart from "that's
/// everything".
pub const SEARCH_RESULTS_CAP: usize = 500;

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
/// `crate::adf::compile` from Markdown). Returns the created `Comment` as
/// reported by Jira (id, author, created timestamp) so callers can append it
/// locally without a second round trip.
pub fn add_comment(cfg: &Config, key: &str, body: &Value) -> Result<crate::domain::Comment> {
    let resp = post_or_put(
        cfg,
        "POST",
        &format!("/rest/api/3/issue/{key}/comment"),
        serde_json::json!({ "body": body }),
    )?;
    Ok(parse_comment(&resp))
}

/// Fetch all comments for an issue, oldest first, paging through the
/// `/comment` endpoint (Jira returns at most `maxResults` per page).
pub fn fetch_comments(cfg: &Config, key: &str) -> Result<Vec<crate::domain::Comment>> {
    let mut out = Vec::new();
    let mut start_at: u64 = 0;
    const PAGE_SIZE: u64 = 100;
    loop {
        let path = format!(
            "/rest/api/3/issue/{key}/comment?startAt={start_at}&maxResults={PAGE_SIZE}&orderBy=created"
        );
        let page = get(cfg, &path)?;
        let comments = page
            .get("comments")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        let got = comments.len() as u64;
        out.extend(comments.iter().map(parse_comment));

        let total = page.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        start_at += got;
        if got == 0 || start_at >= total {
            break;
        }
    }
    Ok(out)
}

fn parse_comment(v: &Value) -> crate::domain::Comment {
    crate::domain::Comment {
        id: v
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        author: str_field(v, &["author", "displayName"]).unwrap_or_else(|| "Unknown".into()),
        created: str_field(v, &["created"]).unwrap_or_default(),
        body: v.get("body").cloned().unwrap_or(Value::Null),
    }
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
        components,parent,issuelinks,description,subtasks"
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
        parent: str_field(&f, &["parent", "key"]),
        links,
        children,
        description: f.get("description").cloned().unwrap_or(Value::Null),
        acceptance_criteria: cfg
            .acceptance_criteria_field
            .as_ref()
            .and_then(|field| f.get(field).cloned()),
        transitions: fetch_transitions(cfg, key).unwrap_or_default(),
        comments: fetch_comments(cfg, key).unwrap_or_default(),
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
fn children_of(cfg: &Config, key: &str) -> Result<Vec<ChildIssue>> {
    // Same escaping as `jql_for`'s Teammate arm — issue keys shouldn't
    // contain quotes, but a JQL string literal is still a string literal.
    let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
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
    fn jql_for_builds_the_expected_query_per_view() {
        use crate::domain::ViewKind;

        assert_eq!(jql_for(&ViewKind::MyWork, "PROJ"), MY_WORK_JQL);
        assert_eq!(
            jql_for(&ViewKind::AllProject, "PROJ"),
            "project = \"PROJ\" ORDER BY updated DESC"
        );
        assert_eq!(
            jql_for(&ViewKind::Teammate("Ada Lovelace".into()), "PROJ"),
            "assignee = \"Ada Lovelace\" AND statusCategory != Done ORDER BY updated DESC"
        );
    }

    #[test]
    fn jql_for_teammate_escapes_embedded_quotes() {
        use crate::domain::ViewKind;

        let jql = jql_for(&ViewKind::Teammate("Robert \"Bob\" Smith".into()), "PROJ");
        assert_eq!(
            jql,
            "assignee = \"Robert \\\"Bob\\\" Smith\" AND statusCategory != Done ORDER BY updated DESC"
        );
    }

    #[test]
    fn jql_for_teammate_escapes_backslashes_before_quotes() {
        use crate::domain::ViewKind;

        // A trailing backslash must be escaped to `\\` *before* the closing
        // quote is appended, or the parser reads `\"` as an escaped quote
        // rather than the string's terminator.
        let jql = jql_for(&ViewKind::Teammate("Robert\\".into()), "PROJ");
        assert_eq!(
            jql,
            "assignee = \"Robert\\\\\" AND statusCategory != Done ORDER BY updated DESC"
        );

        let jql = jql_for(&ViewKind::Teammate("Back\\slash \"Quote\"".into()), "PROJ");
        assert_eq!(
            jql,
            "assignee = \"Back\\\\slash \\\"Quote\\\"\" AND statusCategory != Done ORDER BY updated DESC"
        );
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
    fn search_issues_pages_through_next_page_token_until_is_last() {
        let mut server = mockito::Server::new();
        let base_path = "/rest/api/3/search/jql?jql=any%20jql&maxResults=50&fields=summary,status,issuetype,priority,assignee,updated,issuelinks,parent";
        // First page: no `nextPageToken` yet, isLast=false, hands back a
        // token for the second page.
        let first = server
            .mock("GET", base_path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"issues": [{"key": "DS-1"}], "isLast": false, "nextPageToken": "page-2"}"#,
            )
            .create();
        // Second page: the token must be echoed back as a query param;
        // isLast=true stops the loop.
        let second_path = format!("{base_path}&nextPageToken=page-2");
        let second = server
            .mock("GET", second_path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"issues": [{"key": "DS-2"}], "isLast": true}"#)
            .create();

        let cfg = test_config(server.url());
        let issues = search_issues(&cfg, "any jql").unwrap();

        first.assert();
        second.assert();
        assert_eq!(
            issues.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
            vec!["DS-1", "DS-2"],
            "both pages' issues should be concatenated, in order"
        );
    }

    #[test]
    fn search_issues_stops_at_the_results_cap_even_if_jira_says_theres_more() {
        let mut server = mockito::Server::new();
        // Always claims there's more (isLast=false, same token forever) —
        // the loop must still stop once SEARCH_RESULTS_CAP is reached,
        // rather than paging indefinitely against a misbehaving server.
        let page: String = (0..SEARCH_PAGE_SIZE)
            .map(|i| format!(r#"{{"key": "DS-{i}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let body =
            format!(r#"{{"issues": [{page}], "isLast": false, "nextPageToken": "same-token"}}"#);
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .expect(SEARCH_RESULTS_CAP / SEARCH_PAGE_SIZE)
            .create();

        let cfg = test_config(server.url());
        let issues = search_issues(&cfg, "any jql").unwrap();

        mock.assert();
        assert_eq!(issues.len(), SEARCH_RESULTS_CAP);
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
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,parent,issuelinks,description,subtasks,customfield_10001",
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
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,parent,issuelinks,description,subtasks",
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
    fn fetch_detail_parses_inline_subtasks_for_non_epic_issues() {
        let mut server = mockito::Server::new();
        let issue_mock = server
            .mock(
                "GET",
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,parent,issuelinks,description,subtasks",
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
                "/rest/api/3/issue/DS-1?fields=summary,status,issuetype,priority,assignee,reporter,labels,components,parent,issuelinks,description,subtasks",
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

    #[test]
    fn add_comment_sends_the_adf_body_and_parses_the_response() {
        let mut server = mockito::Server::new();
        let body = serde_json::json!({"type": "doc", "content": []});
        let mock = server
            .mock("POST", "/rest/api/3/issue/DS-1/comment")
            .match_body(mockito::Matcher::Json(serde_json::json!({ "body": body })))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "id": "10042",
                    "author": { "displayName": "Ada Lovelace" },
                    "created": "2026-07-08T09:00:00.000-0400",
                    "body": {"type": "doc", "content": []}
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let comment = add_comment(&cfg, "DS-1", &body).unwrap();

        mock.assert();
        assert_eq!(comment.id, "10042");
        assert_eq!(comment.author, "Ada Lovelace");
        assert_eq!(comment.created, "2026-07-08T09:00:00.000-0400");
    }

    #[test]
    fn fetch_comments_returns_a_single_page() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/issue/DS-1/comment")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("startAt".into(), "0".into()),
                mockito::Matcher::UrlEncoded("maxResults".into(), "100".into()),
                mockito::Matcher::UrlEncoded("orderBy".into(), "created".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "startAt": 0,
                    "maxResults": 100,
                    "total": 2,
                    "comments": [
                        {"id": "1", "author": {"displayName": "Ada"}, "created": "t1",
                         "body": {"type": "doc", "content": []}},
                        {"id": "2", "author": {"displayName": "Grace"}, "created": "t2",
                         "body": {"type": "doc", "content": []}}
                    ]
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let comments = fetch_comments(&cfg, "DS-1").unwrap();

        mock.assert();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].author, "Ada");
        assert_eq!(comments[1].author, "Grace");
    }

    #[test]
    fn fetch_comments_pages_through_all_results() {
        let mut server = mockito::Server::new();
        let page1 = server
            .mock("GET", "/rest/api/3/issue/DS-1/comment")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("startAt".into(), "0".into()),
                mockito::Matcher::UrlEncoded("maxResults".into(), "100".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "startAt": 0,
                    "maxResults": 100,
                    "total": 2,
                    "comments": [
                        {"id": "1", "author": {"displayName": "Ada"}, "created": "t1",
                         "body": {"type": "doc", "content": []}}
                    ]
                }"#,
            )
            .create();
        // The first page only returned one of two total comments (as if
        // `maxResults` had been hit), so `fetch_comments` should request a
        // second page starting where the first left off.
        let page2 = server
            .mock("GET", "/rest/api/3/issue/DS-1/comment")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("startAt".into(), "1".into()),
                mockito::Matcher::UrlEncoded("maxResults".into(), "100".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "startAt": 1,
                    "maxResults": 100,
                    "total": 2,
                    "comments": [
                        {"id": "2", "author": {"displayName": "Grace"}, "created": "t2",
                         "body": {"type": "doc", "content": []}}
                    ]
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let comments = fetch_comments(&cfg, "DS-1").unwrap();

        page1.assert();
        page2.assert();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].id, "1");
        assert_eq!(comments[1].id, "2");
    }

    #[test]
    fn fetch_comments_returns_empty_when_there_are_none() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/issue/DS-1/comment")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"startAt": 0, "maxResults": 100, "total": 0, "comments": []}"#)
            .create();

        let cfg = test_config(server.url());
        let comments = fetch_comments(&cfg, "DS-1").unwrap();

        mock.assert();
        assert!(comments.is_empty());
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
