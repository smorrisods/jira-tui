//! Opening an issue's full detail, by selection or directly by key.

use crate::domain::{demo_detail, IssueDetail, Source};

use super::{async_ops, App, Screen};

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
    ///
    /// Demo/cache sessions resolve inline (no network call to speak of); a
    /// genuine live session dispatches the fetch off the render thread and
    /// navigates to `Screen::Detail` once it lands — see `dispatch_detail_fetch`.
    pub fn open_by_key(&mut self, key: &str) {
        self.detail_scroll = 0;
        if !matches!(self.source, Source::Live { .. }) {
            let detail = self.load_detail(key);
            self.detail_cache.insert(key.to_string(), detail.clone());
            self.detail = Some(detail);
            self.screen = Screen::Detail;
            if let Some(pos) = self.issues.iter().position(|i| i.key == key) {
                self.selected = pos;
            }
            return;
        }
        self.dispatch_detail_fetch(key.to_string(), true);
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

    /// Dispatch a full-detail fetch off the render thread, deduplicating
    /// against an already-in-flight fetch for the same key (the quick-view
    /// panel calls this every tick via `ensure_quick_view_loaded`, so it
    /// must be safe to call repeatedly without piling up requests).
    pub(crate) fn dispatch_detail_fetch(&mut self, key: String, navigate: bool) {
        if self.detail_pending.as_deref() == Some(key.as_str()) {
            return;
        }
        self.detail_generation += 1;
        let generation = self.detail_generation;
        self.detail_pending = Some(key.clone());
        self.loading = true;
        self.status = format!("↻ loading {key}…");
        let tx = self.events_tx.clone();
        async_ops::dispatch_detail_fetch(tx, generation, key, navigate);
    }
}
