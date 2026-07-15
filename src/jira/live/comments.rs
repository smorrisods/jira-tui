//! Issue comments: adding one and paging through the full history.

use anyhow::Result;
use serde_json::Value;

use super::super::config::Config;
use super::support::{get, post_or_put, str_field};

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
}
