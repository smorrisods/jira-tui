//! The live Jira REST client (`ureq`-based): reads, workflow transitions,
//! description/summary writes, comments, and issue creation.
//!
//! `support`, `search`, `mutations`, `comments`, and `fields` are split by
//! REST-endpoint area, each independent of the others (beyond depending on
//! `support`'s HTTP core). `detail` is not a peer of those five — it's an
//! aggregation layer: assembling one `IssueDetail` inherently means calling
//! into several of them (transitions, comments, an Epic's children via
//! `search`), so it will keep growing as `fetch_detail` accretes more
//! sub-fetches, in a way the REST-endpoint files structurally don't. Treat
//! `detail.rs` crossing back over ~500 lines as expected eventually, not as
//! a sign this split needs redoing — the fix then is splitting what
//! `fetch_detail` assembles (e.g. one file per sub-fetch it stitches
//! together), not re-drawing the endpoint-area boundary.

mod attachments;
mod comments;
mod detail;
mod fields;
mod issue_types;
mod mutations;
mod projects;
mod search;
mod sprint;
mod support;
mod versions;

pub use attachments::{download_attachment, fetch_attachments, media_uuid_for, upload_attachment};
// `guess_mime` lives in `crate::mime` (shared across every feature set —
// see that module's doc comment), not in this `live`-only module; re-export
// it here anyway so `jira::guess_mime` keeps resolving for existing/future
// live-feature callers.
pub use crate::mime::guess_mime;
pub use comments::{add_comment, delete_comment, fetch_comments, update_comment};
pub use detail::fetch_detail;
pub use fields::{list_fields, FieldInfo};
pub use issue_types::list_create_issue_types;
pub use mutations::{
    apply_transition, assign_issue, assignable_users, create_issue, fetch_transitions,
    search_users, set_priority, update_description, update_summary,
};
pub use projects::list_projects;
pub use search::{
    fetch_my_work, jql_for, jql_for_version, search_by_text, search_issues, MY_WORK_JQL,
    SEARCH_RESULTS_CAP,
};
pub use sprint::{assign_sprint, list_open_sprints, remove_from_sprint};
pub use support::{get_bytes_public, whoami};
pub use versions::{list_versions, set_affects_versions, set_fix_versions};
