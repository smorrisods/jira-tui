//! Process-level tests for the CLI surface.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jira-tui"))
}

#[test]
fn version_flag_prints_version() {
    let out = bin().arg("--version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("jira-tui v"));
}

#[test]
fn help_flag_documents_key_features() {
    let out = bin().arg("--help").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("USAGE"));
    assert!(stdout.contains("--demo"));
    assert!(stdout.contains("--onboard"));
    assert!(stdout.contains("MOUSE"));
}

#[test]
fn init_writes_config_into_xdg_dir() {
    let tmp = std::env::temp_dir().join(format!("jira-tui-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);

    let out = bin()
        .arg("--init")
        .env("XDG_CONFIG_HOME", tmp.join("config"))
        .env("XDG_CACHE_HOME", tmp.join("cache"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Wrote default config"));

    let cfg = tmp.join("config/jira-tui/config.toml");
    assert!(
        cfg.exists(),
        "config file should be created at the XDG path"
    );
    let body = std::fs::read_to_string(&cfg).unwrap();
    assert!(body.contains("mouse ="));

    // A second run must not overwrite it.
    let out2 = bin()
        .arg("--init")
        .env("XDG_CONFIG_HOME", tmp.join("config"))
        .env("XDG_CACHE_HOME", tmp.join("cache"))
        .output()
        .unwrap();
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    assert!(stdout2.contains("already exists"));

    let _ = std::fs::remove_dir_all(&tmp);
}
