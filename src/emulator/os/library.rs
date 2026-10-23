//! ROM library: scan folders, list rows, scroll.

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
}

impl RomEntry {
    pub fn from_path(path: PathBuf) -> Self {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("ROM")
            .to_string();
        Self {
            path,
            name,
            title: None,
            developer: None,
            year: None,
            cover_rel: None,
        }
    }

    /// Label for the library list: prefer official title.
    pub fn display_name(&self) -> &str {
        self.title.as_deref().filter(|t| !t.is_empty()).unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibRow {
    Rom(usize),
    SelectFolder,
}

pub fn scan_rom_dir(dir: &Path) -> Vec<RomEntry> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("nes"))
            .unwrap_or(false);
        if !ext {
            continue;
        }
        out.push(RomEntry::from_path(path));
    }
    out.sort_by(|a, b| {
        a.display_name()
            .to_lowercase()
            .cmp(&b.display_name().to_lowercase())
    });
    out
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

impl LibRow {
    pub fn label(self, roms: &[RomEntry]) -> String {
        match self {
            LibRow::Rom(i) => roms
                .get(i)
                .map(|r| r.display_name().to_string())
                .unwrap_or_else(|| "?".into()),
            LibRow::SelectFolder => "Select folder...".into(),
        }
    }
}

pub fn build_rows(rom_count: usize) -> Vec<LibRow> {
    let mut rows: Vec<LibRow> = (0..rom_count).map(LibRow::Rom).collect();
    rows.push(LibRow::SelectFolder);
    rows
}
