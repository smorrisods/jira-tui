//! Async dispatch for `refresh`/`switch_view` against live Jira, and
//! applying the results back onto `App` once they arrive.
//!
//! Demo/cache-only sessions skip all of this and resolve inline (there's no
//! network round-trip worth a spinner for) — see `App::refresh` and
//! `App::switch_view`. A genuine live fetch is offloaded via
//! `tokio::task::spawn_blocking` (the Jira REST client is synchronous
//! `ureq`) and its result flows back over an `mpsc` channel, drained by the
//! run loop each iteration and applied here.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::{Comment, IssueDetail, IssueSummary, Source, ViewKind};

use super::{load_issues, load_issues_for, App, Screen};

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
        origin: super::field_mapping::FieldMappingOrigin,
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
    /// A one-shot background fetch of All Project Issues resolved,
    /// dispatched once at startup for a genuine live session purely to
    /// discover teammates earlier than the user manually visiting that
    /// view — see `dispatch_teammate_discovery`. Deliberately carries no
    /// `generation`: it never overwrites `all_issues`/`current_view` (only
    /// merges assignees into `teammates_seen`), so it can't be made stale
    /// by an unrelated refresh/switch_view and is safe to apply whenever
    /// it lands.
    TeammatesDiscovered { issues: Vec<IssueSummary> },
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
                use super::field_mapping::{build_catalog_and_selection, FieldMappingOrigin};

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
                            == super::FieldMappingOutcome::NotAvailable
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
            AppEvent::TeammatesDiscovered { issues } => {
                self.merge_teammates(&issues);
            }
        }
    }
}

/// Spawn `load_issues_for(view, force_demo)` off the render thread for a
/// `refresh`, sending the result back as `AppEvent::Refreshed`.
pub(crate) fn dispatch_refresh(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    view: ViewKind,
    force_demo: bool,
) {
    tokio::spawn(async move {
        let (issues, source, status) = load(view, force_demo).await;
        let _ = tx.send(AppEvent::Refreshed {
            generation,
            issues,
            source,
            status,
        });
    });
}

/// Spawn `load_issues_for(view, force_demo)` off the render thread for a
/// `switch_view`, sending the result back as `AppEvent::ViewSwitched`.
pub(crate) fn dispatch_switch_view(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    view: ViewKind,
    force_demo: bool,
) {
    tokio::spawn(async move {
        let view_for_result = view.clone();
        let (issues, source, status) = load(view, force_demo).await;
        let _ = tx.send(AppEvent::ViewSwitched {
            generation,
            view: view_for_result,
            issues,
            source,
            status,
        });
    });
}

/// `load_issues_for` calls the blocking `ureq`-based Jira client, so it runs
/// on the blocking-task pool rather than a runtime worker thread.
async fn load(view: ViewKind, force_demo: bool) -> (Vec<IssueSummary>, Source, String) {
    tokio::task::spawn_blocking(move || load_issues_for(&view, force_demo))
        .await
        .unwrap_or_else(|_| {
            (
                Vec::new(),
                Source::Demo,
                "internal error: fetch task panicked".into(),
            )
        })
}

/// Spawn a one-shot background fetch of All Project Issues, sending the
/// result back as `AppEvent::TeammatesDiscovered`. Dispatched once from
/// `App::new` for a genuine live session so the view picker's teammate list
/// is populated without the user having to manually visit All Project
/// Issues first — see `App::merge_teammates`, which applies the result
/// without disturbing `all_issues`/`current_view`. `force_demo` is always
/// `false` here: this is only ever dispatched for a live session.
pub(crate) fn dispatch_teammate_discovery(tx: UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let (issues, _source, _status) = load(ViewKind::AllProject, false).await;
        let _ = tx.send(AppEvent::TeammatesDiscovered { issues });
    });
}

