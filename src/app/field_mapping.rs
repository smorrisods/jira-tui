//! Field discovery: browse a live Jira site's custom fields and map one to
//! whichever instance-specific slot (`FieldMappingTarget`) is currently
//! selected.
//!
//! Custom field IDs (`customfield_NNNNN`) are assigned per Jira instance, so
//! there's no single correct value to hardcode. `GET /rest/api/3/field`
//! returns every field's name alongside its ID, so this screen just lets you
//! search that list by name instead of hunting for the ID yourself — the
//! same catalog serves every target, so switching targets (`Tab`) never
//! needs a re-fetch, just a different "what's currently mapped" lookup
//! against the config key already sitting in `FieldMappingTarget::config_key`.
//!
//! Originally hardcoded to Acceptance Criteria alone; generalized once
//! Sprint (issue #123-adjacent work) needed the exact same "search this
//! site's custom fields, remember the choice in config.toml" flow — the
//! underlying persistence (`config::save_field_mapping`) was already
//! field-agnostic, only this module's own API and the UI's copy were narrow.

use super::{async_ops, App, Screen};
use crate::config;

/// Sentinel entry meaning "don't map a custom field" — always present (when
/// the query is empty) so a mapping can be cleared as easily as it's set.
/// The trailing "don't track …" half of the label is filled in per-target
/// at render time (see `ui::field_mapping`), since "don't track acceptance
/// criteria" doesn't read right for a different target.
const NONE_SENTINEL: (&str, &str) = ("", "— none —");

/// Which instance-specific custom field the field-mapping screen (`F`) is
/// currently editing. Add a new variant here (plus its three match arms) to
/// wire up another config-gated custom field the same way — the screen,
/// catalog fetch/cache, and persistence all generalize automatically; only
/// the field's own config key/env var and read/write call sites (Sprint's
/// are `src/jira/live/detail.rs`'s conditional `fields=` append and
/// `App::sprint_field_configured`) are target-specific.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldMappingTarget {
    #[default]
    AcceptanceCriteria,
    Sprint,
}

impl FieldMappingTarget {
    /// Human-readable name for the screen's title/status messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::AcceptanceCriteria => "Acceptance Criteria",
            Self::Sprint => "Sprint",
        }
    }

    /// The `config.toml` key this target reads/writes — must match the
    /// field name on `jira::Config` this target corresponds to
    /// (`acceptance_criteria_field`/`sprint_field`).
    pub fn config_key(self) -> &'static str {
        match self {
            Self::AcceptanceCriteria => "acceptance_criteria_field",
            Self::Sprint => "sprint_field",
        }
    }

    /// The env var override for this target's config key, checked first —
    /// same "env wins over config.toml" precedence every other setting in
    /// this app already follows.
    pub fn env_var(self) -> &'static str {
        match self {
            Self::AcceptanceCriteria => "JIRA_ACCEPTANCE_CRITERIA_FIELD",
            Self::Sprint => "JIRA_SPRINT_FIELD",
        }
    }

    /// The next target in the `Tab` cycle order — wraps around. Add new
    /// variants to this cycle as they're added to the enum itself.
    pub fn next(self) -> Self {
        match self {
            Self::AcceptanceCriteria => Self::Sprint,
            Self::Sprint => Self::AcceptanceCriteria,
        }
    }

    /// The currently mapped field id for this target, checking the env var
    /// override first then `config.toml` — same precedence
    /// `jira::Config::load` uses for every other setting, but a direct,
    /// synchronous, network-free read (unlike the *catalog*, which does
    /// need a live fetch) so `App::cycle_field_mapping_target` can re-derive
    /// "what's currently mapped" on every `Tab` without waiting on
    /// anything.
    fn current_mapping(self) -> Option<String> {
        std::env::var(self.env_var())
            .ok()
            .or_else(|| config::read_kv().get(self.config_key()).cloned())
            .filter(|s| !s.trim().is_empty())
    }
}

/// Index of the catalog entry matching `mapped` (or the leading sentinel at
/// index 0 if there's no mapping, or it's no longer in the catalog).
fn index_of_mapping(catalog: &[(String, String)], mapped: Option<&str>) -> usize {
    match mapped {
        Some(id) => catalog.iter().position(|(fid, _)| fid == id).unwrap_or(0),
        None => 0,
    }
}

