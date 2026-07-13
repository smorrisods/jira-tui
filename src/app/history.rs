//! Browser-style back/forward navigation through issues followed via
//! in-body links (see `app::links`) while viewing the Detail screen.
//!
//! `detail_back`/`detail_forward` hold issue keys, not loaded details —
//! stepping through history re-runs the same load path as any other
//! navigation (`App::show_issue`), so live sessions still fetch through the
//! normal async path and demo/cache sessions resolve inline, same as
//! `open_by_key`. `open_by_key` itself pushes onto `detail_back` and clears
//! `detail_forward` when navigating away from an already-open Detail view;
//! opening fresh from the list or search clears both instead, so history
//! only ever spans a single continuous Detail-viewing session.

use super::App;

impl App {
    /// Whether `←` has an issue to step back to (used to let `←` fall
    /// through to its prior meaning — exiting Detail — when there's no
    /// history yet).
    pub fn can_go_back(&self) -> bool {
        !self.detail_back.is_empty()
    }

    /// Whether `→` has an issue to step forward to (used to let `→` fall
    /// through to its prior meaning — none, in Detail — when there's
    /// nothing to redo).
    pub fn can_go_forward(&self) -> bool {
        !self.detail_forward.is_empty()
    }

    /// `←` — step back to the issue viewed before the current one, pushing
    /// the current issue onto the forward stack so `→` can redo into it.
    pub fn go_back(&mut self) {
        let Some(target) = self.detail_back.pop() else {
            return;
        };
        if let Some(current) = self.detail.as_ref().map(|d| d.key.clone()) {
            self.detail_forward.push(current);
        }
        self.show_issue(&target);
    }

    /// `→` — redo into the issue that was current before the last `←`,
    /// pushing the current issue back onto the back stack.
    pub fn go_forward(&mut self) {
        let Some(target) = self.detail_forward.pop() else {
            return;
        };
        if let Some(current) = self.detail.as_ref().map(|d| d.key.clone()) {
            self.detail_back.push(current);
        }
        self.show_issue(&target);
    }
}
