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
    /// If already viewing another issue's Detail, the outgoing issue is
    /// pushed onto the back-navigation history (see `app::history`) so
    /// `←`/`→` can step through issues followed via in-body links; opening
    /// fresh from the list/search starts a new history instead.
    ///
    /// Demo/cache sessions resolve inline (no network call to speak of); a
    /// genuine live session dispatches the fetch off the render thread and
    /// navigates to `Screen::Detail` once it lands — see `dispatch_detail_fetch`.
    pub fn open_by_key(&mut self, key: &str) {
        if self.screen == Screen::Detail {
            if let Some(current) = self.detail.as_ref().map(|d| d.key.clone()) {
                if current != key {
                    self.detail_back.push(current);
                    self.detail_forward.clear();
                }
            }
        } else {
            self.detail_back.clear();
            self.detail_forward.clear();
        }
        self.show_issue(key);
    }

    /// The actual issue-detail load/display, shared by `open_by_key` and
    /// `app::history`'s back/forward navigation — unlike `open_by_key`, this
    /// doesn't touch the navigation history, since history steps manage
    /// their own back/forward bookkeeping around a call to this.
    pub(crate) fn show_issue(&mut self, key: &str) {
        self.detail_scroll = 0;
        self.link_index = 0;
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

    /// Re-fetch the currently viewed (Detail screen) or quick-viewed issue's
    /// full detail — description, comments, transitions, links — picking up
    /// changes made outside the TUI (e.g. a comment added via the Jira web
    /// UI, another tool, or a teammate) that the session cache wouldn't
    /// otherwise reflect. There's no push/webhook-based live-watch of the
    /// open issue, so this is a manual "check again" bound to `r` (which
    /// otherwise refreshes the issue *list*) whenever Detail or a
    /// keyboard-focused quick view is showing something to refresh.
    ///
    /// Deliberately bypasses `open_by_key`: refreshing the same issue is not
    /// a navigation and must not push/clear the back/forward link-history
    /// stacks (see `app::history`).
    pub fn refresh_detail(&mut self) {
        let key = match self.screen {
            Screen::Detail => self.detail.as_ref().map(|d| d.key.clone()),
            _ if self.quick_view => self.selected_issue().map(|i| i.key.clone()),
            _ => None,
        };
        let Some(key) = key else {
            return;
        };

        self.detail_cache.remove(&key);
        if !matches!(self.source, Source::Live { .. }) {
            let detail = self.load_detail(&key);
            self.detail_cache.insert(key.clone(), detail.clone());
            if self.screen == Screen::Detail {
                self.detail = Some(detail);
            }
            self.status = format!("refreshed {key}");
            self.flash(format!("↻ refreshed {key}"));
            return;
        }

        // `navigate` only controls whether `self.detail`/`detail_scroll`
        // get updated once the fetch resolves (see `AppEvent::DetailLoaded`)
        // — set it when we're actually viewing this issue in Detail, and
        // leave it unset for a quick-view-only refresh, where updating
        // `detail_cache` is all `quick_view_detail` needs.
        self.dispatch_detail_fetch(key, self.screen == Screen::Detail);
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
    /// must be safe to call repeatedly without piling up requests). If a
    /// cache-only quick-view load for this key is already in flight and an
    /// explicit "open" comes in before it resolves, the pending request's
    /// navigate intent is escalated in place rather than dropped or
    /// double-dispatched.
    pub(crate) fn dispatch_detail_fetch(&mut self, key: String, navigate: bool) {
        if let Some((pending_key, pending_navigate)) = self.detail_pending.as_mut() {
            if pending_key == &key {
                *pending_navigate = *pending_navigate || navigate;
                return;
            }
        }
        self.detail_generation += 1;
        let generation = self.detail_generation;
        self.detail_pending = Some((key.clone(), navigate));
        self.loading = true;
        self.status = format!("↻ loading {key}…");
        let tx = self.events_tx.clone();
        async_ops::dispatch_detail_fetch(tx, generation, key);
    }
}
