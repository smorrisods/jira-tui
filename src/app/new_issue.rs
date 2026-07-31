//! Composing a brand-new issue: the project/issue-type/summary form
//! (`Screen::NewIssue`), and the demo/cache-only bookkeeping that lets a
//! locally-created issue survive being reopened and refreshed within the
//! same session. The description step and confirmation preview reuse the
//! existing compose → preview → apply machinery in `app::edit`
//! (`EditTarget::NewIssue`) — this module only owns the form itself and the
//! final "land the new issue" step.

use chrono::Utc;

use crate::domain::{IssueDetail, IssueSummary, IssueType, Priority, Source, Transition};

use super::{async_ops, App, EditorState, Screen};

/// Which field of the new-issue form currently has keyboard focus. Cycled
/// with Tab/Shift+Tab; Enter on `IssueType` opens the issue-type dropdown
/// (see `new_issue_type_picker_open`) instead of editing text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewIssueField {
    #[default]
    Project,
    IssueType,
    Summary,
}

/// State for the new-issue compose form (`Screen::NewIssue`). `project`/
/// `summary` are plain editable text fields — the same push/pop convention
/// as every other free-text field in this codebase (search query, assignee-
/// picker query, field-mapping query) — not a new input abstraction.
/// `available_types` is fetched per-project (`App::open_new_issue`, and
/// again whenever the project field changes), since a project's creatable
/// issue types — including any team-specific custom ones — aren't knowable
/// statically; see `jira::live::list_create_issue_types`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewIssueState {
    pub project: String,
    /// Which project `available_types` was actually fetched for — compared
    /// against `project` to know whether a refetch is needed, and to let a
    /// stale in-flight fetch's result be dropped if the project has changed
    /// again since it was dispatched.
    pub project_for_types: String,
    pub available_types: Vec<IssueType>,
    pub types_loading: bool,
    pub issue_type_index: usize,
    /// The highlighted row while `new_issue_type_picker_open` is true —
    /// separate from `issue_type_index` (the actually-confirmed selection)
    /// so Esc can back out of browsing without changing it, same as the
    /// transition/version/assignee pickers' own cursor-vs-applied split.
    pub type_picker_cursor: usize,
    pub summary: String,
    pub focus: NewIssueField,
}

/// A synthesized issue created while offline (demo/cache): kept around for
/// the rest of the session so it survives being reopened and surviving a
/// later `refresh()`, which would otherwise silently wipe it out — see
/// `App::load_detail` and `App::record_synced`. Never populated for a
/// genuine `Source::Live` session, where the real server copy (not this) is
/// the source of truth.
#[derive(Clone, Debug)]
pub(crate) struct LocallyCreatedIssue {
    pub summary: IssueSummary,
    pub detail: IssueDetail,
}

impl App {
    /// `a` on Home/List — open the new-issue compose form, prefilled with
    /// the configured default project (if any) and that project's fetched
    /// issue types (demo/cache sessions get the static `demo_issue_types()`
    /// stand-in instead, synchronously — there's no live catalog to fetch).
    pub fn open_new_issue(&mut self) {
        if self.edit_pending {
            self.status = "an update is still in progress".into();
            return;
        }
        let project = self.default_new_issue_project();
        self.new_issue = NewIssueState {
            project,
            ..NewIssueState::default()
        };
        self.project_picker_open = false;
        self.new_issue_type_picker_open = false;
        if matches!(self.source, Source::Live { .. }) {
            self.new_issue_types_generation += 1;
            let generation = self.new_issue_types_generation;
            self.new_issue.types_loading = true;
            let project = self.new_issue.project.clone();
            async_ops::dispatch_project_issue_types(self.events_tx.clone(), generation, project);
        } else {
            self.new_issue.available_types = crate::domain::demo_issue_types();
            self.new_issue.project_for_types = self.new_issue.project.clone();
        }
        self.screen = Screen::NewIssue;
    }

    /// Esc on the form itself — there's nothing worth preserving once the
    /// user has backed all the way out of the flow, unlike Esc from the
    /// description-compose step (`cancel_edit`), which returns here with
    /// the form's state left intact.
    pub fn cancel_new_issue(&mut self) {
        self.new_issue = NewIssueState::default();
        self.project_picker_open = false;
        self.new_issue_type_picker_open = false;
        self.screen = Screen::Home;
    }

