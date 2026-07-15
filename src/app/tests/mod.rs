//! Unit tests for `App`, split by the screen/flow each group exercises —
//! mirroring `app/`'s own concern split. `support` carries the shared
//! `App` builders (`demo_app`/`non_demo_app`/`live_app`/`onboarding_app`)
//! and the async event-loop helper `next_event`.

mod board_workflow;
mod list_and_detail;
mod support;
mod view_and_setup;
