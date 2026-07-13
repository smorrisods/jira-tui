//! Process-level tests for the `jira-mcp` server's tool schema — guards
//! against tool/parameter names drifting out of sync with each other (e.g.
//! a tool named `add_comment` taking a `body_markdown` field, or vice versa)
//! since that's exactly the kind of mismatch that trips up an agent calling
//! the server blind.

#![cfg(feature = "mcp")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpProcess {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_jira-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn jira-mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();

        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        Some(serde_json::from_str(&line).expect("response should be valid JSON"))
    }

    fn notify(&mut self, method: &str) {
        let msg = serde_json::json!({"jsonrpc": "2.0", "method": method});
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn list_tools(&mut self) -> Vec<serde_json::Value> {
        self.send(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0.0.0"},
            }),
        );
        self.notify("notifications/initialized");
        let resp = self.send("tools/list", serde_json::json!({})).unwrap();
        resp["result"]["tools"]
            .as_array()
            .expect("tools/list should return a tools array")
            .clone()
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn find_tool<'a>(tools: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    tools
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("expected a tool named `{name}` in tools/list"))
}

fn required_params(tool: &serde_json::Value) -> Vec<String> {
    tool["inputSchema"]["required"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// `add_comment` must be named `add_comment_markdown` — matching
/// `get_description_markdown`/`update_description_markdown` — and its body
/// field must be `body_markdown`, so a caller can infer the parameter name
/// straight from the tool name without checking the schema first.
#[test]
fn add_comment_markdown_tool_name_and_params_are_consistent() {
    let mut mcp = McpProcess::spawn();
    let tools = mcp.list_tools();

    assert!(
        tools.iter().all(|t| t["name"] != "add_comment"),
        "the old `add_comment` tool name should no longer be exposed"
    );

    let tool = find_tool(&tools, "add_comment_markdown");
    let required = required_params(tool);
    assert!(
        required.contains(&"body_markdown".to_string()),
        "add_comment_markdown should require a `body_markdown` field, got: {required:?}"
    );
    assert!(required.contains(&"key".to_string()));
}

/// `update_description_markdown`'s markdown field should itself be named
/// `description_markdown` (previously just `markdown`), matching
/// `create_issue`'s `description_markdown` field for the same concept.
#[test]
fn update_description_markdown_field_name_matches_create_issue() {
    let mut mcp = McpProcess::spawn();
    let tools = mcp.list_tools();

    let tool = find_tool(&tools, "update_description_markdown");
    let required = required_params(tool);
    assert!(
        required.contains(&"description_markdown".to_string()),
        "update_description_markdown should require a `description_markdown` field, got: {required:?}"
    );
    assert!(
        !required.contains(&"markdown".to_string()),
        "the old bare `markdown` field name should no longer be exposed"
    );

    let create_issue = find_tool(&tools, "create_issue");
    assert!(
        create_issue["inputSchema"]["properties"]["description_markdown"].is_object(),
        "create_issue should keep its existing `description_markdown` field"
    );
}

/// Every `*_markdown`-suffixed tool should require a parameter that carries
/// the same `_markdown` suffix, so the tool name alone is a reliable guide
/// to the parameter name.
#[test]
fn markdown_tool_names_and_field_names_stay_in_sync() {
    let mut mcp = McpProcess::spawn();
    let tools = mcp.list_tools();

    for tool in tools.iter().filter(|t| {
        t["name"]
            .as_str()
            .is_some_and(|n| n.ends_with("_markdown") && n.starts_with("update"))
            || t["name"]
                .as_str()
                .is_some_and(|n| n.ends_with("_markdown") && n.starts_with("add"))
    }) {
        let required = required_params(tool);
        assert!(
            required.iter().any(|p| p.ends_with("_markdown")),
            "tool `{}` is markdown-suffixed but has no `_markdown`-suffixed required field: {required:?}",
            tool["name"]
        );
    }
}

/// The server's top-level instructions reference tool names directly (e.g.
/// `add_comment_markdown`) instead of a vague `*_markdown` pattern, so an
/// agent reading only the instructions still lands on the right names.
#[test]
fn server_instructions_name_the_actual_markdown_tools() {
    let mut mcp = McpProcess::spawn();
    let init = mcp
        .send(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0.0.0"},
            }),
        )
        .unwrap();
    let instructions = init["result"]["instructions"]
        .as_str()
        .expect("server should advertise instructions");

    assert!(
        instructions.contains("add_comment_markdown"),
        "instructions should name `add_comment_markdown` explicitly: {instructions}"
    );
    assert!(
        instructions.contains("update_description_markdown"),
        "instructions should name `update_description_markdown` explicitly: {instructions}"
    );
}
