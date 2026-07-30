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
    add_comment, apply_transition, assign_issue, assignable_users, create_issue, delete_comment,
    fetch_comments, fetch_detail, fetch_my_work, fetch_transitions, jql_for, jql_for_version,
    list_create_issue_types, list_fields, list_versions, search_by_text, search_issues,
    search_users, set_affects_versions, set_fix_versions, update_comment, update_description,
    update_summary, whoami, FieldInfo, MY_WORK_JQL, SEARCH_RESULTS_CAP,
};
