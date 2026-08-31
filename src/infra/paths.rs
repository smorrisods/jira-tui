//! Normalize a pasted file path into something `std::fs` can open directly.
//!
//! No terminal emulator on any platform this app runs on (WSL2, native
//! Linux, macOS) turns a drag-and-drop onto the terminal window into a real
//! OS-level drop event — instead the terminal pastes the dropped file's
//! path as text, in a form that varies by platform and even by terminal:
//! Windows drive-letter paths (`C:\Users\...`) or WSL UNC paths
//! (`\\wsl.localhost\<distro>\...`, `\\wsl$\<distro>\...`) under WSL2/
//! Windows Terminal; backslash-escaped or quote-wrapped paths under macOS
//! Terminal.app/iTerm2 and most Linux terminal emulators; `file://` URIs
//! from GTK file managers like Nautilus on Linux. `normalize_dropped_path`
//! recognizes each of those shapes and converts it to a plain, readable
//! local path — a safe no-op on anything that doesn't match one of them,
//! since a WSL2 binary may still receive an already-plain path (e.g. from a
//! remote pairing session), and deliberately *not* gated on `cfg(target_os)`
//! or environment detection, so the exact same logic runs everywhere.

/// Normalize a pasted/dropped file path into a plain local path.
///
/// Applies, in order: multi-line-input truncation (first non-empty line
/// only — see the module doc comment for why more than one line can show
/// up), then surrounding-quote stripping. From there, the Windows
/// drive-letter and WSL UNC forms are checked *before* any backslash
/// unescaping — their backslashes are path separators, not escape
/// characters, so unescaping first would eat exactly the structure those
/// two checks look for (`C:\Users\...` -> `C:Users...`). Only once neither
/// of those matches does backslash-unescaping run, for the macOS/Linux
/// terminal convention where a literal `\<char>` means "this char is part
/// of the path, not a shell metacharacter" (e.g. `My\ Pictures`). A
/// `file://` URI is checked last, on the unescaped result. Anything else —
/// including an already-plain Linux/macOS path — passes through unchanged.
pub fn normalize_dropped_path(input: &str) -> String {
    let first_line = input
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let trimmed = first_line.trim();
    let unquoted = strip_wrapping_quotes(trimmed);

    if let Some(path) = windows_drive_path(unquoted) {
        return path;
    }
    if let Some(path) = wsl_unc_path(unquoted) {
        return path;
    }

    let unescaped = unescape_backslashes(unquoted);
    if let Some(path) = file_uri_path(&unescaped) {
        return path;
    }
    unescaped
}

/// Whether `text` (as originally pasted, before any normalization) looks
/// like more than one file was dropped at once — i.e. has more than one
/// non-empty line. `handle_paste` uses this to decide whether to flash a
/// "using the first" note.
pub fn has_multiple_paths(input: &str) -> bool {
    input.lines().filter(|line| !line.trim().is_empty()).count() > 1
}

