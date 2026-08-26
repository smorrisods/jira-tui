//! Domain models — stable internal shapes independent of Jira's API surface.

mod demo;
mod types;

pub use demo::{
    demo_assignable_users, demo_detail, demo_issue_types, demo_issues, demo_issues_for_version,
    demo_open_sprints, demo_projects, demo_versions, DEMO_CURRENT_USER,
};
pub use types::{
    AssignableUser, Attachment, ChildIssue, Comment, IssueDetail, IssueLink, IssueSummary,
    IssueType, Priority, Project, Source, Sprint, Transition, Version, ViewKind,
};
