//! Navigating issue-key/URL mentions inside the Detail screen and
//! quick-view panel: `Tab`/`Shift+Tab` cycle the highlighted link, `Enter`
//! opens it — another issue via `open_by_key`, or a URL in the system
//! browser via `infra::open_url`.
//!
//! The link list itself isn't cached: it's recomputed on demand from
//! whichever detail is currently shown (via `active_comment_detail` +
//! `render::issue_detail_lines`), the same "recompute, don't cache"
//! approach `app::comments` already uses for jumping to/stepping between
//! comments. `render::issue_detail_lines` is a pure function of the
//! `IssueDetail`, so this always agrees with what `ui::detail`/
//! `ui::list::draw_quick_view` actually rendered.

use crate::infra;
use crate::render::{self, LinkTarget};

use super::App;

impl App {
    pub(crate) fn active_links(&self) -> Vec<LinkTarget> {
        self.active_comment_detail()
            .map(|d| render::issue_detail_lines(d).links)
            .unwrap_or_default()
    }

    /// `}` — highlight the next link, wrapping around.
    pub fn next_link(&mut self) {
        let len = self.active_links().len();
        if len == 0 {
            return;
        }
        self.link_index = (self.link_index + 1) % len;
    }

    /// `{` — highlight the previous link, wrapping around.
    pub fn prev_link(&mut self) {
        let len = self.active_links().len();
        if len == 0 {
            return;
        }
        self.link_index = (self.link_index + len - 1) % len;
    }

    /// `Enter` — open the currently highlighted link: jump to the issue, or
    /// open the URL in the system's default browser.
    pub fn open_highlighted_link(&mut self) {
        let Some(target) = self.active_links().get(self.link_index).cloned() else {
            return;
        };
        match target.kind {
            render::LinkKind::Issue(key) => self.open_by_key(&key),
            render::LinkKind::Url(url) => {
                if infra::open_url(&url).is_ok() {
                    self.flash(format!("↗ opened {url}"));
                } else {
                    self.status = format!("couldn't open {url}");
                }
            }
        }
    }

    /// Whether there's currently at least one navigable link (used to guard
    /// the `{`/`}`/`Enter` keybindings so `Enter` falls through to its
    /// existing meaning — e.g. opening the full issue detail — when there's
    /// nothing to navigate).
    pub fn has_links(&self) -> bool {
        !self.active_links().is_empty()
    }
}
