//! First-run onboarding: the welcome screen and the credential setup form.

use crate::config;
#[cfg(feature = "live")]
use crate::domain::Source;

use super::{App, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WelcomePhase {
    Intro,
    Setup,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Site,
    Email,
    Token,
}

impl App {
    /// Dismiss the welcome screen and remember not to show it again.
    pub fn finish_onboarding(&mut self) {
        config::mark_onboarded();
        self.screen = Screen::Home;
    }

    /// Write the default config file from the welcome screen.
    pub fn write_config_from_welcome(&mut self) {
        match config::write_default_config() {
            Ok((path, true)) => {
                self.status = format!("wrote config to {}", path.display());
            }
            Ok((path, false)) => {
                self.status = format!("config already exists at {}", path.display());
            }
            Err(e) => {
                self.status = format!("could not write config: {e}");
            }
        }
    }

    fn focused_field_mut(&mut self) -> &mut String {
        match self.focus {
            Field::Site => &mut self.field_site,
            Field::Email => &mut self.field_email,
            Field::Token => &mut self.field_token,
        }
    }

    pub fn input_char(&mut self, c: char) {
        self.focused_field_mut().push(c);
    }

    pub fn input_backspace(&mut self) {
        self.focused_field_mut().pop();
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Field::Site => Field::Email,
            Field::Email => Field::Token,
            Field::Token => Field::Site,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Field::Site => Field::Token,
            Field::Email => Field::Site,
            Field::Token => Field::Email,
        };
    }

    /// Validate, verify against Jira, and persist the entered credentials.
    /// On success, switches to live data and finishes onboarding.
    pub fn submit_credentials(&mut self) {
        let site = self.field_site.trim().trim_end_matches('/').to_string();
        let email = self.field_email.trim().to_string();
        let token = self.field_token.trim().to_string();
        if site.is_empty() || email.is_empty() || token.is_empty() {
            self.setup_msg = "Please fill site, email, and token.".into();
            return;
        }

        // Persist first so the standard config path picks them up.
        if let Err(e) = config::save_token(&token) {
            self.setup_msg = format!("Could not save token: {e}");
            return;
        }
        if let Err(e) = config::save_settings(&site, &email, "") {
            self.setup_msg = format!("Could not save settings: {e}");
            return;
        }
        std::env::set_var("JIRA_BASE_URL", &site);
        std::env::set_var("JIRA_EMAIL", &email);
        std::env::set_var("JIRA_API_TOKEN", &token);

        #[cfg(feature = "live")]
        {
            self.setup_msg = "Verifying…".into();
            let (issues, source, status) = super::load_issues(false);
            match source {
                Source::Live { .. } => {
                    self.all_issues = issues;
                    self.source = source;
                    self.status = status;
                    self.recompute_view();
                    config::mark_onboarded();
                    // Offer to map "Acceptance Criteria" (or another custom
                    // field) now, while we're already talking to Jira.
                    let connected_status = self.status.clone();
                    match self.open_field_mapping() {
                        super::FieldMappingOutcome::Opened => {}
                        super::FieldMappingOutcome::NothingToMap
                        | super::FieldMappingOutcome::NotAvailable => {
                            self.screen = Screen::Home;
                            self.status = connected_status;
                        }
                        super::FieldMappingOutcome::Failed(_) => {
                            // A transient failure (network blip, etc.) here
                            // shouldn't block finishing onboarding — but it's
                            // easy to forget the field-mapping screen exists
                            // at all if it silently never appears, so leave a
                            // toast pointing at `F` rather than just the raw
                            // error.
                            self.screen = Screen::Home;
                            self.status = connected_status;
                            self.flash("Couldn't look up custom fields — press F to try again");
                        }
                    }
                }
                _ => {
                    self.setup_msg =
                        "Saved, but Jira did not accept those credentials. Check and retry, or press Esc to continue in demo mode.".into();
                }
            }
        }
        #[cfg(not(feature = "live"))]
        {
            self.setup_msg =
                "Saved. This build has no live support; rebuild with the `live` feature.".into();
        }
    }
}
