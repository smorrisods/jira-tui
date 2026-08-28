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
/// attachment `content`/`thumbnail` URLs are already absolute, and the
/// payload is arbitrary binary data, not JSON. Reuses the same auth header
/// `get` does — but only after confirming `url` is actually on the
/// configured Jira instance (a code-review finding): this is reachable not
/// just from an explicit `d` download, but from merely opening the
/// attachment picker or moving its selection (see
/// `app::attachments::refresh_attachment_preview`), so a compromised or
/// MITM'd Jira API response pointing an attachment's `thumbnail`/`content`
/// field at an attacker-controlled host must not get the user's Basic-auth
/// credential handed to it just from browsing.
pub(super) fn get_bytes(cfg: &Config, url: &str) -> Result<Vec<u8>> {
    if origin(url) != origin(&cfg.base_url) {
        return Err(anyhow!(
            "refusing to send Jira credentials to {url}: it isn't the configured Jira instance ({})",
            cfg.base_url
        ));
    }
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

/// Extracts `scheme://host[:port]` from `url` — e.g.
/// `"https://example.atlassian.net/secure/attachment/1"` ->
/// `"https://example.atlassian.net"`. `None` if `url` has no `://` at all.
/// Used by `get_bytes` to compare a candidate URL's origin against
/// `cfg.base_url`'s by exact equality, never by prefix: a plain
/// `url.starts_with(base_url)` check would be fooled by a hostname that
/// merely starts with the real one (`https://example.atlassian.net.evil.com/...`
/// is a `str::starts_with` match for `https://example.atlassian.net` but a
/// completely different host). Hand-rolled rather than pulling in the `url`
/// crate — not otherwise a dependency of this codebase — matching
/// `get_bytes_public`'s own hand-rolled scheme check above.
fn origin(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&url[..scheme.len() + 3 + end])
}

/// Cap on how many bytes `get_bytes_public` will read from an externally
/// hosted image before giving up. `get_bytes`/`get_bytes_public`'s Jira
/// counterpart above has no such cap — it only ever talks to Jira's own
/// trusted API/attachment endpoints — but a `get_bytes_public` request can
/// land on any host an attacker chooses to put in a description's inline
/// image URL, so an unbounded read is a real resource-exhaustion risk. 20MB
/// is generous for what's meant to be a lightweight inline preview image,
/// not a hard technical limit copied from elsewhere in this codebase (there
/// is no existing convention for a byte cap to match).
const MAX_PUBLIC_BYTES: u64 = 20 * 1024 * 1024;

/// Like `get_bytes`, but for an externally hosted URL rather than one of
/// Jira's own API/attachment endpoints (an ADF `media` node's `type:
/// "external"` `url`, embedded in an issue description). Deliberately
/// attaches no `Authorization` header at all — no `Config` is even taken as
/// a parameter, since there's nothing to authenticate an arbitrary
/// third-party host with, and reusing `get_bytes`' Basic-auth header the way
/// the attachment-download path does would leak the user's Jira credential
/// to that third party.
///
/// Restricted to `https://` — any other scheme (bare `http://` in
/// particular) is rejected before a request is ever attempted, so a hostile
/// inline-image URL embedded in a description can't make this app reach
/// into an internal/local-network `http://` host (basic SSRF hardening, not
/// just a style choice). Redirects are disabled outright
/// (`AgentBuilder::redirects(0)`) rather than merely re-checked per hop:
/// `ureq` gives no hook to inspect a redirect's target scheme before
/// following it, so the only way to rule out an `https -> http` downgrade
/// that would bypass this same scheme check is to never follow a redirect
/// at all — any 3xx response is treated as a failure, same as any other
/// non-2xx status.
pub fn get_bytes_public(url: &str) -> Result<Vec<u8>> {
    let scheme_is_https = url
        .split_once("://")
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("https"));
    if !scheme_is_https {
        return Err(anyhow!("refusing to fetch a non-https URL: {url}"));
    }
    fetch_public_bytes(url)
}

/// The redirect-disabled agent every `fetch_public_bytes` call reuses,
/// built once rather than per call — an external description may embed
/// several inline images (often on the same host/CDN), and a fresh
/// `ureq::Agent` per fetch would pay a new connection-pool/TLS setup for
/// each one instead of reusing keep-alive connections the way `get`/
/// `get_bytes` already do via `ureq`'s own default agent.
fn public_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| ureq::AgentBuilder::new().redirects(0).build())
}

