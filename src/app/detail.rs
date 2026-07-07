//! Opening an issue's full detail, by selection or directly by key.

#[cfg(feature = "live")]
use crate::domain::Source;
use crate::domain::{demo_detail, IssueDetail};

use super::{App, Screen};

impl App {
    pub fn open_detail(&mut self) {
        let Some(issue) = self.selected_issue().cloned() else {
            return;
        };
        self.open_by_key(&issue.key);
    }

    /// Load and open an issue by key directly, regardless of whether it's in
    /// the current filtered/sorted view. Used by search results and "go to
    /// issue". If the key is present in the current view, `selected` is
    /// synced so back-navigation lands somewhere sensible.
    pub fn open_by_key(&mut self, key: &str) {
        self.detail_scroll = 0;
        let detail = self.load_detail(key);
        self.detail_cache.insert(key.to_string(), detail.clone());
        self.detail = Some(detail);
        self.screen = Screen::Detail;
        if let Some(pos) = self.issues.iter().position(|i| i.key == key) {
            self.selected = pos;
        }
    }

    /// Fetch an issue's full detail: live REST when connected, otherwise the
    /// offline demo detail. Used by both `open_by_key` and the quick-view
    /// panel's lazy loader, so it's crate-visible rather than file-private.
    #[allow(unused_variables)]
    pub(crate) fn load_detail(&mut self, key: &str) -> IssueDetail {
        #[cfg(feature = "live")]
        {
            if let Source::Live { .. } = self.source {
                if let Some(cfg) = crate::jira::Config::load() {
                    match crate::jira::fetch_detail(&cfg, key) {
                        Ok(d) => {
                            self.status = format!("Loaded {key}");
                            return d;
                        }
                        Err(e) => {
                            self.status = format!("Live fetch failed ({e}); showing sample");
                        }
                    }
                }
            }
        }
        demo_detail(key)
    }
}
