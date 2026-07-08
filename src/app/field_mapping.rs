//! Field discovery: browse a live Jira site's custom fields and map
//! "Acceptance Criteria" to one of them.
//!
//! Custom field IDs (`customfield_NNNNN`) are assigned per Jira instance, so
//! there's no single correct value to hardcode. `GET /rest/api/3/field`
//! returns every field's name alongside its ID, so this screen just lets you
//! search that list by name instead of hunting for the ID yourself.

use super::{App, Screen};
use crate::config;

/// Sentinel entry meaning "don't map a custom field" — always present (when
/// the query is empty) so a mapping can be cleared as easily as it's set.
#[cfg(feature = "live")]
const NONE_SENTINEL: (&str, &str) = ("", "— none — don't track acceptance criteria —");

/// Index of the catalog entry matching `mapped` (or the leading sentinel at
/// index 0 if there's no mapping, or it's no longer in the catalog).
#[cfg_attr(not(feature = "live"), allow(dead_code))]
fn index_of_mapping(catalog: &[(String, String)], mapped: Option<&str>) -> usize {
    match mapped {
        Some(id) => catalog.iter().position(|(fid, _)| fid == id).unwrap_or(0),
        None => 0,
    }
}

/// Field discovery/mapping screen state.
#[derive(Clone, Debug, Default)]
pub struct FieldMappingState {
    /// Discovered custom fields as (id, name), sorted by name, with a
    /// leading `("", "— none —")` sentinel so mappings can be cleared.
    pub catalog: Vec<(String, String)>,
    pub query: String,
    pub selected: usize,
    /// The field ID currently mapped in `config.toml`, if any — read fresh
    /// each time the screen opens so re-editing shows (and pre-selects)
    /// what's already configured, rather than starting blank.
    pub current_mapping: Option<String>,
}

/// Outcome of trying to open the field-mapping screen, so callers (like the
/// onboarding handoff) can react precisely instead of inferring what
/// happened from `self.screen`.
#[derive(Debug, PartialEq, Eq)]
pub enum FieldMappingOutcome {
    /// The screen opened; the catalog was fetched successfully.
    Opened,
    /// Live mode isn't active, or credentials aren't configured.
    NotAvailable,
    /// Fetched successfully, but the site has no custom fields to map.
    NothingToMap,
    /// The fetch itself failed (network, auth, etc.) — worth retrying later.
    Failed(String),
}

impl App {
    /// Open the field-mapping screen, fetching the site's custom fields.
    /// Also sets `self.status` on every non-`Opened` outcome, so a direct
    /// keybinding invocation (`F`) still surfaces a message even though
    /// callers that need to branch on the outcome can match the return
    /// value instead of re-deriving it from `self.screen`.
    pub fn open_field_mapping(&mut self) -> FieldMappingOutcome {
        #[cfg(feature = "live")]
        {
            use crate::domain::Source;
            use crate::jira;

            if !matches!(self.source, Source::Live { .. }) {
                self.status =
                    "Field mapping needs live credentials — set them up first (--onboard).".into();
                return FieldMappingOutcome::NotAvailable;
            }
            let Some(cfg) = jira::Config::load() else {
                self.status = "No live credentials configured.".into();
                return FieldMappingOutcome::NotAvailable;
            };
            match jira::list_fields(&cfg) {
                Ok(fields) if fields.is_empty() => {
                    self.status = "No custom fields found on this Jira site.".into();
                    FieldMappingOutcome::NothingToMap
                }
                Ok(fields) => {
                    self.field_mapping.catalog =
                        std::iter::once((NONE_SENTINEL.0.to_string(), NONE_SENTINEL.1.to_string()))
                            .chain(fields.into_iter().map(|f| (f.id, f.name)))
                            .collect();
                    self.field_mapping.query.clear();
                    self.field_mapping.current_mapping = cfg.acceptance_criteria_field.clone();
                    // Pre-select whatever's already mapped so re-opening the
                    // screen to edit a mapping shows (and defaults to
                    // keeping) the current choice, rather than resetting to
                    // "none".
                    self.field_mapping.selected = index_of_mapping(
                        &self.field_mapping.catalog,
                        self.field_mapping.current_mapping.as_deref(),
                    );
                    self.screen = Screen::FieldMapping;
                    FieldMappingOutcome::Opened
                }
                Err(e) => {
                    let msg = e.to_string();
                    self.status = format!("Could not fetch fields: {msg}");
                    FieldMappingOutcome::Failed(msg)
                }
            }
        }
        #[cfg(not(feature = "live"))]
        {
            self.status = "This build has no live support; rebuild with the `live` feature.".into();
            FieldMappingOutcome::NotAvailable
        }
    }

    pub fn close_field_mapping(&mut self) {
        self.screen = Screen::Home;
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

        let saved = if id.is_empty() {
            config::save_field_mapping(None)
        } else {
            config::save_field_mapping(Some(&id))
        };

        match saved {
            Ok(_) if id.is_empty() => {
                std::env::remove_var("JIRA_ACCEPTANCE_CRITERIA_FIELD");
                self.field_mapping.current_mapping = None;
                self.status = "Cleared the acceptance criteria field mapping.".into();
            }
            Ok(_) => {
                std::env::set_var("JIRA_ACCEPTANCE_CRITERIA_FIELD", &id);
                self.field_mapping.current_mapping = Some(id.clone());
                self.status = format!("Mapped Acceptance Criteria → {name} ({id})");
                self.flash(format!("✓ mapped {name}"));
            }
            Err(e) => {
                self.status = format!("Could not save field mapping: {e}");
            }
        }
        self.screen = Screen::Home;
    }
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
