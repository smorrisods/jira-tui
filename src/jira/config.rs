//! Live-mode configuration: credentials, site, and instance-specific
//! settings, assembled from environment variables, `config.toml`, and the
//! token file. (Distinct from `crate::config`, which is the app's own XDG
//! config/cache handling — this module just consumes it.)

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Config {
    pub base_url: String,
    pub email: String,
    pub token: String,
    pub project: String,
    /// Custom field ID for "Acceptance Criteria" (e.g. `customfield_10309`),
    /// if your Jira instance has one. This is instance-specific — every Jira
    /// site assigns its own numeric IDs to custom fields — so it's opt-in via
    /// `JIRA_ACCEPTANCE_CRITERIA_FIELD` or `acceptance_criteria_field` in
    /// `config.toml`. Left unset, acceptance criteria are simply not fetched.
    pub acceptance_criteria_field: Option<String>,
}

impl Config {
    /// Assemble config from env vars, an optional config file, and token.txt.
    /// Returns `None` when credentials are insufficient for live mode.
    pub fn load() -> Option<Config> {
        let file = crate::config::read_kv();

        // No baked-in default: every Jira site has a different hostname, so
        // live mode only activates once you've told us which one is yours.
        let base_url = std::env::var("JIRA_BASE_URL")
            .ok()
            .or_else(|| file.get("base_url").cloned())?;
        let email = std::env::var("JIRA_EMAIL")
            .ok()
            .or_else(|| file.get("email").cloned())?;
        // Only used when creating issues; leave unset if you don't create
        // issues from jira-tui or always want to be asked for a project key.
        let project = std::env::var("JIRA_PROJECT")
            .ok()
            .or_else(|| file.get("project").cloned())
            .unwrap_or_default();
        let acceptance_criteria_field = std::env::var("JIRA_ACCEPTANCE_CRITERIA_FIELD")
            .ok()
            .or_else(|| file.get("acceptance_criteria_field").cloned())
            .filter(|s| !s.trim().is_empty());

        let token = std::env::var("JIRA_API_TOKEN")
            .ok()
            .or_else(read_token_file)?;

        if token.trim().is_empty() || base_url.trim().is_empty() {
            return None;
        }

        Some(Config {
            base_url: base_url.trim_end_matches('/').to_string(),
            email,
            token: token.trim().to_string(),
            project,
            acceptance_criteria_field,
        })
    }

    pub fn site_host(&self) -> String {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string()
    }
}

fn read_token_file() -> Option<String> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    // An explicit override (env var or config.toml) takes priority over the
    // defaults below, so you can keep the token anywhere you like.
    let custom = std::env::var("JIRA_TOKEN_FILE")
        .ok()
        .or_else(|| crate::config::read_kv().get("token_file").cloned());
    if let Some(path) = custom {
        candidates.push(std::path::PathBuf::from(path));
    }
    if let Some(p) = crate::config::token_file_path() {
        candidates.push(p);
    }
    candidates.push(std::path::PathBuf::from("token.txt"));
    for candidate in candidates {
        if let Ok(s) = std::fs::read_to_string(&candidate) {
            let t = s.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // See `crate::test_support::lock_env` — this mutates process-global env
    // vars, so it's serialized against other tests that do the same.
    #[test]
    fn load_requires_base_url_email_and_token() {
        let _guard = crate::test_support::lock_env();
        let base = std::env::temp_dir().join(format!("jira-tui-cfgtest-{}", std::process::id()));
        std::env::set_var("XDG_CONFIG_HOME", &base);
        let _ = std::fs::remove_dir_all(&base);

        for var in [
            "JIRA_BASE_URL",
            "JIRA_EMAIL",
            "JIRA_API_TOKEN",
            "JIRA_PROJECT",
            "JIRA_ACCEPTANCE_CRITERIA_FIELD",
            "JIRA_TOKEN_FILE",
        ] {
            std::env::remove_var(var);
        }

        // Nothing configured at all: no live mode.
        assert!(Config::load().is_none());

        // Missing email: still no live mode, even with a base_url + token.
        std::env::set_var("JIRA_BASE_URL", "https://x.atlassian.net");
        std::env::set_var("JIRA_API_TOKEN", "secret");
        assert!(Config::load().is_none());

        // All three present: live mode activates, with sane defaults for
        // the optional fields.
        std::env::set_var("JIRA_EMAIL", "me@example.com");
        let cfg = Config::load().expect("base_url + email + token is enough");
        assert_eq!(cfg.base_url, "https://x.atlassian.net");
        assert_eq!(cfg.email, "me@example.com");
        assert_eq!(cfg.token, "secret");
        assert_eq!(cfg.project, "");
        assert_eq!(cfg.acceptance_criteria_field, None);
        assert_eq!(cfg.site_host(), "x.atlassian.net");

        // A trailing slash on base_url is trimmed.
        std::env::set_var("JIRA_BASE_URL", "https://x.atlassian.net/");
        let cfg = Config::load().unwrap();
        assert_eq!(cfg.base_url, "https://x.atlassian.net");

        // Optional fields, when set, come through.
        std::env::set_var("JIRA_PROJECT", "PROJ");
        std::env::set_var("JIRA_ACCEPTANCE_CRITERIA_FIELD", "customfield_10001");
        let cfg = Config::load().unwrap();
        assert_eq!(cfg.project, "PROJ");
        assert_eq!(
            cfg.acceptance_criteria_field.as_deref(),
            Some("customfield_10001")
        );

        for var in [
            "JIRA_BASE_URL",
            "JIRA_EMAIL",
            "JIRA_API_TOKEN",
            "JIRA_PROJECT",
            "JIRA_ACCEPTANCE_CRITERIA_FIELD",
        ] {
            std::env::remove_var(var);
        }
        std::env::remove_var("XDG_CONFIG_HOME");
        let _ = std::fs::remove_dir_all(&base);
    }
}
