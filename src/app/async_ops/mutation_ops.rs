//! Issue-mutation dispatch: transitions, assignment, description updates,
//! and comments. Each is a `dispatch_*`/`*_blocking` pair with no mutual
//! dependencies, so they move here verbatim.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::Comment;

use super::super::{App, Screen};
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

/// Spawn a Fix/Affects Version update off the render thread, sending the
/// result back as `AppEvent::VersionsApplied`. `fix_versions`/
/// `affects_versions` are each `None` when that field is unchanged in the
/// picker (see `App::confirm_version_picker`) — `set_versions_blocking`
/// skips a field entirely rather than sending a needless write for it.
pub(crate) fn dispatch_set_versions(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    key: String,
    fix_versions: Option<Vec<String>>,
    affects_versions: Option<Vec<String>>,
) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let fix_for_result = fix_versions.clone();
        let affects_for_result = affects_versions.clone();
        let (fix_error, affects_error) = tokio::task::spawn_blocking(move || {
            set_versions_blocking(&key, fix_versions.as_deref(), affects_versions.as_deref())
        })
        .await
        .unwrap_or_else(|_| {
            (
                Some("internal error: task panicked".into()),
                Some("internal error: task panicked".into()),
            )
        });
        let _ = tx.send(AppEvent::VersionsApplied {
            generation,
            key: key_for_result,
            fix_versions: if fix_error.is_none() {
                fix_for_result
            } else {
                None
            },
            fix_error,
            affects_versions: if affects_error.is_none() {
                affects_for_result
            } else {
                None
            },
            affects_error,
        });
    });
}

/// Mirrors `apply_transition_blocking`'s "no credentials means nothing to do
/// live" shape, for both fields independently — a `None` field is skipped
/// entirely rather than sent as an empty write.
#[allow(unused_variables)]
fn set_versions_blocking(
    key: &str,
    fix_versions: Option<&[String]>,
    affects_versions: Option<&[String]>,
) -> (Option<String>, Option<String>) {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            let fix_error = fix_versions.and_then(|v| {
                crate::jira::set_fix_versions(&cfg, key, v)
                    .err()
                    .map(|e| e.to_string())
            });
            let affects_error = affects_versions.and_then(|v| {
                crate::jira::set_affects_versions(&cfg, key, v)
                    .err()
                    .map(|e| e.to_string())
            });
            return (fix_error, affects_error);
        }
    }
    (None, None)
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

impl App {
    /// Applies `AppEvent::TransitionApplied` — see `dispatch_transition` above.
    pub(super) fn apply_transition_applied(
        &mut self,
        generation: u64,
        key: String,
        to: String,
        error: Option<String>,
    ) {
        if generation != self.transition_generation {
            return;
        }
        self.loading = false;
        self.transition_pending = false;
        if let Some(e) = error {
            self.status = format!("transition failed: {e}");
            return;
        }
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.status = to.clone();
            }
        }
        if let Some(sum) = self.issues.iter_mut().find(|i| i.key == key) {
            sum.status = to.clone();
        }
        self.status = format!("moved {key} → {to}");
        self.flash(format!("✓ moved to {to}"));
        if to == "Done" {
            self.trigger_jax_party();
        }
    }

    /// Applies `AppEvent::DescriptionUpdated` — see
    /// `dispatch_update_description` above.
    pub(super) fn apply_description_updated(
        &mut self,
        generation: u64,
        key: String,
        adf: serde_json::Value,
        error: Option<String>,
        return_screen: Screen,
    ) {
        if generation != self.edit_generation {
            return;
        }
        self.loading = false;
        self.edit_pending = false;
        self.screen = return_screen;
        if let Some(e) = error {
            self.status = format!("update failed: {e}");
            return;
        }
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.description = adf;
            }
        }
        self.status = format!("updated {key} description");
        self.flash("✓ description updated");
        self.trigger_jax_party();
    }

    /// Applies `AppEvent::CommentAdded` — see `dispatch_add_comment` above.
    pub(super) fn apply_comment_added(
        &mut self,
        generation: u64,
        key: String,
        result: Result<Comment, String>,
        return_screen: Screen,
    ) {
        if generation != self.edit_generation {
            return;
        }
        self.loading = false;
        self.edit_pending = false;
        self.screen = return_screen;
        let comment = match result {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("comment failed: {e}");
                return;
            }
        };
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.comments.push(comment.clone());
            }
        }
        if let Some(cached) = self.detail_cache.get_mut(&key) {
            cached.comments.push(comment);
        }
        self.status = format!("added comment to {key}");
        self.flash("✓ comment added");
        self.trigger_jax_party();
    }

    /// Applies `AppEvent::AssigneeApplied` — see `dispatch_assign` above.
    pub(super) fn apply_assignee_applied(
        &mut self,
        generation: u64,
        key: String,
        display_name: Option<String>,
        error: Option<String>,
    ) {
        if generation != self.assignee_generation {
            // A newer picker interaction (or the picker closing and
            // reopening) superseded this result; drop it silently,
            // mirroring `apply_transition_applied`'s stale-generation guard.
            return;
        }
        self.loading = false;
        self.assignee_pending = false;
        if let Some(e) = error {
            self.status = format!("assign failed: {e}");
            return;
        }
        self.apply_assignee_locally(&key, display_name.as_deref());
        self.status = match &display_name {
            Some(name) => format!("assigned {key} to {name}"),
            None => format!("unassigned {key}"),
        };
        self.flash(match &display_name {
            Some(name) => format!("✓ assigned to {name}"),
            None => "✓ unassigned".to_string(),
        });
    }

    /// Applies `AppEvent::VersionsApplied` — see `dispatch_set_versions`
    /// above. Unlike the other mutation applies, both fields can fail (or
    /// succeed) independently in one round-trip, so each is reported and
    /// applied on its own terms rather than the usual single error branch.
    pub(super) fn apply_versions_applied(
        &mut self,
        generation: u64,
        key: String,
        fix_versions: Option<Vec<String>>,
        fix_error: Option<String>,
        affects_versions: Option<Vec<String>>,
        affects_error: Option<String>,
    ) {
        if generation != self.version_generation {
            return;
        }
        self.loading = false;
        self.version_pending = false;
        self.apply_versions_locally(&key, fix_versions, affects_versions);
        match (&fix_error, &affects_error) {
            (None, None) => {
                self.status = format!("updated {key} versions");
                self.flash("✓ versions updated");
            }
            (Some(e), None) => self.status = format!("fix version update failed: {e}"),
            (None, Some(e)) => self.status = format!("affects version update failed: {e}"),
            (Some(fe), Some(ae)) => self.status = format!("version update failed: {fe}; {ae}"),
        }
    }
}
