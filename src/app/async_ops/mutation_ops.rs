//! Issue-mutation dispatch: transitions, assignment, description updates,
//! and comments. Each is a `dispatch_*`/`*_blocking` pair with no mutual
//! dependencies, so they move here verbatim.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::Comment;

use super::super::{App, ReleaseBulkKind, Screen};
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

/// Spawn a bulk add-to-release or remove-from-release off the render
/// thread, sending the result back as `AppEvent::ReleaseBulkApplied`. See
/// `App::release_remove_selected`/`release_add_to_release` for the two
/// call sites.
pub(crate) fn dispatch_release_bulk(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    version_name: String,
    keys: Vec<String>,
    kind: ReleaseBulkKind,
) {
    tokio::spawn(async move {
        let version_for_result = version_name.clone();
        let results =
            tokio::task::spawn_blocking(move || release_bulk_blocking(&version_name, &keys, kind))
                .await
                .unwrap_or_default();
        let _ = tx.send(AppEvent::ReleaseBulkApplied {
            generation,
            version_name: version_for_result,
            kind,
            results,
        });
    });
}

/// For each key: fetch its current `fixVersions` (needed so add/remove only
/// touches `version_name`, preserving any other release the issue already
/// targets — Jira has no add/remove endpoint, only "replace the whole
/// array"), edit it, and write the result back. One issue's failure doesn't
/// stop the rest — each gets its own `Result` in the returned `Vec`.
#[allow(unused_variables)]
fn release_bulk_blocking(
    version_name: &str,
    keys: &[String],
    kind: ReleaseBulkKind,
) -> Vec<(String, Result<(), String>)> {
    #[cfg(feature = "live")]
    {
        let Some(cfg) = crate::jira::Config::load() else {
            return keys
                .iter()
                .map(|k| (k.clone(), Err("no credentials configured".to_string())))
                .collect();
        };
        keys.iter()
            .map(|key| {
                let outcome = (|| {
                    let detail = crate::jira::fetch_detail(&cfg, key).map_err(|e| e.to_string())?;
                    let mut versions = detail.fix_versions;
                    match kind {
                        ReleaseBulkKind::Add => {
                            if !versions.iter().any(|v| v == version_name) {
                                versions.push(version_name.to_string());
                            }
                        }
                        ReleaseBulkKind::Remove => versions.retain(|v| v != version_name),
                    }
                    crate::jira::set_fix_versions(&cfg, key, &versions).map_err(|e| e.to_string())
                })();
                (key.clone(), outcome)
            })
            .collect()
    }
    #[cfg(not(feature = "live"))]
    Vec::new()
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

/// Spawn a new-issue creation off the render thread, sending the result back
/// as `AppEvent::IssueCreated`. `local_key` is the key `create_issue_blocking`
/// falls back to if there's no live config to actually create against — the
/// same "second safety net" shape as `add_comment_blocking`'s optimistic
/// local comment, precomputed by the caller (`App::apply_new_issue`, via
/// `next_local_key`) since only it knows the session's local-key counter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_create_issue(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    project: String,
    issue_type: String,
    summary: String,
    description: Option<serde_json::Value>,
    local_key: String,
) {
    tokio::spawn(async move {
        let issue_type_for_result = issue_type.clone();
        let summary_for_result = summary.clone();
        let description_for_result = description.clone();
        let result = tokio::task::spawn_blocking(move || {
            create_issue_blocking(
                &project,
                &issue_type,
                &summary,
                description.as_ref(),
                &local_key,
            )
        })
        .await
        .unwrap_or_else(|_| Err("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::IssueCreated {
            generation,
            issue_type: issue_type_for_result,
            summary: summary_for_result,
            description: description_for_result,
            result,
        });
    });
}

