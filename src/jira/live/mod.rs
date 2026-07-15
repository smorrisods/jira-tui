//! The live Jira REST client (`ureq`-based): reads, workflow transitions,
//! description/summary writes, comments, and issue creation.
//!
//! Split by REST-endpoint area — `support` (HTTP core + shared parsers,
//! everything else depends on it), `search` (JQL + paged search, which
//! `detail` also depends on for an Epic's children), `mutations`
//! (transitions/assignment/field writes/create), `comments`, `fields`, and
//! `detail` (the most interconnected file, assembled from the others).

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
