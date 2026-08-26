//! Filename → MIME-type guessing, shared across every feature set.
//!
//! This used to live inside `jira::live::attachments` (the `live`-only REST
//! module), but the TUI upload flow's Input → Confirm preview step
//! (`app::attachments::AttachmentUpload`) needs a guessed MIME type before
//! any network call happens — CLAUDE.md's "Preview before any mutating Jira
//! call" means that preview has to render even in a `--no-default-features`
//! build, where `jira` doesn't compile at all. Living here instead keeps it
//! available everywhere; `jira::guess_mime` re-exports it under the `live`
//! feature for the actual upload request's own `Content-Type` header.

/// Guess a MIME type from a filename's extension, for callers (TUI file
/// picker, MCP write tool) that have a local path but no server-provided
/// content type. Case-insensitive on the extension; anything unrecognized
/// (or missing an extension) falls back to a generic binary type.
pub fn guess_mime(filename: &str) -> &'static str {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "log" => "text/plain",
        "zip" => "application/zip",
        "json" => "application/json",
        "csv" => "text/csv",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_mime_covers_common_extensions() {
        assert_eq!(guess_mime("photo.png"), "image/png");
        assert_eq!(guess_mime("photo.PNG"), "image/png");
        assert_eq!(guess_mime("photo.jpg"), "image/jpeg");
        assert_eq!(guess_mime("photo.jpeg"), "image/jpeg");
        assert_eq!(guess_mime("anim.gif"), "image/gif");
        assert_eq!(guess_mime("icon.svg"), "image/svg+xml");
        assert_eq!(guess_mime("doc.pdf"), "application/pdf");
        assert_eq!(guess_mime("notes.txt"), "text/plain");
        assert_eq!(guess_mime("README.md"), "text/markdown");
        assert_eq!(guess_mime("server.log"), "text/plain");
        assert_eq!(guess_mime("archive.zip"), "application/zip");
        assert_eq!(guess_mime("data.json"), "application/json");
        assert_eq!(guess_mime("table.csv"), "text/csv");
        assert_eq!(guess_mime("legacy.doc"), "application/msword");
        assert_eq!(
            guess_mime("modern.docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(guess_mime("legacy.xls"), "application/vnd.ms-excel");
        assert_eq!(
            guess_mime("modern.xlsx"),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
    }

    #[test]
    fn guess_mime_falls_back_for_unknown_or_missing_extensions() {
        assert_eq!(guess_mime("mystery.bin"), "application/octet-stream");
        assert_eq!(
            guess_mime("no_extension_at_all"),
            "application/octet-stream"
        );
    }
}
