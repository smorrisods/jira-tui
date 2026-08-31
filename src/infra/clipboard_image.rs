//! Best-effort read of an image from the OS clipboard, for the in-TUI
//! Markdown editor's `Ctrl+V` paste (see `src/keys/mod.rs`). This is the
//! capture mechanism only — writing the resulting file's path into the
//! editor buffer as an upload/embed is a deliberate, separate piece of work
//! built on top of this.
//!
//! Unlike `infra::osc52_copy` (a copy-*out* mechanism: it asks the terminal
//! emulator to set the system clipboard via an escape sequence, so it needs
//! no windowing-system dependency), reading an image back out of the
//! clipboard has no terminal-level equivalent — there's no escape sequence a
//! program can use to ask "what's on the clipboard". Every desktop
//! environment needs its own external tool shelled out to, so this file
//! tries each one this codebase knows about, in priority order for whichever
//! session it detects itself running in, and treats a missing tool the same
//! as an empty clipboard: a clear status message, never a crash.
//!
//! The runtime, tool-shelling parts of this (`is_installed`, `fetch_bytes`,
//! and friends) can't be exercised in CI — none of `wl-paste`/`xclip`/
//! `xsel`/`pngpaste`/`powershell.exe` are installed there, and there's no
//! real clipboard to read from. Everything that can reasonably be pure logic
//! *is* pure logic, tested in isolation below: which tool a session prefers
//! (`candidates_for`), which one actually gets picked given a fabricated
//! "what's installed" list (`select_tool`), parsing a tool's advertised MIME
//! types (`pick_image_mime`), sniffing image bytes by magic number
//! (`sniff_image_mime`), the mime → extension mapping (`extension_for_mime`,
//! the inverse of `crate::mime::guess_mime`'s extension → mime table), and
//! the WSL Windows-path → `/mnt/c/...` translation (`windows_path_to_wsl`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// The result of a single `Ctrl+V` paste attempt. Deliberately not a
/// `Result<Option<PathBuf>, _>`: every failure mode here is expected and
/// needs its own user-facing wording (which tool to install vs. "nothing to
/// paste"), so a caller matching this exhaustively can flash the right
/// message without needing its own error-to-string translation — and
/// without a `Result` an unhandled `Err` can never be the reason this
/// silently no-ops (see the module tests at the bottom).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardImageOutcome {
    /// An image was captured and written to this path under
    /// `std::env::temp_dir()`.
    Captured(PathBuf),
    /// No supported clipboard tool is installed for this session. The
    /// string names what to install.
    NoToolAvailable(String),
    /// A supported tool ran, but the clipboard doesn't currently hold an
    /// image.
    NoImage,
    /// A tool is installed and ran, but something else went wrong (a
    /// process failed to spawn mid-flow, a temp file couldn't be written,
    /// etc.) — still best-effort, still just a status message, never a
    /// panic.
    Failed(String),
}

/// Try, in priority order for whatever desktop session this looks like, to
/// read an image off the system clipboard and stash it in a stable temp
/// file. Entirely best-effort: an unsupported platform, a missing tool, or
/// an empty clipboard are all ordinary outcomes, not errors a caller has to
/// handle specially.
pub fn capture_clipboard_image() -> ClipboardImageOutcome {
    let session = detect_session();
    let candidates = candidates_for(session);
    let installed: Vec<ClipboardTool> = candidates
        .iter()
        .copied()
        .filter(|&t| is_installed(t))
        .collect();
    let Some(tool) = select_tool(session, &installed) else {
        return ClipboardImageOutcome::NoToolAvailable(no_tool_hint(session));
    };
    match fetch_bytes(tool) {
        ToolFetch::NoImage => ClipboardImageOutcome::NoImage,
        ToolFetch::Bytes(bytes, mime) => match write_temp_image(&bytes, &mime) {
            Ok(path) => ClipboardImageOutcome::Captured(path),
            Err(e) => ClipboardImageOutcome::Failed(e),
        },
        ToolFetch::Error(e) => ClipboardImageOutcome::Failed(e),
    }
}

// ---------------------------------------------------------------------
// Session / tool selection (pure — see the tests at the bottom)
// ---------------------------------------------------------------------

