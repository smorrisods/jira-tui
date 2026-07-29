//! Project versions ("releases"): listing them, and setting an issue's
//! `fixVersions`/`versions` fields. Jira replaces both fields wholesale on
//! write — there's no "add"/"remove" endpoint — so `set_fix_versions`/
//! `set_affects_versions` always take the complete resulting set of version
//! names for the issue, matching how `App::confirm_version_picker` and the
//! release drill-down's bulk actions compute it before calling in.

use anyhow::Result;
use serde_json::Value;

use super::super::config::Config;
use super::support::{get, send, url_encode};
use crate::domain::Version;

/// Every version (released or not) defined on the configured project —
/// backs the release review screen's version picker (`app::release`) and
/// the per-issue Fix/Affects Version picker (`R`). `GET
/// /project/{key}/versions` returns the whole list in one call; Jira
/// projects rarely have enough versions to need paging.
pub fn list_versions(cfg: &Config, project: &str) -> Result<Vec<Version>> {
    let project = url_encode(project);
    let data = get(cfg, &format!("/rest/api/3/project/{project}/versions"))?;
    let arr = data.as_array().cloned().unwrap_or_default();
    Ok(arr.iter().filter_map(version_from).collect())
}

fn version_from(v: &Value) -> Option<Version> {
    Some(Version {
        id: v.get("id").and_then(|x| x.as_str())?.to_string(),
        name: v.get("name").and_then(|x| x.as_str())?.to_string(),
        released: v.get("released").and_then(|x| x.as_bool()).unwrap_or(false),
        release_date: v
            .get("releaseDate")
            .and_then(|x| x.as_str())
            .map(String::from),
    })
}

/// Replace an issue's fix versions (the releases it's targeted to ship in)
/// with exactly `version_names` — Jira takes version objects by name here
/// (an id would also work, but names are what the rest of this codebase's
/// domain model carries, matching `components`).
pub fn set_fix_versions(cfg: &Config, key: &str, version_names: &[String]) -> Result<()> {
    send(
        cfg,
        "PUT",
        &format!("/rest/api/3/issue/{key}"),
        serde_json::json!({ "fields": { "fixVersions": version_objects(version_names) } }),
    )
}

/// Replace an issue's affected versions (where the bug/gap was found) with
/// exactly `version_names`.
pub fn set_affects_versions(cfg: &Config, key: &str, version_names: &[String]) -> Result<()> {
    send(
        cfg,
        "PUT",
        &format!("/rest/api/3/issue/{key}"),
        serde_json::json!({ "fields": { "versions": version_objects(version_names) } }),
    )
}

fn version_objects(names: &[String]) -> Vec<Value> {
    names
        .iter()
        .map(|n| serde_json::json!({ "name": n }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::support::test_config;
    use super::*;

    #[test]
    fn list_versions_parses_released_and_unreleased_entries() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/project/PROJ/versions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {"id": "1", "name": "v1.0.0", "released": true, "releaseDate": "2026-01-01"},
                    {"id": "2", "name": "v1.1.0", "released": false}
                ]"#,
            )
            .create();

        let cfg = test_config(server.url());
        let versions = list_versions(&cfg, "PROJ").unwrap();

        mock.assert();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].name, "v1.0.0");
        assert!(versions[0].released);
        assert_eq!(versions[0].release_date.as_deref(), Some("2026-01-01"));
        assert_eq!(versions[1].name, "v1.1.0");
        assert!(!versions[1].released);
        assert_eq!(versions[1].release_date, None);
    }

    #[test]
    fn list_versions_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rest/api/3/project/PROJ/versions")
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        assert!(list_versions(&cfg, "PROJ").is_err());
    }

    #[test]
    fn set_fix_versions_sends_the_full_replacement_set() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("PUT", "/rest/api/3/issue/DS-1")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "fields": { "fixVersions": [{"name": "v1.0.0"}, {"name": "v1.1.0"}] }
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        set_fix_versions(&cfg, "DS-1", &["v1.0.0".into(), "v1.1.0".into()]).unwrap();

        mock.assert();
    }

    #[test]
    fn set_fix_versions_with_an_empty_list_clears_it() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("PUT", "/rest/api/3/issue/DS-1")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "fields": { "fixVersions": [] }
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        set_fix_versions(&cfg, "DS-1", &[]).unwrap();

        mock.assert();
    }

    #[test]
    fn set_affects_versions_sends_the_versions_field() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("PUT", "/rest/api/3/issue/DS-1")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "fields": { "versions": [{"name": "v1.0.0"}] }
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        set_affects_versions(&cfg, "DS-1", &["v1.0.0".into()]).unwrap();

        mock.assert();
    }

    #[test]
    fn set_affects_versions_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("PUT", "/rest/api/3/issue/DS-1")
            .with_status(500)
            .create();

        let cfg = test_config(server.url());
        assert!(set_affects_versions(&cfg, "DS-1", &["v1.0.0".into()]).is_err());
    }
}
