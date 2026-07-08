//! Build script — generates `jira-tui`'s man page from the same `Cli`
//! definition used for argument parsing, so the two can never drift apart.
//!
//! `src/cli.rs` is shared by both this build script (via `include!`, since a
//! build script is a separate compilation with no access to the crate's own
//! modules) and `main.rs` (via `mod cli`) — one source of truth for the CLI
//! surface.

use clap::CommandFactory;
use std::env;
use std::fs;
use std::path::PathBuf;

include!("src/cli.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/cli.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set by cargo"));
    let man_dir = out_dir.join("man");
    fs::create_dir_all(&man_dir).expect("failed to create man page output directory");

    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer = Vec::new();
    man.render(&mut buffer)
        .expect("failed to render jira-tui.1 man page");
    fs::write(man_dir.join("jira-tui.1"), buffer).expect("failed to write jira-tui.1");
}