/// Which desktop clipboard environment this process appears to be running
/// in. Detected once per `capture_clipboard_image` call from `cfg!` and a
/// handful of environment/filesystem checks, then threaded through as a
/// plain value so the actual tool-priority logic (`candidates_for`,
/// `select_tool`) stays pure and testable without needing to fake any of
/// those checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Session {
    Wayland,
    X11,
    MacOs,
    Wsl,
    /// Headless, an unsupported OS, or a windowing system we don't have a
    /// tool for — `candidates_for` returns an empty list either way.
    Unknown,
}

fn detect_session() -> Session {
    if cfg!(target_os = "macos") {
        return Session::MacOs;
    }
    if is_wsl() {
        return Session::Wsl;
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return Session::Wayland;
    }
    if std::env::var_os("DISPLAY").is_some() {
        return Session::X11;
    }
    Session::Unknown
}

/// WSL2 has no `WAYLAND_DISPLAY`/`DISPLAY` of its own by default (there's no
/// Linux-side compositor), so it'd otherwise fall through to `Unknown` —
/// checked first via the interop-specific env vars WSL sets, falling back to
/// sniffing `/proc/version` for "microsoft" (present in both WSL1 and WSL2
/// kernel version strings) in case those env vars aren't inherited by
/// whatever spawned this process.
fn is_wsl() -> bool {
    if std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some() {
        return true;
    }
    std::fs::read_to_string("/proc/version")
        .map(|v| v.to_lowercase().contains("microsoft"))
        .unwrap_or(false)
}

/// Which external tool each family of clipboard tooling shells out to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipboardTool {
    WlPaste,
    Xclip,
    Xsel,
    PngPaste,
    PowerShell,
}

fn binary_name(tool: ClipboardTool) -> &'static str {
    match tool {
        ClipboardTool::WlPaste => "wl-paste",
        ClipboardTool::Xclip => "xclip",
        ClipboardTool::Xsel => "xsel",
        ClipboardTool::PngPaste => "pngpaste",
        ClipboardTool::PowerShell => "powershell.exe",
    }
}

/// The tools worth trying for a given session, in priority order — `xsel`
/// only as an X11 fallback behind `xclip`, per the brief: `xclip` can query
/// available MIME types up front (`-t TARGETS`), `xsel` can't, so it's only
/// reached when `xclip` itself isn't installed.
fn candidates_for(session: Session) -> &'static [ClipboardTool] {
    match session {
        Session::Wayland => &[ClipboardTool::WlPaste],
        Session::X11 => &[ClipboardTool::Xclip, ClipboardTool::Xsel],
        Session::MacOs => &[ClipboardTool::PngPaste],
        Session::Wsl => &[ClipboardTool::PowerShell],
        Session::Unknown => &[],
    }
}

/// Pick which tool to actually use: the first of the session's preferred
/// candidates (see `candidates_for`) that's present in `installed`. Split
/// out as its own pure function — taking the already-detected "what's
/// installed" list rather than probing itself — specifically so it's
/// testable with a fabricated `installed` list, without shelling out to
/// check for any real tool (see this module's tests).
fn select_tool(session: Session, installed: &[ClipboardTool]) -> Option<ClipboardTool> {
    candidates_for(session)
        .iter()
        .find(|t| installed.contains(t))
        .copied()
}

/// A short, specific "here's what to install" hint for `NoToolAvailable`,
/// per session.
fn no_tool_hint(session: Session) -> String {
    match session {
        Session::Wayland => {
            "no clipboard image tool found — install wl-clipboard (wl-paste)".into()
        }
        Session::X11 => "no clipboard image tool found — install xclip or xsel".into(),
        Session::MacOs => {
            "no clipboard image tool found — install pngpaste (e.g. brew install pngpaste)".into()
        }
        Session::Wsl => {
            "no clipboard image tool found — powershell.exe isn't reachable from WSL".into()
        }
        Session::Unknown => "clipboard image paste isn't supported in this environment".into(),
    }
}

