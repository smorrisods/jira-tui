//! Sprint assignment: listing a board's open sprints, moving an issue into
//! one, and removing an issue from its sprint (back to the backlog).
//!
//! Unlike every other write in this module, sprint membership isn't a
//! classic-API issue field write (`PUT /rest/api/3/issue/{key}`) — even
//! though Jira happens to expose it as a custom field (`sprint_field`,
//! `customfield_NNNNN` on this repo's own probed instance) on reads, Jira
//! Software's own Agile REST API (`/rest/agile/1.0/...`, a different base
//! path on the same host — `support::get`/`send` are already
//! `cfg.base_url`-relative, so no new HTTP plumbing is needed) is the
//! documented, supported way to change it: `POST
//! /rest/agile/1.0/sprint/{sprintId}/issue` to move an issue in, `POST
//! /rest/agile/1.0/backlog/issue` to take it back out.
//!
//! Ground truth: the read shape (the custom field's array-of-sprint-history
//! payload, including that closed entries can lack `boardId`) and the board/
//! sprint-listing endpoints were verified live against a real Jira Cloud
//! site. The two write endpoints below are per Atlassian's own Agile REST
//! API docs (`developer.atlassian.com/cloud/jira/software/rest/api-group-sprint`,
//! `api-group-backlog`) — deliberately *not* live-tested here, to avoid
//! mutating a real production issue's sprint assignment just to verify a
//! request shape jira-tui doesn't own.

use anyhow::Result;
use serde_json::Value;

use super::super::config::Config;
use super::support::{get, send};
use crate::domain::Sprint;

/// Which sprint (if any) an issue's sprint-history array currently means:
/// Jira only ever lets an issue be a member of one *open* sprint at a time,
/// but the custom field returns every sprint it was ever placed in (not
/// necessarily in chronological order — confirmed live against a real
/// instance), so this picks the one that's still actually relevant. Prefers
/// `active` (in progress right now) over `future` (queued next); `closed`
/// entries are sprint history, not the current sprint, so an issue with only
/// closed sprints in its history resolves to `None` (equivalent to "no
/// current sprint"), same as an issue with no history at all.
pub(crate) fn current_sprint(sprints: &[Sprint]) -> Option<Sprint> {
    sprints
        .iter()
        .find(|s| s.state == "active")
        .or_else(|| sprints.iter().find(|s| s.state == "future"))
        .cloned()
}

