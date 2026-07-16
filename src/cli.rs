// Command-line argument definitions for the `jira-tui` binary.
//
// This is the single source of truth for the CLI surface: `main.rs` uses it
// to parse arguments (`Cli::parse()`), and `build.rs` `include!`s this same
// file to generate the man page via `clap_mangen` from the identical
// `Cli` type. Keeping one definition means `--help` and the man page can
// never drift out of sync with each other or with the real flag set.

use clap::Parser;

/// A developer-first, keyboard-driven Jira terminal UI.
#[derive(Parser, Debug)]
#[command(
    name = "jira-tui",
    version,
    about = "A developer-first, keyboard-driven Jira terminal UI.",
    after_help = "\
MOUSE:
    Press 'm' to toggle mouse mode (click to open, wheel to scroll, drag to
    copy via OSC 52, middle-click to toggle quick view, right-click to go
    back). Hold Shift while dragging to use your terminal's native selection
    instead.

EDITING:
    In an issue, press 't' to change status and 'e' to edit the description in
    $EDITOR (VISUAL/EDITOR, falling back to vi). Edits are recompiled to ADF and
    previewed before anything is sent to Jira.

LIVE MODE:
    Set JIRA_EMAIL and JIRA_API_TOKEN (or a token.txt file), and optionally
    JIRA_BASE_URL / JIRA_PROJECT, to load your real assigned work. Without
    them, jira-tui runs against built-in sample data (or the last cached list)."
)]
pub struct Cli {
    /// Force offline demo mode (ignore any credentials)
    #[arg(long)]
    pub demo: bool,

    /// Open straight to the animated about panel
    #[arg(long)]
    pub about: bool,

    /// Re-run the first-run welcome / live setup
    #[arg(long)]
    pub onboard: bool,

    /// Write a default config to ~/.config/jira-tui/config.toml, then exit
    #[arg(long)]
    pub init: bool,

    /// Disable local issue caching for this run (equivalent to setting
    /// JIRA_NO_CACHE, or cache_enabled = false in config.toml)
    #[arg(long)]
    pub no_cache: bool,
}
