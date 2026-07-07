//! The workflow transition picker: listing and applying status transitions.

#[cfg(feature = "live")]
use crate::domain::Source;

use super::App;

impl App {
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
}
