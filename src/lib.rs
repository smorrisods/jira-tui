//! jira-tui library surface.
//!
//! The binary (`main.rs`) is a thin shell over these modules; exposing them as
//! a library lets the integration test suite drive the real rendering and state
//! logic headlessly.

pub mod adf;
pub mod app;
#[cfg(feature = "live")]
pub mod cache;
pub mod config;
/// Opt-in `eprintln!`-based debug tracing (`debug_trace!`), toggleable via
/// the `JIRA_TUI_DEBUG` env var or the command palette — see the module
/// doc comment.
pub mod debug;
pub mod domain;
pub mod git;
pub mod infra;
pub mod jira;
#[cfg(feature = "mcp")]
pub mod mcp;
/// Filename → MIME-type guessing (`guess_mime`), split out of `jira::live`
/// so it's available in every feature set — see the module doc comment.
pub mod mime;
/// Builds the flat line-list shared by the full Detail screen and the
/// quick-view panel. Lives outside both `app` and `ui` so `app` can compute
/// scroll offsets (e.g. jump-to-comments) without depending on the `ui`
/// crate module, while `ui` uses it to actually render.
pub mod render;
/// In-app misspelling detection for the built-in editor — see
/// `assets/dictionaries/en` for the bundled dictionary's provenance.
pub mod spellcheck;
#[cfg(test)]
pub(crate) mod test_support;
pub mod ui;
