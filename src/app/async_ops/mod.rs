//! Async dispatch for `refresh`/`switch_view` against live Jira, and
//! applying the results back onto `App` once they arrive.
//!
//! Demo/cache-only sessions skip all of this and resolve inline (there's no
//! network round-trip worth a spinner for) — see `App::refresh` and
//! `App::switch_view`. A genuine live fetch is offloaded via
//! `tokio::task::spawn_blocking` (the Jira REST client is synchronous
//! `ureq`) and its result flows back over an `mpsc` channel, drained by the
//! run loop each iteration and applied here.
//!
//! Split into `events` (the `AppEvent` enum + a thin `apply_event`
//! dispatcher — one match arm per variant, each just calling an
//! `App::apply_*` method), `list_ops` (refresh/switch view/detail load/
//! teammate discovery), `mutation_ops` (transitions, assignment,
//! description updates, comments), and `setup_ops` (field-mapping lookup,
//! onboarding credential verification). Each of `list_ops`/`mutation_ops`/
//! `setup_ops` holds `dispatch_*`/`*_blocking` pairs *and* the
//! `App::apply_*` method that applies that operation's result, so a given
//! operation's dispatch and apply logic live together — `events.rs` only
//! grows by one match arm (not a whole logic block) per new `AppEvent`
//! variant.

mod events;
mod list_ops;
mod mutation_ops;
mod setup_ops;

pub use events::AppEvent;

pub(crate) use list_ops::{
    dispatch_detail_fetch, dispatch_refresh, dispatch_switch_view, dispatch_teammate_discovery,
};
pub(crate) use mutation_ops::{
    dispatch_add_comment, dispatch_assign, dispatch_transition, dispatch_update_description,
};
pub(crate) use setup_ops::dispatch_field_mapping;
#[cfg(feature = "live")]
pub(crate) use setup_ops::dispatch_verify_credentials;
