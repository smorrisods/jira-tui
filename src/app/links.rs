//! Navigating issue-key/URL mentions inside the Detail screen and
//! quick-view panel: `Tab`/`Shift+Tab` cycle the highlighted link, `Enter`
//! opens it — another issue via `open_by_key`, or a URL in the system
//! browser via `infra::open_url`.
//!
//! The link list itself isn't cached: it's recomputed on demand from
//! whichever detail is currently shown (via `active_comment_detail` +
//! `render::wide_detail`/`narrow_detail`/`quick_view_wide`/`quick_view_narrow`),
//! the same "recompute, don't cache" approach `app::comments` already uses
//! for jumping to/stepping between comments — this always agrees with what
//! `ui::detail`/`ui::quick_view` actually rendered.

use ratatui::text::Line;

use crate::infra;
use crate::render::{self, DetailPane, LinkTarget};
use crate::ui::detail_columns::{detail_layout_for_width, DetailLayout};
use crate::ui::quick_view_columns::{quick_view_layout_for_width, QuickViewLayout};

use super::{App, Screen};

impl App {
    /// Every navigable link in whichever document is actually on screen:
    /// the Detail screen's wide layout (identity, main, then the side rail
    /// top-to-bottom — see `render::wide_detail_links`) or narrow layout
    /// (one document), picked via the last-rendered `detail_area`'s width
    /// (same idiom `app::mouse::link_at` and `app::comments` already use);
    /// the quick-view panel's wide (description then meta) or narrow (one
    /// document) layout everywhere else, picked via `quick_view_area`'s
    /// width the same way.
    pub(crate) fn active_links(&self) -> Vec<LinkTarget> {
        let Some(detail) = self.active_comment_detail() else {
            return Vec::new();
        };
        if self.screen != Screen::Detail {
            let updated = self.issue_updated(&detail.key).to_string();
            let width = self.quick_view_description_width();
            return match quick_view_layout_for_width(self.quick_view_area.get().width) {
                QuickViewLayout::Wide => {
                    render::quick_view_wide_links(&render::quick_view_wide(detail, &updated, width))
                }
                QuickViewLayout::Narrow => {
                    render::quick_view_narrow(detail, &updated, width)
                        .panel
                        .links
                }
            };
        }
        let current_user = self.current_user_display();
        let updated = self.issue_updated(&detail.key).to_string();
        let width = self.detail_main_width();
        // Same `MediaSizing` `ui::detail` actually painted with, so a link
        // target's recorded line always matches what's really on screen —
        // see `App::with_detail_media_sizing` and `app::comments`'s own
        // call site for why this has to agree.
        self.with_detail_media_sizing(width as u16, |media| {
            match detail_layout_for_width(self.detail_area.get().width) {
                DetailLayout::Wide => render::wide_detail_links(&render::wide_detail(
                    detail,
                    &current_user,
                    &updated,
                    width,
                    media,
                )),
                DetailLayout::Narrow => {
                    render::narrow_detail(
                        detail,
                        &current_user,
                        &updated,
                        self.facts_folded,
                        width,
                        media,
                    )
                    .lines
                    .links
                }
            }
        })
    }

    /// The rendered lines for a given `DetailPane`, recomputed fresh (see
    /// this module's doc comment) — used by `app::mouse::link_at` for
    /// wrap-aware click-to-line mapping, which needs the actual line
    /// content wrapping was computed against, not just the flattened target
    /// list `active_links` returns. `None` if `pane` isn't showing at all
    /// right now (e.g. `Workflow` while on the quick-view panel, or a rail
    /// pane while Detail is in its narrow, rail-less layout).
    pub(crate) fn active_pane_lines(&self, pane: DetailPane) -> Option<Vec<Line<'static>>> {
        let detail = self.active_comment_detail()?;
        if self.screen != Screen::Detail {
            // Quick view only ever has Main (description) and Meta panes.
            let updated = self.issue_updated(&detail.key).to_string();
            let width = self.quick_view_description_width();
            return match quick_view_layout_for_width(self.quick_view_area.get().width) {
                QuickViewLayout::Wide => {
                    let wide = render::quick_view_wide(detail, &updated, width);
                    match pane {
                        DetailPane::Main => Some(wide.description.lines),
                        DetailPane::Meta => Some(wide.meta.lines),
                        _ => None,
                    }
                }
                QuickViewLayout::Narrow => match pane {
                    DetailPane::Main => Some(
                        render::quick_view_narrow(detail, &updated, width)
                            .panel
                            .lines,
                    ),
                    _ => None,
                },
            };
        }
        let current_user = self.current_user_display();
        let updated = self.issue_updated(&detail.key).to_string();
        let width = self.detail_main_width();
        // As `active_links` above — the same `MediaSizing` `ui::detail`
        // actually painted with, so wrap-aware click-to-line mapping agrees
        // with what's really on screen.
        self.with_detail_media_sizing(width as u16, |media| {
            match detail_layout_for_width(self.detail_area.get().width) {
                DetailLayout::Narrow => match pane {
                    DetailPane::Main => Some(
                        render::narrow_detail(
                            detail,
                            &current_user,
                            &updated,
                            self.facts_folded,
                            width,
                            media,
                        )
                        .lines
                        .lines,
                    ),
                    _ => None,
                },
                DetailLayout::Wide => {
                    let wide = render::wide_detail(detail, &current_user, &updated, width, media);
                    Some(match pane {
                        DetailPane::Identity => wide.identity.lines,
                        DetailPane::Main => wide.main.lines,
                        DetailPane::Workflow => wide.workflow.lines,
                        DetailPane::Meta => wide.meta.lines,
                        DetailPane::Links => wide.links.lines,
                        DetailPane::Children => wide.children.lines,
                        DetailPane::Attachments => wide.attachments.lines,
                    })
                }
            }
        })
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
            render::LinkKind::Issue(key) => self.follow_link(&key),
            render::LinkKind::Url(url) => {
                if infra::open_url(&url).is_ok() {
                    self.flash(format!("↗ opened {url}"));
                } else {
                    self.status = format!("couldn't open {url}");
                }
            }
        }
    }

    /// Follow an in-body link from whichever issue's content is actually on
    /// screen (`active_comment_detail` — Detail's issue, or else the
    /// quick-viewed one), parenting `key` under it in the navigation
    /// history (`app::history`) so `←`/`,` returns to the exact issue the
    /// link was found on, regardless of whether it was read via the Detail
    /// screen or the quick-view panel — the two are equivalent here, since
    /// a history node is just "the issue," not "the issue as viewed
    /// through a particular screen."
    pub fn follow_link(&mut self, key: &str) {
        match self.active_comment_detail().map(|d| d.key.clone()) {
            Some(parent) => self.nav.visit_link(&parent, key),
            None => self.nav.visit_fresh(key),
        }
        self.show_issue(key);
    }

    /// Whether there's currently at least one navigable link (used to guard
    /// the `{`/`}`/`Enter` keybindings so `Enter` falls through to its
    /// existing meaning — e.g. opening the full issue detail — when there's
    /// nothing to navigate).
    pub fn has_links(&self) -> bool {
        !self.active_links().is_empty()
    }
}
