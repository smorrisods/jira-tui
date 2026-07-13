//! Model Context Protocol (MCP) server for Jira — lets agents read and write
//! issues without ever touching raw ADF JSON.
//!
//! Reuses the same `jira::Config`/token loading as the TUI (same XDG config
//! dir, same `token.txt`/env var fallbacks), and the same `adf::compile` /
//! `adf::to_markdown` round-trip used by the in-TUI editor: agents always
//! read and write Markdown, and this server handles the ADF conversion.
//!
//! Read tools fall back to the baked-in demo data when no live credentials
//! are configured (mirroring the TUI's "always explorable" behaviour).
//! Write tools require live credentials — mutating the static demo data
//! would be a no-op, so they return a clear configuration error instead.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler, ServiceExt,
};
use serde::Deserialize;

use crate::domain::{demo_detail, demo_issues, IssueSummary};

fn live_cfg() -> Result<crate::jira::Config, McpError> {
    crate::jira::Config::load().ok_or_else(|| {
        McpError::internal_error(
            "no Jira credentials configured — set JIRA_EMAIL, JIRA_API_TOKEN, and \
             optionally JIRA_BASE_URL / JIRA_PROJECT (same env vars / token.txt / \
             XDG config used by the jira-tui TUI)"
                .to_string(),
            None,
        )
    })
}

fn to_json(value: &impl serde::Serialize) -> Result<String, McpError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| McpError::internal_error(format!("failed to encode JSON: {e}"), None))
}