/// Strip one layer of wrapping `"..."` or `'...'` quotes, if present.
fn strip_wrapping_quotes(s: &str) -> &str {
    for quote in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(quote) && s.ends_with(quote) {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// Un-escape `\<char>` sequences (e.g. `My\ Pictures` -> `My Pictures`) —
/// how macOS Terminal.app/iTerm2 and most Linux terminal emulators
/// represent a dropped path containing spaces or other shell-special
/// characters. A trailing lone backslash (nothing left to escape) is kept
/// as-is rather than silently dropped.
fn unescape_backslashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                out.push(next);
                chars.next();
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// `C:\...` (any drive letter, case-insensitive) -> `/mnt/c/...` — the
/// WSL2 mount convention, with backslashes converted to forward slashes.
fn windows_drive_path(s: &str) -> Option<String> {
    let mut chars = s.chars();
    let drive = chars.next()?;
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    if chars.next() != Some(':') {
        return None;
    }
    if chars.next() != Some('\\') {
        return None;
    }
    let rest = &s[3..];
    let rest = rest.replace('\\', "/");
    Some(format!("/mnt/{}/{rest}", drive.to_ascii_lowercase()))
}

/// `\\wsl.localhost\<distro>\...` or `\\wsl$\<distro>\...` -> the in-distro
/// absolute path, stripping the leading `\\wsl...\<distro>` segment(s) and
/// converting backslashes to forward slashes.
fn wsl_unc_path(s: &str) -> Option<String> {
    for prefix in ["\\\\wsl.localhost\\", "\\\\wsl$\\"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            // `rest` is `<distro>\<path...>`; drop the distro segment.
            let after_distro = match rest.find('\\') {
                Some(idx) => &rest[idx + 1..],
                None => "",
            };
            let path = after_distro.replace('\\', "/");
            return Some(format!("/{path}"));
        }
    }
    None
}

/// `file://...` URI -> a plain absolute path, percent-decoded. Handles both
/// `file:///path` (empty/local authority) and `file://host/path` (rare,
/// but treated the same — the host segment is dropped).
fn file_uri_path(s: &str) -> Option<String> {
    let rest = s.strip_prefix("file://")?;
    // Skip an optional authority (host) component: `file://host/path` vs.
    // `file:///path` (authority-less, the overwhelmingly common case).
    // Either way what we want starts at the next `/`.
    let path_start = rest.find('/').unwrap_or(0);
    let path = &rest[path_start..];
    Some(percent_decode(path))
}

/// Minimal `%XX` percent-decoder — just enough for `file://` URIs from
/// GTK/Nautilus-style drops. No existing dependency covers this (the `url`
/// crate is available but only under the `live` feature, and this needs to
/// work in every build), so this is a small hand-rolled decoder rather than
/// pulling in a new crate for one call site. Invalid/incomplete `%` escapes
/// are passed through literally rather than dropped.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_wrapping_double_quotes() {
        assert_eq!(
            normalize_dropped_path("\"/home/scott/My Notes.txt\""),
            "/home/scott/My Notes.txt"
        );
    }

    #[test]
    fn strips_wrapping_single_quotes() {
        assert_eq!(
            normalize_dropped_path("'/home/scott/My Notes.txt'"),
            "/home/scott/My Notes.txt"
        );
    }

    #[test]
    fn unescapes_backslash_escaped_spaces() {
        assert_eq!(
            normalize_dropped_path("/Users/scott/My\\ Pictures/photo.png"),
            "/Users/scott/My Pictures/photo.png"
        );
    }

    #[test]
    fn converts_windows_drive_letter_path_to_wsl_mount() {
        assert_eq!(
            normalize_dropped_path("C:\\Users\\scott\\Documents\\notes.txt"),
            "/mnt/c/Users/scott/Documents/notes.txt"
        );
    }

    #[test]
    fn converts_uppercase_drive_letter_too() {
        assert_eq!(
            normalize_dropped_path("D:\\data\\file.txt"),
            "/mnt/d/data/file.txt"
        );
    }

    #[test]
    fn converts_wsl_localhost_unc_path() {
        assert_eq!(
            normalize_dropped_path("\\\\wsl.localhost\\Ubuntu\\home\\scott\\notes.txt"),
            "/home/scott/notes.txt"
        );
    }

    #[test]
    fn converts_wsl_dollar_unc_path() {
        assert_eq!(
            normalize_dropped_path("\\\\wsl$\\Ubuntu\\home\\scott\\notes.txt"),
            "/home/scott/notes.txt"
        );
    }

    #[test]
    fn decodes_file_uri_with_percent_encoded_space() {
        assert_eq!(
            normalize_dropped_path("file:///home/scott/My%20Notes.txt"),
            "/home/scott/My Notes.txt"
        );
    }

    #[test]
    fn plain_linux_path_passes_through_unchanged() {
        assert_eq!(
            normalize_dropped_path("/home/scott/notes.txt"),
            "/home/scott/notes.txt"
        );
    }

    #[test]
    fn plain_macos_path_passes_through_unchanged() {
        assert_eq!(
            normalize_dropped_path("/Users/scott/notes.txt"),
            "/Users/scott/notes.txt"
        );
    }

    #[test]
    fn multi_line_paste_uses_only_the_first_non_empty_line() {
        assert_eq!(
            normalize_dropped_path("/home/scott/a.txt\n/home/scott/b.txt\n"),
            "/home/scott/a.txt"
        );
    }

    #[test]
    fn has_multiple_paths_detects_more_than_one_non_empty_line() {
        assert!(has_multiple_paths("/home/scott/a.txt\n/home/scott/b.txt"));
        assert!(!has_multiple_paths("/home/scott/a.txt\n"));
        assert!(!has_multiple_paths("/home/scott/a.txt"));
    }
}
