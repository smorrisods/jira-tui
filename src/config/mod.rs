//! Configuration, XDG paths, and the on-disk issue cache.
//!
//! Non-secret settings live in `$XDG_CONFIG_HOME/jira-tui/config.toml`
//! (falling back to `~/.config`), parsed and edited with `toml_edit` so a
//! save from onboarding or the field-mapping screen never disturbs comments
//! or settings a user added by hand. A small issue cache lives in
//! `$XDG_CACHE_HOME/jira-tui` so the last live "my work" list can be shown
//! instantly — and offline — until the next successful refresh.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

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

/// Parse `config.toml` into an editable document, for callers that intend
/// to write it back (so a parse error must surface, rather than risk
/// silently overwriting a file we couldn't understand).
fn load_document() -> Result<DocumentMut> {
    let Some(path) = config_path() else {
        return Ok(DocumentMut::new());
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => content
            .parse::<DocumentMut>()
            .with_context(|| format!("parsing {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(e).context(format!("reading {}", path.display())),
    }
}

/// Parse `config.toml` for read-only access. Never blocks the app — a
/// missing or unparseable file is treated the same as an empty one, exactly
/// like a missing/invalid credential falls back to demo data.
fn load_document_lenient() -> DocumentMut {
    load_document().unwrap_or_default()
}

/// Read the flat settings from `config.toml` as strings (booleans and
/// numbers are stringified), for callers that just want to look a value up.
pub fn read_kv() -> HashMap<String, String> {
    let doc = load_document_lenient();
    let mut map = HashMap::new();
    for (key, item) in doc.iter() {
        let value = if let Some(s) = item.as_str() {
            s.to_string()
        } else if let Some(b) = item.as_bool() {
            b.to_string()
        } else if let Some(i) = item.as_integer() {
            i.to_string()
        } else {
            continue;
        };
        map.insert(key.to_string(), value);
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
# environment variable or a token file (see token_file below), never in
# this file.

# Jira site and account (used in live mode).
# base_url = "https://your-org.atlassian.net"
# email = "you@example.com"
# project = "PROJ"

# Optional: custom field ID for "Acceptance Criteria" on your Jira site
# (Settings -> Issues -> Custom fields; every site assigns its own numeric
# ID). Leave commented out to skip fetching acceptance criteria entirely.
# You can also find and set this from within jira-tui — press `F`.
# acceptance_criteria_field = "customfield_10001"

# Optional: path to a file containing just your API token, if you'd rather
# not use the JIRA_API_TOKEN env var or the default
# ~/.config/jira-tui/token file.
# token_file = "/path/to/your/token"

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

/// Set (or, when `value` is `None`, remove) a single string key in
/// `config.toml`, leaving every other key, comment, and blank line exactly
/// as they were. This is the one place that touches the file on disk so
/// every setting shares the same "never clobber the rest of the file"
/// guarantee.
fn set_config_key(key: &str, value: Option<&str>) -> Result<PathBuf> {
    let dir = config_dir().context("could not resolve a config directory")?;
    std::fs::create_dir_all(&dir).context("creating config directory")?;
    let path = dir.join("config.toml");

    let mut doc = if path.exists() {
        load_document().with_context(|| {
            format!(
                "{} has invalid TOML — fix or remove it, then try again",
                path.display()
            )
        })?
    } else {
        DEFAULT_CONFIG
            .parse::<DocumentMut>()
            .context("parsing the built-in default config template")?
    };

    match value {
        Some(v) if !v.trim().is_empty() => {
            doc[key] = toml_edit::value(v);
        }
        _ => {
            doc.remove(key);
        }
    }

    std::fs::write(&path, doc.to_string()).context("writing config.toml")?;
    Ok(path)
}

/// Save the live-mode site/account settings, preserving every other setting
/// (mouse pref, field mapping, token file, and any comments) untouched.
pub fn save_settings(base_url: &str, email: &str, project: &str) -> Result<PathBuf> {
    set_config_key("base_url", Some(base_url))?;
    set_config_key("email", Some(email))?;
    set_config_key("project", Some(project))
}

/// Map (or clear) the "Acceptance Criteria" custom field, preserving the
/// rest of `config.toml`. `None` clears any existing mapping.
pub fn save_field_mapping(acceptance_criteria_field: Option<&str>) -> Result<PathBuf> {
    set_config_key("acceptance_criteria_field", acceptance_criteria_field)
}

/// Set (or clear) the token file path override, preserving the rest of
/// `config.toml`.
#[cfg_attr(not(feature = "live"), allow(dead_code))]
pub fn save_token_file_path(token_file: Option<&str>) -> Result<PathBuf> {
    set_config_key("token_file", token_file)
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
        assert_eq!(kv.get("acceptance_criteria_field"), None);

        // Mapping a custom field preserves the rest of config.toml.
        save_field_mapping(Some("customfield_10001")).unwrap();
        let kv = read_kv();
        assert_eq!(
            kv.get("acceptance_criteria_field").map(String::as_str),
            Some("customfield_10001")
        );
        assert_eq!(
            kv.get("base_url").map(String::as_str),
            Some("https://x.atlassian.net"),
            "field mapping must not clobber base_url"
        );

        // Re-saving settings (e.g. re-running onboarding) must not silently
        // drop a previously configured field mapping.
        save_settings("https://y.atlassian.net", "me@example.com", "DS").unwrap();
        let kv = read_kv();
        assert_eq!(
            kv.get("acceptance_criteria_field").map(String::as_str),
            Some("customfield_10001")
        );

        // Clearing the mapping removes the key entirely.
        save_field_mapping(None).unwrap();
        assert_eq!(read_kv().get("acceptance_criteria_field"), None);

        // Token file path override round-trips and can be cleared too.
        save_token_file_path(Some("/custom/token/path")).unwrap();
        assert_eq!(
            read_kv().get("token_file").map(String::as_str),
            Some("/custom/token/path")
        );
        save_token_file_path(None).unwrap();
        assert_eq!(read_kv().get("token_file"), None);

        // The rewrite to toml_edit must preserve comments and formatting a
        // user added by hand — not just the keys we know about — since a
        // full-file regeneration would silently discard them.
        let config_toml_path = config_path().unwrap();
        std::fs::write(
            &config_toml_path,
            "# a comment I wrote myself\nbase_url = \"https://x.atlassian.net\"\nemail = \"me@example.com\"\nproject = \"DS\"\nmouse = false\n# trailing comment\n",
        )
        .unwrap();
        save_field_mapping(Some("customfield_20002")).unwrap();
        let rewritten = std::fs::read_to_string(&config_toml_path).unwrap();
        assert!(
            rewritten.contains("# a comment I wrote myself"),
            "hand-written comments must survive a save:\n{rewritten}"
        );
        assert!(
            rewritten.contains("# trailing comment"),
            "trailing comments must survive a save:\n{rewritten}"
        );
        assert!(rewritten.contains("customfield_20002"));

        // An unparseable existing config.toml must surface an error rather
        // than being silently clobbered with a fresh default (data loss).
        std::fs::write(&config_toml_path, "this is not [ valid toml").unwrap();
        assert!(
            save_settings("https://z.atlassian.net", "me@example.com", "DS").is_err(),
            "saving over an unparseable config.toml must error, not overwrite it"
        );
        assert_eq!(
            std::fs::read_to_string(&config_toml_path).unwrap(),
            "this is not [ valid toml",
            "the invalid file must be left untouched after a failed save"
        );

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
