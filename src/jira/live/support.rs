//! HTTP request primitives and shared response-parsing helpers used by
//! every other file in this module.

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value;

use super::super::config::Config;
use crate::domain::{IssueSummary, Priority};

pub(super) fn auth_header(cfg: &Config) -> String {
    let raw = format!("{}:{}", cfg.email, cfg.token);
    format!("Basic {}", STANDARD.encode(raw.as_bytes()))
}

pub(super) fn get(cfg: &Config, path: &str) -> Result<Value> {
    let url = format!("{}{}", cfg.base_url, path);
    let resp = ureq::get(&url)
        .set("Authorization", &auth_header(cfg))
        .set("Accept", "application/json")
        .call()
        .map_err(|e| anyhow!("Jira request failed: {e}"))?;
    let value: Value = resp.into_json().context("decoding Jira JSON")?;
    Ok(value)
}

pub(super) fn send(cfg: &Config, method: &str, path: &str, body: Value) -> Result<()> {
    post_or_put(cfg, method, path, body)?;
    Ok(())
}

/// Like `send`, but returns the decoded JSON response body (needed when the
/// caller wants to read back server-assigned fields, e.g. a new comment's id
/// and timestamp).
pub(super) fn post_or_put(cfg: &Config, method: &str, path: &str, body: Value) -> Result<Value> {
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

pub(super) fn url_encode(s: &str) -> String {
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

pub(super) fn priority_from(name: &str) -> Priority {
    match name {
        "Highest" => Priority::Highest,
        "High" => Priority::High,
        "Low" => Priority::Low,
        "Lowest" => Priority::Lowest,
        _ => Priority::Medium,
    }
}

pub(super) fn str_field(fields: &Value, path: &[&str]) -> Option<String> {
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

pub(super) fn summary_from(issue: &Value) -> IssueSummary {
    let key = issue
        .get("key")
        .and_then(|k| k.as_str())
        .unwrap_or("?")
        .to_string();
    let f = issue.get("fields").cloned().unwrap_or(Value::Null);
    let raw_updated = str_field(&f, &["updated"]);
    IssueSummary {
        key,
        summary: str_field(&f, &["summary"]).unwrap_or_default(),
        issue_type: str_field(&f, &["issuetype", "name"]).unwrap_or_else(|| "Task".into()),
        status: str_field(&f, &["status", "name"]).unwrap_or_else(|| "Unknown".into()),
        priority: priority_from(&str_field(&f, &["priority", "name"]).unwrap_or_default()),
        assignee: str_field(&f, &["assignee", "displayName"]),
        blocked: is_blocked(&f),
        updated: raw_updated
            .as_deref()
            .map(|s| s.chars().take(10).collect())
            .unwrap_or_default(),
        updated_at: raw_updated.as_deref().and_then(parse_jira_updated),
        // Used to group issues into board swimlanes. Usually an Epic, but
        // whatever Jira reports as the parent for issues without one.
        epic: str_field(&f, &["parent", "key"]),
    }
}

/// Parses Jira's `updated` timestamp shape (e.g.
/// `"2024-01-02T03:04:05.000+0000"`) into a UTC instant, for time-window
/// queries like `App::done_this_week` — the display string (`updated`,
/// above) stays a plain truncated date and is unaffected by parse failures.
fn parse_jira_updated(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.3f%z")
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

pub fn whoami(cfg: &Config) -> Result<String> {
    let me = get(cfg, "/rest/api/3/myself")?;
    Ok(me
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or("me")
        .to_string())
}

/// Shared by every `jira::live` test file (`search`, `mutations`,
/// `comments`, `fields`, `detail`, and this one) — `pub(super)` so any
/// sibling's own `#[cfg(test)] mod tests` can reach it via
/// `super::support::test_config`, instead of each file carrying its own copy.
#[cfg(test)]
pub(super) fn test_config(base_url: String) -> Config {
    Config {
        base_url,
        email: "me@example.com".into(),
        token: "secret-token".into(),
        project: "PROJ".into(),
        acceptance_criteria_field: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
