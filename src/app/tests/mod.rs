//! Unit tests for `App`, split by concern — mirroring `app/`'s own
//! per-submodule split (`assign`, `board`, `comments`, `detail`, `edit`,
//! `field_mapping`, `history`, `links`, `mouse`, `onboarding`, `palette`,
//! `query`, `quick_view`, `search`, `sort_filter`, `spell_suggest`,
//! `versions`, `view_switch`), plus a `transitions` file for the workflow-
//! transition tests. `support` carries the shared `App` builders (`demo_app`/
//! `non_demo_app`/`live_app`/`onboarding_app`) and the async event-loop
//! helper `next_event`.

mod assign;
mod board;
mod comments;
mod detail;
mod edit;
mod field_mapping;
mod history;
mod links;
mod mouse;
mod onboarding;
mod palette;
mod query;
mod quick_view;
mod search;
mod sort_filter;
mod spell_suggest;
mod support;
mod transitions;
mod versions;
mod view_switch;
