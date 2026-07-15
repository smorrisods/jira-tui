//! JQL building and paged issue search — the piece `detail::children_of`
//! also depends on for an Epic's child stories.

use anyhow::{anyhow, Result};

use super::super::config::Config;
use super::support::{get, summary_from, url_encode};
use crate::domain::IssueSummary;

/// The fixed JQL behind "my work" — exposed as a constant (not just inline
/// in `fetch_my_work`) so the cache layer can record exactly which query
/// produced a cached view, without duplicating the string.
pub const MY_WORK_JQL: &str =
    "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";

pub fn fetch_my_work(cfg: &Config) -> Result<Vec<IssueSummary>> {
    search_issues(cfg, MY_WORK_JQL)
}

/// Build the JQL for a given view. `project` is the Jira project key
/// (`Config.project`, already used for issue creation).
pub fn jql_for(view: &crate::domain::ViewKind, project: &str) -> String {
    use crate::domain::ViewKind;
    match view {
        ViewKind::MyWork => MY_WORK_JQL.to_string(),
        ViewKind::AllProject => format!("project = \"{project}\" ORDER BY updated DESC"),
        ViewKind::Teammate(name) => {
            // Escape backslashes *before* quotes — a JQL string literal
            // uses `\` as its escape character, so a name ending in `\`
            // would otherwise absorb the closing quote as an escaped
            // character instead of terminating the string.
            let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
            format!("assignee = \"{escaped}\" AND statusCategory != Done ORDER BY updated DESC")
        }
    }
}

/// Run an arbitrary JQL query and return matching issue summaries, paging
/// through the enhanced JQL search endpoint's `nextPageToken` until Jira
/// reports `isLast`, or until `SEARCH_RESULTS_CAP` is hit — whichever comes
/// first. Used both for "my work"/the view switcher (a handful of fixed JQL
/// variants) and the MCP server's free-form search tool.
///
/// A single page used to be the whole story (`maxResults=50`, no paging),
/// which meant `AllProject`/large-project views silently truncated at the
/// 50 most-recently-updated issues — including the teammate picker, which
/// is seeded from whatever's currently loaded. `SEARCH_RESULTS_CAP` still
/// bounds the total so a very large project can't page forever.
pub fn search_issues(cfg: &Config, jql: &str) -> Result<Vec<IssueSummary>> {
    let mut all = Vec::new();
    let mut next_page_token: Option<String> = None;

    loop {
        let encoded = url_encode(jql);
        // Enhanced JQL search endpoint (`/search/jql`); the classic
        // `/search` endpoint has been sunset on Jira Cloud.
        let mut path = format!(
            "/rest/api/3/search/jql?jql={encoded}&maxResults={SEARCH_PAGE_SIZE}&fields=summary,status,issuetype,priority,assignee,updated,issuelinks,parent"
        );
        if let Some(token) = &next_page_token {
            path.push_str(&format!("&nextPageToken={}", url_encode(token)));
        }

        let data = get(cfg, &path)?;
        let issues = data
            .get("issues")
            .and_then(|i| i.as_array())
            .ok_or_else(|| anyhow!("no issues array in response"))?;
        all.extend(issues.iter().map(summary_from));

        // Missing `isLast` (e.g. an older mock/response) is treated as "this
        // was the only page" rather than looping forever.
        let is_last = data.get("isLast").and_then(|v| v.as_bool()).unwrap_or(true);
        if is_last || all.len() >= SEARCH_RESULTS_CAP {
            break;
        }
        let Some(token) = data
            .get("nextPageToken")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            // Jira said there's more, but didn't give us a token to fetch
            // it with — stop rather than loop on the same page forever.
            break;
        };
        next_page_token = Some(token);
    }

    Ok(all)
}

/// Results per page for `search_issues`'s paging loop.
pub(super) const SEARCH_PAGE_SIZE: usize = 50;

