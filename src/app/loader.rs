//! The top-level data loader: fetching (or falling back to demo/cached
//! data for) whichever view is active, and the on-disk cache lifecycle
//! that sits in front of a live fetch.

use crate::domain::{demo_issues, IssueSummary, Source, ViewKind};

use super::{async_ops, App};

impl App {
    /// Reload the current view. Demo/cache-only sessions resolve inline
    /// (there's no network round-trip worth a spinner for); a genuine live
    /// fetch dispatches onto the runtime instead of blocking the render
    /// thread — see `async_ops::dispatch_load`.
    pub fn refresh(&mut self) {
        let force_demo = matches!(self.source, Source::Demo);
        let kind = self.current_view.clone();
        if force_demo {
            let (issues, source, status) = load_issues_for(&kind, force_demo);
            self.all_issues = issues;
            self.source = source;
            self.status = format!("↻ {status}");
            self.recompute_view();
            return;
        }
        let generation = self.bump_generation();
        self.loading = true;
        self.status = format!("↻ loading {}…", kind.label());
        let tx = self.events_tx.clone();
        async_ops::dispatch_refresh(tx, generation, kind, force_demo);
    }
}

/// Open the on-disk cache for the current site, running the one-time
/// legacy `my-work.json` import along the way — unless caching is
/// disabled (`cache_enabled` setting / `--no-cache` / `JIRA_NO_CACHE`), or
/// the cache can't be opened at all (treated the same as "no cache
/// available", exactly like a missing/corrupt `my-work.json` always was).
#[cfg(feature = "live")]
fn open_cache_for_site(cfg: &crate::jira::Config) -> Option<(crate::cache::Cache, i64)> {
    if !crate::config::Settings::load().cache_enabled {
        return None;
    }
    let mut cache = crate::cache::Cache::open().ok()?;
    let site_id = cache.site_id(&cfg.base_url).ok()?;
    cache.migrate_legacy_json(site_id, crate::jira::MY_WORK_JQL);
    Some((cache, site_id))
}

/// The issues to show offline for a given view. `MyWork`/`AllProject` both
/// show the full baked-in demo set (the demo dataset stands in for "the
/// whole project" — there's no offline notion of a distinct "my" subset);
/// `Teammate` filters it down to that teammate's assigned issues, so the
/// view picker is meaningfully explorable with zero credentials.
fn demo_view_for(view: &ViewKind) -> Vec<IssueSummary> {
    match view {
        ViewKind::MyWork | ViewKind::AllProject => demo_issues(),
        ViewKind::Teammate(name) => demo_issues()
            .into_iter()
            .filter(|i| i.assignee.as_deref() == Some(name.as_str()))
            .collect(),
    }
}

pub(super) fn load_issues(force_demo: bool) -> (Vec<IssueSummary>, Source, String) {
    load_issues_for(&ViewKind::MyWork, force_demo)
}

/// Fetch (or fall back to demo/cached data for) whichever view is active.
/// Every view (My Work, All Project Issues, a teammate's work) gets its own
/// durable on-disk SQLite cache entry (`ViewKind::cache_kind`), so switching
/// views doesn't always re-hit the API and still has an offline fallback.
pub(super) fn load_issues_for(
    view: &ViewKind,
    force_demo: bool,
) -> (Vec<IssueSummary>, Source, String) {
    if !force_demo {
        #[cfg(feature = "live")]
        {
            if let Some(cfg) = crate::jira::Config::load() {
                let user = crate::jira::whoami(&cfg).unwrap_or_else(|_| "me".into());
                let mut cache = open_cache_for_site(&cfg);
                let jql = crate::jira::jql_for(view, &cfg.project);
                let kind = view.cache_kind();
                match crate::jira::search_issues(&cfg, &jql) {
                    Ok(issues) if !issues.is_empty() => {
                        let host = cfg.site_host();
                        let n = issues.len();
                        if let Some((cache, site_id)) = &mut cache {
                            let _ = cache.save_view(*site_id, &kind, &view.label(), &jql, &issues);
                        }
                        // search_issues now pages until Jira reports
                        // `isLast`, but still stops at SEARCH_RESULTS_CAP so
                        // a very large project can't page forever — flag it
                        // when that cap was actually hit.
                        let status = if n >= crate::jira::SEARCH_RESULTS_CAP {
                            format!("Loaded {n} issues from Jira (capped at {n}; more may exist)")
                        } else {
                            format!("Loaded {n} issues from Jira")
                        };
                        return (issues, Source::Live { site: host, user }, status);
                    }
                    Ok(_) => {
                        return (
                            demo_view_for(view),
                            Source::Demo,
                            format!("No issues found for {} — showing sample data", view.label()),
                        );
                    }
                    Err(e) => {
                        // Prefer the last cached list over sample data offline.
                        let cached = cache
                            .as_ref()
                            .and_then(|(cache, site_id)| cache.load_view(*site_id, &kind).ok())
                            .flatten();
                        if let Some(cached) = cached {
                            let n = cached.len();
                            return (
                                cached,
                                Source::Cache { user },
                                format!("Jira unreachable ({e}) — showing {n} cached issues"),
                            );
                        }
                        return (
                            demo_view_for(view),
                            Source::Demo,
                            format!("Jira unreachable ({e}) — showing sample data"),
                        );
                    }
                }
            }
        }
    }
    (
        demo_view_for(view),
        Source::Demo,
        "Offline demo — set JIRA_EMAIL + token for live mode".into(),
    )
}