/// Parse a `customfield_XXXXX` sprint-history array (Jira Software's shape:
/// `[{id, name, state, boardId, goal, startDate, endDate, completeDate?}, ...]`,
/// where `boardId` is only present on some entries — verified live) into
/// `Sprint`s. Also reused for the board sprint-listing endpoint's `values`
/// array, which carries the same per-entry shape (plus `self`/`originBoardId`/
/// `createdDate`, none of which this domain model needs). Missing/malformed
/// entries are skipped rather than failing the whole parse, matching
/// `attachments::parse_attachments`'s defensive style.
pub(crate) fn parse_sprint_field(value: &Value) -> Vec<Sprint> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    Some(Sprint {
                        id: sprint_id(v)?,
                        name: v.get("name")?.as_str()?.to_string(),
                        state: v.get("state")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Jira's sprint `id` comes back as a bare JSON number (unlike e.g.
/// `Version.id`, which is a string) — accept either shape defensively and
/// normalize to a string, matching how the rest of this domain model stores
/// ids.
fn sprint_id(v: &Value) -> Option<String> {
    let id = v.get("id")?;
    id.as_str()
        .map(str::to_string)
        .or_else(|| id.as_i64().map(|n| n.to_string()))
}

/// Every open (active or future) sprint on the configured board — backs the
/// per-issue sprint picker (`S`). Closed sprints are excluded server-side
/// (`state=active,future`, verified live): they're history, not something
/// you'd move an issue into. Doesn't loop pages: a board's *open* sprints
/// are a small, bounded set in practice (Jira only lets one sprint be active
/// at a time; "future" is typically a handful of planned ones) — see
/// `versions::list_versions`'s identical "rarely need paging" reasoning.
pub fn list_open_sprints(cfg: &Config, board_id: &str) -> Result<Vec<Sprint>> {
    let path = format!("/rest/agile/1.0/board/{board_id}/sprint?state=active,future");
    let data = get(cfg, &path)?;
    let values = data.get("values").cloned().unwrap_or(Value::Null);
    Ok(parse_sprint_field(&values))
}

/// Move an issue into `sprint_id` — Jira Software's Agile REST API, not the
/// classic issue-field write (see this module's doc comment for why).
pub fn assign_sprint(cfg: &Config, sprint_id: &str, key: &str) -> Result<()> {
    send(
        cfg,
        "POST",
        &format!("/rest/agile/1.0/sprint/{sprint_id}/issue"),
        serde_json::json!({ "issues": [key] }),
    )
}

/// Move an issue back to the backlog (out of any sprint) — Jira Software's
/// Agile REST API; there's no dedicated "remove from sprint" endpoint, the
/// backlog is the closest (and the only server-supported way to un-sprint an
/// issue without picking a different sprint to move it to instead).
pub fn remove_from_sprint(cfg: &Config, key: &str) -> Result<()> {
    send(
        cfg,
        "POST",
        "/rest/agile/1.0/backlog/issue",
        serde_json::json!({ "issues": [key] }),
    )
}

#[cfg(test)]
mod tests {
    use super::super::support::test_config;
    use super::*;

    fn sprint(id: &str, name: &str, state: &str) -> Sprint {
        Sprint {
            id: id.into(),
            name: name.into(),
            state: state.into(),
        }
    }

    #[test]
    fn current_sprint_prefers_active_over_future_and_closed() {
        let sprints = vec![
            sprint("1", "Sprint 1", "closed"),
            sprint("3", "Sprint 3", "future"),
            sprint("2", "Sprint 2", "active"),
        ];
        assert_eq!(
            current_sprint(&sprints),
            Some(sprint("2", "Sprint 2", "active"))
        );
    }

    #[test]
    fn current_sprint_falls_back_to_future_when_no_active_sprint() {
        let sprints = vec![
            sprint("1", "Sprint 1", "closed"),
            sprint("2", "Sprint 2", "future"),
        ];
        assert_eq!(
            current_sprint(&sprints),
            Some(sprint("2", "Sprint 2", "future"))
        );
    }

    #[test]
    fn current_sprint_is_none_when_only_closed_sprints_exist() {
        let sprints = vec![
            sprint("1", "Sprint 1", "closed"),
            sprint("2", "Sprint 2", "closed"),
        ];
        assert_eq!(current_sprint(&sprints), None);
    }

    #[test]
    fn current_sprint_is_none_for_an_empty_history() {
        assert_eq!(current_sprint(&[]), None);
    }

    #[test]
    fn parse_sprint_field_accepts_a_numeric_id_and_missing_board_id() {
        // Real shape, verified live: a closed sprint entry can omit
        // `boardId` entirely, and `id` is a bare JSON number, not a string
        // (unlike `Version.id`).
        let value = serde_json::json!([
            {"id": 5375, "name": "Design Sprint 10", "state": "closed", "goal": ""}
        ]);
        let sprints = parse_sprint_field(&value);
        assert_eq!(sprints, vec![sprint("5375", "Design Sprint 10", "closed")]);
    }

    #[test]
    fn parse_sprint_field_skips_entries_missing_required_fields() {
        let value = serde_json::json!([
            {"id": 1, "state": "active"},
            {"id": 2, "name": "Sprint 2", "state": "active"}
        ]);
        let sprints = parse_sprint_field(&value);
        assert_eq!(sprints, vec![sprint("2", "Sprint 2", "active")]);
    }

    #[test]
    fn parse_sprint_field_defaults_to_empty_for_a_non_array_value() {
        assert!(parse_sprint_field(&Value::Null).is_empty());
    }

    #[test]
    fn list_open_sprints_filters_to_active_and_future_via_the_state_query() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/agile/1.0/board/843/sprint?state=active,future")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "maxResults": 50,
                    "startAt": 0,
                    "total": 2,
                    "isLast": true,
                    "values": [
                        {"id": 6414, "name": "Sprint 80", "state": "future", "boardId": 843, "goal": ""},
                        {"id": 6415, "name": "Sprint 81", "state": "future", "boardId": 843, "goal": ""}
                    ]
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let sprints = list_open_sprints(&cfg, "843").unwrap();

        mock.assert();
        assert_eq!(sprints.len(), 2);
        assert_eq!(sprints[0].name, "Sprint 80");
        assert_eq!(sprints[1].name, "Sprint 81");
    }

    #[test]
    fn list_open_sprints_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock(
                "GET",
                "/rest/agile/1.0/board/843/sprint?state=active,future",
            )
            .with_status(404)
            .create();

        let cfg = test_config(server.url());
        assert!(list_open_sprints(&cfg, "843").is_err());
    }

    #[test]
    fn assign_sprint_posts_the_issue_key_to_the_sprint_endpoint() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/rest/agile/1.0/sprint/6415/issue")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "issues": ["DS-1"]
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        assign_sprint(&cfg, "6415", "DS-1").unwrap();

        mock.assert();
    }

    #[test]
    fn assign_sprint_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/rest/agile/1.0/sprint/6415/issue")
            .with_status(400)
            .create();

        let cfg = test_config(server.url());
        assert!(assign_sprint(&cfg, "6415", "DS-1").is_err());
    }

    #[test]
    fn remove_from_sprint_posts_the_issue_key_to_the_backlog_endpoint() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/rest/agile/1.0/backlog/issue")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "issues": ["DS-1"]
            })))
            .with_status(204)
            .create();

        let cfg = test_config(server.url());
        remove_from_sprint(&cfg, "DS-1").unwrap();

        mock.assert();
    }

    #[test]
    fn remove_from_sprint_surfaces_http_errors() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/rest/agile/1.0/backlog/issue")
            .with_status(500)
            .create();

        let cfg = test_config(server.url());
        assert!(remove_from_sprint(&cfg, "DS-1").is_err());
    }
}
