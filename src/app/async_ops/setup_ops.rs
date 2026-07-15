//! Field-mapping lookup and onboarding's credential-verification fetch.

use tokio::sync::mpsc::UnboundedSender;

use crate::domain::{IssueSummary, Source};

#[cfg(feature = "live")]
use super::super::load_issues;
use super::super::App;
use super::AppEvent;

/// Spawn a custom-field lookup off the render thread, sending the result
/// back as `AppEvent::FieldsLoaded`. Only dispatched by
/// `App::dispatch_field_mapping` once it's already confirmed a live source
/// and loaded credentials — this always makes the real network call.
pub(crate) fn dispatch_field_mapping(
    tx: UnboundedSender<AppEvent>,
    generation: u64,
    origin: super::super::field_mapping::FieldMappingOrigin,
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
pub(super) type FieldsFetchResult = Result<(Vec<(String, String)>, Option<String>), String>;

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
/// `onboarding.rs`'s live-gated `submit_credentials`, so this is compiled
/// out entirely (not merely unreachable) in a no-live build — gating the
/// function itself, rather than suppressing the resulting dead-code lint,
/// means there's nothing to suppress at either this definition or its
/// re-export in `async_ops/mod.rs`.
#[cfg(feature = "live")]
pub(crate) fn dispatch_verify_credentials(tx: UnboundedSender<AppEvent>, generation: u64) {
    tokio::spawn(async move {
        let (issues, source, status) = tokio::task::spawn_blocking(|| load_issues(false))
            .await
            .unwrap_or_else(|_| {
                (
                    Vec::new(),
                    crate::domain::Source::Demo,
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

impl App {
    /// Applies `AppEvent::FieldsLoaded` — see `dispatch_field_mapping` above.
    pub(super) fn apply_fields_loaded(
        &mut self,
        generation: u64,
        origin: super::super::field_mapping::FieldMappingOrigin,
        result: FieldsFetchResult,
    ) {
        use super::super::field_mapping::{build_catalog_and_selection, FieldMappingOrigin};
        use super::super::Screen;

        if generation != self.field_mapping_generation {
            return;
        }
        self.loading = false;
        self.field_mapping_pending = false;

        let connected_status = match &origin {
            FieldMappingOrigin::Direct => None,
            FieldMappingOrigin::Onboarding { connected_status } => Some(connected_status.clone()),
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
                // catalog.len() - 1 for the leading "none" sentinel; clears
                // the "↻ looking up…" status left by `dispatch_field_mapping`
                // (unlike most other async ops, the old synchronous code
                // never set a status on this path, so there's nothing to
                // "restore" — just something that isn't a stale spinner
                // message).
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
                    // A transient failure here shouldn't block finishing
                    // onboarding — but it's easy to forget the field-mapping
                    // screen exists at all if it silently never appears, so
                    // leave a toast pointing at `F` rather than just the raw
                    // error.
                    self.screen = Screen::Home;
                    self.status = status;
                    self.flash("Couldn't look up custom fields — press F to try again");
                }
            }
        }
    }

    /// Applies `AppEvent::CredentialsVerified` — see
    /// `dispatch_verify_credentials` above.
    pub(super) fn apply_credentials_verified(
        &mut self,
        generation: u64,
        issues: Vec<IssueSummary>,
        source: Source,
        status: String,
    ) {
        use super::super::Screen;

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
                // Offer to map "Acceptance Criteria" (or another custom
                // field) now, while we're already talking to Jira. That
                // lookup dispatches its own async fetch — see
                // `FieldMappingOrigin::Onboarding`.
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
}