/// Probe args safe to run without touching the clipboard, for detecting
/// whether a tool is actually installed. `pngpaste` gets an empty arg list
/// deliberately: it takes a single `<destination|->` argument, so any
/// unrecognized flag risks being read as a literal output filename (and
/// therefore actually reading and writing the clipboard as a side effect of
/// a mere presence check) — called with zero arguments it just prints usage
/// and exits without touching anything.
fn probe_args(tool: ClipboardTool) -> &'static [&'static str] {
    match tool {
        ClipboardTool::WlPaste => &["--version"],
        ClipboardTool::Xclip => &["-version"],
        ClipboardTool::Xsel => &["--version"],
        ClipboardTool::PngPaste => &[],
        ClipboardTool::PowerShell => &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$PSVersionTable.PSVersion",
        ],
    }
}

/// Whether `tool`'s binary is actually runnable — spawning it (with the
/// side-effect-free probe args above) and getting any exit status back at
/// all, regardless of that status, is enough: only a spawn failure (not
/// found, not executable, ...) means it's genuinely unavailable.
fn is_installed(tool: ClipboardTool) -> bool {
    std::process::Command::new(binary_name(tool))
        .args(probe_args(tool))
        .output()
        .is_ok()
}

// ---------------------------------------------------------------------
// Byte capture (shells out — not unit tested directly, see module docs)
// ---------------------------------------------------------------------

enum ToolFetch {
    NoImage,
    Bytes(Vec<u8>, String),
    Error(String),
}

/// Run `bin args...` and capture stdout, treating "the process spawned at
/// all" as success — best-effort clipboard tools are allowed to exit
/// nonzero on an empty clipboard, so the exit status alone isn't a reliable
/// "did this work" signal; callers instead judge success by whether stdout
/// actually came back non-empty.
fn run(bin: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    std::process::Command::new(bin)
        .args(args)
        .output()
        .map(|out| out.stdout)
        .map_err(|e| format!("{bin}: {e}"))
}

fn fetch_bytes(tool: ClipboardTool) -> ToolFetch {
    match tool {
        ClipboardTool::WlPaste => fetch_via_list_and_type("wl-paste", &["--list-types"], |mime| {
            vec!["-t".to_string(), mime.to_string()]
        }),
        ClipboardTool::Xclip => fetch_via_list_and_type(
            "xclip",
            &["-selection", "clipboard", "-t", "TARGETS", "-o"],
            |mime| {
                vec![
                    "-selection".into(),
                    "clipboard".into(),
                    "-t".into(),
                    mime.to_string(),
                    "-o".into(),
                ]
            },
        ),
        ClipboardTool::Xsel => fetch_via_sniff("xsel", &["--clipboard", "--output"]),
        ClipboardTool::PngPaste => match run("pngpaste", &["-"]) {
            Ok(bytes) if bytes.is_empty() => ToolFetch::NoImage,
            Ok(bytes) => ToolFetch::Bytes(bytes, "image/png".to_string()),
            Err(e) => ToolFetch::Error(e),
        },
        ClipboardTool::PowerShell => fetch_via_powershell(),
    }
}

/// Shared shape for `wl-paste`/`xclip`: list the clipboard's advertised MIME
/// types, pick an image one (`pick_image_mime`), then re-invoke the tool
/// asking for that specific type's bytes.
fn fetch_via_list_and_type(
    bin: &str,
    list_args: &[&str],
    type_args: impl Fn(&str) -> Vec<String>,
) -> ToolFetch {
    let list = match run(bin, list_args) {
        Ok(out) => out,
        Err(e) => return ToolFetch::Error(e),
    };
    let Some(mime) = pick_image_mime(&String::from_utf8_lossy(&list)) else {
        return ToolFetch::NoImage;
    };
    let args = type_args(&mime);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(bin, &arg_refs) {
        Ok(bytes) if bytes.is_empty() => ToolFetch::NoImage,
        Ok(bytes) => ToolFetch::Bytes(bytes, mime),
        Err(e) => ToolFetch::Error(e),
    }
}

/// `xsel` has no reliable way to ask what's on the clipboard before reading
/// it, so this reads the raw bytes unconditionally and sniffs the result by
/// magic number (`sniff_image_mime`) — anything that doesn't look like a
/// known image format is treated the same as an empty clipboard.
fn fetch_via_sniff(bin: &str, args: &[&str]) -> ToolFetch {
    match run(bin, args) {
        Ok(bytes) if bytes.is_empty() => ToolFetch::NoImage,
        Ok(bytes) => match sniff_image_mime(&bytes) {
            Some(mime) => ToolFetch::Bytes(bytes, mime.to_string()),
            None => ToolFetch::NoImage,
        },
        Err(e) => ToolFetch::Error(e),
    }
}