    pub fn new_issue_input_char(&mut self, c: char) {
        match self.new_issue.focus {
            NewIssueField::Project => self.new_issue.project.push(c),
            NewIssueField::Summary => self.new_issue.summary.push(c),
            NewIssueField::IssueType => {}
        }
    }

    pub fn new_issue_backspace(&mut self) {
        match self.new_issue.focus {
            NewIssueField::Project => {
                self.new_issue.project.pop();
            }
            NewIssueField::Summary => {
                self.new_issue.summary.pop();
            }
            NewIssueField::IssueType => {}
        }
    }

    /// Tab — advance focus to the next field, wrapping around.
    pub fn new_issue_next_field(&mut self) {
        let next = match self.new_issue.focus {
            NewIssueField::Project => NewIssueField::IssueType,
            NewIssueField::IssueType => NewIssueField::Summary,
            NewIssueField::Summary => NewIssueField::Project,
        };
        self.new_issue_focus_field(next);
    }

    /// Shift+Tab — retreat focus to the previous field, wrapping around.
    pub fn new_issue_prev_field(&mut self) {
        let prev = match self.new_issue.focus {
            NewIssueField::Project => NewIssueField::Summary,
            NewIssueField::IssueType => NewIssueField::Project,
            NewIssueField::Summary => NewIssueField::IssueType,
        };
        self.new_issue_focus_field(prev);
    }

    /// Move keyboard focus to `field` — shared by `Tab`/`Shift+Tab`
    /// (`new_issue_next_field`/`new_issue_prev_field`) and by mouse
    /// click-to-focus (`App::mouse_down`), so a click leaving the Project
    /// field triggers the same issue-type refetch tabbing away does.
    pub fn new_issue_focus_field(&mut self, field: NewIssueField) {
        if self.new_issue.focus == field {
            return;
        }
        let leaving_project = self.new_issue.focus == NewIssueField::Project;
        self.new_issue.focus = field;
        if leaving_project {
            self.refresh_new_issue_types_if_project_changed();
        }
    }

    /// Keeps `available_types` in sync with the (trimmed) project field —
    /// called both when the user tabs off the Project field and, defensively,
    /// right before `confirm_new_issue_form` validates, so pressing Enter
    /// right after editing the project (without ever tabbing away) can't
    /// slip a stale-project catalog past validation. A no-op once already in
    /// sync — cheap to call unconditionally.
    /// `pub(crate)` (not private) so `project_picker.rs`'s
    /// `confirm_project_picker` can trigger the same resync a manual
    /// project-field edit would, without duplicating this logic.
    pub(crate) fn refresh_new_issue_types_if_project_changed(&mut self) {
        let project = self.new_issue.project.trim().to_string();
        if project == self.new_issue.project_for_types {
            return;
        }
        if !matches!(self.source, Source::Live { .. }) {
            // Demo/cache's catalog doesn't vary by project — just keep the
            // bookkeeping field in sync so this comparison doesn't keep
            // re-triggering (and so it doesn't spuriously look "stale" to
            // `confirm_new_issue_form`, which would otherwise block
            // submission in demo mode after any project-field edit).
            self.new_issue.available_types = crate::domain::demo_issue_types();
            self.new_issue.project_for_types = project;
            self.new_issue.issue_type_index = 0;
            return;
        }
        if project.is_empty() {
            return;
        }
        self.new_issue_types_generation += 1;
        let generation = self.new_issue_types_generation;
        self.new_issue.types_loading = true;
        async_ops::dispatch_project_issue_types(self.events_tx.clone(), generation, project);
    }

    /// Enter on the IssueType field — opens the dropdown popup listing
    /// every entry in `available_types` at once (replacing the old
    /// left/right scroller), pre-selecting whatever's currently confirmed.
    /// A no-op while types are still loading or the catalog is empty —
    /// nothing to open a picker over.
    pub fn open_new_issue_type_picker(&mut self) {
        if self.new_issue.types_loading || self.new_issue.available_types.is_empty() {
            return;
        }
        self.new_issue.type_picker_cursor = self.new_issue.issue_type_index;
        self.new_issue_type_picker_open = true;
    }

