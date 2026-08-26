//! Opening an issue's full detail, by selection or directly by key.

use crate::domain::{demo_detail, IssueDetail, Source};

use super::{async_ops, App, Screen};

impl App {
    /// `x` — fold/unfold the narrow Detail layout's facts panel to one line
    /// (SPEC.md §6). A no-op in the wide layout — there's no facts panel to
    /// fold there — so this is deliberately unguarded by screen width.
    ///
    /// Resets `detail_scroll` to the top: folding/unfolding changes the
    /// narrow document's total line count, so the same absolute scroll
    /// offset would otherwise land on unrelated content after the toggle.
    pub fn toggle_facts_folded(&mut self) {
        self.facts_folded = !self.facts_folded;
        self.detail_scroll = 0;
    }

    pub fn open_detail(&mut self) {
        let Some(issue) = self.selected_issue().cloned() else {
            return;
        };
        self.open_by_key(&issue.key);
    }

    /// Load and open an issue by key directly, regardless of whether it's in
    /// the current filtered/sorted view. Used by search results, board/
    /// release cards, and "go to issue" — every genuinely fresh navigation,
    /// as opposed to following an in-body link (see `App::follow_link`,
    /// which is what `←`/`→`/`,`/`.` end up stepping back through). If the
    /// key is present in the current view, `selected` is synced so
    /// back-navigation lands somewhere sensible.
    ///
    /// Demo/cache sessions resolve inline (no network call to speak of); a
    /// genuine live session dispatches the fetch off the render thread and
    /// navigates to `Screen::Detail` once it lands — see `dispatch_detail_fetch`.
    pub fn open_by_key(&mut self, key: &str) {
        self.nav.visit_fresh(key);
        self.show_issue(key);
    }

    /// The actual issue-detail load/display, shared by `open_by_key` and
    /// `app::history`'s back/forward navigation — unlike `open_by_key`, this
    /// doesn't touch the navigation history, since history steps manage
    /// their own back/forward bookkeeping around a call to this.
    pub(crate) fn show_issue(&mut self, key: &str) {
        self.detail_scroll = 0;
        self.link_index = 0;
        self.facts_folded = false;
        if !matches!(self.source, Source::Live { .. }) {
            let detail = self.load_detail(key);
            self.resolve_detail_sync(key, detail);
            return;
        }
        // A locally-fabricated key (created while offline, before the
        // session reconnected to a genuine live source) can never exist
        // server-side — dispatching a live fetch for it would just 404 and
        // fall back to `demo_detail`'s generic "not found" placeholder,
        // regressing the exact bug the non-live branch above is fixed for.
        // Resolve it the same way, synchronously.
        if let Some(found) = self.locally_created.iter().find(|c| c.summary.key == key) {
            let detail = found.detail.clone();
            self.resolve_detail_sync(key, detail);
            return;
        }
        self.dispatch_detail_fetch(key.to_string(), true);
    }

    /// Land an already-resolved detail synchronously: cache it, show it, and
    /// sync `selected` if the key is in the current view. Shared by
    /// `show_issue`'s non-live branch and its `locally_created` shortcut for
    /// a live session above.
    fn resolve_detail_sync(&mut self, key: &str, detail: IssueDetail) {
        self.detail_cache.insert(key.to_string(), detail.clone());
        self.detail = Some(detail);
        #[cfg(feature = "images")]
        {
            self.invalidate_attachment_preview();
            self.invalidate_inline_images();
            self.refresh_inline_images();
        }
        self.screen = Screen::Detail;
        if let Some(pos) = self.issues.iter().position(|i| i.key == key) {
            self.selected = pos;
        }
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
                #[cfg(feature = "images")]
                {
                    self.invalidate_attachment_preview();
                    self.invalidate_inline_images();
                    self.refresh_inline_images();
                }
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
        // A freshly-created demo/cache issue isn't in `demo_issues()` (which
        // is regenerated from scratch on every call) — without this lookup,
        // reopening it would land on `demo_detail_not_found`'s generic
        // placeholder even though it was just created. See `app::new_issue`.
        if let Some(found) = self.locally_created.iter().find(|c| c.summary.key == key) {
            return found.detail.clone();
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
