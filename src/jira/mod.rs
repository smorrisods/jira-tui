//! Jira configuration and (optional) live REST client.
//!
//! Reads credentials from the environment or a `token.txt` file, and an
//! optional `~/.config/jira-tui/config.toml` for non-secret settings. When
//! `live` is disabled or credentials are missing, the app falls back to demo
//! data — the TUI is always explorable.

#[cfg(feature = "live")]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Config {
    pub base_url: String,
    pub email: String,
    pub token: String,
    pub project: String,
    /// Custom field ID for "Acceptance Criteria" (e.g. `customfield_10309`),
    /// if your Jira instance has one. This is instance-specific — every Jira
    /// site assigns its own numeric IDs to custom fields — so it's opt-in via
    /// `JIRA_ACCEPTANCE_CRITERIA_FIELD` or `acceptance_criteria_field` in
    /// `config.toml`. Left unset, acceptance criteria are simply not fetched.
    pub acceptance_criteria_field: Option<String>,
}

#[cfg(feature = "live")]
impl Config {
    /// Assemble config from env vars, an optional config file, and token.txt.
    /// Returns `None` when credentials are insufficient for live mode.
    pub fn load() -> Option<Config> {
        let file = crate::config::read_kv();

        // No baked-in default: every Jira site has a different hostname, so
        // live mode only activates once you've told us which one is yours.
        let base_url = std::env::var("JIRA_BASE_URL")
            .ok()
            .or_else(|| file.get("base_url").cloned())?;
        let email = std::env::var("JIRA_EMAIL")
            .ok()
            .or_else(|| file.get("email").cloned())?;
        // Only used when creating issues; leave unset if you don't create
        // issues from jira-tui or always want to be asked for a project key.
        let project = std::env::var("JIRA_PROJECT")
            .ok()
            .or_else(|| file.get("project").cloned())
            .unwrap_or_default();
        let acceptance_criteria_field = std::env::var("JIRA_ACCEPTANCE_CRITERIA_FIELD")
            .ok()
            .or_else(|| file.get("acceptance_criteria_field").cloned())
            .filter(|s| !s.trim().is_empty());

        let token = std::env::var("JIRA_API_TOKEN")
            .ok()
            .or_else(read_token_file)?;

        if token.trim().is_empty() || base_url.trim().is_empty() {
            return None;
        }

        Some(Config {
            base_url: base_url.trim_end_matches('/').to_string(),
            email,
            token: token.trim().to_string(),
            project,
            acceptance_criteria_field,
        })
    }

    pub fn site_host(&self) -> String {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string()
    }
}

#[cfg(feature = "live")]
fn read_token_file() -> Option<String> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(p) = crate::config::token_file_path() {
        candidates.push(p);
    }
    candidates.push(std::path::PathBuf::from("token.txt"));
    candidates.push(std::path::PathBuf::from("../jira-tasks/token.txt"));
    for candidate in candidates {
        if let Ok(s) = std::fs::read_to_string(&candidate) {
            let t = s.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

#[cfg(feature = "live")]
pub use live::{
    add_comment, apply_transition, create_issue, fetch_detail, fetch_my_work, fetch_transitions,
    search_issues, update_description, update_summary, whoami,
};

#[cfg(feature = "live")]
mod live {
    use super::Config;
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
}
