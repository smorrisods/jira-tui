//! Lightweight Git context detection using the `git` CLI (no libgit2 dep).

use std::process::Command;

#[derive(Clone, Debug, Default)]
pub struct GitContext {
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub issue_key: Option<String>,
}

impl GitContext {
    pub fn detect() -> Self {
        let repo = git(&["rev-parse", "--show-toplevel"])
            .map(|p| p.rsplit('/').next().unwrap_or(&p).to_string());
        let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]);
        let issue_key = branch.as_deref().and_then(parse_issue_key);
        GitContext {
            repo,
            branch,
            issue_key,
        }
    }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Extract a Jira-style key such as `DS-123` from a branch name.
///
/// Requires an uppercase project code (as Jira issues the them), e.g.
/// `DS-123-add-login` or `feature/DS-2722_thing`. This deliberately ignores
/// lowercase words like `release-2024` to avoid false positives.
pub fn parse_issue_key(branch: &str) -> Option<String> {
    let bytes = branch.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // find start of an uppercase alphabetic run
        if bytes[i].is_ascii_uppercase() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_uppercase() {
                i += 1;
            }
            let letters = &branch[start..i];
            // must be followed by '-' then digits
            if (2..=10).contains(&letters.len()) && i < bytes.len() && bytes[i] == b'-' {
                let dstart = i + 1;
                let mut j = dstart;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > dstart {
                    return Some(format!("{}-{}", letters, &branch[dstart..j]));
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_issue_key;

    #[test]
    fn parses_various_branch_shapes() {
        assert_eq!(parse_issue_key("DS-123-add-login"), Some("DS-123".into()));
        assert_eq!(
            parse_issue_key("feature/DS-2722_thing"),
            Some("DS-2722".into())
        );
        assert_eq!(parse_issue_key("scott/DS-9/fix"), Some("DS-9".into()));
        assert_eq!(parse_issue_key("main"), None);
        assert_eq!(parse_issue_key("release-2024"), None);
        assert_eq!(parse_issue_key("ds-9-lowercase"), None);
    }
}
