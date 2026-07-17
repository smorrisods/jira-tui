//! The assignee picker: reassigning (or unassigning) the currently viewed
//! issue to a teammate, with type-to-filter and arrow-key navigation. Mirrors
//! `transitions.rs`'s open/move/confirm shape, with an added query string
//! (see `ui/search.rs` for the type-to-filter precedent).

use crate::domain::{AssignableUser, Source};

use super::{async_ops, App, Screen};

/// One row in the assignee picker: either the "Unassign" action or a
/// specific teammate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssigneeRow {
    Unassign,
    User(AssignableUser),
}

/// State for the open assignee picker. Recomputed (via
/// `App::recompute_assignee_rows`) on open and after every keystroke that
/// changes `query`, rather than filtered lazily at draw time.
#[derive(Clone, Debug, Default)]
pub struct AssigneePickerState {
    /// The issue being (re)assigned. Set when the picker opens; `None`
    /// otherwise (including in `App::default()`/`new()`).
    pub key: Option<String>,
    pub query: String,
    pub rows: Vec<AssigneeRow>,
    pub selected: usize,
}

impl App {
    /// Open the assignee picker for the currently viewed/quick-viewed issue.
    /// Unlike `open_transitions`, this doesn't require `list_focus` to be on
    /// the quick-view panel — like the comment key (`c`), opening a modal
    /// picker captures all subsequent input anyway, so there's no ambiguity
    /// to guard against.
    pub fn open_assignee_picker(&mut self) {
        if self.assignee_pending {
            self.status = "an assignment is already in progress".into();
            return;
        }
        let Some(key) = self.assignee_target_key() else {
            return;
        };
        self.assignee_picker.key = Some(key);
        self.assignee_picker.query.clear();
        self.recompute_assignee_rows();
        self.assignee_picker_open = true;
    }

    pub fn close_assignee_picker(&mut self) {
        self.assignee_picker_open = false;
    }

    /// The key of the issue the picker should act on: the open Detail issue,
    /// or the quick-view panel's issue if quick view is showing one. Screen-
    /// gated the same way `comment_target_key`/`refresh_detail` resolve
    /// their own targets — regression fix: this used to check `self.detail`
    /// unconditionally regardless of screen, so a stale `self.detail` left
    /// over from a previous Detail visit could silently outrank the issue
    /// actually showing in quick view.
    fn assignee_target_key(&self) -> Option<String> {
        match self.screen {
            Screen::Detail | Screen::Preview | Screen::Edit => {
                self.detail.as_ref().map(|d| d.key.clone())
            }
            _ => self.quick_view_detail().map(|d| d.key.clone()),
        }
    }

    /// Every assignable user for the current source: the live-fetched list
    /// cached at startup (`App::assignable_users`, populated by
    /// `dispatch_teammate_discovery`) for a live session, or the baked-in
    /// demo roster otherwise. Computed fresh each time rather than cached
    /// separately, since it's cheap and stateless.
    fn assignable_users_source(&self) -> Vec<AssignableUser> {
        if matches!(self.source, Source::Live { .. }) {
            self.assignable_users.clone()
        } else {
            crate::domain::demo_assignable_users()
        }
    }

    /// Rebuild `assignee_picker.rows` from the current query: "Unassign"
    /// always first, then "You" pinned next (matched against the assignable
    /// list by display name — see `App::me_display_name`), then everyone
    /// else alphabetically, all filtered by a case-insensitive substring
    /// match against `query`. Resets `selected` to 0 whenever the row list
    /// changes shape, mirroring `search.rs`'s recompute-on-keystroke shape.
    pub(crate) fn recompute_assignee_rows(&mut self) {
        let query = self.assignee_picker.query.to_lowercase();
        let me = self.me_display_name().to_string();
        let mut users = self.assignable_users_source();
        users.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        let (mine, others): (Vec<_>, Vec<_>) =
            users.into_iter().partition(|u| u.display_name == me);

        let mut rows = Vec::new();
        if "unassign".contains(&query) {
            rows.push(AssigneeRow::Unassign);
        }
        for user in mine.into_iter().chain(others) {
            if query.is_empty() || user.display_name.to_lowercase().contains(&query) {
                rows.push(AssigneeRow::User(user));
            }
        }

        self.assignee_picker.rows = rows;
        self.assignee_picker.selected = 0;
    }

    pub fn assignee_picker_input_char(&mut self, c: char) {
        self.assignee_picker.query.push(c);
        self.recompute_assignee_rows();
    }

    pub fn assignee_picker_backspace(&mut self) {
        self.assignee_picker.query.pop();
        self.recompute_assignee_rows();
    }

    pub fn assignee_picker_move(&mut self, delta: isize) {
        let len = self.assignee_picker.rows.len();
        if len == 0 {
            return;
        }
        let mut idx = self.assignee_picker.selected as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.assignee_picker.selected = idx as usize;
    }

    /// Apply the highlighted row (live if possible, always locally).
    ///
    /// Demo/cache sessions apply the local assignee update inline; a
    /// genuine live session dispatches the assignment off the render thread
    /// and applies the update once it resolves — see
    /// `async_ops::dispatch_assign`.
    pub fn confirm_assignee(&mut self) {
        let Some(key) = self.assignee_picker.key.clone() else {
            self.assignee_picker_open = false;
            return;
        };
        let Some(row) = self
            .assignee_picker
            .rows
            .get(self.assignee_picker.selected)
            .cloned()
        else {
            self.assignee_picker_open = false;
            return;
        };
        self.assignee_picker_open = false;

        let (account_id, display_name): (Option<String>, Option<String>) = match row {
            AssigneeRow::Unassign => (None, None),
            AssigneeRow::User(u) => (Some(u.account_id), Some(u.display_name)),
        };

        if !matches!(self.source, Source::Live { .. }) {
            self.apply_assignee_locally(&key, display_name.as_deref());
            self.status = match &display_name {
                Some(name) => format!("assigned {key} to {name}"),
                None => format!("unassigned {key}"),
            };
            self.flash(match &display_name {
                Some(name) => format!("✓ assigned to {name}"),
                None => "✓ unassigned".to_string(),
            });
            return;
        }

        self.assignee_generation += 1;
        let generation = self.assignee_generation;
        self.assignee_pending = true;
        self.loading = true;
        self.status = match &display_name {
            Some(name) => format!("↻ assigning {key} to {name}…"),
            None => format!("↻ unassigning {key}…"),
        };
        let tx = self.events_tx.clone();
        async_ops::dispatch_assign(tx, generation, key, account_id, display_name);
    }

    /// Update `display_name` (or clear it) for `key` everywhere it's cached:
    /// the open Detail, the quick-view detail cache, and the list summary —
    /// shared by both the demo/cache-synchronous path above and
    /// `AppEvent::AssigneeApplied`'s handler.
    pub(crate) fn apply_assignee_locally(&mut self, key: &str, display_name: Option<&str>) {
        if let Some(d) = self.detail.as_mut() {
            if d.key == key {
                d.assignee = display_name.map(str::to_string);
            }
        }
        if let Some(d) = self.detail_cache.get_mut(key) {
            d.assignee = display_name.map(str::to_string);
        }
        if let Some(sum) = self.issues.iter_mut().find(|i| i.key == key) {
            sum.assignee = display_name.map(str::to_string);
        }
        if let Some(sum) = self.all_issues.iter_mut().find(|i| i.key == key) {
            sum.assignee = display_name.map(str::to_string);
        }
    }
}
