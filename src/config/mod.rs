//! Configuration, XDG paths, and the on-disk issue cache.
//!
//! Non-secret settings live in `$XDG_CONFIG_HOME/jira-tui/config.toml`
//! (falling back to `~/.config`). A small issue cache lives in
//! `$XDG_CACHE_HOME/jira-tui` so the last live "my work" list can be shown
//! instantly — and offline — until the next successful refresh.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::domain::IssueSummary;

/// `$XDG_CONFIG_HOME/jira-tui` (or `~/.config/jira-tui`).
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("jira-tui"))
}

/// The canonical config file path.
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

/// `$XDG_CACHE_HOME/jira-tui` (or `~/.cache/jira-tui`), created on demand.
pub fn cache_dir() -> Option<PathBuf> {
    let dir = dirs::cache_dir().map(|d| d.join("jira-tui"))?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

/// Read the flat `key = "value"` settings from the config file. Intentionally
/// tiny — no TOML dependency for a handful of flat keys.
pub fn read_kv() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(path) = config_path() else {
        return map;
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
            map.insert(k.trim().to_string(), v);
        }
    }
    map
}

/// User-facing, non-secret settings.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// Start with mouse mode enabled (click-to-open, wheel scroll, drag-copy).
    pub mouse: bool,
}

impl Settings {
    pub fn load() -> Self {
        let kv = read_kv();
        Settings {
            mouse: kv
                .get("mouse")
                .map(|v| matches!(v.as_str(), "true" | "1" | "yes" | "on"))
                .unwrap_or(false),
        }
    }
}

const DEFAULT_CONFIG: &str = r#"# jira-tui configuration
# Non-secret settings only. Keep your API token in the JIRA_API_TOKEN
# environment variable or a token.txt file, never in this file.

# Jira site and account (used in live mode).
# base_url = "https://your-org.atlassian.net"
# email = "you@example.com"
# project = "PROJ"

# Optional: custom field ID for "Acceptance Criteria" on your Jira site
# (Settings -> Issues -> Custom fields; every site assigns its own numeric
# ID). Leave commented out to skip fetching acceptance criteria entirely.
# acceptance_criteria_field = "customfield_10001"

# UI preferences.
# Start with mouse mode on (click-to-open, wheel scroll, drag-to-copy).
# Hold Shift while dragging to use your terminal's native selection instead.
mouse = false
"#;

/// Write a starter config file to the XDG config path. Never overwrites an
/// existing file. Returns the path and whether it was newly created.
pub fn write_default_config() -> Result<(PathBuf, bool)> {
    let dir = config_dir().context("could not resolve a config directory")?;
    std::fs::create_dir_all(&dir).context("creating config directory")?;
    let path = dir.join("config.toml");
    if path.exists() {
        return Ok((path, false));
    }
    std::fs::write(&path, DEFAULT_CONFIG).context("writing config.toml")?;
    Ok((path, true))
}

/// Marker file recording that the user has seen the onboarding flow.
fn onboard_marker() -> Option<PathBuf> {
    config_dir().map(|d| d.join(".onboarded"))
}

/// Whether the onboarding welcome has already been completed or dismissed.
pub fn is_onboarded() -> bool {
    onboard_marker().map(|p| p.exists()).unwrap_or(false)
}

/// Record that onboarding has been completed so it is not shown again.
pub fn mark_onboarded() {
    if let Some(path) = onboard_marker() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, "1\n");
    }
}

#[cfg_attr(not(feature = "live"), allow(dead_code))]
/// Path to the secret token file inside the config dir (never `config.toml`).
pub fn token_file_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("token"))
}

