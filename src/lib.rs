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
pub mod domain;
pub mod git;
pub mod infra;
pub mod jira;
#[cfg(feature = "mcp")]
pub mod mcp;
/// Builds the flat line-list shared by the full Detail screen and the
/// quick-view panel. Lives outside both `app` and `ui` so `app` can compute
/// scroll offsets (e.g. jump-to-comments) without depending on the `ui`
/// crate module, while `ui` uses it to actually render.
pub mod render;
#[cfg(test)]
pub(crate) mod test_support;
pub mod ui;