/// Field discovery/mapping screen state.
#[derive(Clone, Debug, Default)]
pub struct FieldMappingState {
    /// Which config-gated custom field this screen is currently editing.
    /// Deliberately *not* reset on every `App::open_field_mapping` — the
    /// screen remembers whichever target you last worked with, the same way
    /// `current_mapping` already gets re-read fresh rather than reset, so
    /// re-opening to tweak the same field doesn't lose your place.
    pub target: FieldMappingTarget,
    /// Discovered custom fields as (id, name), sorted by name, with a
    /// leading `("", "— none —")` sentinel so mappings can be cleared.
    pub catalog: Vec<(String, String)>,
    pub query: String,
    pub selected: usize,
    /// `target`'s field ID currently mapped in `config.toml`, if any — read
    /// fresh each time the screen opens (or `Tab` switches target) so
    /// re-editing shows (and pre-selects) what's already configured, rather
    /// than starting blank.
    pub current_mapping: Option<String>,
}

/// Where a field-mapping lookup was triggered from, so the async result can
/// be applied the same way each caller used to branch on the old
/// synchronous return value.
#[derive(Debug)]
pub enum FieldMappingOrigin {
    /// The `F` key — stays wherever the user currently is on
    /// success/failure; `open_field_mapping`'s own status message (applied
    /// once the fetch resolves) is the final word, no extra screen change.
    Direct,
    /// The onboarding credential-verification handoff — falls back to
    /// `Screen::Home` with the "connected" status (captured before the
    /// lookup started) on anything other than a successful catalog fetch,
    /// exactly like the old synchronous `match` used to.
    Onboarding { connected_status: String },
}

/// Outcome of *attempting* to start a field-mapping lookup — this only
/// covers what's knowable synchronously (whether a real fetch was even
/// dispatched). What the fetch actually finds (a catalog, an empty site, or
/// a failure) is no longer a return value — it's applied once the fetch
/// resolves, via `self.screen`/`self.status` (see `AppEvent::FieldsLoaded`).
#[derive(Debug, PartialEq, Eq)]
pub enum FieldMappingOutcome {
    /// A lookup was dispatched (or one was already in flight) — watch
    /// `self.screen`/`self.status` for the result.
    Pending,
    /// Live mode isn't active, or credentials aren't configured — decided
    /// without a network round-trip, so there's nothing to wait on.
    NotAvailable,
}

impl App {
    /// Open the field-mapping screen, fetching the site's custom fields.
    /// Demo/cache sessions resolve `NotAvailable` synchronously (there's no
    /// fetch to dispatch); a genuine live session dispatches the lookup off
    /// the render thread — see `dispatch_field_mapping`. Missing
    /// credentials are *not* checked synchronously here — that's decided
    /// inside the dispatched fetch itself, same as detail/transition/edit,
    /// so it surfaces as an `Err` on `AppEvent::FieldsLoaded` instead.
    pub fn open_field_mapping(&mut self) -> FieldMappingOutcome {
        self.dispatch_field_mapping(FieldMappingOrigin::Direct)
    }

    /// Same lookup, but for the onboarding handoff right after verifying
    /// fresh credentials — see `FieldMappingOrigin::Onboarding` for how the
    /// post-resolution branching differs from the `F` key. Only called from
    /// `onboarding.rs`'s live-gated verification flow, so it's dead code in
    /// a no-live build.
    /// Same lookup, but always targets Acceptance Criteria regardless of
    /// whichever target the screen was last left on — onboarding's
    /// credential-verification handoff is specifically about getting a new
    /// user's Acceptance Criteria field set up, not whatever `F` happened
    /// to be showing last time it was open. See `FieldMappingOrigin::Onboarding`
    /// for how the post-resolution branching differs from the `F` key.
    /// Only called from `onboarding.rs`'s live-gated verification flow, so
    /// it's dead code in a no-live build.
    #[cfg_attr(not(feature = "live"), allow(dead_code))]
    pub(crate) fn open_field_mapping_for_onboarding(
        &mut self,
        connected_status: String,
    ) -> FieldMappingOutcome {
        self.field_mapping.target = FieldMappingTarget::AcceptanceCriteria;
        self.dispatch_field_mapping(FieldMappingOrigin::Onboarding { connected_status })
    }