/// Spawn a full-detail fetch off the render thread, sending the result back
/// as `AppEvent::DetailLoaded`. Only dispatched when `App::load_detail`
/// would otherwise make a real live-network call (see `detail.rs`) — demo
/// and cache sessions resolve inline.
pub(crate) fn dispatch_detail_fetch(tx: UnboundedSender<AppEvent>, generation: u64, key: String) {
    tokio::spawn(async move {
        let key_for_result = key.clone();
        let (detail, status) = tokio::task::spawn_blocking(move || fetch_detail_blocking(&key))
            .await
            .unwrap_or_else(|_| {
                (
                    crate::domain::demo_detail(&key_for_result),
                    Some("internal error: fetch task panicked".into()),
                )
            });
        let _ = tx.send(AppEvent::DetailLoaded {
            generation,
            key: key_for_result,
            detail: Box::new(detail),
            status,
        });
    });
}

/// Mirrors the live branch of `App::load_detail`, minus the `&mut self`
/// access a background task can't have. Falls back to the offline demo
/// detail on any failure, exactly like the synchronous version.
#[allow(unused_variables)]
fn fetch_detail_blocking(key: &str) -> (IssueDetail, Option<String>) {
    #[cfg(feature = "live")]
    {
        if let Some(cfg) = crate::jira::Config::load() {
            match crate::jira::fetch_detail(&cfg, key) {
                Ok(d) => return (d, Some(format!("Loaded {key}"))),
                Err(e) => {
                    return (
                        crate::domain::demo_detail(key),
                        Some(format!("Live fetch failed ({e}); showing sample")),
                    )
                }
            }
        }
    }
    (crate::domain::demo_detail(key), None)
}

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

/// Spawn a custom-field lookup off the render thread, sending the result
/// back as `AppEvent::FieldsLoaded`. Only dispatched by
/// `App::dispatch_field_mapping` once it's already confirmed a live source
/// and loaded credentials — this always makes the real network call.
pub(crate) fn dispatch_field_mapping(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    origin: super::field_mapping::FieldMappingOrigin,
) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(list_fields_blocking)
            .await
            .unwrap_or_else(|_| Err("internal error: task panicked".into()));
        let _ = tx.send(AppEvent::FieldsLoaded {
            generation,
            origin,
            result,
        });
    });
}

/// A field-mapping fetch's result: the catalog as `(id, name)` pairs
/// alongside the field currently mapped in `config.toml` (if any).
type FieldsFetchResult = Result<(Vec<(String, String)>, Option<String>), String>;

/// Mirrors the live branch of the old synchronous `open_field_mapping`,
/// including the "no credentials configured" case, which is now an `Err`
/// applied via `AppEvent::FieldsLoaded` instead of a synchronous
/// `NotAvailable` return (see `field_mapping.rs`'s module docs).
#[allow(unused_variables)]
fn list_fields_blocking() -> FieldsFetchResult {
    #[cfg(feature = "live")]
    {
        let Some(cfg) = crate::jira::Config::load() else {
            return Err("No live credentials configured.".into());
        };
        crate::jira::list_fields(&cfg)
            .map(|fields| {
                let current_mapping = cfg.acceptance_criteria_field.clone();
                (
                    fields.into_iter().map(|f| (f.id, f.name)).collect(),
                    current_mapping,
                )
            })
            .map_err(|e| e.to_string())
    }
    #[cfg(not(feature = "live"))]
    Err("This build has no live support; rebuild with the `live` feature.".into())
}

/// Spawn onboarding's credential-verification fetch off the render thread,
/// sending the result back as `AppEvent::CredentialsVerified`. Reuses
/// `load_issues` (the same `ViewKind::MyWork` fetch behind a plain
/// `refresh`), since verifying fresh credentials is just loading My Work
/// with whatever was just saved to the environment/config. Only called from
/// `onboarding.rs`'s live-gated `submit_credentials`, so it's dead code in
/// a no-live build.
#[cfg_attr(not(feature = "live"), allow(dead_code))]
pub(crate) fn dispatch_verify_credentials(tx: UnboundedSender<AppEvent>, generation: u64) {
    tokio::spawn(async move {
        let (issues, source, status) = tokio::task::spawn_blocking(|| load_issues(false))
            .await
            .unwrap_or_else(|_| {
                (
                    Vec::new(),
                    Source::Demo,
                    "internal error: fetch task panicked".into(),
                )
            });
        let _ = tx.send(AppEvent::CredentialsVerified {
            generation,
            issues,
            source,
            status,
        });
    });
}
