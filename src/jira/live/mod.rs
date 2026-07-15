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

mod comments;
mod detail;
mod fields;
mod mutations;
mod search;
mod support;

pub use comments::add_comment;
pub use detail::fetch_detail;
pub use fields::{list_fields, FieldInfo};
pub use mutations::{
    apply_transition, assign_issue, assignable_users, create_issue, fetch_transitions,
    update_description, update_summary,
};
pub use search::{fetch_my_work, jql_for, search_issues, MY_WORK_JQL, SEARCH_RESULTS_CAP};
pub use support::whoami;