fn issue_summary_json(issues: &[IssueSummary]) -> Result<String, McpError> {
    to_json(&issues)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetIssueParams {
    /// Issue key, e.g. "DS-123".
    key: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchIssuesParams {
    /// JQL query (live mode) or a plain substring to match against issue
    /// key/summary (demo mode, when no Jira credentials are configured).
    query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateIssueParams {
    /// Short one-line summary.
    summary: String,
    /// Jira issue type name, e.g. "Task", "Bug", "Story".
    issue_type: String,
    /// Optional description body, written in Markdown. Converted to ADF
    /// automatically — never send raw ADF JSON here.
    description_markdown: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateSummaryParams {
    /// Issue key, e.g. "DS-123".
    key: String,
    /// New one-line summary.
    summary: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddCommentMarkdownParams {
    /// Issue key, e.g. "DS-123".
    key: String,
    /// Comment body, written in Markdown. Converted to ADF automatically —
    /// never send raw ADF JSON here.
    body_markdown: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TransitionIssueParams {
    /// Issue key, e.g. "DS-123".
    key: String,
    /// Transition id or exact transition name (see `list_transitions`).
    transition: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateDescriptionMarkdownParams {
    /// Issue key, e.g. "DS-123".
    key: String,
    /// New description body, written in Markdown. Converted to ADF
    /// automatically — never send raw ADF JSON here.
    description_markdown: String,
}

#[derive(Clone)]
pub struct JiraMcpServer {
    // Read by the code the `#[tool_router]`/`#[tool_handler]` macros
    // generate for `call_tool`/`list_tools`; the dead-code lint can't see
    // through that macro-generated access.
    #[allow(dead_code)]
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

impl Default for JiraMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl JiraMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List the current user's assigned, not-done issues (falls back to demo data if no Jira credentials are configured)."
    )]
    fn list_my_work(&self) -> Result<String, McpError> {
        match live_cfg() {
            Ok(cfg) => {
                let issues = crate::jira::fetch_my_work(&cfg)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                issue_summary_json(&issues)
            }
            Err(_) => issue_summary_json(&demo_issues()),
        }
    }

    #[tool(
        description = "Get full details for one issue by key, including its description as Markdown (never raw ADF)."
    )]
    fn get_issue(
        &self,
        Parameters(GetIssueParams { key }): Parameters<GetIssueParams>,
    ) -> Result<String, McpError> {
        match live_cfg() {
            Ok(cfg) => {
                let detail = crate::jira::fetch_detail(&cfg, &key)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                let description_markdown = crate::adf::to_markdown(&detail.description);
                to_json(&serde_json::json!({
                    "key": detail.key,
                    "summary": detail.summary,
                    "issue_type": detail.issue_type,
                    "status": detail.status,
                    "priority": detail.priority.label(),
                    "assignee": detail.assignee,
                    "reporter": detail.reporter,
                    "labels": detail.labels,
                    "components": detail.components,
                    "parent": detail.parent,
                    "description_markdown": description_markdown,
                    "transitions": detail.transitions,
                }))
            }
            Err(_) => {
                let detail = demo_detail(&key);
                let description_markdown = crate::adf::to_markdown(&detail.description);
                to_json(&serde_json::json!({
                    "key": detail.key,
                    "summary": detail.summary,
                    "issue_type": detail.issue_type,
                    "status": detail.status,
                    "priority": detail.priority.label(),
                    "assignee": detail.assignee,
                    "reporter": detail.reporter,
                    "labels": detail.labels,
                    "components": detail.components,
                    "parent": detail.parent,
                    "description_markdown": description_markdown,
                    "transitions": detail.transitions,
                    "note": "demo data — no Jira credentials configured",
                }))
            }
        }
    }

    #[tool(
        description = "Search issues by JQL (live mode) or by a plain substring match against key/summary (demo mode)."
    )]
    fn search_issues(
        &self,
        Parameters(SearchIssuesParams { query }): Parameters<SearchIssuesParams>,
    ) -> Result<String, McpError> {
        match live_cfg() {
            Ok(cfg) => {
                let issues = crate::jira::search_issues(&cfg, &query)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                issue_summary_json(&issues)
            }
            Err(_) => {
                let needle = query.to_lowercase();
                let matches: Vec<IssueSummary> = demo_issues()
                    .into_iter()
                    .filter(|i| {
                        i.key.to_lowercase().contains(&needle)
                            || i.summary.to_lowercase().contains(&needle)
                    })
                    .collect();
                issue_summary_json(&matches)
            }
        }
    }

    #[tool(
        description = "Create a new issue. Requires live Jira credentials. Description is Markdown, converted to ADF automatically."
    )]
    fn create_issue(
        &self,
        Parameters(CreateIssueParams {
            summary,
            issue_type,
            description_markdown,
        }): Parameters<CreateIssueParams>,
    ) -> Result<String, McpError> {
        let cfg = live_cfg()?;
        let description = description_markdown.as_deref().map(crate::adf::compile);
        let key = crate::jira::create_issue(&cfg, &summary, &issue_type, description.as_ref())
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(format!("Created {key}"))
    }

    #[tool(description = "Update an issue's summary. Requires live Jira credentials.")]
    fn update_summary(
        &self,
        Parameters(UpdateSummaryParams { key, summary }): Parameters<UpdateSummaryParams>,
    ) -> Result<String, McpError> {
        let cfg = live_cfg()?;
        crate::jira::update_summary(&cfg, &key, &summary)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(format!("Updated summary for {key}"))
    }

    #[tool(
        description = "Add a comment to an issue, from Markdown (converted to ADF automatically — never send raw ADF JSON here). Requires live Jira credentials."
    )]
    fn add_comment_markdown(
        &self,
        Parameters(AddCommentMarkdownParams { key, body_markdown }): Parameters<
            AddCommentMarkdownParams,
        >,
    ) -> Result<String, McpError> {
        let cfg = live_cfg()?;
        let body = crate::adf::compile(&body_markdown);
        crate::jira::add_comment(&cfg, &key, &body)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(format!("Added comment to {key}"))
    }

    #[tool(description = "List the workflow transitions available from an issue's current status.")]
    fn list_transitions(
        &self,
        Parameters(GetIssueParams { key }): Parameters<GetIssueParams>,
    ) -> Result<String, McpError> {
        let cfg = live_cfg()?;
        let transitions = crate::jira::fetch_transitions(&cfg, &key)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        to_json(&transitions)
    }

    #[tool(
        description = "Apply a workflow transition to an issue, by transition id or exact name (see list_transitions). Requires live Jira credentials."
    )]
    fn transition_issue(
        &self,
        Parameters(TransitionIssueParams { key, transition }): Parameters<TransitionIssueParams>,
    ) -> Result<String, McpError> {
        let cfg = live_cfg()?;
        let transitions = crate::jira::fetch_transitions(&cfg, &key)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let matched = transitions
            .iter()
            .find(|t| t.id == transition || t.name.eq_ignore_ascii_case(&transition))
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!(
                        "no transition '{transition}' available for {key}; call list_transitions first"
                    ),
                    None,
                )
            })?;
        crate::jira::apply_transition(&cfg, &key, &matched.id)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(format!("Transitioned {key} to {}", matched.to))
    }

    #[tool(
        description = "Get an issue's description as Markdown (full round-trip parity with the TUI's in-app editor). Never returns raw ADF."
    )]
    fn get_description_markdown(
        &self,
        Parameters(GetIssueParams { key }): Parameters<GetIssueParams>,
    ) -> Result<String, McpError> {
        let cfg = live_cfg()?;
        let detail = crate::jira::fetch_detail(&cfg, &key)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(crate::adf::to_markdown(&detail.description))
    }

    #[tool(
        description = "Replace an issue's description from Markdown (full round-trip parity with the TUI's in-app editor). Converts to ADF automatically — never send raw ADF JSON here. Requires live Jira credentials."
    )]
    fn update_description_markdown(
        &self,
        Parameters(UpdateDescriptionMarkdownParams {
            key,
            description_markdown,
        }): Parameters<UpdateDescriptionMarkdownParams>,
    ) -> Result<String, McpError> {
        let cfg = live_cfg()?;
        let adf = crate::adf::compile(&description_markdown);
        crate::jira::update_description(&cfg, &key, &adf)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(format!("Updated description for {key}"))
    }
}

#[tool_handler]
impl ServerHandler for JiraMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("jira-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Jira MCP server for jira-tui. Always read/write issue descriptions and \
                 comments as Markdown via add_comment_markdown, get_description_markdown, \
                 and update_description_markdown (and create_issue's description_markdown \
                 field) — never construct raw ADF JSON yourself. Read tools work against \
                 demo data with no configuration; write tools need JIRA_EMAIL / \
                 JIRA_API_TOKEN (and optionally JIRA_BASE_URL / JIRA_PROJECT) set the same \
                 way the jira-tui TUI expects.",
            )
    }
}

/// Serve the Jira MCP server over stdio until the client disconnects.
pub async fn serve_stdio() -> anyhow::Result<()> {
    let service = JiraMcpServer::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
