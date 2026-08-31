//! The attachment upload flow's file browser (`app::attachments::AttachmentUpload::Browse`,
//! `u` on the Detail screen — see `attachments.rs`'s module doc for the
//! flow's overall shape): a plain directory listing with type-to-filter
//! narrowing, no async I/O involved (`std::fs::read_dir` is fast enough to
//! call synchronously on every navigation, unlike the network-backed
//! pickers under `app::async_ops`).
//!
//! `Backspace` edits the live filter while it's non-empty; once the filter
//! is empty, `Backspace` instead goes up one directory level. This dual
//! role avoids a separate "filter mode" toggle — there's only ever one
//! typing target — while still leaving `Backspace` free to navigate once
//! there's nothing left to erase, mirroring how a shell's `cd ..` only
//! makes sense once you're not mid-word.

use std::path::{Path, PathBuf};

/// One entry in a `FileBrowserState`'s current directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// State for the open file browser. `entries` is always `unfiltered`
/// narrowed by `filter` — see `apply_filter` — so callers only ever need to
/// read `entries`, never `unfiltered` directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileBrowserState {
    pub cwd: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub filter: String,
    /// The unfiltered listing of `cwd`, re-read only on navigation (descend/
    /// ascend), not on every filter keystroke — kept separate so typing into
    /// the filter never re-hits the filesystem.
    unfiltered: Vec<FileEntry>,
}

impl FileBrowserState {
    /// Open a browser rooted at `start_dir`, immediately listing it. A
    /// directory-read failure (e.g. an unreadable home directory) yields an
    /// empty listing rather than failing outright — the second element of
    /// the tuple carries a message for the caller to flash via `App::status`,
    /// matching how every other read failure here is reported.
    pub fn new(start_dir: PathBuf) -> (Self, Option<String>) {
        let mut state = FileBrowserState {
            cwd: start_dir,
            entries: Vec::new(),
            unfiltered: Vec::new(),
            selected: 0,
            filter: String::new(),
        };
        let err = state.reload();
        (state, err)
    }

    /// Re-read `cwd` and reapply the current filter. On failure the listing
    /// is left empty but `cwd` itself is unchanged, so a permission-denied
    /// subdirectory doesn't strand the browser somewhere it can't display
    /// anything at all — the caller can still back out with another
    /// `Backspace`/`go_up`.
    fn reload(&mut self) -> Option<String> {
        match read_dir_sorted(&self.cwd) {
            Ok(entries) => {
                self.unfiltered = entries;
                self.apply_filter();
                None
            }
            Err(e) => {
                self.unfiltered.clear();
                self.entries.clear();
                self.selected = 0;
                Some(format!("{}: {e}", self.cwd.display()))
            }
        }
    }

    /// Recompute `entries` from `unfiltered` and the current `filter`
    /// (case-insensitive substring match on `name`), clamping `selected`
    /// back into bounds if the narrower list is now shorter.
    fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.entries = self
            .unfiltered
            .iter()
            .filter(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    /// Move the highlighted row by `delta`, clamped within bounds — same
    /// shape as `App::picker_move`/`sprint_picker_move`.
    pub fn move_selection(&mut self, delta: isize) {
        let len = self.entries.len();
        if len == 0 {
            return;
        }
        let mut idx = self.selected as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len as isize {
            idx = len as isize - 1;
        }
        self.selected = idx as usize;
    }

    /// Type a printable character into the filter, live-narrowing `entries`.
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.apply_filter();
    }

    /// `Backspace`: edit the filter while it's non-empty, otherwise go up a
    /// directory level — see this module's doc comment for the rationale.
    /// Returns an error message if going up hit a directory-read failure.
    pub fn backspace(&mut self) -> Option<String> {
        if !self.filter.is_empty() {
            self.filter.pop();
            self.apply_filter();
            return None;
        }
        self.go_up()
    }

    /// Go up one directory level, resetting the filter and re-listing. A
    /// no-op at the filesystem root, where `cwd.parent()` is `None`.
    pub fn go_up(&mut self) -> Option<String> {
        let parent = self.cwd.parent()?.to_path_buf();
        self.cwd = parent;
        self.filter.clear();
        self.reload()
    }

    /// The highlighted entry, if any — `None` for an empty or
    /// fully-filtered-out listing.
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected)
    }

    /// Descend into `dir` (the caller checks `FileEntry::is_dir` first),
    /// resetting the filter and selection and re-listing.
    pub fn descend(&mut self, dir: PathBuf) -> Option<String> {
        self.cwd = dir;
        self.filter.clear();
        self.reload()
    }
}