    fn dispatch_field_mapping(&mut self, origin: FieldMappingOrigin) -> FieldMappingOutcome {
        use crate::domain::Source;

        if !matches!(self.source, Source::Live { .. }) {
            self.status =
                "Field mapping needs live credentials — set them up first (--onboard).".into();
            return FieldMappingOutcome::NotAvailable;
        }
        if self.field_mapping_pending {
            self.status = "Already looking up custom fields…".into();
            return FieldMappingOutcome::Pending;
        }
        self.field_mapping_generation += 1;
        let generation = self.field_mapping_generation;
        self.field_mapping_pending = true;
        self.loading = true;
        self.status = "↻ looking up custom fields…".into();
        let tx = self.events_tx.clone();
        async_ops::dispatch_field_mapping(tx, generation, self.field_mapping.target, origin);
        FieldMappingOutcome::Pending
    }

    pub fn close_field_mapping(&mut self) {
        self.screen = Screen::Home;
    }

    /// `Tab` on the field-mapping screen: switch which custom field slot
    /// you're mapping. Reuses the already-fetched catalog (the same
    /// `GET /rest/api/3/field` list serves every target — see this module's
    /// doc comment) and just re-derives "what's currently mapped"/the
    /// pre-selected row for the new target, synchronously — no network
    /// round-trip needed, unlike the initial open. A no-op before the
    /// catalog has loaded (nothing to re-derive against yet).
    pub fn cycle_field_mapping_target(&mut self) {
        if self.field_mapping.catalog.is_empty() {
            return;
        }
        self.field_mapping.target = self.field_mapping.target.next();
        self.field_mapping.query.clear();
        // The catalog's leading "none" row names the *previous* target
        // ("don't track Acceptance Criteria") — rewrite it in place rather
        // than refetching just to relabel one row.
        if let Some(sentinel) = self.field_mapping.catalog.first_mut() {
            sentinel.1 = format!(
                "{} — don't track {}",
                NONE_SENTINEL.1,
                self.field_mapping.target.label()
            );
        }
        self.field_mapping.current_mapping = self.field_mapping.target.current_mapping();
        self.field_mapping.selected = index_of_mapping(
            &self.field_mapping.catalog,
            self.field_mapping.current_mapping.as_deref(),
        );
    }

    pub fn field_mapping_input_char(&mut self, c: char) {
        self.field_mapping.query.push(c);
        self.field_mapping.selected = 0;
    }

    pub fn field_mapping_backspace(&mut self) {
        self.field_mapping.query.pop();
        self.field_mapping.selected = 0;
    }