/// WSL2 has no Linux-side clipboard of its own — `Get-Clipboard` via
/// `powershell.exe` (present through Windows interop) is the only reliable
/// read. The script saves the image to a Windows-side temp path (there's no
/// way to stream bytes straight back over stdout as anything but text), then
/// this reads it back in over the translated `/mnt/c/...` path
/// (`windows_path_to_wsl`) and cleans up the Windows-side copy — the file
/// this function ultimately reports lives under `std::env::temp_dir()` like
/// every other tool's, written by `write_temp_image` in the caller.
fn fetch_via_powershell() -> ToolFetch {
    let win_name = temp_filename("image/png");
    let script = format!(
        "$img = Get-Clipboard -Format Image; if ($img) {{ $p = Join-Path $env:TEMP '{win_name}'; $img.Save($p, [System.Drawing.Imaging.ImageFormat]::Png); Write-Output $p }} else {{ exit 1 }}"
    );
    let out = match run(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", &script],
    ) {
        Ok(out) => out,
        Err(e) => return ToolFetch::Error(e),
    };
    let win_path = String::from_utf8_lossy(&out).trim().to_string();
    if win_path.is_empty() {
        return ToolFetch::NoImage;
    }
    let Some(wsl_path) = windows_path_to_wsl(&win_path) else {
        return ToolFetch::Error(format!("couldn't translate windows path: {win_path}"));
    };
    match std::fs::read(&wsl_path) {
        Ok(bytes) => {
            // Best-effort cleanup of the Windows-side intermediate file —
            // the caller writes its own copy under `std::env::temp_dir()`
            // regardless, so this one's no longer needed either way.
            let _ = std::fs::remove_file(&wsl_path);
            ToolFetch::Bytes(bytes, "image/png".to_string())
        }
        Err(e) => ToolFetch::Error(format!("couldn't read {wsl_path}: {e}")),
    }
}

// ---------------------------------------------------------------------
// Pure parsing/translation helpers (see the tests below)
// ---------------------------------------------------------------------

/// Pick an image MIME type out of a newline-separated list of types a tool
/// advertised (`wl-paste --list-types`'s own output, or `xclip`'s `TARGETS`
/// query, which mixes in non-MIME pseudo-targets like `TARGETS` or
/// `UTF8_STRING` alongside real ones) — `image/png` if it's offered
/// (screenshots overwhelmingly are), otherwise the first `image/*` type
/// seen, otherwise `None`.
fn pick_image_mime(list_output: &str) -> Option<String> {
    let mut first_image: Option<String> = None;
    for line in list_output.lines() {
        let line = line.trim();
        if line.eq_ignore_ascii_case("image/png") {
            return Some("image/png".to_string());
        }
        if first_image.is_none() && line.to_ascii_lowercase().starts_with("image/") {
            first_image = Some(line.to_string());
        }
    }
    first_image
}

/// Identify an image format from its leading bytes ("magic numbers"), for
/// tools (`xsel`) that can't be asked what type the clipboard holds up
/// front.
fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(&[0x42, 0x4D]) {
        Some("image/bmp")
    } else {
        None
    }
}

/// The mime → extension inverse of `crate::mime::guess_mime`'s extension →
/// mime table (restricted to the image types this module actually detects),
/// used to name the temp file `write_temp_image` produces. Falls back to
/// `png` for anything unrecognized — the overwhelming common case
/// (screenshots) is PNG regardless.
fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "png",
    }
}

/// Translate a Windows-style absolute path (as `powershell.exe`'s stdout
/// reports it, e.g. `C:\Users\me\AppData\Local\Temp\foo.png`) to the WSL
/// path it's reachable at under the standard `/mnt/<drive>/...` interop
/// mount. Returns `None` for anything that doesn't look like a drive-letter
/// absolute path.
fn windows_path_to_wsl(path: &str) -> Option<String> {
    let path = path.trim();
    let mut chars = path.chars();
    let drive = chars.next()?;
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    if chars.next()? != ':' {
        return None;
    }
    let rest = path.get(2..)?;
    let rest = rest.strip_prefix('\\').or_else(|| rest.strip_prefix('/'))?;
    let rest = rest.replace('\\', "/");
    Some(format!("/mnt/{}/{}", drive.to_ascii_lowercase(), rest))
}

