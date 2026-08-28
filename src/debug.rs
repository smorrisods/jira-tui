//! Lightweight opt-in debug tracing, for diagnosing live-only behaviour
//! that's otherwise silent by design (a failed preview fetch, an unmatched
//! media node, ...) — see `debug_trace!`. Grew out of a one-off
//! `JIRA_TUI_DEBUG_MEDIA` env-var check added to chase down issue #130's
//! DS-1880 follow-up; generalized into its own module once that turned out
//! to actually be useful, rather than staying scoped to inline images.
//!
//! Off by default. Turned on either "from the outside" — the `JIRA_TUI_DEBUG`
//! env var, checked once at startup via `init_from_env` — or "from the
//! inside" — the command palette's `PaletteAction::ToggleDebugLogging`,
//! live during a running session. Either way, the Nerd Info popup
//! (`ui::nerd_info`) reports the current state, so it's never a silent
//! surprise which one is active.
//!
//! Traced lines go to stderr, which the alt-screen TUI never writes to
//! itself, so they're safe to capture separately (`jira-tui 2> debug.log`)
//! without corrupting the terminal display.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Call once at startup, before anything that might want to trace —
/// `main()`, right alongside the other one-time env-driven setup.
pub fn init_from_env() {
    if std::env::var_os("JIRA_TUI_DEBUG").is_some() {
        ENABLED.store(true, Ordering::Relaxed);
    }
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Flip the flag and return its new state — the whole implementation of
/// `PaletteAction::ToggleDebugLogging`.
pub fn toggle() -> bool {
    let new = !is_enabled();
    ENABLED.store(new, Ordering::Relaxed);
    new
}

/// `eprintln!`, but only when tracing is on — every call site stays a
/// true no-op (not even the `format!` allocation runs) at the default off
/// setting, so leaving these calls in shipped code costs nothing. Callers
/// write their own subsystem tag into the message itself (e.g.
/// `debug_trace!("uuid-probe: ...")`) rather than this macro imposing one,
/// since a single flag may end up covering more than one subsystem over
/// time.
#[macro_export]
macro_rules! debug_trace {
    ($($arg:tt)*) => {
        if $crate::debug::is_enabled() {
            eprintln!("[jira-tui] {}", format!($($arg)*));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `toggle` must actually flip persisted state, not just report a
    /// one-off computed value — the palette action's only job is calling
    /// this, so a bug here would be invisible from that call site alone.
    #[test]
    fn toggle_flips_and_persists_across_calls() {
        let before = is_enabled();

        let after_first = toggle();
        assert_eq!(after_first, !before);
        assert_eq!(is_enabled(), after_first);

        let after_second = toggle();
        assert_eq!(after_second, before);
        assert_eq!(is_enabled(), before);
    }
}
