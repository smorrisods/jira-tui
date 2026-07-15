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
//! Split into `events` (the `AppEvent` enum + `apply_event` dispatcher,
//! kept together as the state-machine core), `list_ops` (refresh/switch
//! view/detail load/teammate discovery), `mutation_ops` (transitions,
//! assignment, description updates, comments — each a `dispatch_*`/
//! `*_blocking` pair with no mutual dependencies), and `setup_ops`
//! (field-mapping lookup, onboarding credential verification).

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
#[cfg_attr(not(feature = "live"), allow(unused_imports))]
pub(crate) use setup_ops::dispatch_verify_credentials;