#[allow(unused_variables)]
fn create_issue_blocking(
    project: &str,
    issue_type: &str,
    summary: &str,
    description: Option<&serde_json::Value>,
    local_key: &str,
) -> Result<String, String> {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            return crate::jira::create_issue(&cfg, project, summary, issue_type, description)
                .map_err(|e| e.to_string());
        }
    }
    Ok(local_key.to_string())
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

    /// Applies `AppEvent::IssueCreated` — see `dispatch_create_issue` above.
    /// On failure, lands back on `Screen::NewIssue` (not `Screen::Home`) so
    /// the user can fix a bad project/permission error and resubmit without
    /// retyping the summary — the compose form's own state (`self.new_issue`)
    /// hasn't been touched yet at this point, so it's still there to retry
    /// against.
    pub(super) fn apply_issue_created(
        &mut self,
        generation: u64,
        issue_type: String,
        summary: String,
        description: Option<serde_json::Value>,
        result: Result<String, String>,
    ) {
        if generation != self.edit_generation {
            return;
        }
        self.loading = false;
        self.edit_pending = false;
        // Safe to reset now regardless of outcome: `edit_pending` just went
        // false, so `apply_edit`'s re-entrancy guard no longer needs
        // `edit_target`/`edit_return_screen` to still describe this session
        // (unlike while the dispatch was in flight — see `apply_new_issue`).
        // Both branches below set `self.screen` directly rather than reading
        // `edit_return_screen`, so this can't strand either one.
        self.reset_edit_target();
        let key = match result {
            Ok(k) => k,
            Err(e) => {
                self.status = format!("create failed: {e}");
                self.screen = Screen::NewIssue;
                return;
            }
        };
        self.land_new_issue(key.clone(), issue_type, summary, description);
        self.new_issue = super::super::NewIssueState::default();
        self.status = format!("created {key}");
        self.flash(format!("✓ created {key}"));
        self.trigger_jax_party();
        self.open_by_key(&key);
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

    /// Applies `AppEvent::ReleaseBulkApplied` — see `dispatch_release_bulk`
    /// above. Each successful `Remove` drops that issue from
    /// `release.issues`/`release.selected`; a successful `Add` doesn't
    /// touch `release.issues` directly (the issue may not have been in the
    /// drilled list at all) — `refresh_release_drill_if_showing` re-fetches
    /// instead, so the list reflects the real server state rather than a
    /// locally-guessed one.
    pub(super) fn apply_release_bulk_applied(
        &mut self,
        generation: u64,
        version_name: String,
        kind: ReleaseBulkKind,
        results: Vec<(String, Result<(), String>)>,
    ) {
        if generation != self.release_bulk_generation {
            return;
        }
        self.loading = false;
        self.release.bulk_pending = false;

        // Whether the drill-down is still showing the exact version this
        // bulk op was for — the user may have backed out and drilled into a
        // different one while it was in flight. `release_bulk_generation`
        // only distinguishes this op from a *newer* one, not from unrelated
        // navigation in between, so it can't stand in for this check: a
        // late-arriving Remove would otherwise `retain()`/clamp
        // `release.issues` against whatever version is now on screen,
        // possibly dropping an issue that belongs to both versions from the
        // wrong one's list. Mirrors the Add branch's own
        // `refresh_release_drill_if_showing` guard below.
        let still_showing =
            self.release.drilled.as_ref().map(|v| v.name.as_str()) == Some(version_name.as_str());

        let mut failures = 0usize;
        for (key, result) in &results {
            match result {
                Ok(()) => {
                    if still_showing {
                        self.release.selected.remove(key);
                        if kind == ReleaseBulkKind::Remove {
                            self.release.issues.retain(|i| &i.key != key);
                        }
                    }
                    self.apply_versions_locally_for_bulk(key, &version_name, kind);
                }
                Err(_) => failures += 1,
            }
        }
        let succeeded = results.len() - failures;
        self.status = if failures == 0 {
            self.flash(format!("✓ updated {succeeded} issue(s)"));
            format!("updated {succeeded} issue(s) for {version_name}")
        } else {
            format!("updated {succeeded} issue(s), {failures} failed for {version_name}")
        };

        if kind == ReleaseBulkKind::Remove {
            if still_showing {
                let len = self.release.issues.len();
                self.release.issue_cursor = self.release.issue_cursor.min(len.saturating_sub(1));
            }
        } else {
            self.refresh_release_drill_if_showing(&version_name);
        }
    }
}
