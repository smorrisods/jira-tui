//! The `AppEvent` enum and `apply_event` dispatcher — the state-machine
//! core that every `dispatch_*` fn in the sibling files feeds into via the
//! `mpsc` channel described in the module doc comment.

use crate::domain::{AssignableUser, Comment, IssueDetail, IssueSummary, Source, ViewKind};

use super::super::{App, Screen};
use super::setup_ops::FieldsFetchResult;

/// Sent back from a spawned fetch once it completes. Carries the
/// `generation` it was dispatched under so a fetch that's been superseded
/// by a newer refresh/switch_view (triggered before this one resolved) can
/// be dropped instead of clobbering fresher state.
pub enum AppEvent {
    Refreshed {
        generation: u64,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    },
    ViewSwitched {
        generation: u64,
        view: ViewKind,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    },
    /// A full-detail fetch resolved. `navigate` distinguishes an explicit
    /// "open" (jump to `Screen::Detail` once loaded) from the quick-view
    /// panel's lazy background load (cache only, stay put). Whether to
    /// navigate is decided at apply-time from `App::detail_pending` rather
    /// than carried on the event itself, so a fetch dispatched as a
    /// cache-only quick-view load that later gets "upgraded" by an explicit
    /// open (before the first one resolves) still navigates once it lands.
    DetailLoaded {
        generation: u64,
        key: String,
        detail: Box<IssueDetail>,
        status: Option<String>,
    },
    /// A workflow transition resolved (or failed) against live Jira.
    TransitionApplied {
        generation: u64,
        key: String,
        to: String,
        error: Option<String>,
    },
    /// A description update resolved. `return_screen` is where the edit
    /// flow should land once applied, matching the synchronous behaviour
    /// this replaces.
    DescriptionUpdated {
        generation: u64,
        key: String,
        adf: serde_json::Value,
        error: Option<String>,
        return_screen: Screen,
    },
    /// A new comment resolved — either the server's copy of the comment
    /// (live) or the locally-composed one (no credentials/offline).
    CommentAdded {
        generation: u64,
        key: String,
        result: Result<Comment, String>,
        return_screen: Screen,
    },
    /// A field-mapping custom-field lookup resolved. `origin` decides how
    /// the result is applied — see `FieldMappingOrigin`. Fields are plain
    /// `(id, name)` pairs rather than `jira::FieldInfo` so this variant (and
    /// `apply_event`) compile under every feature set. The `Option<String>`
    /// alongside them is the field currently mapped in `config.toml` (read
    /// fresh inside the same fetch, since it needs the same `Config` the
    /// fetch itself loads), used to pre-select the catalog.
    FieldsLoaded {
        generation: u64,
        origin: super::super::field_mapping::FieldMappingOrigin,
        result: FieldsFetchResult,
    },
    /// Onboarding's credential-verification fetch resolved. Whether the
    /// credentials were actually accepted is decided at apply-time from
    /// `source` (a genuine `Source::Live` means success), exactly like the
    /// synchronous `submit_credentials` this replaces used to check.
    CredentialsVerified {
        generation: u64,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    },
    /// A one-shot background fetch of the project's assignable users
    /// resolved, dispatched once at startup for a genuine live session
    /// purely to discover teammates earlier than the user manually
    /// visiting All Project Issues — see `dispatch_teammate_discovery`.
    /// Also seeds `App::assignable_users`, the same list the assignee
    /// picker (`A`) draws from, so opening it doesn't need its own
    /// dedicated fetch. Deliberately carries no `generation`: it never
    /// overwrites `all_issues`/`current_view` (only merges names into
    /// `teammates_seen` and replaces `assignable_users` wholesale), so it
    /// can't be made stale by an unrelated refresh/switch_view and is safe
    /// to apply whenever it lands.
    TeammatesDiscovered { users: Vec<AssignableUser> },
    /// An assignee change (or unassign, when `account_id`/`display_name`
    /// were `None`) resolved against live Jira — see
    /// `App::confirm_assignee`/`dispatch_assign`.
    AssigneeApplied {
        generation: u64,
        key: String,
        display_name: Option<String>,
        error: Option<String>,
    },
}

