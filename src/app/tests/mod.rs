//! Unit tests for `App`, split by concern — mirroring `app/`'s own
//! per-submodule split (`assign`, `attachments`, `board`, `comments`,
//! `detail`, `edit`, `field_mapping`, `history`, `links`, `mouse`,
//! `new_issue`, `onboarding`, `palette`, `paste`, `priority`, `query`,
//! `quick_view`, `search`, `sort_filter`, `spell_suggest`, `sprint`,
//! `versions`, `view_switch`), plus a `transitions` file for the
//! workflow-transition tests. `support` carries the shared `App` builders
//! (`demo_app`/`non_demo_app`/`live_app`/`onboarding_app`) and the async
//! event-loop helper `next_event`.

mod assign;
mod attachments;
mod board;
mod comments;
mod detail;
mod edit;
mod field_mapping;
mod history;
#[cfg(feature = "images")]
mod inline_images;
mod links;
mod mouse;
mod new_issue;
mod onboarding;
mod palette;
mod paste;
mod priority;
mod query;
mod quick_view;
mod release;
mod search;
mod sort_filter;
mod spell_suggest;
mod sprint;
mod support;
mod transitions;
mod versions;
mod view_switch;