/// The request/response half of `get_bytes_public`, split out so tests can
/// exercise it directly against a plain-`http` `mockito` server without
/// weakening `get_bytes_public`'s own `https://`-only gate to accommodate
/// the test — `mockito` has no TLS support, so there is no way to drive a
/// real request through that gate.
fn fetch_public_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = public_agent()
        .get(url)
        .call()
        .map_err(|e| anyhow!("public image fetch failed: {e}"))?;
    if resp.status() >= 300 {
        return Err(anyhow!(
            "public image fetch was redirected (status {}); refusing to follow it",
            resp.status()
        ));
    }
    let mut buf = Vec::new();
    resp.into_reader()
        .take(MAX_PUBLIC_BYTES + 1)
        .read_to_end(&mut buf)
        .context("reading public image bytes")?;
    if buf.len() as u64 > MAX_PUBLIC_BYTES {
        return Err(anyhow!(
            "public image exceeds the {MAX_PUBLIC_BYTES}-byte cap"
        ));
    }
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
        sprint_field: None,
        sprint_board_id: None,
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
    fn origin_extracts_scheme_and_host_up_to_the_first_path_query_or_fragment() {
        assert_eq!(
            origin("https://example.atlassian.net/secure/attachment/1"),
            Some("https://example.atlassian.net")
        );
        assert_eq!(
            origin("https://example.atlassian.net?x=1"),
            Some("https://example.atlassian.net")
        );
        assert_eq!(
            origin("https://example.atlassian.net#frag"),
            Some("https://example.atlassian.net")
        );
        assert_eq!(
            origin("https://example.atlassian.net"),
            Some("https://example.atlassian.net")
        );
        assert_eq!(origin("not-a-url"), None);
    }

    #[test]
    fn get_bytes_refuses_a_url_on_a_different_host_than_the_configured_jira_instance() {
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", mockito::Matcher::Any).expect(0).create();
        let cfg = test_config(server.url());

        assert!(get_bytes(&cfg, "http://evil.example.com/secure/attachment/1").is_err());

        mock.assert();
    }

    #[test]
    fn get_bytes_refuses_a_hostname_that_merely_starts_with_the_configured_origin() {
        // Regression guard for a `str::starts_with`-based check, which this
        // exact shape would bypass: `cfg.base_url` (e.g.
        // `http://127.0.0.1:PORT`) is a literal string prefix of
        // `http://127.0.0.1:PORT.evil.com/...`, even though they're
        // completely different hosts.
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", mockito::Matcher::Any).expect(0).create();
        let cfg = test_config(server.url());

        let attacker_url = format!("{}.evil.com/secure/attachment/1", cfg.base_url);
        assert!(get_bytes(&cfg, &attacker_url).is_err());

        mock.assert();
    }

    #[test]
    fn fetch_public_bytes_sends_no_authorization_header() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/image.png")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(b"public-image-bytes".as_slice())
            .create();

        let url = format!("{}/image.png", server.url());
        let bytes = fetch_public_bytes(&url).unwrap();

        mock.assert();
        assert_eq!(bytes, b"public-image-bytes");
    }

    #[test]
    fn get_bytes_public_rejects_a_non_https_url_without_attempting_a_request() {
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", "/image.png").expect(0).create();

        // `server.url()` is a plain `http://` URL, which is exactly the
        // scheme this must reject before ever reaching the network.
        let url = format!("{}/image.png", server.url());
        assert!(get_bytes_public(&url).is_err());

        mock.assert();
    }

    #[test]
    fn fetch_public_bytes_refuses_to_follow_a_redirect() {
        // Regression guard for an `https -> http` scheme-downgrade bypass:
        // even a same-scheme redirect must never be followed, since `ureq`
        // gives this code no way to re-check a redirect's target scheme
        // before following it.
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/redirect.png")
            .with_status(302)
            .with_header("location", "/final.png")
            .create();
        let final_mock = server.mock("GET", "/final.png").expect(0).create();

        let url = format!("{}/redirect.png", server.url());
        assert!(fetch_public_bytes(&url).is_err());

        mock.assert();
        final_mock.assert();
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
