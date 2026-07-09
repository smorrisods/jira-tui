//! Jira configuration and (optional) live REST client.
//!
//! Reads credentials from the environment or a token file, and an optional
//! `~/.config/jira-tui/config.toml` for non-secret settings. When `live` is
//! disabled or credentials are missing, the app falls back to demo data —
//! the TUI is always explorable.
//!
//! Split into `config` (credentials/settings assembly) and `live` (the
//! actual `ureq`-based REST client), both gated behind the `live` feature.
//! `ureq` is a blocking client — the async run loop (`src/main.rs`) offloads
//! every call here onto `tokio::task::spawn_blocking` rather than awaiting
//! it directly.

#[cfg(feature = "live")]
mod config;
#[cfg(feature = "live")]
mod live;

#[cfg(feature = "live")]
pub use config::Config;

#[cfg(feature = "live")]
pub use live::{
    add_comment, apply_transition, create_issue, fetch_detail, fetch_my_work, fetch_transitions,
    jql_for, list_fields, search_issues, update_description, update_summary, whoami, FieldInfo,
    MY_WORK_JQL, SEARCH_RESULTS_CAP,
};