/// A filename that won't collide with a concurrent capture (or a previous
/// one from the same process): `jira-tui-clip-<pid>-<nanos>-<counter>.<ext>`.
/// No new dependency for this — `std::process::id()` plus a monotonic
/// in-process counter plus a wall-clock timestamp is already enough entropy
/// for a single-user temp file naming scheme, and this codebase has no UUID
/// crate in its dependency tree to reach for instead.
fn temp_filename(mime: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let ext = extension_for_mime(mime);
    format!("jira-tui-clip-{pid}-{nanos}-{n}.{ext}")
}

fn write_temp_image(bytes: &[u8], mime: &str) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(temp_filename(mime));
    std::fs::write(&path, bytes).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_prefer_wl_paste_on_wayland_only() {
        assert_eq!(candidates_for(Session::Wayland), &[ClipboardTool::WlPaste]);
    }

    #[test]
    fn candidates_prefer_xclip_over_xsel_on_x11() {
        assert_eq!(
            candidates_for(Session::X11),
            &[ClipboardTool::Xclip, ClipboardTool::Xsel]
        );
    }

    #[test]
    fn candidates_are_empty_for_unknown_sessions() {
        assert_eq!(candidates_for(Session::Unknown), &[]);
    }

    #[test]
    fn select_tool_picks_the_first_installed_candidate_in_priority_order() {
        // xclip preferred over xsel when both are "installed".
        assert_eq!(
            select_tool(Session::X11, &[ClipboardTool::Xsel, ClipboardTool::Xclip]),
            Some(ClipboardTool::Xclip)
        );
        // Falls back to xsel when xclip isn't in the installed list.
        assert_eq!(
            select_tool(Session::X11, &[ClipboardTool::Xsel]),
            Some(ClipboardTool::Xsel)
        );
    }

    #[test]
    fn select_tool_is_none_when_nothing_on_the_candidate_list_is_installed() {
        assert_eq!(select_tool(Session::Wayland, &[ClipboardTool::Xclip]), None);
        assert_eq!(select_tool(Session::Wayland, &[]), None);
    }

    #[test]
    fn select_tool_is_none_for_an_unsupported_session_even_with_tools_installed() {
        // A stray xclip on e.g. macOS shouldn't get picked — there's no
        // X11 session to have offered it as a candidate in the first place.
        assert_eq!(select_tool(Session::MacOs, &[ClipboardTool::Xclip]), None);
    }

    #[test]
    fn pick_image_mime_prefers_png_even_if_listed_after_other_image_types() {
        let list = "text/plain\nimage/jpeg\nimage/png\nimage/tiff\n";
        assert_eq!(pick_image_mime(list).as_deref(), Some("image/png"));
    }

    #[test]
    fn pick_image_mime_falls_back_to_the_first_image_type_without_png() {
        let list = "text/plain\nimage/jpeg\nimage/tiff\n";
        assert_eq!(pick_image_mime(list).as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn pick_image_mime_none_when_the_clipboard_holds_no_image_type() {
        let list = "text/plain\nUTF8_STRING\nTARGETS\n";
        assert_eq!(pick_image_mime(list), None);
    }

    #[test]
    fn pick_image_mime_ignores_targets_style_pseudo_entries() {
        // xclip's `TARGETS` query mixes in non-MIME entries alongside real
        // ones — only lines that actually look like `image/...` count.
        let list = "TARGETS\nSTRING\nUTF8_STRING\nimage/png\n";
        assert_eq!(pick_image_mime(list).as_deref(), Some("image/png"));
    }

    #[test]
    fn sniff_image_mime_identifies_known_formats_by_magic_number() {
        assert_eq!(
            sniff_image_mime(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0]),
            Some("image/png")
        );
        assert_eq!(
            sniff_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0]),
            Some("image/jpeg")
        );
        assert_eq!(sniff_image_mime(b"GIF89a...."), Some("image/gif"));
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0, 0, 0, 0]);
        webp.extend_from_slice(b"WEBP");
        assert_eq!(sniff_image_mime(&webp), Some("image/webp"));
        assert_eq!(sniff_image_mime(&[0x42, 0x4D, 0, 0]), Some("image/bmp"));
    }

    #[test]
    fn sniff_image_mime_none_for_non_image_bytes() {
        assert_eq!(sniff_image_mime(b"hello, clipboard"), None);
        assert_eq!(sniff_image_mime(b""), None);
    }

    #[test]
    fn extension_for_mime_covers_the_detected_formats_and_falls_back_to_png() {
        assert_eq!(extension_for_mime("image/png"), "png");
        assert_eq!(extension_for_mime("image/jpeg"), "jpg");
        assert_eq!(extension_for_mime("image/gif"), "gif");
        assert_eq!(extension_for_mime("image/webp"), "webp");
        assert_eq!(extension_for_mime("image/bmp"), "bmp");
        assert_eq!(extension_for_mime("image/tiff"), "png");
    }

    #[test]
    fn windows_path_to_wsl_translates_a_typical_temp_path() {
        assert_eq!(
            windows_path_to_wsl(r"C:\Users\me\AppData\Local\Temp\jira-tui-clip-1.png"),
            Some("/mnt/c/Users/me/AppData/Local/Temp/jira-tui-clip-1.png".to_string())
        );
    }

    #[test]
    fn windows_path_to_wsl_lowercases_the_drive_letter() {
        assert_eq!(
            windows_path_to_wsl(r"D:\clip.png"),
            Some("/mnt/d/clip.png".to_string())
        );
    }

    #[test]
    fn windows_path_to_wsl_trims_surrounding_whitespace_from_powershell_output() {
        assert_eq!(
            windows_path_to_wsl("  C:\\clip.png\r\n"),
            Some("/mnt/c/clip.png".to_string())
        );
    }

    #[test]
    fn windows_path_to_wsl_none_for_non_windows_paths() {
        assert_eq!(windows_path_to_wsl("/mnt/c/already/wsl.png"), None);
        assert_eq!(windows_path_to_wsl("relative\\path.png"), None);
        assert_eq!(windows_path_to_wsl(""), None);
    }

    #[test]
    fn temp_filename_is_unique_across_back_to_back_calls() {
        let a = temp_filename("image/png");
        let b = temp_filename("image/png");
        assert_ne!(
            a, b,
            "the monotonic counter must keep back-to-back names apart"
        );
        assert!(a.ends_with(".png"));
        assert!(a.starts_with("jira-tui-clip-"));
    }

    #[test]
    fn temp_filename_uses_the_mime_specific_extension() {
        assert!(temp_filename("image/jpeg").ends_with(".jpg"));
    }

    #[test]
    fn write_temp_image_round_trips_bytes_to_a_file_under_temp_dir() {
        let bytes = b"not really a png, just test bytes";
        let path = write_temp_image(bytes, "image/png").expect("write should succeed");
        assert!(path.starts_with(std::env::temp_dir()));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let _ = std::fs::remove_file(&path);
    }

    // No real clipboard tool exists in CI (or most dev sandboxes), so
    // `capture_clipboard_image` itself resolves to `NoToolAvailable` almost
    // everywhere this runs — which is exactly the "no tool found" outcome
    // the brief requires to resolve cleanly rather than panic or propagate
    // an unhandled error. `Unknown`-session environments (headless, no
    // `DISPLAY`/`WAYLAND_DISPLAY`, not WSL/macOS) hit the same path via an
    // empty candidate list.
    #[test]
    fn capture_clipboard_image_never_panics_and_always_resolves() {
        match capture_clipboard_image() {
            ClipboardImageOutcome::NoToolAvailable(hint) => assert!(!hint.is_empty()),
            ClipboardImageOutcome::NoImage
            | ClipboardImageOutcome::Captured(_)
            | ClipboardImageOutcome::Failed(_) => {
                // Any of these is also a legitimate, non-panicking outcome
                // on a dev machine that does have a clipboard tool
                // installed — this test only asserts "didn't crash".
            }
        }
    }
}
