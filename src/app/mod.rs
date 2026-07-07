//! Application state, event handling, and the data-loading glue.

use std::cell::Cell;
use std::collections::HashMap;

use ratatui::layout::Rect;

use crate::config::{self, Settings};
use crate::domain::{demo_detail, demo_issues, IssueDetail, IssueSummary, Source};
use crate::git::GitContext;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Welcome,
    Home,
    List,
    Detail,
    Preview,
    Edit,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Updated,
    Priority,
    Status,
    Key,
}

impl SortKey {
    pub fn label(&self) -> &'static str {
        match self {
            SortKey::Updated => "updated",
            SortKey::Priority => "priority",
            SortKey::Status => "status",
            SortKey::Key => "key",
        }
    }
    fn next(&self) -> SortKey {
        match self {
            SortKey::Updated => SortKey::Priority,
            SortKey::Priority => SortKey::Status,
            SortKey::Status => SortKey::Key,
            SortKey::Key => SortKey::Updated,
        }
    }
}

pub struct App {
    /// Full server-side list; `issues` is the filtered + sorted view of this.
    pub all_issues: Vec<IssueSummary>,
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

    // Sort + filter.
    pub sort_key: SortKey,
    pub sort_asc: bool,
    pub filter_status: Option<String>,

    // Quick-view panel + a cache of opened issue details.
    pub quick_view: bool,
    pub detail_cache: HashMap<String, IssueDetail>,

    /// Ambient Jax companion (pure entertainment 🦦).
    pub show_jax: bool,

    // In-TUI editor.
    pub editor: EditorState,

    /// Transient toast message; shown while `tick < flash_until`.
    pub flash_msg: String,
    pub flash_until: u64,

    // Mouse mode + drag selection.
    pub mouse_enabled: bool,
    pub selecting: bool,
    pub sel_start_y: u16,
    pub sel_end_y: u16,
    /// Row range (inclusive, screen coords) whose text should be copied.
    pub pending_copy: Option<(u16, u16)>,

    // Draw geometry recorded during render, for mapping mouse coordinates.
    pub list_area: Cell<Rect>,
    pub list_start: Cell<usize>,
    pub detail_area: Cell<Rect>,

    // Onboarding welcome + credential setup.
    pub welcome_phase: WelcomePhase,
    pub field_site: String,
    pub field_email: String,
    pub field_token: String,
    pub focus: Field,
    pub setup_msg: String,

    // Transition picker + round-trip edit.
    pub picker_open: bool,
    pub picker_index: usize,
    pub pending_edit: Option<serde_json::Value>,
    /// Set by a key handler to ask the run loop to launch `$EDITOR`.
    pub request_edit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WelcomePhase {
    Intro,
    Setup,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Site,
    Email,
    Token,
}

/// A minimal multi-line text editor for in-TUI description editing.
#[derive(Clone, Debug, Default)]
pub struct EditorState {
    pub lines: Vec<String>,
    pub cx: usize,
    pub cy: usize,
    pub scroll: u16,
}

impl EditorState {
    pub fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        EditorState {
            lines,
            cx: 0,
            cy: 0,
            scroll: 0,
        }
    }

    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    fn line_len(&self, y: usize) -> usize {
        self.lines.get(y).map(|l| l.chars().count()).unwrap_or(0)
    }

    pub fn insert_char(&mut self, c: char) {
        let line = &mut self.lines[self.cy];
        let byte = line
            .char_indices()
            .nth(self.cx)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        line.insert(byte, c);
        self.cx += 1;
    }

    pub fn newline(&mut self) {
        let line = self.lines[self.cy].clone();
        let byte = line
            .char_indices()
            .nth(self.cx)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        let (left, right) = line.split_at(byte);
        self.lines[self.cy] = left.to_string();
        self.lines.insert(self.cy + 1, right.to_string());
        self.cy += 1;
        self.cx = 0;
    }

