//! `gamelist.txt` — per-folder game metadata next to ROMs.
//!
//! Format: blocks of 4 lines separated by a blank line:
//! 1. Official title
//! 2. Developer
//! 3. Release year
//! 4. Relative path to cover image

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::library::RomEntry;

pub const FILENAME: &str = "gamelist.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameMeta {
    pub title: String,
    pub developer: String,
    pub year: String,
    /// Path relative to the folder that contains `gamelist.txt`.
    pub cover_rel: String,
}

/// Load `dir/gamelist.txt`.
pub fn load(dir: &Path) -> Vec<GameMeta> {
    let path = dir.join(FILENAME);
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse(&text)
}

pub fn parse(text: &str) -> Vec<GameMeta> {
    let mut out = Vec::new();
    let mut lines = text.lines().map(|l| l.trim_end()).peekable();
    while lines.peek().is_some() {
        // Skip blank separators.
        while matches!(lines.peek(), Some(l) if l.trim().is_empty()) {
            lines.next();
        }
        let Some(title) = lines.next() else { break };
        let title = title.trim();
        if title.is_empty() {
            continue;
        }
        let developer = lines.next().unwrap_or("").trim();
        let year = lines.next().unwrap_or("").trim();
        let cover_rel = lines.next().unwrap_or("").trim();
        if cover_rel.is_empty() {
            continue;
        }
        out.push(GameMeta {
            title: title.to_string(),
            developer: if developer.is_empty() {
                "-".into()
            } else {
                developer.to_string()
            },
            year: if year.is_empty() {
                "-".into()
            } else {
                year.to_string()
            },
            cover_rel: cover_rel.replace('\\', "/"),
        });
    }
    out
}

pub fn format(entries: &[GameMeta]) -> String {
    let mut blocks = Vec::with_capacity(entries.len());
    for e in entries {
        let dev = if e.developer.is_empty() {
            "-"
        } else {
            e.developer.as_str()
        };
        let year = if e.year.is_empty() {
            "-"
        } else {
            e.year.as_str()
        };
        blocks.push(format!(
            "{}\n{}\n{}\n{}",
            e.title.trim(),
            dev,
            year,
            e.cover_rel.replace('\\', "/")
        ));
    }
    let mut s = blocks.join("\n\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

pub fn save(dir: &Path, entries: &[GameMeta]) -> Result<(), String> {
    let path = dir.join(FILENAME);
    fs::write(&path, format(entries)).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Relative cover path used as the stable key for a ROM in `gamelist.txt`.
pub fn cover_rel_for(rom: &Path) -> Option<String> {
    let stem = rom.file_stem()?.to_str()?;
    Some(format!("images/{stem}.png"))
}

/// Attach metadata from each ROM's folder `gamelist.txt`.
pub fn apply_to_roms(roms: &mut [RomEntry]) {
    let mut cache: HashMap<PathBuf, Vec<GameMeta>> = HashMap::new();
    for rom in roms.iter_mut() {
        let Some(dir) = rom.path.parent() else {
            continue;
        };
        let dir_buf = dir.to_path_buf();
        if !cache.contains_key(&dir_buf) {
            cache.insert(dir_buf.clone(), load(&dir_buf));
        }
        let list = cache.get(&dir_buf).map(|v| v.as_slice()).unwrap_or(&[]);
        if let Some(meta) = match_rom(rom, list) {
            rom.title = Some(meta.title.clone());
            rom.developer = Some(meta.developer.clone());
            rom.year = Some(meta.year.clone());
            rom.cover_rel = Some(meta.cover_rel.clone());
        }
    }
}

fn match_rom<'a>(rom: &RomEntry, list: &'a [GameMeta]) -> Option<&'a GameMeta> {
    let want = cover_rel_for(&rom.path)?;
    let want_norm = want.replace('\\', "/").to_ascii_lowercase();
    list.iter()
        .find(|e| e.cover_rel.replace('\\', "/").to_ascii_lowercase() == want_norm)
}

/// Resolve absolute cover path from a ROM entry (gamelist path or default naming).
pub fn resolve_cover_path(rom: &RomEntry) -> Option<PathBuf> {
    let parent = rom.path.parent()?;
    if let Some(rel) = rom.cover_rel.as_deref() {
        if let Some(p) = existing_image(parent, rel) {
            return Some(p);
        }
    }
    if let Some(rel) = cover_rel_for(&rom.path) {
        existing_image(parent, &rel)
    } else {
        None
    }
}

fn existing_image(parent: &Path, rel: &str) -> Option<PathBuf> {
    let p = parent.join(rel);
    if p.is_file() {
        return Some(p);
    }
    let stem = p.file_stem()?.to_str()?;
    let dir = p.parent()?;
    for ext in ["png", "jpg", "jpeg", "PNG", "JPG", "JPEG"] {
        let alt = dir.join(format!("{stem}.{ext}"));
        if alt.is_file() {
            return Some(alt);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_blocks() {
        let entries = vec![
            GameMeta {
                title: "Metroid".into(),
                developer: "Nintendo".into(),
                year: "1986".into(),
                cover_rel: "images/Metroid (USA).png".into(),
            },
            GameMeta {
                title: "The Legend of Zelda".into(),
                developer: "Nintendo".into(),
                year: "1986".into(),
                cover_rel: "images/Zelda.png".into(),
            },
        ];
        let text = format(&entries);
        assert_eq!(
            text,
            "Metroid\nNintendo\n1986\nimages/Metroid (USA).png\n\nThe Legend of Zelda\nNintendo\n1986\nimages/Zelda.png\n"
        );
        assert_eq!(parse(&text), entries);
    }
}
