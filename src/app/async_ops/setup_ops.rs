//! Field-mapping lookup and onboarding's credential-verification fetch.

use tokio::sync::mpsc::UnboundedSender;

use super::super::load_issues;
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
