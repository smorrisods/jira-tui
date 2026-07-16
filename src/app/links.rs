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
use crate::ui::detail_columns::{detail_layout_for_width, DetailLayout};

use super::{App, Screen};

impl App {
    /// Every navigable link in whichever document is actually on screen:
    /// the Detail screen's wide layout (identity, main, then the side rail
    /// top-to-bottom — see `render::wide_detail_links`) or narrow layout
    /// (one document), picked via the last-rendered `detail_area`'s width
    /// (same idiom `app::mouse::link_at` and `app::comments` already use);
    /// the quick-view panel's unchanged flat document everywhere else.
    pub(crate) fn active_links(&self) -> Vec<LinkTarget> {
        let Some(detail) = self.active_comment_detail() else {
            return Vec::new();
        };
        if self.screen != Screen::Detail {
            return render::issue_detail_lines(detail).links;
        }
        let current_user = self.current_user_display();
        let updated = self.issue_updated(&detail.key).to_string();
        match detail_layout_for_width(self.detail_area.get().width) {
            DetailLayout::Wide => {
                render::wide_detail_links(&render::wide_detail(detail, &current_user, &updated))
            }
            DetailLayout::Narrow => {
                render::narrow_detail(detail, &current_user, &updated, self.facts_folded)
                    .lines
                    .links
            }
        }
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