    pub fn backspace(&mut self) {
        if self.cx > 0 {
            let line = &mut self.lines[self.cy];
            let byte = line
                .char_indices()
                .nth(self.cx - 1)
                .map(|(i, _)| i)
                .unwrap();
            line.remove(byte);
            self.cx -= 1;
        } else if self.cy > 0 {
            let removed = self.lines.remove(self.cy);
            self.cy -= 1;
            self.cx = self.line_len(self.cy);
            self.lines[self.cy].push_str(&removed);
        }
    }

    pub fn left(&mut self) {
        if self.cx > 0 {
            self.cx -= 1;
        } else if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.line_len(self.cy);
        }
    }

    pub fn right(&mut self) {
        if self.cx < self.line_len(self.cy) {
            self.cx += 1;
        } else if self.cy + 1 < self.lines.len() {
            self.cy += 1;
            self.cx = 0;
        }
    }

    pub fn up(&mut self) {
        if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.cx.min(self.line_len(self.cy));
        }
    }

    pub fn down(&mut self) {
        if self.cy + 1 < self.lines.len() {
            self.cy += 1;
            self.cx = self.cx.min(self.line_len(self.cy));
        }
    }
}

impl App {
    pub fn new(force_demo: bool) -> Self {
        let git = GitContext::detect();
        let (issues, source, status) = load_issues(force_demo);
        let settings = Settings::load();

        let mut app = App {
            all_issues: issues.clone(),
            issues,
            selected: 0,
            screen: if config::is_onboarded() {
                Screen::Home
            } else {
                Screen::Welcome
            },
            detail: None,
            detail_scroll: 0,
            source,
            git,
            tick: 0,
            status,
            show_help: false,
            should_quit: false,
            sort_key: SortKey::Updated,
            sort_asc: false,
            filter_status: None,
            quick_view: false,
            detail_cache: HashMap::new(),
            show_jax: false,
            editor: EditorState::default(),
            flash_msg: String::new(),
            flash_until: 0,
            mouse_enabled: settings.mouse,
            selecting: false,
            sel_start_y: 0,
            sel_end_y: 0,
            pending_copy: None,
            list_area: Cell::new(Rect::default()),
            list_start: Cell::new(0),
            detail_area: Cell::new(Rect::default()),
            welcome_phase: WelcomePhase::Intro,
            field_site: "https://your-org.atlassian.net".to_string(),
            field_email: String::new(),
            field_token: String::new(),
            focus: Field::Site,
            setup_msg: String::new(),
            picker_open: false,
            picker_index: 0,
            pending_edit: None,
            request_edit: false,
        };
        app.recompute_view();

        // If the current branch maps to a known issue, pre-select it.
        if let Some(key) = app.git.issue_key.clone() {
            if let Some(idx) = app.issues.iter().position(|i| i.key == key) {
                app.selected = idx;
            }
        }
        app
    }

    // ── Sort + filter ────────────────────────────────────────────────────────

