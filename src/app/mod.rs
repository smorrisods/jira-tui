//! Application state, event handling, and the data-loading glue.

use crate::domain::{demo_detail, demo_issues, IssueDetail, IssueSummary, Source};
use crate::git::GitContext;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    List,
    Detail,
    About,
}

pub struct App {
    pub issues: Vec<IssueSummary>,
    pub selected: usize,
    pub screen: Screen,
    pub detail: Option<IssueDetail>,
    pub detail_scroll: u16,
    pub source: Source,
    pub git: GitContext,
    pub tick: u64,
    pub status: String,
    pub show_help: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new(force_demo: bool) -> Self {
        let git = GitContext::detect();
        let (issues, source, status) = load_issues(force_demo);

        // If the current branch maps to a known issue, pre-select it.
        let mut selected = 0;
        if let Some(key) = git.issue_key.as_ref() {
            if let Some(idx) = issues.iter().position(|i| &i.key == key) {
                selected = idx;
            }
        }

        App {
            issues,
            selected,
            screen: Screen::Home,
            detail: None,
            detail_scroll: 0,
            source,
            git,
            tick: 0,
            status,
            show_help: false,
            should_quit: false,
        }
    }

    pub fn selected_issue(&self) -> Option<&IssueSummary> {
        self.issues.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.issues.is_empty() {
            return;
        }
        let len = self.issues.len() as isize;
        let mut idx = self.selected as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len {
            idx = len - 1;
        }
        self.selected = idx as usize;
    }

    pub fn assigned_to_me(&self) -> Vec<&IssueSummary> {
        self.issues
            .iter()
            .filter(|i| i.assignee.is_some() && i.status != "Done")
            .collect()
    }

    pub fn blocked(&self) -> Vec<&IssueSummary> {
        self.issues.iter().filter(|i| i.blocked).collect()
    }

    pub fn open_detail(&mut self) {
        let Some(issue) = self.selected_issue().cloned() else {
            return;
        };
        self.detail_scroll = 0;
        self.detail = Some(self.load_detail(&issue.key));
        self.screen = Screen::Detail;
    }

    #[allow(unused_variables)]
    fn load_detail(&mut self, key: &str) -> IssueDetail {
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

    pub fn refresh(&mut self) {
        let force_demo = matches!(self.source, Source::Demo);
        let (issues, source, status) = load_issues(force_demo);
        self.issues = issues;
        self.source = source;
        self.status = format!("↻ {status}");
        if self.selected >= self.issues.len() {
            self.selected = self.issues.len().saturating_sub(1);
        }
    }
}

fn load_issues(force_demo: bool) -> (Vec<IssueSummary>, Source, String) {
    if !force_demo {
        #[cfg(feature = "live")]
        {
            if let Some(cfg) = crate::jira::Config::load() {
                let user = crate::jira::whoami(&cfg).unwrap_or_else(|_| "me".into());
                match crate::jira::fetch_my_work(&cfg) {
                    Ok(issues) if !issues.is_empty() => {
                        let host = cfg.site_host();
                        let n = issues.len();
                        return (
                            issues,
                            Source::Live { site: host, user },
                            format!("Loaded {n} issues from Jira"),
                        );
                    }
                    Ok(_) => {
                        return (
                            demo_issues(),
                            Source::Demo,
                            "No live issues found — showing sample data".into(),
                        );
                    }
                    Err(e) => {
                        return (
                            demo_issues(),
                            Source::Demo,
                            format!("Jira unreachable ({e}) — showing sample data"),
                        );
                    }
                }
            }
        }
    }
    (
        demo_issues(),
        Source::Demo,
        "Offline demo — set JIRA_EMAIL + token for live mode".into(),
    )
}
