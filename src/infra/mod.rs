//! Clipboard support via the OSC 52 terminal escape.
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