    /// Rebuild `issues` from `all_issues` applying the current filter and sort,
    /// preserving the selected issue by key where possible.
    pub fn recompute_view(&mut self) {
        let cur_key = self.selected_issue().map(|i| i.key.clone());

        let mut view: Vec<IssueSummary> = self
            .all_issues
            .iter()
            .filter(|i| {
                self.filter_status
                    .as_ref()
                    .map(|s| &i.status == s)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        let key_num = |k: &str| -> u64 {
            k.rsplit('-')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0)
        };
        view.sort_by(|a, b| {
            let ord = match self.sort_key {
                SortKey::Updated => a.updated.cmp(&b.updated),
                SortKey::Priority => a.priority.rank().cmp(&b.priority.rank()),
                SortKey::Status => a.status.cmp(&b.status),
                SortKey::Key => key_num(&a.key).cmp(&key_num(&b.key)),
            };
            if self.sort_asc {
                ord
            } else {
                ord.reverse()
            }
        });

        self.issues = view;
        self.selected = cur_key
            .and_then(|k| self.issues.iter().position(|i| i.key == k))
            .unwrap_or(0)
            .min(self.issues.len().saturating_sub(1));
    }

    pub fn cycle_sort(&mut self) {
        self.sort_key = self.sort_key.next();
        self.recompute_view();
        self.status = format!(
            "sort: {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        );
    }

    pub fn toggle_sort_dir(&mut self) {
        self.sort_asc = !self.sort_asc;
        self.recompute_view();
        self.status = format!(
            "sort: {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        );
    }

    /// Cycle the status filter through: all → each distinct status → all.
    pub fn cycle_filter(&mut self) {
        let mut statuses: Vec<String> = Vec::new();
        for i in &self.all_issues {
            if !statuses.contains(&i.status) {
                statuses.push(i.status.clone());
            }
        }
        statuses.sort();
        self.filter_status = match &self.filter_status {
            None => statuses.first().cloned(),
            Some(cur) => {
                let idx = statuses.iter().position(|s| s == cur);
                match idx {
                    Some(i) if i + 1 < statuses.len() => Some(statuses[i + 1].clone()),
                    _ => None,
                }
            }
        };
        self.recompute_view();
        self.status = match &self.filter_status {
            Some(s) => format!("filter: {s}"),
            None => "filter: all".into(),
        };
    }

    pub fn sort_label(&self) -> String {
        format!(
            "sort {} {}",
            self.sort_key.label(),
            if self.sort_asc { "↑" } else { "↓" }
        )
    }

    pub fn filter_label(&self) -> Option<String> {
        self.filter_status.as_ref().map(|s| format!("filter {s}"))
    }

    pub fn selected_issue(&self) -> Option<&IssueSummary> {
        self.issues.get(self.selected)
    }

    /// Show a transient toast for roughly 1.5s (tied to the animation tick).
    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash_msg = msg.into();
        self.flash_until = self.tick + 18;
    }

    /// The active toast message, if one is currently showing.
    pub fn active_flash(&self) -> Option<&str> {
        if self.tick < self.flash_until && !self.flash_msg.is_empty() {
            Some(&self.flash_msg)
        } else {
            None
        }
    }

    /// The Jira URL for the selected issue, when we know the site.
    pub fn selected_issue_url(&self) -> Option<String> {
        let issue = self.selected_issue()?;
        let site = match &self.source {
            Source::Live { site, .. } => site.clone(),
            _ => "your-org.atlassian.net".to_string(),
        };
        Some(format!("https://{site}/browse/{}", issue.key))
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
        self.all_issues
            .iter()
            .filter(|i| i.assignee.is_some() && i.status != "Done")
            .collect()
    }

    pub fn blocked(&self) -> Vec<&IssueSummary> {
        self.all_issues.iter().filter(|i| i.blocked).collect()
    }

    pub fn open_detail(&mut self) {
        let Some(issue) = self.selected_issue().cloned() else {
            return;
        };
        self.detail_scroll = 0;
        let detail = self.load_detail(&issue.key);
        self.detail_cache.insert(issue.key.clone(), detail.clone());
        self.detail = Some(detail);
        self.screen = Screen::Detail;
    }

    /// The cached detail for the currently selected issue, if any (quick view).
    pub fn quick_view_detail(&self) -> Option<&IssueDetail> {
        let key = &self.selected_issue()?.key;
        self.detail_cache.get(key)
    }

    // ── In-TUI editor ────────────────────────────────────────────────────────

    /// Open the built-in editor preloaded with the description Markdown.
    pub fn begin_tui_edit(&mut self) {
        if let Some(md) = self.description_markdown() {
            self.editor = EditorState::from_text(&md);
            self.screen = Screen::Edit;
        }
    }

    /// Compile the editor buffer and move to the confirmation preview.
    pub fn commit_tui_edit(&mut self) {
        let text = self.editor.to_text();
        self.finish_edit(&text);
    }

    pub fn open_transitions(&mut self) {
        if let Some(detail) = self.detail.as_ref() {
            if detail.transitions.is_empty() {
                self.status = "no transitions available".into();
                return;
            }
            // Pre-select the current status if present.
            self.picker_index = detail
                .transitions
                .iter()
                .position(|t| t.to == detail.status)
                .unwrap_or(0);
            self.picker_open = true;
        }
    }

    pub fn close_picker(&mut self) {
        self.picker_open = false;
    }

    pub fn picker_move(&mut self, delta: isize) {
        let len = self
            .detail
            .as_ref()
            .map(|d| d.transitions.len())
            .unwrap_or(0);
        if len == 0 {
            return;
        }
        let mut idx = self.picker_index as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.picker_index = idx as usize;
    }

    /// Apply the highlighted transition (live if possible, always locally).
    pub fn confirm_transition(&mut self) {
        let Some(detail) = self.detail.as_ref() else {
            self.picker_open = false;
            return;
        };
        let Some(t) = detail.transitions.get(self.picker_index).cloned() else {
            self.picker_open = false;
            return;
        };
        let key = detail.key.clone();
        self.picker_open = false;

        #[cfg(feature = "live")]
        {
            if let Source::Live { .. } = self.source {
                if let Some(cfg) = crate::jira::Config::load() {
                    if let Err(e) = crate::jira::apply_transition(&cfg, &key, &t.id) {
                        self.status = format!("transition failed: {e}");
                        return;
                    }
                }
            }
        }

        if let Some(d) = self.detail.as_mut() {
            d.status = t.to.clone();
        }
        if let Some(sum) = self.issues.iter_mut().find(|i| i.key == key) {
            sum.status = t.to.clone();
        }
        self.status = format!("moved {key} → {}", t.to);
        self.flash(format!("✓ moved to {}", t.to));
    }

    // ── Round-trip edit ──────────────────────────────────────────────────────

    /// Markdown for the current issue description, to seed an editor session.
    pub fn description_markdown(&self) -> Option<String> {
        self.detail
            .as_ref()
            .map(|d| crate::adf::to_markdown(&d.description))
    }

    /// Compile edited Markdown to ADF and show it for confirmation.
    pub fn finish_edit(&mut self, markdown: &str) {
        let adf = crate::adf::compile(markdown);
        self.pending_edit = Some(adf);
        self.detail_scroll = 0;
        self.screen = Screen::Preview;
    }

    pub fn cancel_edit(&mut self) {
        self.pending_edit = None;
        self.screen = Screen::Detail;
    }

    /// Apply the previewed description (live if possible, always locally).
    pub fn apply_edit(&mut self) {
        let Some(adf) = self.pending_edit.take() else {
            self.screen = Screen::Detail;
            return;
        };
        let Some(key) = self.detail.as_ref().map(|d| d.key.clone()) else {
            self.screen = Screen::Detail;
            return;
        };

        #[cfg(feature = "live")]
        {
            if let Source::Live { .. } = self.source {
                if let Some(cfg) = crate::jira::Config::load() {
                    if let Err(e) = crate::jira::update_description(&cfg, &key, &adf) {
                        self.status = format!("update failed: {e}");
                        self.screen = Screen::Detail;
                        return;
                    }
                }
            }
        }

        if let Some(d) = self.detail.as_mut() {
            d.description = adf;
        }
        self.status = format!("updated {key} description");
        self.flash("✓ description updated");
        self.screen = Screen::Detail;
    }

    // ── Onboarding ───────────────────────────────────────────────────────────

    /// Dismiss the welcome screen and remember not to show it again.
    pub fn finish_onboarding(&mut self) {
        config::mark_onboarded();
        self.screen = Screen::Home;
    }

    /// Write the default config file from the welcome screen.
    pub fn write_config_from_welcome(&mut self) {
        match config::write_default_config() {
            Ok((path, true)) => {
                self.status = format!("wrote config to {}", path.display());
            }
            Ok((path, false)) => {
                self.status = format!("config already exists at {}", path.display());
            }
            Err(e) => {
                self.status = format!("could not write config: {e}");
            }
        }
    }

    // ── Credential setup form ────────────────────────────────────────────────

    fn focused_field_mut(&mut self) -> &mut String {
        match self.focus {
            Field::Site => &mut self.field_site,
            Field::Email => &mut self.field_email,
            Field::Token => &mut self.field_token,
        }
    }

    pub fn input_char(&mut self, c: char) {
        self.focused_field_mut().push(c);
    }

    pub fn input_backspace(&mut self) {
        self.focused_field_mut().pop();
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Field::Site => Field::Email,
            Field::Email => Field::Token,
            Field::Token => Field::Site,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Field::Site => Field::Token,
            Field::Email => Field::Site,
            Field::Token => Field::Email,
        };
    }

    /// Validate, verify against Jira, and persist the entered credentials.
    /// On success, switches to live data and finishes onboarding.
    pub fn submit_credentials(&mut self) {
        let site = self.field_site.trim().trim_end_matches('/').to_string();
        let email = self.field_email.trim().to_string();
        let token = self.field_token.trim().to_string();
        if site.is_empty() || email.is_empty() || token.is_empty() {
            self.setup_msg = "Please fill site, email, and token.".into();
            return;
        }

        // Persist first so the standard config path picks them up.
        if let Err(e) = config::save_token(&token) {
            self.setup_msg = format!("Could not save token: {e}");
            return;
        }
        if let Err(e) = config::save_settings(&site, &email, "DS") {
            self.setup_msg = format!("Could not save settings: {e}");
            return;
        }
        std::env::set_var("JIRA_BASE_URL", &site);
        std::env::set_var("JIRA_EMAIL", &email);
        std::env::set_var("JIRA_API_TOKEN", &token);

        #[cfg(feature = "live")]
        {
            self.setup_msg = "Verifying…".into();
            let (issues, source, status) = load_issues(false);
            match source {
                Source::Live { .. } => {
                    self.all_issues = issues;
                    self.source = source;
                    self.status = status;
                    self.recompute_view();
                    config::mark_onboarded();
                    self.screen = Screen::Home;
                }
                _ => {
                    self.setup_msg =
                        "Saved, but Jira did not accept those credentials. Check and retry, or press Esc to continue in demo mode.".into();
                }
            }
        }
        #[cfg(not(feature = "live"))]
        {
            self.setup_msg =
                "Saved. This build has no live support; rebuild with the `live` feature.".into();
        }
    }

    // ── Mouse mode + drag selection ──────────────────────────────────────────

    /// Map a screen row to an issue index within the recorded list area.
    pub fn list_index_at(&self, y: u16) -> Option<usize> {
        let area = self.list_area.get();
        if area.height == 0 || y < area.y || y >= area.y + area.height {
            return None;
        }
        let idx = self.list_start.get() + (y - area.y) as usize;
        (idx < self.issues.len()).then_some(idx)
    }

    pub fn mouse_down(&mut self, y: u16) {
        if matches!(self.screen, Screen::Home | Screen::List) {
            if let Some(idx) = self.list_index_at(y) {
                self.selected = idx;
            }
        }
        self.selecting = true;
        self.sel_start_y = y;
        self.sel_end_y = y;
    }

    pub fn mouse_drag(&mut self, y: u16) {
        if self.selecting {
            self.sel_end_y = y;
        }
    }

    pub fn mouse_up(&mut self, y: u16) {
        if !self.selecting {
            return;
        }
        self.selecting = false;
        self.sel_end_y = y;
        if self.sel_start_y == self.sel_end_y {
            // A click, not a drag: open the issue under the cursor.
            if matches!(self.screen, Screen::Home | Screen::List) && self.list_index_at(y).is_some()
            {
                self.open_detail();
            }
        } else {
            let a = self.sel_start_y.min(self.sel_end_y);
            let b = self.sel_start_y.max(self.sel_end_y);
            self.pending_copy = Some((a, b));
        }
    }

    /// The inclusive row range currently being drag-selected, for highlighting.
    pub fn selection_range(&self) -> Option<(u16, u16)> {
        self.selecting.then(|| {
            (
                self.sel_start_y.min(self.sel_end_y),
                self.sel_start_y.max(self.sel_end_y),
            )
        })
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
        self.all_issues = issues;
        self.source = source;
        self.status = format!("↻ {status}");
        self.recompute_view();
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
                        crate::config::cache_issues(&issues);
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
                        // Prefer the last cached list over sample data offline.
                        if let Some(cached) = crate::config::load_cached_issues() {
                            let n = cached.len();
                            return (
                                cached,
                                Source::Cache { user },
                                format!("Jira unreachable ({e}) — showing {n} cached issues"),
                            );
                        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn demo_app() -> App {
        let mut app = App::new(true);
        app.screen = Screen::Home;
        app
    }

    #[test]
    fn move_selection_clamps_to_bounds() {
        let mut app = demo_app();
        app.selected = 0;
        app.move_selection(-5);
        assert_eq!(app.selected, 0);
        app.move_selection(1000);
        assert_eq!(app.selected, app.issues.len() - 1);
    }

    #[test]
    fn list_index_at_maps_rows_to_issues() {
        let app = demo_app();
        app.list_area.set(Rect::new(0, 4, 80, 8));
        app.list_start.set(0);
        assert_eq!(app.list_index_at(4), Some(0));
        assert_eq!(app.list_index_at(6), Some(2));
        // Above the list area.
        assert_eq!(app.list_index_at(0), None);
        // Below the populated rows.
        assert_eq!(app.list_index_at(200), None);
    }

    #[test]
    fn click_opens_detail() {
        let mut app = demo_app();
        app.list_area.set(Rect::new(0, 4, 80, 8));
        app.list_start.set(0);
        app.mouse_down(5);
        app.mouse_up(5);
        assert_eq!(app.screen, Screen::Detail);
        assert!(app.detail.is_some());
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn drag_sets_a_pending_copy_range() {
        let mut app = demo_app();
        app.list_area.set(Rect::new(0, 4, 80, 8));
        app.mouse_down(5);
        assert_eq!(app.selection_range(), Some((5, 5)));
        app.mouse_drag(8);
        assert_eq!(app.selection_range(), Some((5, 8)));
        app.mouse_up(8);
        assert_eq!(app.pending_copy, Some((5, 8)));
        assert!(!app.selecting);
        assert_eq!(app.screen, Screen::Home, "a drag must not open detail");
    }

    #[test]
    fn credential_form_edits_focused_field() {
        let mut app = demo_app();
        app.focus = Field::Email;
        app.input_char('a');
        app.input_char('b');
        assert_eq!(app.field_email, "ab");
        app.input_backspace();
        assert_eq!(app.field_email, "a");
        app.focus_next();
        assert_eq!(app.focus, Field::Token);
        app.focus_prev();
        assert_eq!(app.focus, Field::Email);
    }

    #[test]
    fn submit_with_empty_fields_reports_and_does_not_panic() {
        let mut app = demo_app();
        app.field_site.clear();
        app.field_email.clear();
        app.field_token.clear();
        app.submit_credentials();
        assert!(!app.setup_msg.is_empty());
    }

    #[test]
    fn selected_issue_url_is_a_browse_link() {
        let app = demo_app();
        let url = app.selected_issue_url().unwrap();
        assert!(url.contains("/browse/DS-"));
    }

    #[test]
    fn confirm_transition_updates_status_locally() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        // Pick the "Done" transition (index 3 in the demo model).
        app.open_transitions();
        app.picker_index = 3;
        app.confirm_transition();
        assert!(!app.picker_open);
        assert_eq!(app.detail.as_ref().unwrap().status, "Done");
        // The summary list reflects it too.
        let key = &app.detail.as_ref().unwrap().key;
        assert_eq!(
            app.issues.iter().find(|i| &i.key == key).unwrap().status,
            "Done"
        );
    }

    #[test]
    fn edit_flow_previews_then_applies() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        let md = app.description_markdown().unwrap();
        assert!(md.contains("Problem"));

        app.finish_edit("## Edited\n\nBrand new body.");
        assert_eq!(app.screen, Screen::Preview);
        assert!(app.pending_edit.is_some());

        app.apply_edit();
        assert_eq!(app.screen, Screen::Detail);
        assert!(app.pending_edit.is_none());
        // The new ADF is now the description.
        let desc = &app.detail.as_ref().unwrap().description;
        let text = crate::adf::to_markdown(desc);
        assert!(text.contains("Edited"));
        assert!(text.contains("Brand new body"));
    }

    #[test]
    fn cancel_edit_discards_pending() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.finish_edit("## Nope");
        app.cancel_edit();
        assert_eq!(app.screen, Screen::Detail);
        assert!(app.pending_edit.is_none());
    }

    #[test]
    fn filter_narrows_and_clears() {
        let mut app = demo_app();
        let total = app.all_issues.len();
        // Cycle to the first status filter.
        app.cycle_filter();
        assert!(app.filter_status.is_some());
        let filtered = app.filter_status.clone().unwrap();
        assert!(app.issues.iter().all(|i| i.status == filtered));
        assert!(app.issues.len() <= total);
        // Cycle all the way back to "all".
        for _ in 0..20 {
            if app.filter_status.is_none() {
                break;
            }
            app.cycle_filter();
        }
        assert!(app.filter_status.is_none());
        assert_eq!(app.issues.len(), total);
    }

    #[test]
    fn sort_reorders_and_preserves_selection() {
        let mut app = demo_app();
        // Select a known issue, then re-sort; selection should follow the key.
        let key = app.issues[2].key.clone();
        app.selected = 2;
        app.sort_key = SortKey::Key;
        app.sort_asc = true;
        app.recompute_view();
        assert_eq!(app.selected_issue().unwrap().key, key);
        // Ascending by key: keys are non-decreasing.
        let nums: Vec<u64> = app
            .issues
            .iter()
            .map(|i| i.key.rsplit('-').next().unwrap().parse().unwrap())
            .collect();
        assert!(nums.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn quick_view_uses_cached_detail_after_open() {
        let mut app = demo_app();
        app.selected = 0;
        assert!(app.quick_view_detail().is_none());
        app.open_detail();
        // Returning to the list, the opened issue is cached for quick view.
        assert!(app.detail_cache.contains_key(&app.issues[0].key));
    }

    #[test]
    fn in_tui_editor_edits_then_commits_to_preview() {
        let mut app = demo_app();
        app.selected = 0;
        app.open_detail();
        app.begin_tui_edit();
        assert_eq!(app.screen, Screen::Edit);
        assert!(!app.editor.lines.is_empty());
        // Type a heading on a fresh first line.
        app.editor.cx = 0;
        app.editor.cy = 0;
        for c in "X ".chars() {
            app.editor.insert_char(c);
        }
        app.commit_tui_edit();
        assert_eq!(app.screen, Screen::Preview);
        assert!(app.pending_edit.is_some());
    }

    #[test]
    fn editor_newline_and_backspace_merge_lines() {
        let mut ed = EditorState::from_text("ab");
        ed.cx = 1;
        ed.newline();
        assert_eq!(ed.lines, vec!["a".to_string(), "b".to_string()]);
        assert_eq!((ed.cy, ed.cx), (1, 0));
        ed.backspace();
        assert_eq!(ed.lines, vec!["ab".to_string()]);
        assert_eq!((ed.cy, ed.cx), (0, 1));
    }
}