/// The most `search_issues` will ever return across every page, so a very
/// large project can't page indefinitely. Exposed so callers (the view
/// loader) can tell a genuinely truncated result apart from "that's
/// everything".
pub const SEARCH_RESULTS_CAP: usize = 500;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Priority;

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
    fn jql_for_builds_the_expected_query_per_view() {
        use crate::domain::ViewKind;

        assert_eq!(jql_for(&ViewKind::MyWork, "PROJ"), MY_WORK_JQL);
        assert_eq!(
            jql_for(&ViewKind::AllProject, "PROJ"),
            "project = \"PROJ\" ORDER BY updated DESC"
        );
        assert_eq!(
            jql_for(&ViewKind::Teammate("Ada Lovelace".into()), "PROJ"),
            "assignee = \"Ada Lovelace\" AND statusCategory != Done ORDER BY updated DESC"
        );
    }

    #[test]
    fn jql_for_teammate_escapes_embedded_quotes() {
        use crate::domain::ViewKind;

        let jql = jql_for(&ViewKind::Teammate("Robert \"Bob\" Smith".into()), "PROJ");
        assert_eq!(
            jql,
            "assignee = \"Robert \\\"Bob\\\" Smith\" AND statusCategory != Done ORDER BY updated DESC"
        );
    }

    #[test]
    fn jql_for_teammate_escapes_backslashes_before_quotes() {
        use crate::domain::ViewKind;

        // A trailing backslash must be escaped to `\\` *before* the closing
        // quote is appended, or the parser reads `\"` as an escaped quote
        // rather than the string's terminator.
        let jql = jql_for(&ViewKind::Teammate("Robert\\".into()), "PROJ");
        assert_eq!(
            jql,
            "assignee = \"Robert\\\\\" AND statusCategory != Done ORDER BY updated DESC"
        );

        let jql = jql_for(&ViewKind::Teammate("Back\\slash \"Quote\"".into()), "PROJ");
        assert_eq!(
            jql,
            "assignee = \"Back\\\\slash \\\"Quote\\\"\" AND statusCategory != Done ORDER BY updated DESC"
        );
    }

    #[test]
    fn fetch_my_work_sends_the_expected_jql_and_parses_issues() {
        let mut server = mockito::Server::new();
        let expected_path = "/rest/api/3/search/jql?jql=assignee%20%3D%20currentUser%28%29%20AND%20statusCategory%20%21%3D%20Done%20ORDER%20BY%20updated%20DESC&maxResults=50&fields=summary,status,issuetype,priority,assignee,updated,issuelinks,parent";
        let mock = server
            .mock("GET", expected_path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "issues": [
                        {
                            "key": "DS-1",
                            "fields": {
                                "summary": "Fix the thing",
                                "issuetype": {"name": "Bug"},
                                "status": {"name": "In Progress"},
                                "priority": {"name": "Highest"},
                                "assignee": {"displayName": "Ada Lovelace"},
                                "updated": "2024-01-02T03:04:05.000+0000",
                                "parent": {"key": "DS-0"},
                                "issuelinks": [
                                    {
                                        "type": {"inward": "is blocked by"},
                                        "inwardIssue": {"key": "DS-9"}
                                    }
                                ]
                            }
                        }
                    ]
                }"#,
            )
            .create();

        let cfg = test_config(server.url());
        let issues = fetch_my_work(&cfg).unwrap();

        mock.assert();
        assert_eq!(issues.len(), 1);
        let issue = &issues[0];
        assert_eq!(issue.key, "DS-1");
        assert_eq!(issue.summary, "Fix the thing");
        assert_eq!(issue.issue_type, "Bug");
        assert_eq!(issue.status, "In Progress");
        assert_eq!(issue.priority, Priority::Highest);
        assert_eq!(issue.assignee.as_deref(), Some("Ada Lovelace"));
        assert_eq!(issue.updated, "2024-01-02");
        assert_eq!(issue.epic.as_deref(), Some("DS-0"));
        assert!(
            issue.blocked,
            "an inward 'is blocked by' link must set blocked"
        );
    }

    #[test]
    fn search_issues_errors_when_response_has_no_issues_array() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"unexpected": true}"#)
            .create();

        let cfg = test_config(server.url());
        assert!(search_issues(&cfg, "any jql").is_err());
    }

    #[test]
    fn search_issues_pages_through_next_page_token_until_is_last() {
        let mut server = mockito::Server::new();
        let base_path = "/rest/api/3/search/jql?jql=any%20jql&maxResults=50&fields=summary,status,issuetype,priority,assignee,updated,issuelinks,parent";
        // First page: no `nextPageToken` yet, isLast=false, hands back a
        // token for the second page.
        let first = server
            .mock("GET", base_path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"issues": [{"key": "DS-1"}], "isLast": false, "nextPageToken": "page-2"}"#,
            )
            .create();
        // Second page: the token must be echoed back as a query param;
        // isLast=true stops the loop.
        let second_path = format!("{base_path}&nextPageToken=page-2");
        let second = server
            .mock("GET", second_path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"issues": [{"key": "DS-2"}], "isLast": true}"#)
            .create();

        let cfg = test_config(server.url());
        let issues = search_issues(&cfg, "any jql").unwrap();

        first.assert();
        second.assert();
        assert_eq!(
            issues.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
            vec!["DS-1", "DS-2"],
            "both pages' issues should be concatenated, in order"
        );
    }

    #[test]
    fn search_issues_stops_at_the_results_cap_even_if_jira_says_theres_more() {
        let mut server = mockito::Server::new();
        // Always claims there's more (isLast=false, same token forever) —
        // the loop must still stop once SEARCH_RESULTS_CAP is reached,
        // rather than paging indefinitely against a misbehaving server.
        let page: String = (0..SEARCH_PAGE_SIZE)
            .map(|i| format!(r#"{{"key": "DS-{i}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let body =
            format!(r#"{{"issues": [{page}], "isLast": false, "nextPageToken": "same-token"}}"#);
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .expect(SEARCH_RESULTS_CAP / SEARCH_PAGE_SIZE)
            .create();

        let cfg = test_config(server.url());
        let issues = search_issues(&cfg, "any jql").unwrap();

        mock.assert();
        assert_eq!(issues.len(), SEARCH_RESULTS_CAP);
    }
}
