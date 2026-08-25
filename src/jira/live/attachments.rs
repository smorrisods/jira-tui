//! Issue attachments: the shared parse helper used by both `detail::fetch_detail`
//! (which pulls attachment metadata in alongside everything else) and the
//! standalone `fetch_attachments`, a lighter-weight call for callers (e.g. an
//! MCP tool) that only want attachment metadata without paying for
//! transitions/comments/children too.

use serde_json::Value;

use super::super::config::Config;
use super::support::{get, get_bytes, post_multipart, str_field};
use crate::domain::Attachment;

/// Parse a `fields.attachment` JSON array (Jira's shape:
/// `{id, filename, size, mimeType, created, content}`) into `Vec<Attachment>`.
/// Missing/malformed fields fall back to empty defaults rather than failing
/// the whole parse.
pub(crate) fn parse_attachments(fields: &Value) -> Vec<Attachment> {
    fields
        .get("attachment")
        .and_then(|a| a.as_array())
        .map(|arr| parse_attachment_array(arr))
        .unwrap_or_default()
}

/// Shared per-object mapping used by both `parse_attachments` (which unwraps
/// an issue's `fields.attachment` array) and `upload_attachment` (whose
/// response is already a bare array of the same per-entry shape).
fn parse_attachment_array(arr: &[Value]) -> Vec<Attachment> {
    arr.iter().map(parse_attachment).collect()
}

fn parse_attachment(v: &Value) -> Attachment {
    let created = str_field(v, &["created"]).unwrap_or_default();
    Attachment {
        id: str_field(v, &["id"]).unwrap_or_default(),
        filename: str_field(v, &["filename"]).unwrap_or_default(),
        mime_type: str_field(v, &["mimeType"]).unwrap_or_default(),
        size: v.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
        // Truncated to the date portion, matching `IssueSummary::updated`'s
        // convention (see `support::summary_from`).
        created: created.chars().take(10).collect(),
        content_url: str_field(v, &["content"]).unwrap_or_default(),
    }
}

/// Fetch just an issue's attachment metadata — lighter-weight than
/// `fetch_detail`, which pulls attachments alongside transitions, comments,
/// and children.
pub fn fetch_attachments(cfg: &Config, key: &str) -> anyhow::Result<Vec<Attachment>> {
    let path = format!("/rest/api/3/issue/{key}?fields=attachment");
    let issue = get(cfg, &path)?;
    let f = issue.get("fields").cloned().unwrap_or(Value::Null);
    Ok(parse_attachments(&f))
}

/// Download an attachment's raw bytes from its `content_url` (already an
/// absolute URL, per Jira's `attachment.content` field) — used by
/// `app::async_ops::mutation_ops::dispatch_attachment_download`'s blocking
/// half to save an attachment to disk.
pub fn download_attachment(cfg: &Config, content_url: &str) -> anyhow::Result<Vec<u8>> {
    get_bytes(cfg, content_url)
}

/// Upload a file to an issue's attachments. Jira's response to this
/// endpoint is a bare JSON array of attachment objects (same per-entry
/// shape as `fields.attachment`), so it reuses `parse_attachment_array`
/// rather than duplicating the field mapping.
pub fn upload_attachment(
    cfg: &Config,
    key: &str,
    filename: &str,
    mime: &str,
    bytes: &[u8],
) -> anyhow::Result<Vec<Attachment>> {
    let path = format!("/rest/api/3/issue/{key}/attachments");
    let value = post_multipart(cfg, &path, filename, mime, bytes)?;
    let arr = value.as_array().cloned().unwrap_or_default();
    Ok(parse_attachment_array(&arr))
}

// `guess_mime` used to live here, but moved to `crate::mime` so it's
// available in every feature set, not just `live` — see that module's doc
// comment. `jira::guess_mime` (re-exported from `live::mod`) still resolves
// to it, so nothing outside this file needed to change.

#[cfg(test)]
mod tests {
    use super::super::support::test_config;
    use super::*;

    #[test]
    fn fetch_attachments_parses_the_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/issue/DS-1?fields=attachment")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "fields": {
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

        let cfg = test_config(server.url());
        let attachments = fetch_attachments(&cfg, "DS-1").unwrap();

        mock.assert();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].id, "10001");
        assert_eq!(attachments[0].filename, "accordion-mockup.png");
        assert_eq!(attachments[0].size, 245_760);
        assert_eq!(attachments[0].mime_type, "image/png");
        assert_eq!(attachments[0].created, "2026-07-08");
        assert_eq!(
            attachments[0].content_url,
            "https://demo.atlassian.net/secure/attachment/10001/accordion-mockup.png"
        );
    }

    #[test]
    fn fetch_attachments_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rest/api/3/issue/DS-1?fields=attachment")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        assert!(fetch_attachments(&cfg, "DS-1").is_err());
    }

    #[test]
    fn download_attachment_returns_the_raw_bytes() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/secure/attachment/10001/accordion-mockup.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(b"\x89PNG\r\n\x1a\nnot a real png, just test bytes")
            .create();

        let cfg = test_config(server.url());
        let url = format!(
            "{}/secure/attachment/10001/accordion-mockup.png",
            cfg.base_url
        );
        let bytes = download_attachment(&cfg, &url).unwrap();

        mock.assert();
        assert_eq!(bytes, b"\x89PNG\r\n\x1a\nnot a real png, just test bytes");
    }

    #[test]
    fn download_attachment_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/secure/attachment/missing.png")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        let url = format!("{}/secure/attachment/missing.png", cfg.base_url);
        assert!(download_attachment(&cfg, &url).is_err());
    }

    #[test]
    fn upload_attachment_sends_the_xsrf_bypass_header_and_parses_the_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/rest/api/3/issue/DS-1/attachments")
            .match_header("x-atlassian-token", "no-check")
            .match_body(mockito::Matcher::Regex(
                "Content-Disposition: form-data; name=\"file\"; filename=\"report\\.txt\""
                    .to_string(),
            ))
            .match_body(mockito::Matcher::Regex("hello from the report".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {
                        "id": "10002",
                        "filename": "report.txt",
                        "size": 21,
                        "mimeType": "text/plain",
                        "created": "2026-08-25T10:00:00.000-0400",
                        "content": "https://demo.atlassian.net/secure/attachment/10002/report.txt"
                    }
                ]"#,
            )
            .create();

        let cfg = test_config(server.url());
        let attachments = upload_attachment(
            &cfg,
            "DS-1",
            "report.txt",
            "text/plain",
            b"hello from the report",
        )
        .unwrap();

        mock.assert();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].id, "10002");
        assert_eq!(attachments[0].filename, "report.txt");
        assert_eq!(attachments[0].mime_type, "text/plain");
        assert_eq!(attachments[0].created, "2026-08-25");
        assert_eq!(
            attachments[0].content_url,
            "https://demo.atlassian.net/secure/attachment/10002/report.txt"
        );
    }

    #[test]
    fn upload_attachment_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/rest/api/3/issue/DS-1/attachments")
            .with_status(413)
            .create();

        let cfg = test_config(server.url());
        assert!(upload_attachment(&cfg, "DS-1", "big.zip", "application/zip", b"x").is_err());
    }
}
