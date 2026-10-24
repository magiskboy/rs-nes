//! Generic one-level directory browser (library + in-app folder picker).

use std::fs;
use std::path::{Path, PathBuf};

/// One row in the current directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEntry {
    /// Go to parent (`..`).
    Parent,
    Dir { name: String, path: PathBuf },
    File { name: String, path: PathBuf },
}

impl FsEntry {
    pub fn label(&self) -> String {
        match self {
            FsEntry::Parent => "..".into(),
            FsEntry::Dir { name, .. } => format!("{name}/"),
            FsEntry::File { name, .. } => name.clone(),
        }
    }

    pub fn is_dir_like(&self) -> bool {
        matches!(self, FsEntry::Parent | FsEntry::Dir { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowsePurpose {
    /// Open `.nes` files; folders are for navigation only.
    PlayRom,
    /// Pick the current directory (no files listed).
    PickFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowseFilter {
    pub dirs_only: bool,
    pub extensions: &'static [&'static str],
}

impl BrowseFilter {
    pub const NES_ROMS: Self = Self {
        dirs_only: false,
        extensions: &["nes"],
    };
    pub const DIRS_ONLY: Self = Self {
        dirs_only: true,
        extensions: &[],
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowseEvent {
    None,
    EnterDir(PathBuf),
    OpenFile(PathBuf),
    /// Confirm current folder (`PickFolder`).
    ChooseFolder(PathBuf),
    GoParent,
    /// Back at picker root — leave picker without choosing.
    Cancel,
}

/// One-level filesystem browser with optional multi-root virtual top.
#[derive(Debug, Clone)]
pub struct DirBrowser {
    /// Library / start roots. When `cwd` is `None`, these are listed as dirs.
    roots: Vec<PathBuf>,
    /// `None` = virtual root (list `roots`).
    cwd: Option<PathBuf>,
    entries: Vec<FsEntry>,
    filter: BrowseFilter,
    purpose: BrowsePurpose,
    /// Loose files (CLI) merged into listings when appropriate.
    extra_files: Vec<PathBuf>,
    pub sel: usize,
    pub scroll: usize,
}

impl DirBrowser {
    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
            cwd: None,
            entries: Vec::new(),
            filter: BrowseFilter::NES_ROMS,
            purpose: BrowsePurpose::PlayRom,
            extra_files: Vec::new(),
            sel: 0,
            scroll: 0,
        }
    }

    pub fn purpose(&self) -> BrowsePurpose {
        self.purpose
    }

    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    pub fn entries(&self) -> &[FsEntry] {
        &self.entries
    }

    /// Breadcrumb / title: current folder name or `"Library"`.
    pub fn title(&self) -> String {
        match self.purpose {
            BrowsePurpose::PickFolder => {
                if let Some(cwd) = &self.cwd {
                    cwd.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Select folder")
                        .to_string()
                } else {
                    "Select folder".into()
                }
            }
            BrowsePurpose::PlayRom => {
                if let Some(cwd) = &self.cwd {
                    cwd.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("GAME LIBRARY")
                        .to_string()
                } else {
                    "GAME LIBRARY".into()
                }
            }
        }
    }

    /// Enter PlayRom mode with the given library roots (existing dirs only).
    ///
    /// `extra_files` are loose ROM paths (e.g. CLI args) shown when their parent
    /// is the current directory, or at the virtual root if outside all roots.
    pub fn open_library(&mut self, roots: Vec<PathBuf>, extra_files: &[PathBuf]) {
        let roots: Vec<PathBuf> = roots.into_iter().filter(|p| p.is_dir()).collect();
        self.purpose = BrowsePurpose::PlayRom;
        self.filter = BrowseFilter::NES_ROMS;
        self.roots = roots;
        self.extra_files = extra_files.to_vec();
        self.cwd = match self.roots.len() {
            1 => Some(self.roots[0].clone()),
            _ => None,
        };
        self.sel = 0;
        self.scroll = 0;
        self.refresh();
    }

    /// Enter PickFolder mode starting at `start` (or its parent / cwd fallback).
    pub fn open_picker(&mut self, start: PathBuf) {
        let start = if start.is_dir() {
            start
        } else {
            start
                .parent()
                .filter(|p| p.is_dir())
                .map(Path::to_path_buf)
                .unwrap_or_else(fallback_start_dir)
        };
        self.purpose = BrowsePurpose::PickFolder;
        self.filter = BrowseFilter::DIRS_ONLY;
        self.roots = vec![start.clone()];
        self.extra_files.clear();
        self.cwd = Some(start);
        self.sel = 0;
        self.scroll = 0;
        self.refresh();
    }

    pub fn refresh(&mut self) {
        let extras = if self.purpose == BrowsePurpose::PlayRom {
            self.extra_files.clone()
        } else {
            Vec::new()
        };
        self.entries = match &self.cwd {
            None => {
                let mut entries = self.virtual_root_entries();
                append_extra_files(&mut entries, &extras, None, &self.roots);
                entries
            }
            Some(dir) => {
                let mut entries = scan_level(dir, self.filter, self.can_go_parent());
                append_extra_files(&mut entries, &extras, Some(dir), &self.roots);
                entries
            }
        };
        if self.sel >= self.entries.len() && !self.entries.is_empty() {
            self.sel = self.entries.len() - 1;
        }
        if self.entries.is_empty() {
            self.sel = 0;
        }
    }

    fn virtual_root_entries(&self) -> Vec<FsEntry> {
        let mut dirs: Vec<FsEntry> = self
            .roots
            .iter()
            .map(|p| FsEntry::Dir {
                name: p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("roms")
                    .to_string(),
                path: p.clone(),
            })
            .collect();
        dirs.sort_by(|a, b| {
            a.label()
                .to_lowercase()
                .cmp(&b.label().to_lowercase())
        });
        dirs
    }

    pub fn can_go_parent(&self) -> bool {
        match self.purpose {
            BrowsePurpose::PlayRom => match &self.cwd {
                None => false,
                Some(cwd) => {
                    if self.roots.len() > 1 {
                        // At a root → back to virtual; below root → normal parent.
                        true
                    } else if let Some(root) = self.roots.first() {
                        cwd != root
                    } else {
                        false
                    }
                }
            },
            BrowsePurpose::PickFolder => self
                .cwd
                .as_ref()
                .and_then(|p| p.parent())
                .is_some_and(|p| !p.as_os_str().is_empty()),
        }
    }

    pub fn ensure_scroll_visible(&mut self, visible: usize) {
        if self.entries.is_empty() {
            self.scroll = 0;
            return;
        }
        let visible = visible.max(1);
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.sel >= self.scroll + visible {
            self.scroll = self.sel + 1 - visible;
        }
        let max_scroll = self.entries.len().saturating_sub(visible);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// Confirm on the selected entry (or `Use this folder` via [`Self::choose_cwd`]).
    pub fn activate(&mut self) -> BrowseEvent {
        let Some(entry) = self.entries.get(self.sel).cloned() else {
            return BrowseEvent::None;
        };
        match entry {
            FsEntry::Parent => self.go_parent(),
            FsEntry::Dir { path, .. } => {
                self.enter_dir(path.clone());
                BrowseEvent::EnterDir(path)
            }
            FsEntry::File { path, .. } => {
                if self.purpose == BrowsePurpose::PlayRom {
                    BrowseEvent::OpenFile(path)
                } else {
                    BrowseEvent::None
                }
            }
        }
    }

    pub fn choose_cwd(&self) -> BrowseEvent {
        match (&self.purpose, &self.cwd) {
            (BrowsePurpose::PickFolder, Some(cwd)) => BrowseEvent::ChooseFolder(cwd.clone()),
            _ => BrowseEvent::None,
        }
    }

    pub fn back(&mut self) -> BrowseEvent {
        if self.can_go_parent() {
            self.go_parent()
        } else if self.purpose == BrowsePurpose::PickFolder {
            BrowseEvent::Cancel
        } else {
            BrowseEvent::None
        }
    }

    fn enter_dir(&mut self, path: PathBuf) {
        self.cwd = Some(path);
        self.sel = 0;
        self.scroll = 0;
        self.refresh();
    }

    fn go_parent(&mut self) -> BrowseEvent {
        match self.purpose {
            BrowsePurpose::PlayRom => {
                let Some(cwd) = self.cwd.clone() else {
                    return BrowseEvent::None;
                };
                if self.roots.len() > 1 && self.roots.iter().any(|r| r == &cwd) {
                    self.cwd = None;
                    self.sel = 0;
                    self.scroll = 0;
                    self.refresh();
                    return BrowseEvent::GoParent;
                }
                if let Some(root) = self.roots.first() {
                    if &cwd == root {
                        return BrowseEvent::None;
                    }
                }
                let parent = cwd.parent().map(Path::to_path_buf);
                // Stay under the library root when single-root.
                if let (Some(root), Some(parent)) = (self.roots.first(), parent.as_ref()) {
                    if !parent.starts_with(root) && parent != root {
                        self.cwd = Some(root.clone());
                        self.sel = 0;
                        self.scroll = 0;
                        self.refresh();
                        return BrowseEvent::GoParent;
                    }
                }
                self.cwd = parent;
                self.sel = 0;
                self.scroll = 0;
                self.refresh();
                BrowseEvent::GoParent
            }
            BrowsePurpose::PickFolder => {
                let Some(cwd) = self.cwd.clone() else {
                    return BrowseEvent::Cancel;
                };
                match cwd.parent() {
                    Some(p) if !p.as_os_str().is_empty() => {
                        self.cwd = Some(p.to_path_buf());
                        self.sel = 0;
                        self.scroll = 0;
                        self.refresh();
                        BrowseEvent::GoParent
                    }
                    _ => BrowseEvent::Cancel,
                }
            }
        }
    }

    /// Select a file entry by path if it is in the current listing.
    pub fn select_file(&mut self, path: &Path, visible: usize) -> bool {
        if let Some(i) = self.entries.iter().position(|e| match e {
            FsEntry::File { path: p, .. } => p == path,
            _ => false,
        }) {
            self.sel = i;
            self.ensure_scroll_visible(visible);
            true
        } else {
            false
        }
    }

    /// Navigate so `path`'s parent is cwd and the file is selected (PlayRom).
    pub fn reveal_file(&mut self, path: &Path, visible: usize) {
        if self.purpose != BrowsePurpose::PlayRom {
            return;
        }
        let Some(parent) = path.parent() else {
            return;
        };
        // If parent is under a known root, go there; else ignore.
        let under_root = self.roots.iter().any(|r| parent.starts_with(r));
        if !under_root && !self.roots.is_empty() {
            return;
        }
        self.cwd = Some(parent.to_path_buf());
        self.refresh();
        let _ = self.select_file(path, visible);
    }
}

fn fallback_start_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Show loose files when browsing their parent, or at virtual root if outside roots.
fn append_extra_files(
    entries: &mut Vec<FsEntry>,
    extra_files: &[PathBuf],
    cwd: Option<&Path>,
    roots: &[PathBuf],
) {
    for path in extra_files {
        if !path.is_file() {
            continue;
        }
        let already = entries.iter().any(|e| matches!(e, FsEntry::File { path: p, .. } if p == path));
        if already {
            continue;
        }
        let parent = path.parent();
        let include = match cwd {
            Some(dir) => parent == Some(dir),
            None => parent.is_some_and(|p| !roots.iter().any(|r| p.starts_with(r))),
        };
        if !include {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("ROM")
            .to_string();
        entries.push(FsEntry::File {
            name,
            path: path.clone(),
        });
    }
}

fn scan_level(dir: &Path, filter: BrowseFilter, show_parent: bool) -> Vec<FsEntry> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if name.eq_ignore_ascii_case("images") {
                    continue;
                }
                dirs.push(FsEntry::Dir { name, path });
            } else if path.is_file() && !filter.dirs_only {
                let ok = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| {
                        filter
                            .extensions
                            .iter()
                            .any(|want| ext.eq_ignore_ascii_case(want))
                    });
                if ok {
                    files.push(FsEntry::File { name, path });
                }
            }
        }
    }
    dirs.sort_by(|a, b| a.label().to_lowercase().cmp(&b.label().to_lowercase()));
    files.sort_by(|a, b| a.label().to_lowercase().cmp(&b.label().to_lowercase()));
    let mut out = Vec::new();
    if show_parent {
        out.push(FsEntry::Parent);
    }
    out.extend(dirs);
    out.extend(files);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scan_hides_images_and_lists_nes() {
        let dir = std::env::temp_dir().join("rs_nes_browser_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("images")).unwrap();
        fs::create_dir_all(dir.join("Action")).unwrap();
        fs::write(dir.join("game.nes"), b"NES").unwrap();
        fs::write(dir.join("notes.txt"), b"x").unwrap();

        let entries = scan_level(&dir, BrowseFilter::NES_ROMS, true);
        assert!(entries.iter().any(|e| matches!(e, FsEntry::Parent)));
        assert!(entries.iter().any(|e| matches!(e, FsEntry::Dir { name, .. } if name == "Action")));
        assert!(entries.iter().any(|e| matches!(e, FsEntry::File { name, .. } if name == "game.nes")));
        assert!(!entries.iter().any(|e| matches!(e, FsEntry::Dir { name, .. } if name == "images")));
        assert!(!entries.iter().any(|e| matches!(e, FsEntry::File { name, .. } if name == "notes.txt")));

        let _ = fs::remove_dir_all(&dir);
    }
}
