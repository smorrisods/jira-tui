//! Domain models — stable internal shapes independent of Jira's API surface.

mod demo;
mod types;

pub use demo::{demo_assignable_users, demo_detail, demo_issues, DEMO_CURRENT_USER};
pub use types::{
    AssignableUser, ChildIssue, Comment, IssueDetail, IssueLink, IssueSummary, Priority, Source,
    Transition, ViewKind,
};
