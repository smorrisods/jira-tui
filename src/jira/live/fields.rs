//! Custom-field discovery for the field-mapping screen.

use anyhow::{anyhow, Result};

use super::super::config::Config;
use super::support::get;

/// A Jira field's ID and human-readable name, as returned by
/// `GET /rest/api/3/field`.
#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub id: String,
    pub name: String,
}

/// List the site's custom fields (id + name), sorted by name. Used by
/// the field-mapping screen to let you find, e.g., "Acceptance Criteria"
/// without knowing its instance-specific `customfield_NNNNN` ID up front.
pub fn list_fields(cfg: &Config) -> Result<Vec<FieldInfo>> {
    let value = get(cfg, "/rest/api/3/field")?;
    let arr = value
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow!("unexpected /field response shape"))?;

    let mut fields: Vec<FieldInfo> = arr
        .into_iter()
        .filter_map(|f| {
            let id = f.get("id")?.as_str()?.to_string();
            // Built-in fields (summary, status, ...) are already handled
            // by name; only custom fields have instance-specific IDs
            // worth mapping here.
            if !id.starts_with("customfield_") {
                return None;
            }
            let name = f.get("name")?.as_str()?.to_string();
            Some(FieldInfo { id, name })
        })
        .collect();
    fields.sort_by_key(|a| a.name.to_lowercase());
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(base_url: String) -> Config {
        Config {
            base_url,
            email: "me@example.com".into(),
            token: "secret-token".into(),
            project: "PROJ".into(),
            acceptance_criteria_field: None,
        }
    }

    #[test]
    fn list_fields_filters_to_custom_fields_and_sorts_by_name() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/rest/api/3/field")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {"id": "summary", "name": "Summary"},
                    {"id": "customfield_10002", "name": "Story Points"},
                    {"id": "customfield_10001", "name": "Acceptance Criteria"}
                ]"#,
            )
            .create();

        let cfg = test_config(server.url());
        let fields = list_fields(&cfg).unwrap();

        mock.assert();
        // Built-in fields (no `customfield_` prefix) are excluded, and the
        // rest are sorted by name, not by id or response order.
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "Acceptance Criteria");
        assert_eq!(fields[0].id, "customfield_10001");
        assert_eq!(fields[1].name, "Story Points");
    }
}