    /// Fields matching the current search query (case-insensitive substring
    /// match against the field name or ID). The "none" sentinel only shows
    /// while the query is empty, so searching narrows to real fields.
    pub fn filtered_field_catalog(&self) -> Vec<&(String, String)> {
        let q = self.field_mapping.query.trim().to_lowercase();
        self.field_mapping
            .catalog
            .iter()
            .filter(|(id, name)| {
                if q.is_empty() {
                    return true;
                }
                if id.is_empty() {
                    return false;
                }
                name.to_lowercase().contains(&q) || id.to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn field_mapping_move(&mut self, delta: isize) {
        let len = self.filtered_field_catalog().len();
        if len == 0 {
            self.field_mapping.selected = 0;
            return;
        }
        let new = self.field_mapping.selected as isize + delta;
        self.field_mapping.selected = new.clamp(0, len as isize - 1) as usize;
    }

    /// Map the selected field as the acceptance-criteria custom field (or
    /// clear the mapping, if the "none" sentinel is selected) and persist it
    /// to `config.toml`.
    pub fn confirm_field_mapping(&mut self) {
        let selection = self
            .filtered_field_catalog()
            .get(self.field_mapping.selected)
            .map(|f| (f.0.clone(), f.1.clone()));
        let Some((id, name)) = selection else {
            self.screen = Screen::Home;
            return;
        };

        let target = self.field_mapping.target;
        let saved = if id.is_empty() {
            config::save_field_mapping(target.config_key(), None)
        } else {
            config::save_field_mapping(target.config_key(), Some(&id))
        };

        match saved {
            Ok(_) if id.is_empty() => {
                std::env::remove_var(target.env_var());
                self.field_mapping.current_mapping = None;
                self.status = format!("Cleared the {} field mapping.", target.label());
            }
            Ok(_) => {
                std::env::set_var(target.env_var(), &id);
                self.field_mapping.current_mapping = Some(id.clone());
                self.status = format!("Mapped {} → {name} ({id})", target.label());
                self.flash(format!("✓ mapped {name}"));
            }
            Err(e) => {
                self.status = format!("Could not save field mapping: {e}");
            }
        }
        self.screen = Screen::Home;
    }
}

/// Build the catalog (with a leading "none" sentinel naming `target`) and
/// the pre-selected index from a resolved fetch. Used by
/// `AppEvent::FieldsLoaded`'s handler once the async lookup completes —
/// factored out here purely so it sits next to `index_of_mapping` and the
/// sentinel it uses. Takes plain `(id, name)` pairs rather than
/// `jira::FieldInfo` so this (and the always-compiled `apply_event` that
/// calls it) don't need to be gated behind the `live` feature.
pub(crate) fn build_catalog_and_selection(
    fields: Vec<(String, String)>,
    target: FieldMappingTarget,
    current_mapping: Option<&str>,
) -> (Vec<(String, String)>, usize) {
    let none_label = format!("{} — don't track {}", NONE_SENTINEL.1, target.label());
    let catalog: Vec<(String, String)> = std::iter::once((NONE_SENTINEL.0.to_string(), none_label))
        .chain(fields)
        .collect();
    let selected = index_of_mapping(&catalog, current_mapping);
    (catalog, selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_app() -> App {
        App::new(true)
    }

    #[test]
    fn filters_by_name_or_id_case_insensitively() {
        let mut app = demo_app();
        app.field_mapping.catalog = vec![
            (String::new(), "— none —".into()),
            ("customfield_10001".into(), "Acceptance Criteria".into()),
            ("customfield_10002".into(), "Story Points".into()),
        ];

        app.field_mapping.query = "accept".into();
        let filtered = app.filtered_field_catalog();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1, "Acceptance Criteria");

        app.field_mapping.query = "10002".into();
        let filtered = app.filtered_field_catalog();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1, "Story Points");

        app.field_mapping.query.clear();
        assert_eq!(app.filtered_field_catalog().len(), 3);
    }

    #[test]
    fn move_clamps_to_filtered_bounds() {
        let mut app = demo_app();
        app.field_mapping.catalog = vec![
            (String::new(), "— none —".into()),
            ("customfield_10001".into(), "Acceptance Criteria".into()),
        ];
        app.field_mapping.selected = 0;
        app.field_mapping_move(-5);
        assert_eq!(app.field_mapping.selected, 0);
        app.field_mapping_move(5);
        assert_eq!(app.field_mapping.selected, 1);
    }

    #[test]
    fn reopening_pre_selects_the_currently_mapped_field() {
        let catalog = vec![
            (String::new(), "— none —".into()),
            ("customfield_10001".into(), "Acceptance Criteria".into()),
            ("customfield_10002".into(), "Story Points".into()),
        ];

        // A previously mapped field is pre-selected, not reset to "none".
        assert_eq!(
            index_of_mapping(&catalog, Some("customfield_10002")),
            2,
            "re-opening the screen should default to the currently mapped field"
        );

        // No mapping configured: defaults to the "none" sentinel.
        assert_eq!(index_of_mapping(&catalog, None), 0);

        // A mapping that no longer exists on the site (e.g. the field was
        // deleted) falls back to "none" rather than panicking or drifting.
        assert_eq!(index_of_mapping(&catalog, Some("customfield_99999")), 0);
    }

    #[test]
    fn opening_without_live_credentials_reports_not_available() {
        // Demo mode (or any non-live source) must never crash or silently
        // swallow the attempt — callers like onboarding rely on getting a
        // precise outcome back rather than having to infer it from the
        // screen, so a failed handoff can still leave a clear, actionable
        // status/flash message.
        let mut app = demo_app();
        let before = app.screen;
        let outcome = app.open_field_mapping();
        assert_eq!(outcome, FieldMappingOutcome::NotAvailable);
        assert_eq!(app.screen, before, "must not navigate away on failure");
        assert!(!app.status.is_empty());
    }
}