    /// Esc — close the popup without changing `issue_type_index`.
    pub fn close_new_issue_type_picker(&mut self) {
        self.new_issue_type_picker_open = false;
    }

    /// Up/Down (or j/k) while the popup is open — moves the highlighted
    /// row, clamped at either end (not wraparound), matching the
    /// transition picker's `picker_move`.
    pub fn new_issue_type_picker_move(&mut self, delta: isize) {
        let len = self.new_issue.available_types.len();
        if len == 0 {
            return;
        }
        let mut idx = self.new_issue.type_picker_cursor as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.new_issue.type_picker_cursor = idx as usize;
    }

    /// Enter while the popup is open — commits the highlighted row as the
    /// confirmed selection and closes the popup.
    pub fn confirm_new_issue_type_picker(&mut self) {
        self.new_issue.issue_type_index = self.new_issue.type_picker_cursor;
        self.new_issue_type_picker_open = false;
    }

    /// Enter on the form — validate and advance to the description-compose
    /// step. Rejects (with a status message, staying on the form) an empty
    /// project/summary, a still-loading issue-type fetch, or an empty
    /// issue-type catalog (nothing valid to create with).
    pub fn confirm_new_issue_form(&mut self) {
        if self.new_issue.project.trim().is_empty() {
            self.status = "enter a project key".into();
            return;
        }
        if self.new_issue.summary.trim().is_empty() {
            self.status = "enter a summary".into();
            return;
        }
        // Guards against submitting with a catalog fetched for a project
        // the user has since typed over without ever tabbing off the field
        // (the only other place this resync runs) — a no-op if already
        // in sync.
        self.refresh_new_issue_types_if_project_changed();
        if self.new_issue.types_loading {
            self.status = "still loading issue types…".into();
            return;
        }
        if self.new_issue.available_types.is_empty() {
            self.status = format!(
                "no issue types available for {}",
                self.new_issue.project.trim()
            );
            return;
        }
        self.editor = EditorState::from_text("");
        self.begin_new_issue_description_edit_target();
        self.screen = Screen::Edit;
    }

    fn default_new_issue_project(&self) -> String {
        #[cfg(feature = "live")]
        {
            if let Some(cfg) = crate::jira::Config::load() {
                if !cfg.project.is_empty() {
                    return cfg.project;
                }
            }
        }
        String::new()
    }

    /// Apply the previewed new issue (live if possible, always locally) —
    /// the `EditTarget::NewIssue` arm of `apply_edit`. Demo/cache sessions
    /// synthesize the issue inline and open it immediately; a live session
    /// dispatches off the render thread and lands once it resolves — see
    /// `async_ops::dispatch_create_issue`/`apply_issue_created`.
    pub(crate) fn apply_new_issue(&mut self) {
        let project = self.new_issue.project.trim().to_string();
        let issue_type = self
            .new_issue
            .available_types
            .get(self.new_issue.issue_type_index)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let summary = self.new_issue.summary.trim().to_string();
        let description = self.pending_edit.take();

        if !matches!(self.source, Source::Live { .. }) {
            let key = self.next_local_key(&project);
            self.land_new_issue(key.clone(), issue_type, summary, description);
            // Unlike the live branch below, this whole operation completes
            // atomically — there's no in-flight window where `back_out_of_preview`
            // needs `edit_target` to still read `NewIssue`, so resetting here
            // (rather than leaving it stale until the next `begin_*` call)
            // is safe. Deliberately NOT done at the top of this function: a
            // live dispatch stays in flight for a while, and clearing
            // `edit_target`/`edit_return_screen` before it resolves would
            // break that special-casing (see `back_out_of_preview`) for
            // exactly the ordinary Esc-then-resubmit navigation
            // `App::apply_edit`'s re-entrancy guard also has to account for.
            self.reset_edit_target();
            self.new_issue = NewIssueState::default();
            self.status = format!("created {key}");
            self.flash(format!("✓ created {key}"));
            self.trigger_jax_party();
            self.open_by_key(&key);
            return;
        }

        self.edit_generation += 1;
        let generation = self.edit_generation;
        self.edit_pending = true;
        self.loading = true;
        self.status = "↻ creating issue…".into();
        let local_key = self.next_local_key(&project);
        let tx = self.events_tx.clone();
        async_ops::dispatch_create_issue(
            tx,
            generation,
            project,
            issue_type,
            summary,
            description,
            local_key,
        );
    }

