//! HTTP request primitives and shared response-parsing helpers used by
//! every other file in this module.

use std::io::Read;

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

/// Like `get`, but for a full absolute URL (not a `cfg.base_url`-relative
/// path) and returning raw bytes rather than parsed JSON — Jira's
/// attachment `content` URL is already absolute, and the payload is
/// arbitrary binary data, not JSON. Reuses the same auth header `get` does.
pub(super) fn get_bytes(cfg: &Config, url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .set("Authorization", &auth_header(cfg))
        .call()
        .map_err(|e| anyhow!("Jira request failed: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .context("reading attachment bytes")?;
    Ok(buf)
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

/// Multipart/form-data boundary for `post_multipart`. Fixed rather than
/// generated per call: every call sends exactly one part, so there's no risk
/// of the payload's own bytes colliding with it — that would require the
/// uploaded file to itself contain this exact line, which a boundary this
/// long makes vanishingly unlikely.
/// A fresh boundary per upload — reusing one fixed string across every
/// request would let a file whose own bytes happen to contain that exact
/// line truncate/corrupt the multipart body Jira receives. Nanosecond
/// timestamp plus a per-process counter (rather than a `rand` dependency,
/// which this repo otherwise has no use for) is enough entropy that a byte
/// sequence colliding with it in an uploaded file is not a realistic
/// concern.
fn multipart_boundary() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("--jira-tui-boundary-{nanos:x}-{n:x}")
}

/// Strip characters that would let a hostile filename break out of the
/// `Content-Disposition` header's quoted `filename="..."` value: `"` would
/// close the quote early, CR/LF would inject extra header lines into the
/// multipart body.
fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .filter(|c| !matches!(c, '"' | '\r' | '\n'))
        .collect()
}

/// Hand-build a multipart/form-data body containing a single `file` part.
/// Split out from `post_multipart` as a pure function so the framing
/// (escaping, CRLF terminators, boundary placement) is unit-testable without
/// a network round-trip.
fn build_multipart_body(boundary: &str, filename: &str, mime: &str, bytes: &[u8]) -> Vec<u8> {
    let safe_name = sanitize_filename(filename);
    let mut body = Vec::with_capacity(bytes.len() + 256);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{safe_name}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {mime}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

/// POST a file as a `multipart/form-data` body — used for Jira's
/// attachment-upload endpoint, the one write call that isn't plain JSON.
/// Hand-rolled rather than pulling in a multipart crate, matching this
/// file's existing style of hand-rolling small protocol bits (see
/// `url_encode` above).
pub(super) fn post_multipart(
    cfg: &Config,
    path: &str,
    filename: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<Value> {
    let url = format!("{}{}", cfg.base_url, path);
    let boundary = multipart_boundary();
    let body = build_multipart_body(&boundary, filename, mime, bytes);
    let resp = ureq::post(&url)
        .set("Authorization", &auth_header(cfg))
        .set("Accept", "application/json")
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        // Jira's XSRF/CSRF check blocks any state-changing REST call unless
        // it either carries a matching browser session cookie or explicitly
        // opts out with this header. A plain REST client never has the
        // cookie, so without this the upload gets bounced with a 403 before
        // it reaches the attachment handler at all — a well-known but easy
        // to miss Jira Cloud/Server trap for exactly this endpoint.
        .set("X-Atlassian-Token", "no-check")
        .send_bytes(&body)
        .map_err(|e| anyhow!("Jira attachment upload failed: {e}"))?;
    let value: Value = resp.into_json().context("decoding Jira JSON")?;
    Ok(value)
}

pub(super) fn delete(cfg: &Config, path: &str) -> Result<()> {
    let url = format!("{}{}", cfg.base_url, path);
    ureq::delete(&url)
        .set("Authorization", &auth_header(cfg))
        .set("Accept", "application/json")
        .call()
        .map_err(|e| anyhow!("Jira delete failed: {e}"))?;
    Ok(())
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

    #[test]
    fn multipart_body_escapes_a_quote_in_the_filename() {
        let body = build_multipart_body("BOUND", "evil\".txt", "text/plain", b"hi");
        let text = String::from_utf8_lossy(&body);
        // The quote is stripped, not merely escaped, so it can't close the
        // `filename="..."` value early and inject extra header syntax.
        assert!(text.contains("filename=\"evil.txt\""));
        assert!(!text.contains("evil\".txt"));
    }

    #[test]
    fn multipart_body_strips_cr_and_lf_from_the_filename() {
        let body =
            build_multipart_body("BOUND", "evil\r\nX-Injected: yes.txt", "text/plain", b"hi");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("filename=\"evilX-Injected: yes.txt\""));
        assert!(!text.contains("X-Injected: yes.txt\r\n\r\nContent-Type"));
    }

    #[test]
    fn multipart_boundary_is_unique_per_call() {
        // Regression: a single fixed boundary reused across every upload
        // could be truncated/corrupted by a file whose own bytes happen to
        // contain that exact line. Consecutive calls (even within the same
        // nanosecond, on a coarse-resolution clock) must never collide.
        let a = multipart_boundary();
        let b = multipart_boundary();
        assert_ne!(a, b);
    }

    #[test]
    fn multipart_body_has_the_expected_framing() {
        let body = build_multipart_body("BOUND", "report.txt", "text/plain", b"payload-bytes");
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with("--BOUND\r\n"));
        assert!(text.contains(
            "Content-Disposition: form-data; name=\"file\"; filename=\"report.txt\"\r\n"
        ));
        assert!(text.contains("Content-Type: text/plain\r\n\r\n"));
        assert!(text.contains("payload-bytes"));
        assert!(text.trim_end().ends_with("--BOUND--"));
    }
}
