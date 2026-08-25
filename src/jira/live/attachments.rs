//! Issue attachments: the shared parse helper used by both `detail::fetch_detail`
//! (which pulls attachment metadata in alongside everything else) and the
//! standalone `fetch_attachments`, a lighter-weight call for callers (e.g. an
//! MCP tool) that only want attachment metadata without paying for
//! transitions/comments/children too.

use serde_json::Value;

use super::super::config::Config;
use super::support::{get, str_field};
use crate::domain::Attachment;

/// Parse a `fields.attachment` JSON array (Jira's shape:
/// `{id, filename, size, mimeType, created, content}`) into `Vec<Attachment>`.
/// Missing/malformed fields fall back to empty defaults rather than failing
/// the whole parse.
pub(crate) fn parse_attachments(fields: &Value) -> Vec<Attachment> {
    fields
        .get("attachment")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().map(parse_attachment).collect())
        .unwrap_or_default()
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
}
