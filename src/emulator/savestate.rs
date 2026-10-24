//! Binary savestates (`RSNS` format) under the user config directory.

use crate::emulator::config;
use crate::nes::Nes;
use std::fs;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"RSNS";
const VERSION: u32 = 1;

/// `{config_dir}/states/{rom_stem}.s{1|2|3}` — slots are 0..=2.
pub fn slot_path(rom_path: &Path, slot: u8) -> Option<PathBuf> {
    let stem = rom_path.file_stem()?.to_str()?;
    let dir = config::config_dir()?.join("states");
    Some(dir.join(format!("{stem}.s{}", slot + 1)))
}

/// Whether a slot file already exists.
pub fn slot_exists(rom_path: &Path, slot: u8) -> bool {
    slot_path(rom_path, slot).is_some_and(|p| p.is_file())
}

pub fn save(nes: &Nes, rom_path: &Path, slot: u8) -> Result<PathBuf, String> {
    if slot > 2 {
        return Err("slot must be 0..=2".into());
    }
    let path = slot_path(rom_path, slot).ok_or("cannot resolve savestate path")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let cart = nes.cartridge().ok_or("no cartridge")?;
    let stem = rom_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("rom");
    let stem_bytes = stem.as_bytes();
    if stem_bytes.len() > 255 {
        return Err("ROM stem too long".into());
    }

    let mut body = Vec::new();
    nes.export_state(&mut body)?;

    let mut file = Vec::new();
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&VERSION.to_le_bytes());
    file.push(cart.mapper_id);
    file.push(stem_bytes.len() as u8);
    file.extend_from_slice(stem_bytes);
    file.extend_from_slice(&(body.len() as u32).to_le_bytes());
    // Lightweight checksum: XOR of body bytes (detects casual corruption).
    let checksum = body.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
    file.extend_from_slice(&checksum.to_le_bytes());
    file.extend_from_slice(&body);

    fs::write(&path, &file).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load(nes: &mut Nes, rom_path: &Path, slot: u8) -> Result<(), String> {
    if slot > 2 {
        return Err("slot must be 0..=2".into());
    }
    let path = slot_path(rom_path, slot).ok_or("cannot resolve savestate path")?;
    let bytes = fs::read(&path).map_err(|_| format!("empty slot {}", slot + 1))?;
    apply_bytes(nes, rom_path, &bytes)
}

fn apply_bytes(nes: &mut Nes, rom_path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 4 + 4 + 1 + 1 + 4 + 4 {
        return Err("savestate too short".into());
    }
    if &bytes[0..4] != MAGIC {
        return Err("bad savestate magic".into());
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != VERSION {
        return Err(format!("unsupported savestate version {version}"));
    }
    let mapper_id = bytes[8];
    let stem_len = bytes[9] as usize;
    let mut off = 10;
    if bytes.len() < off + stem_len + 8 {
        return Err("savestate truncated".into());
    }
    let file_stem = std::str::from_utf8(&bytes[off..off + stem_len])
        .map_err(|_| "bad stem encoding")?;
    off += stem_len;
    let body_len = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    let checksum = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
    off += 4;
    if bytes.len() < off + body_len {
        return Err("savestate body truncated".into());
    }
    let body = &bytes[off..off + body_len];
    let sum = body.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
    if sum != checksum {
        return Err("savestate checksum mismatch".into());
    }

    let cart = nes.cartridge().ok_or("no cartridge")?;
    if cart.mapper_id != mapper_id {
        return Err(format!(
            "savestate mapper {mapper_id} != loaded {}",
            cart.mapper_id
        ));
    }
    let rom_stem = rom_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if file_stem != rom_stem {
        return Err(format!(
            "savestate ROM stem '{file_stem}' != '{rom_stem}'"
        ));
    }

    let mut cursor = body;
    nes.import_state(&mut cursor)?;
    if !cursor.is_empty() {
        // Extra trailing bytes are tolerated for forward-compat padding.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cart::Loader;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn round_trip_changes_pc() {
        let rom = match Loader::load("roms/super-mario-bros.nes") {
            Ok(r) => r,
            Err(_) => return, // skip if ROM missing in CI
        };
        let mut nes = Nes::from_rom(rom).unwrap();
        nes.run_frames(2).unwrap();
        let pc0 = nes.pc();

        let dir = std::env::temp_dir().join(format!(
            "rs-nes-ss-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        // Point config via writing next to a fake rom path under our temp dir —
        // `slot_path` uses config_dir; exercise encode/decode via apply_bytes.
        let mut blob = Vec::new();
        let cart = nes.cartridge().unwrap();
        blob.extend_from_slice(MAGIC);
        blob.extend_from_slice(&VERSION.to_le_bytes());
        blob.push(cart.mapper_id);
        let stem = b"super-mario-bros";
        blob.push(stem.len() as u8);
        blob.extend_from_slice(stem);
        let mut body = Vec::new();
        nes.export_state(&mut body).unwrap();
        blob.extend_from_slice(&(body.len() as u32).to_le_bytes());
        let checksum = body.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
        blob.extend_from_slice(&checksum.to_le_bytes());
        blob.extend_from_slice(&body);

        // Mutate then restore.
        nes.cpu.pc = 0x1234;
        assert_ne!(nes.pc(), pc0);
        let rom_path = PathBuf::from("roms/super-mario-bros.nes");
        apply_bytes(&mut nes, &rom_path, &blob).unwrap();
        assert_eq!(nes.pc(), pc0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_slot_errors() {
        let rom = match Loader::load("roms/super-mario-bros.nes") {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut nes = Nes::from_rom(rom).unwrap();
        let path = PathBuf::from("roms/super-mario-bros.nes");
        // Use a slot path that won't exist — may still resolve under config_dir.
        let err = load(&mut nes, &path, 2);
        assert!(err.is_err());
    }
}
