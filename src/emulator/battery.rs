//! Battery-backed PRG-RAM (`.sav`) next to the ROM.

use crate::cart::Cartridge;
use std::fs;
use std::path::{Path, PathBuf};

/// `game.nes` → `game.sav` in the same folder.
pub fn sav_path(rom: &Path) -> PathBuf {
    rom.with_extension("sav")
}

/// Load `.sav` into the cart when battery-backed. Missing file is a no-op.
pub fn load_into(cart: &mut Cartridge, rom_path: &Path) {
    if cart.battery_bytes().is_none() {
        return;
    }
    let path = sav_path(rom_path);
    let Ok(data) = fs::read(&path) else {
        return;
    };
    cart.load_battery(&data);
}

/// Write PRG-RAM to `.sav` when the cart has a battery.
pub fn save_from(cart: &Cartridge, rom_path: &Path) {
    let Some(data) = cart.battery_bytes() else {
        return;
    };
    let path = sav_path(rom_path);
    if let Err(e) = fs::write(&path, data) {
        eprintln!("battery save {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cart::{Cartridge, Loader};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn mmc1_battery_cart() -> Cartridge {
        let mut bytes = vec![0u8; 16 + 0x8000];
        bytes[0..4].copy_from_slice(b"NES\x1a");
        bytes[4] = 2;
        bytes[5] = 0;
        bytes[6] = 0x12;
        let rom = Loader::parse(&bytes).unwrap();
        Cartridge::from_rom(rom).unwrap()
    }

    #[test]
    fn round_trip_sav_file() {
        let dir = std::env::temp_dir().join(format!(
            "rs-nes-sav-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let rom = dir.join("zelda.nes");
        fs::write(&rom, b"dummy").unwrap();

        let mut cart = mmc1_battery_cart();
        cart.prg_ram_mut()[0] = 0x42;
        cart.prg_ram_mut()[100] = 0x99;
        save_from(&cart, &rom);

        let mut cart2 = mmc1_battery_cart();
        load_into(&mut cart2, &rom);
        assert_eq!(cart2.prg_ram()[0], 0x42);
        assert_eq!(cart2.prg_ram()[100], 0x99);

        let _ = fs::remove_dir_all(&dir);
    }
}