impl App {
    /// Bump and return the current fetch generation counter. Every
    /// dispatched fetch is tagged with the generation it was started
    /// under; `apply_event` drops results whose generation has since gone
    /// stale.
    pub(crate) fn bump_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// Apply a completed fetch's result, unless it's been superseded by a
    /// newer refresh/switch_view dispatched after it.
    pub fn apply_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Refreshed {
                generation,
                issues,
                source,
                status,
            } => {
                if generation != self.generation {
                    return;
                }
                self.loading = false;
                self.all_issues = issues;
                self.source = source;
                self.status = format!("↻ {status}");
                self.recompute_view();
            }
            AppEvent::ViewSwitched {
                generation,
                view,
                issues,
                source,
                status,
            } => {
                if generation != self.generation {
                    return;
                }
                self.loading = false;
                self.all_issues = issues;
                self.source = source;
                let label = view.label();
                self.current_view = view;
                self.status = format!("↻ {status}");
                self.selected = 0;
                self.recompute_view();
                self.flash(format!("viewing: {label}"));
            }
            AppEvent::DetailLoaded {
                generation,
                key,
                detail,
                status,
            } => {
                if generation != self.detail_generation {
                    return;
                }
                self.loading = false;
                // The escalated navigate intent lives on `detail_pending`,
                // not the event — a fetch dispatched as a cache-only
                // quick-view load can be "upgraded" by an explicit open
                // before it resolves (see `dispatch_detail_fetch`).
                let navigate = self
                    .detail_pending
                    .take()
                    .map(|(_, navigate)| navigate)
                    .unwrap_or(false);
                if let Some(status) = status {
                    self.status = status;
                }
                self.detail_cache.insert(key.clone(), (*detail).clone());
                if navigate {
                    self.detail_scroll = 0;
                    self.detail = Some(*detail);
                    self.screen = Screen::Detail;
                    if let Some(pos) = self.issues.iter().position(|i| i.key == key) {
                        self.selected = pos;
                    }
                }
            }
            AppEvent::TransitionApplied {
                generation,
                key,
                to,
                error,
            } => {
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
            }
            AppEvent::DescriptionUpdated {
                generation,
                key,
                adf,
                error,
                return_screen,
            } => {
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
            }
            AppEvent::CommentAdded {
                generation,
                key,
                result,
                return_screen,
            } => {
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
            }
            AppEvent::FieldsLoaded {
                generation,
                origin,
                result,
            } => {
                use super::super::field_mapping::{
                    build_catalog_and_selection, FieldMappingOrigin,
                };

                if generation != self.field_mapping_generation {
                    return;
                }
                self.loading = false;
                self.field_mapping_pending = false;

                let connected_status = match &origin {
                    FieldMappingOrigin::Direct => None,
                    FieldMappingOrigin::Onboarding { connected_status } => {
                        Some(connected_status.clone())
                    }
                };

                match result {
                    Ok((fields, current_mapping)) if fields.is_empty() => {
                        self.field_mapping.current_mapping = current_mapping;
                        self.status = "No custom fields found on this Jira site.".into();
                        if let Some(status) = connected_status {
                            self.screen = Screen::Home;
                            self.status = status;
                        }
                    }
                    Ok((fields, current_mapping)) => {
                        // catalog.len() - 1 for the leading "none" sentinel;
                        // clears the "↻ looking up…" status left by
                        // `dispatch_field_mapping` (unlike most other async
                        // ops, the old synchronous code never set a status
                        // on this path, so there's nothing to "restore" —
                        // just something that isn't a stale spinner message).
                        let count = fields.len();
                        let (catalog, selected) =
                            build_catalog_and_selection(fields, current_mapping.as_deref());
                        self.field_mapping.catalog = catalog;
                        self.field_mapping.query.clear();
                        self.field_mapping.selected = selected;
                        self.field_mapping.current_mapping = current_mapping;
                        self.screen = Screen::FieldMapping;
                        self.status = format!("Loaded {count} custom fields.");
                    }
                    Err(e) => {
                        self.status = format!("Could not fetch fields: {e}");
                        if let Some(status) = connected_status {
                            // A transient failure here shouldn't block
                            // finishing onboarding — but it's easy to forget
                            // the field-mapping screen exists at all if it
                            // silently never appears, so leave a toast
                            // pointing at `F` rather than just the raw error.
                            self.screen = Screen::Home;
                            self.status = status;
                            self.flash("Couldn't look up custom fields — press F to try again");
                        }
                    }
                }
            }
            AppEvent::CredentialsVerified {
                generation,
                issues,
                source,
                status,
            } => {
                if generation != self.onboarding_generation {
                    return;
                }
                self.loading = false;
                self.onboarding_pending = false;
                match source {
                    Source::Live { .. } => {
                        self.all_issues = issues;
                        self.source = source;
                        self.status = status;
                        self.recompute_view();
                        crate::config::mark_onboarded();
                        // Offer to map "Acceptance Criteria" (or another
                        // custom field) now, while we're already talking to
                        // Jira. That lookup dispatches its own async fetch —
                        // see `FieldMappingOrigin::Onboarding`.
                        let connected_status = self.status.clone();
                        if self.open_field_mapping_for_onboarding(connected_status.clone())
                            == super::super::FieldMappingOutcome::NotAvailable
                        {
                            self.screen = Screen::Home;
                            self.status = connected_status;
                        }
                    }
                    _ => {
                        self.onboarding.setup_msg =
                                "Saved, but Jira did not accept those credentials. Check and retry, or press Esc to continue in demo mode.".into();
                    }
                }
            }
            AppEvent::TeammatesDiscovered { users } => {
                let names: Vec<String> = users.iter().map(|u| u.display_name.clone()).collect();
                self.merge_teammate_names(&names);
                self.assignable_users = users;
            }
            AppEvent::AssigneeApplied {
                generation,
                key,
                display_name,
                error,
            } => {
                if generation != self.assignee_generation {
                    // A newer picker interaction (or the picker closing and
                    // reopening) superseded this result; drop it silently,
                    // mirroring `TransitionApplied`'s stale-generation guard.
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
        }
    }
}