/// List `dir`'s entries: dotfiles hidden, directories first, then
/// alphabetical case-insensitive within each group. Symlinks are classified
/// by what they point to (`Path::is_dir` follows symlinks, unlike
/// `DirEntry::file_type`), so a symlinked directory browses like any other.
fn read_dir_sorted(dir: &Path) -> std::io::Result<Vec<FileEntry>> {
    let mut entries: Vec<FileEntry> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = e.path();
            let is_dir = path.is_dir();
            Some(FileEntry { name, path, is_dir })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a scratch directory tree for these tests:
    /// `root/{Zebra/, apple/, banana.txt, Cactus.txt, .hidden}` — enough to
    /// exercise sorting (dirs first, case-insensitive alphabetical), dotfile
    /// hiding, and descend/ascend.
    fn scratch_tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jira-tui-file-browser-test-{name}-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Zebra")).unwrap();
        std::fs::create_dir_all(root.join("apple")).unwrap();
        std::fs::write(root.join("banana.txt"), b"banana").unwrap();
        std::fs::write(root.join("Cactus.txt"), b"cactus").unwrap();
        std::fs::write(root.join(".hidden"), b"shh").unwrap();
        root
    }

    #[test]
    fn new_lists_dotfiles_hidden_dirs_first_then_alphabetical_case_insensitive() {
        let root = scratch_tree("sort");
        let (state, err) = FileBrowserState::new(root.clone());
        assert_eq!(err, None);
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["apple", "Zebra", "banana.txt", "Cactus.txt"],
            "expected dirs first (case-insensitive alpha), then files (case-insensitive alpha), \
             with .hidden excluded — got {names:?}"
        );
        assert!(state.entries[0].is_dir);
        assert!(state.entries[1].is_dir);
        assert!(!state.entries[2].is_dir);
        assert!(!state.entries[3].is_dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn descend_into_a_directory_relists_and_resets_selection_and_filter() {
        let root = scratch_tree("descend");
        std::fs::write(root.join("apple").join("core.txt"), b"core").unwrap();
        let (mut state, _) = FileBrowserState::new(root.clone());
        state.filter_push('z');
        state.selected = 0;
        assert_eq!(state.entries.len(), 1, "filter should narrow to Zebra only");

        // Clear the filter before descending, mirroring how the app resets
        // it on Enter — descend() itself doesn't know the row it's given
        // came from a filtered view.
        let apple_dir = root.join("apple");
        let err = state.descend(apple_dir.clone());
        assert_eq!(err, None);
        assert_eq!(state.cwd, apple_dir);
        assert_eq!(state.selected, 0);
        assert_eq!(state.filter, "");
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["core.txt"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn go_up_returns_to_the_parent_directory() {
        let root = scratch_tree("ascend");
        let apple_dir = root.join("apple");
        let (mut state, _) = FileBrowserState::new(apple_dir.clone());
        assert_eq!(state.cwd, apple_dir);

        let err = state.go_up();
        assert_eq!(err, None);
        assert_eq!(state.cwd, root);
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"apple"));
        assert!(names.contains(&"Zebra"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn backspace_edits_the_filter_before_falling_back_to_go_up() {
        let root = scratch_tree("backspace");
        let apple_dir = root.join("apple");
        let (mut state, _) = FileBrowserState::new(apple_dir.clone());
        state.filter_push('x');
        state.filter_push('y');
        assert_eq!(state.filter, "xy");

        // Filter is non-empty: backspace edits it, cwd stays put.
        let err = state.backspace();
        assert_eq!(err, None);
        assert_eq!(state.filter, "x");
        assert_eq!(state.cwd, apple_dir);

        let err = state.backspace();
        assert_eq!(err, None);
        assert_eq!(state.filter, "");
        assert_eq!(state.cwd, apple_dir);

        // Filter is now empty: backspace goes up a directory instead.
        let err = state.backspace();
        assert_eq!(err, None);
        assert_eq!(state.cwd, root);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn filter_narrows_entries_and_clamps_selection() {
        let root = scratch_tree("filter");
        let (mut state, _) = FileBrowserState::new(root.clone());
        state.selected = state.entries.len() - 1;

        state.filter_push('c');
        // "Cactus.txt" matches (case-insensitive); nothing else does.
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Cactus.txt"]);
        assert_eq!(
            state.selected, 0,
            "selection should clamp back into the narrowed list"
        );

        state.filter_push('c'); // "Cactusc" now matches nothing
        assert!(state.entries.is_empty());
        assert_eq!(state.selected, 0);

        state.backspace();
        state.backspace();
        assert_eq!(state.filter, "");
        assert_eq!(
            state.entries.len(),
            4,
            "clearing the filter restores the full listing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn move_selection_clamps_within_bounds() {
        let root = scratch_tree("move");
        let (mut state, _) = FileBrowserState::new(root.clone());
        let len = state.entries.len();
        state.move_selection(-5);
        assert_eq!(state.selected, 0);
        state.move_selection(len as isize + 5);
        assert_eq!(state.selected, len - 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn new_on_an_unreadable_directory_reports_an_error_and_stays_empty() {
        let missing = std::env::temp_dir().join(format!(
            "jira-tui-file-browser-test-missing-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&missing);
        let (state, err) = FileBrowserState::new(missing.clone());
        assert!(
            err.is_some(),
            "a nonexistent directory should report an error"
        );
        assert!(state.entries.is_empty());
    }
}
