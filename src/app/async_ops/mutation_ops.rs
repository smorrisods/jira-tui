//! Issue-mutation dispatch: transitions, assignment, description updates,
//! and comments. Each is a `dispatch_*`/`*_blocking` pair with no mutual
//! dependencies, so they move here verbatim.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::Comment;

use super::super::Screen;
use super::AppEvent;

/// Spawn a workflow transition off the render thread, sending the result
/// back as `AppEvent::TransitionApplied`.
pub(crate) fn dispatch_transition(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    transition_id: String,
    to: String,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let to_for_result = to.clone();
        let error =
            tokio::task::spawn_blocking(move || apply_transition_blocking(&key, &transition_id))
                .await
                .unwrap_or_else(|_| Some("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::TransitionApplied {
            generation,
            key: key_for_result,
            to: to_for_result,
            error,
        });
    });
}

/// Mirrors the live branch of the old synchronous `confirm_transition`: no
/// credentials/config means "nothing to do live", not an error.
#[allow(unused_variables)]
fn apply_transition_blocking(key: &str, transition_id: &str) -> Option<String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::apply_transition(&cfg, key, transition_id)
                .err()
                .map(|e| e.to_string());
        }
    }
    None
}

/// Spawn an assignee change off the render thread, sending the result back
/// as `AppEvent::AssigneeApplied`. `account_id`/`display_name` are both
/// `None` together to unassign, or both `Some` to assign to a specific
/// teammate — mirrors `dispatch_transition`'s shape.
pub(crate) fn dispatch_assign(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    account_id: Option<String>,
    display_name: Option<String>,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let display_name_for_result = display_name.clone();
        let error =
            tokio::task::spawn_blocking(move || assign_issue_blocking(&key, account_id.as_deref()))
                .await
                .unwrap_or_else(|_| Some("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::AssigneeApplied {
            generation,
            key: key_for_result,
            display_name: if error.is_none() {
                display_name_for_result
            } else {
                None
            },
            error,
        });
    });
}

/// Mirrors `apply_transition_blocking`'s "no credentials means nothing to do
/// live" shape.
#[allow(unused_variables)]
fn assign_issue_blocking(key: &str, account_id: Option<&str>) -> Option<String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::assign_issue(&cfg, key, account_id)
                .err()
                .map(|e| e.to_string());
        }
    }
    None
}

/// Spawn a description update off the render thread, sending the result
/// back as `AppEvent::DescriptionUpdated`.
pub(crate) fn dispatch_update_description(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    adf: serde_json::Value,
    return_screen: Screen,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let adf_for_result = adf.clone();
        let error = tokio::task::spawn_blocking(move || update_description_blocking(&key, &adf))
            .await
            .unwrap_or_else(|_| Some("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::DescriptionUpdated {
            generation,
            key: key_for_result,
            adf: adf_for_result,
            error,
            return_screen,
        });
    });
}

#[allow(unused_variables)]
fn update_description_blocking(key: &str, adf: &serde_json::Value) -> Option<String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::update_description(&cfg, key, adf)
                .err()
                .map(|e| e.to_string());
        }
    }
    None
}

/// Spawn a new-comment post off the render thread, sending the result back
/// as `AppEvent::CommentAdded`. `local_author`/`local_id` seed the
/// locally-composed fallback comment used when there's no live client to
/// post to (mirrors the old synchronous behaviour, which always built this
/// optimistic comment before possibly overwriting it with the server's
/// copy).
pub(crate) fn dispatch_add_comment(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    adf: serde_json::Value,
    local_author: String,
    local_id: String,
    return_screen: Screen,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let result = tokio::task::spawn_blocking(move || {
            add_comment_blocking(&key, &adf, &local_author, &local_id)
        })
        .await
        .unwrap_or_else(|_| Err("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::CommentAdded {
            generation,
            key: key_for_result,
            result,
            return_screen,
        });
    });
}

#[allow(unused_variables)]
fn add_comment_blocking(
    key: &str,
    adf: &serde_json::Value,
    local_author: &str,
    local_id: &str,
) -> Result<Comment, String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::add_comment(&cfg, key, adf).map_err(|e| e.to_string());
        }
    }
    Ok(Comment {
        id: local_id.to_string(),
        author: local_author.to_string(),
        created: "just now".into(),
        body: adf.clone(),
    })
}
