//! `jira-mcp` — a Model Context Protocol server exposing Jira over stdio.
//!
//! Thin entry point: the actual server (tools, ADF/Markdown conversion,
//! demo-data fallback) lives in `jira_tui::mcp`.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    jira_tui::mcp::serve_stdio().await
}
