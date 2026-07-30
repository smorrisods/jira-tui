//! A project's creatable issue types (standard and custom) — backs the new-
//! issue compose form's type picker (`app::new_issue`). Deliberately stops at
//! the type catalog itself (id/name/subtask), not the heavier per-type
//! *field* metadata Jira's createmeta API also exposes — that's genuinely
//! custom-field support, which is out of scope for this feature.

use anyhow::Result;
use serde_json::Value;

use super::super::config::Config;
use super::support::{get, url_encode};
use crate::domain::IssueType;

/// Every issue type configured as creatable on `project` — standard (Task/
/// Bug/Story/Epic) and any project-specific custom types. `GET
/// /issue/createmeta/{project}/issuetypes` returns the whole list in one
/// call, wrapped in an `issueTypes` array (this endpoint's paginated
/// response envelope — `maxResults`/`startAt`/`total`/`isLast`/`issueTypes`
/// — names its payload field after the resource rather than a generic
/// `values`, unlike e.g. `list_versions`'s endpoint); Jira projects rarely
/// have enough issue types to need paging.
pub fn list_create_issue_types(cfg: &Config, project: &str) -> Result<Vec<IssueType>> {
    let project = url_encode(project);
    let data = get(
        cfg,
        &format!("/rest/api/3/issue/createmeta/{project}/issuetypes"),
    )?;
    let arr = data
        .get("issueTypes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(arr.iter().filter_map(issue_type_from).collect())
}

fn issue_type_from(v: &Value) -> Option<IssueType> {
    Some(IssueType {
        id: v.get("id").and_then(|x| x.as_str())?.to_string(),
        name: v.get("name").and_then(|x| x.as_str())?.to_string(),
        subtask: v.get("subtask").and_then(|x| x.as_bool()).unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::super::support::test_config;
    use super::*;

    #[test]
    fn list_create_issue_types_parses_standard_and_custom_types() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/issue/createmeta/PROJ/issuetypes")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "maxResults": 50,
                    "startAt": 0,
                    "total": 3,
                    "isLast": true,
                    "issueTypes": [
                        {"id": "10000", "name": "Task", "subtask": false},
                        {"id": "10003", "name": "Sub-task", "subtask": true},
                        {"id": "10500", "name": "Develop", "subtask": false}
                    ]
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let types = list_create_issue_types(&cfg, "PROJ").unwrap();

        mock.assert();
        assert_eq!(types.len(), 3);
        assert_eq!(types[0].name, "Task");
        assert!(!types[0].subtask);
        assert_eq!(types[1].name, "Sub-task");
        assert!(types[1].subtask);
        assert_eq!(types[2].name, "Develop");
        assert!(!types[2].subtask);
    }

    #[test]
    fn list_create_issue_types_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rest/api/3/issue/createmeta/PROJ/issuetypes")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        assert!(list_create_issue_types(&cfg, "PROJ").is_err());
    }
}
