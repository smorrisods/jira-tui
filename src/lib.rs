//! jira-tui library surface.
//!
//! The binary (`main.rs`) is a thin shell over these modules; exposing them as
//! a library lets the integration test suite drive the real rendering and state
//! logic headlessly.

pub mod adf;
pub mod app;
pub mod config;
pub mod domain;
pub mod git;
pub mod infra;
pub mod jira;
pub mod ui;