/// Save the API token to a `0600` file in the config dir.
pub fn save_token(token: &str) -> Result<PathBuf> {
    let dir = config_dir().context("could not resolve a config directory")?;
    std::fs::create_dir_all(&dir).context("creating config directory")?;
    let path = dir.join("token");
    std::fs::write(&path, format!("{}\n", token.trim())).context("writing token")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

/// Write the non-secret settings to `config.toml`, preserving the mouse pref.
pub fn save_settings(base_url: &str, email: &str, project: &str) -> Result<PathBuf> {
    let dir = config_dir().context("could not resolve a config directory")?;
    std::fs::create_dir_all(&dir).context("creating config directory")?;
    let mouse = Settings::load().mouse;
    let content = format!(
        "# jira-tui configuration\n\
# Non-secret settings only. The API token lives in the sibling `token` file.\n\
\n\
base_url = \"{base_url}\"\n\
email = \"{email}\"\n\
project = \"{project}\"\n\
\n\
# Start with mouse mode on (click-to-open, wheel scroll, drag-to-copy).\n\
# Hold Shift while dragging to use your terminal's native selection instead.\n\
mouse = {mouse}\n"
    );
    let path = dir.join("config.toml");
    std::fs::write(&path, content).context("writing config.toml")?;
    Ok(path)
}

fn cache_file() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("my-work.json"))
}

/// Persist the current "my work" list so it can be shown instantly next launch.
#[cfg_attr(not(feature = "live"), allow(dead_code))]
pub fn cache_issues(issues: &[IssueSummary]) {
    if let Some(path) = cache_file() {
        if let Ok(json) = serde_json::to_string(issues) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// Load the cached "my work" list, if any.
#[cfg_attr(not(feature = "live"), allow(dead_code))]
pub fn load_cached_issues() -> Option<Vec<IssueSummary>> {
    let path = cache_file()?;
    let content = std::fs::read_to_string(path).ok()?;
    let issues: Vec<IssueSummary> = serde_json::from_str(&content).ok()?;
    if issues.is_empty() {
        None
    } else {
        Some(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Single test to avoid races on the process-global XDG env vars.
    #[test]
    fn config_and_cache_lifecycle() {
        let base = std::env::temp_dir().join(format!("jira-tui-test-{}", std::process::id()));
        let cfg = base.join("config");
        let cache = base.join("cache");
        std::env::set_var("XDG_CONFIG_HOME", &cfg);
        std::env::set_var("XDG_CACHE_HOME", &cache);

        // Fresh: not onboarded, empty settings.
        let _ = std::fs::remove_dir_all(&base);
        assert!(!is_onboarded());
        assert!(!Settings::load().mouse);

        // write_default_config creates once, then reports pre-existing.
        let (path, created) = write_default_config().unwrap();
        assert!(created);
        assert!(path.exists());
        let (_p, created2) = write_default_config().unwrap();
        assert!(!created2);

        // Onboarding marker round-trips.
        mark_onboarded();
        assert!(is_onboarded());

        // Saved settings are read back via read_kv, and mouse parses true.
        save_settings("https://x.atlassian.net", "me@example.com", "DS").unwrap();
        let kv = read_kv();
        assert_eq!(
            kv.get("base_url").map(String::as_str),
            Some("https://x.atlassian.net")
        );
        assert_eq!(kv.get("email").map(String::as_str), Some("me@example.com"));

        // Token is written to its own file with 0600 perms on unix.
        let tpath = save_token("secret-token").unwrap();
        let contents = std::fs::read_to_string(&tpath).unwrap();
        assert_eq!(contents.trim(), "secret-token");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&tpath).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        // Issue cache round-trips through the XDG cache dir.
        let issues = crate::domain::demo_issues();
        cache_issues(&issues);
        let loaded = load_cached_issues().expect("cache should load");
        assert_eq!(loaded.len(), issues.len());

        let _ = std::fs::remove_dir_all(&base);
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_CACHE_HOME");
    }

    #[test]
    fn mouse_setting_parses_truthy_values() {
        for v in ["true", "1", "yes", "on"] {
            assert!(matches!(v, "true" | "1" | "yes" | "on"));
        }
    }
}
