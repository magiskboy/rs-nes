//! ROM library: scan folders, list rows, scroll.

use crate::cart::{mapper_supported, Loader};
use crate::emulator::os::browser::{BrowsePurpose, DirBrowser, FsEntry};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RomEntry {
    pub path: PathBuf,
    /// ROM file stem (raw filename without extension).
    pub name: String,
    /// Official title from `gamelist.txt` (if scanned).
    pub title: Option<String>,
    pub developer: Option<String>,
    pub year: Option<String>,
    /// Cover path relative to the ROM folder (`images/….png`).
    pub cover_rel: Option<String>,
    /// iNES mapper number from the ROM header (`None` if unreadable).
    pub mapper_id: Option<u8>,
    /// Whether the core can run this mapper.
    pub supported: bool,
}

impl RomEntry {
    pub fn from_path(path: PathBuf) -> Self {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("ROM")
            .to_string();
        let mapper_id = Loader::peek_mapper(&path);
        let supported = mapper_id.is_some_and(mapper_supported);
        Self {
            path,
            name,
            title: None,
            developer: None,
            year: None,
            cover_rel: None,
            mapper_id,
            supported,
        }
    }

    /// Label for the library list: prefer official title.
    pub fn display_name(&self) -> &str {
        self.title
            .as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or(&self.name)
    }

    /// List label; unsupported ROMs append `[mapper N]`.
    pub fn list_label(&self) -> String {
        let name = self.display_name();
        if self.supported {
            name.to_string()
        } else if let Some(id) = self.mapper_id {
            format!("{name}  [mapper {id}]")
        } else {
            format!("{name}  [bad ROM]")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibRow {
    /// Index into [`DirBrowser::entries`].
    Entry(usize),
    SelectFolder,
    UseThisFolder,
}

pub fn default_rom_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("roms"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("roms"));
            dirs.push(parent.join("../Resources/roms"));
        }
    }
    dirs
}

/// All `.nes` files under `dir` (skips hidden dirs and `images/`).
pub fn scan_roms_recursive(dir: &Path) -> Vec<RomEntry> {
    let mut out = Vec::new();
    scan_roms_recursive_into(dir, &mut out);
    out.sort_by(|a, b| {
        a.display_name()
            .to_lowercase()
            .cmp(&b.display_name().to_lowercase())
    });
    out
}

fn scan_roms_recursive_into(dir: &Path, out: &mut Vec<RomEntry>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if name.eq_ignore_ascii_case("images") {
                continue;
            }
            scan_roms_recursive_into(&path, out);
        } else if path.is_file() {
            let nes = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("nes"));
            if nes {
                out.push(RomEntry::from_path(path));
            }
        }
    }
}

/// Collect unique ROMs under every root (recursive) plus optional loose files.
pub fn collect_roms(roots: &[PathBuf], extra_files: &[PathBuf]) -> Vec<RomEntry> {
    let mut seen = std::collections::HashSet::new();
    let mut roms = Vec::new();
    for dir in roots {
        for entry in scan_roms_recursive(dir) {
            let key = entry.path.to_string_lossy().to_lowercase();
            if seen.insert(key) {
                roms.push(entry);
            }
        }
    }
    for path in extra_files {
        if !path.is_file() {
            continue;
        }
        let key = path.to_string_lossy().to_lowercase();
        if seen.insert(key) {
            roms.push(RomEntry::from_path(path.clone()));
        }
    }
    roms.sort_by(|a, b| {
        a.display_name()
            .to_lowercase()
            .cmp(&b.display_name().to_lowercase())
    });
    roms
}

impl LibRow {
    pub fn label(self, browser: &DirBrowser, roms: &[RomEntry]) -> String {
        match self {
            LibRow::Entry(i) => match browser.entries().get(i) {
                Some(FsEntry::File { path, .. }) => roms
                    .iter()
                    .find(|r| r.path == *path)
                    .map(|r| r.list_label())
                    .unwrap_or_else(|| {
                        path.file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?")
                            .to_string()
                    }),
                Some(e) => e.label(),
                None => "?".into(),
            },
            LibRow::SelectFolder => "Select folder...".into(),
            LibRow::UseThisFolder => "Use this folder".into(),
        }
    }

    pub fn unsupported(self, browser: &DirBrowser, roms: &[RomEntry]) -> bool {
        match self {
            LibRow::Entry(i) => match browser.entries().get(i) {
                Some(FsEntry::File { path, .. }) => roms
                    .iter()
                    .find(|r| r.path == *path)
                    .is_some_and(|r| !r.supported),
                _ => false,
            },
            LibRow::SelectFolder | LibRow::UseThisFolder => false,
        }
    }

    pub fn is_action(self) -> bool {
        matches!(self, LibRow::SelectFolder | LibRow::UseThisFolder)
    }

    pub fn is_folder_row(self, browser: &DirBrowser) -> bool {
        match self {
            LibRow::Entry(i) => browser
                .entries()
                .get(i)
                .is_some_and(|e| e.is_dir_like()),
            _ => false,
        }
    }
}

/// Build list rows from the browser plus a trailing action for the current purpose.
pub fn build_rows(browser: &DirBrowser) -> Vec<LibRow> {
    let mut rows: Vec<LibRow> = (0..browser.entries().len())
        .map(LibRow::Entry)
        .collect();
    match browser.purpose() {
        BrowsePurpose::PlayRom => rows.push(LibRow::SelectFolder),
        BrowsePurpose::PickFolder => rows.push(LibRow::UseThisFolder),
    }
    rows
}
