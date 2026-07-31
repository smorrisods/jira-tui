//! Every Jira project the authenticated user can access — backs the new-
//! issue compose form's project picker (`app::project_picker`), so a
//! project can be browsed by name rather than typed in blind by key.

use anyhow::Result;
use serde_json::Value;

use super::super::config::Config;
use super::support::get;
use crate::domain::Project;

const PAGE_SIZE: u32 = 50;

/// `GET /rest/api/3/project/search`, paged via `startAt`/`maxResults` until
/// Jira reports `isLast` — this endpoint wraps its results in the
/// `values`/`isLast` "PageBean" envelope shared by most Jira Cloud v3
/// search endpoints (distinct from `list_create_issue_types`'s
/// resource-named `issueTypes` field, and from `assignable_users`'s flat,
/// unwrapped array). A missing `isLast` (e.g. an older mock/response) is
/// treated as "this was the only page", matching `search_issues`'s
/// same default.
pub fn list_projects(cfg: &Config) -> Result<Vec<Project>> {
    let mut all = Vec::new();
    let mut start_at: u32 = 0;
    loop {
        let path = format!("/rest/api/3/project/search?startAt={start_at}&maxResults={PAGE_SIZE}");
        let data = get(cfg, &path)?;
        let values = data
            .get("values")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let page_len = values.len();
        all.extend(values.iter().filter_map(project_from));

        let is_last = data.get("isLast").and_then(|v| v.as_bool()).unwrap_or(true);
        if is_last || page_len == 0 {
            break;
        }
        start_at += page_len as u32;
    }
    Ok(all)
}

fn project_from(v: &Value) -> Option<Project> {
    Some(Project {
        id: v.get("id").and_then(|x| x.as_str())?.to_string(),
        key: v.get("key").and_then(|x| x.as_str())?.to_string(),
        name: v.get("name").and_then(|x| x.as_str())?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::support::test_config;
    use super::*;

    #[test]
    fn list_projects_parses_a_single_page() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/project/search?startAt=0&maxResults=50")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "maxResults": 50,
                    "startAt": 0,
                    "total": 2,
                    "isLast": true,
                    "values": [
                        {"id": "10000", "key": "DS", "name": "Design System"},
                        {"id": "10001", "key": "ENG", "name": "Engineering"}
                    ]
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let projects = list_projects(&cfg).unwrap();

        mock.assert();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].key, "DS");
        assert_eq!(projects[0].name, "Design System");
        assert_eq!(projects[1].key, "ENG");
    }

    #[test]
    fn list_projects_pages_until_is_last() {
        let mut server = mockito::Server::new();
        let page_one_values: Vec<Value> = (0..50)
            .map(|i| serde_json::json!({"id": i.to_string(), "key": format!("P{i}"), "name": format!("Project {i}")}))
            .collect();
        let first = server
            .mock("GET", "/rest/api/3/project/search?startAt=0&maxResults=50")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "maxResults": 50,
                    "startAt": 0,
                    "total": 51,
                    "isLast": false,
                    "values": page_one_values,
                })
                .to_string(),
            )
            .create();
        let second = server
            .mock("GET", "/rest/api/3/project/search?startAt=50&maxResults=50")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "maxResults": 50,
                    "startAt": 50,
                    "total": 51,
                    "isLast": true,
                    "values": [{"id": "50", "key": "P50", "name": "Project 50"}]
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let projects = list_projects(&cfg).unwrap();

        first.assert();
        second.assert();
        assert_eq!(projects.len(), 51);
        assert_eq!(projects[0].key, "P0");
        assert_eq!(projects[50].key, "P50");
    }

    #[test]
    fn list_projects_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rest/api/3/project/search?startAt=0&maxResults=50")
            .with_status(500)
            .create();

        let cfg = test_config(server.url());
        assert!(list_projects(&cfg).is_err());
    }
}