    /// The next locally-synthesized key for `project` (or a "DEMO" fallback
    /// if it's somehow empty by the time this is called — validation on the
    /// form should already have rejected that). Seeded well above any
    /// baked-in demo dataset key so a locally-created issue can never
    /// collide with one, regardless of what project the user typed.
    pub(crate) fn next_local_key(&mut self, project: &str) -> String {
        let id = self.locally_created_next_id;
        self.locally_created_next_id += 1;
        let prefix = if project.is_empty() { "DEMO" } else { project };
        format!("{prefix}-{id}")
    }

    /// A demo/cache issue's current fix versions, for computing a release
    /// bulk add/remove's baseline — checks `locally_created` first, falling
    /// back to `crate::domain::demo_detail(key)`. A locally-created key isn't
    /// in the baked-in demo dataset, so `demo_detail` alone would silently
    /// resolve to its generic "not found" placeholder (empty fix versions)
    /// instead of the issue's real (if minimal) starting state.
    pub(crate) fn demo_or_local_fix_versions(&self, key: &str) -> Vec<String> {
        if let Some(found) = self.locally_created.iter().find(|c| c.summary.key == key) {
            return found.detail.fix_versions.clone();
        }
        crate::domain::demo_detail(key).fix_versions
    }

    /// Construct and insert a brand-new issue's summary/detail — shared by
    /// the demo/cache-inline path (`apply_new_issue`) and the live-success
    /// path (`apply_issue_created`). Demo/cache issues are also remembered
    /// in `locally_created` so they survive being reopened and a later
    /// `refresh()` within the session; a genuine live create needs no such
    /// bookkeeping, since a real subsequent refresh's real JQL search will
    /// legitimately find the real server copy on its own.
    pub(crate) fn land_new_issue(
        &mut self,
        key: String,
        issue_type: String,
        summary: String,
        description: Option<serde_json::Value>,
    ) {
        let issue_summary = IssueSummary {
            key: key.clone(),
            summary: summary.clone(),
            issue_type: issue_type.clone(),
            status: "To Do".into(),
            priority: Priority::Medium,
            assignee: None,
            blocked: false,
            updated: "just now".into(),
            updated_at: Some(Utc::now()),
            epic: None,
        };
        let detail = IssueDetail {
            key,
            summary,
            issue_type,
            status: "To Do".into(),
            priority: Priority::Medium,
            assignee: None,
            reporter: Some(self.current_user_display()),
            labels: Vec::new(),
            components: Vec::new(),
            fix_versions: Vec::new(),
            affects_versions: Vec::new(),
            parent: None,
            links: Vec::new(),
            children: Vec::new(),
            description: description.unwrap_or_else(|| crate::adf::compile("")),
            acceptance_criteria: None,
            transitions: new_issue_transitions(),
            comments: Vec::new(),
        };

        self.all_issues.push(issue_summary.clone());
        self.recompute_view();

        if !matches!(self.source, Source::Live { .. }) {
            self.detail_cache
                .insert(issue_summary.key.clone(), detail.clone());
            self.locally_created.push(LocallyCreatedIssue {
                summary: issue_summary,
                detail,
            });
        }
    }
}

/// The same fixed workflow `demo_detail` gives every offline issue, so a
/// newly-created demo/cache issue is transition-able like any other.
fn new_issue_transitions() -> Vec<Transition> {
    ["To Do", "In Progress", "In Review", "Done"]
        .iter()
        .enumerate()
        .map(|(i, name)| Transition {
            id: (i + 1).to_string(),
            name: name.to_string(),
            to: name.to_string(),
        })
        .collect()
}
