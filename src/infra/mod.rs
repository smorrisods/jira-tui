//! Small terminal/OS integrations: clipboard support via the OSC 52
//! escape, and opening URLs in the system's default browser.
//!
//! OSC 52 asks the terminal emulator to set the system clipboard, so it needs
//! no X11/Wayland dependency and works over SSH (in terminals that allow it).

use std::io::Write;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

/// Copy `text` to the system clipboard using OSC 52. Best-effort: if the
/// terminal ignores the sequence, nothing happens (and nothing breaks).
pub fn osc52_copy(text: &str) -> std::io::Result<()> {
    let encoded = STANDARD.encode(text.as_bytes());
    let seq = format!("\x1b]52;c;{encoded}\x07");
    let mut out = std::io::stdout();
    out.write_all(seq.as_bytes())?;
    out.flush()
}

/// Open `url` in the system's default browser by shelling out to the
/// platform opener (`xdg-open` on Linux, `open` on macOS, `cmd /c start` on
/// Windows) — no extra crate dependency, mirroring how `git::GitContext`
/// shells out to `git` rather than linking a Git library. Best-effort: a
/// missing opener or a headless environment just means nothing visibly
/// happens.
pub fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}
