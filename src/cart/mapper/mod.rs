//! Cartridge + mapper dispatch.
//!
//! Learning layout — one file per board:
//! - [`nrom`] — mapper 0 (Super Mario Bros, …)
//! - [`mmc1`] — mapper 1 (Zelda, Metroid, …)
//! - [`uxrom`] — mapper 2 (Contra, Mega Man, …)
//! - [`cnrom`] — mapper 3 (many early titles)
//! - [`mmc3`] — mapper 4 (SMB3, Mega Man 3–6, …)
//! - [`axrom`] — mapper 7 (Battletoads, …)
//!
//! To add a mapper later:
//! 1. Create `src/cart/mapper/<name>.rs` implementing bank logic
//! 2. Add a variant to [`MapperImpl`]
//! 3. Wire it in [`MapperImpl::from_id`]

mod axrom;
mod cnrom;
mod mmc1;
mod mmc3;
mod nrom;
mod uxrom;

use crate::error::NesError;
use crate::cart::{Mirroring, Rom};
use axrom::Axrom;
use cnrom::Cnrom;
use mmc1::Mmc1;
use mmc3::Mmc3;
use nrom::Nrom;
use uxrom::Uxrom;

/// Concrete mapper chips currently supported.
///
/// Using an enum (not `dyn Trait`) keeps dispatch obvious for learners:
/// open this match when adding a new board.
#[derive(Debug)]
enum MapperImpl {
    Nrom(Nrom),
    Mmc1(Mmc1),
    Uxrom(Uxrom),
    Cnrom(Cnrom),
    Mmc3(Mmc3),
    Axrom(Axrom),
}

impl MapperImpl {
    fn from_id(mapper_id: u8, prg_len: usize, chr_len: usize) -> Result<Self, NesError> {
        let prg_banks_16k = (prg_len / 0x4000).max(1);
        let chr_banks_8k = (chr_len / 0x2000).max(1);
        match mapper_id {
            0 => Ok(Self::Nrom(Nrom::new())),
            1 => Ok(Self::Mmc1(Mmc1::new(prg_len, chr_len.max(0x2000)))),
            2 => Ok(Self::Uxrom(Uxrom::new(prg_banks_16k))),
            3 => Ok(Self::Cnrom(Cnrom::new(chr_banks_8k))),
            4 => Ok(Self::Mmc3(Mmc3::new(prg_len, chr_len.max(0x2000)))),
            7 => Ok(Self::Axrom(Axrom::new(prg_len))),
            id => Err(NesError::UnsupportedMapper(id)),
        }
    }

    /// PRG-RAM size for this board (learning default: 8 KiB when used).
    fn wants_prg_ram(mapper_id: u8, battery: bool) -> bool {
        matches!(mapper_id, 1 | 4) || battery
    }

    fn cpu_read(&self, addr: u16, prg: &[u8], prg_ram: &[u8]) -> u8 {
        match self {
            Self::Nrom(m) => m.cpu_read(addr, prg, prg_ram),
            Self::Mmc1(m) => m.cpu_read(addr, prg, prg_ram),
            Self::Uxrom(m) => m.cpu_read(addr, prg, prg_ram),
            Self::Cnrom(m) => m.cpu_read(addr, prg, prg_ram),
            Self::Mmc3(m) => m.cpu_read(addr, prg, prg_ram),
            Self::Axrom(m) => m.cpu_read(addr, prg, prg_ram),
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8, prg_ram: &mut [u8]) {
        match self {
            Self::Nrom(m) => m.cpu_write(addr, value, prg_ram),
            Self::Mmc1(m) => m.cpu_write(addr, value, prg_ram),
            Self::Uxrom(m) => m.cpu_write(addr, value, prg_ram),
            Self::Cnrom(m) => m.cpu_write(addr, value, prg_ram),
            Self::Mmc3(m) => m.cpu_write(addr, value, prg_ram),
            Self::Axrom(m) => m.cpu_write(addr, value, prg_ram),
        }
    }

    fn chr_read(&self, addr: u16, chr: &[u8]) -> u8 {
        match self {
            Self::Nrom(m) => m.chr_read(addr, chr),
            Self::Mmc1(m) => m.chr_read(addr, chr),
            Self::Uxrom(m) => m.chr_read(addr, chr),
            Self::Cnrom(m) => m.chr_read(addr, chr),
            Self::Mmc3(m) => m.chr_read(addr, chr),
            Self::Axrom(m) => m.chr_read(addr, chr),
        }
    }

    fn chr_write(&mut self, addr: u16, value: u8, chr: &mut [u8], chr_is_ram: bool) {
        match self {
            Self::Nrom(m) => m.chr_write(addr, value, chr, chr_is_ram),
            Self::Mmc1(m) => m.chr_write(addr, value, chr, chr_is_ram),
            Self::Uxrom(m) => m.chr_write(addr, value, chr, chr_is_ram),
            Self::Cnrom(m) => m.chr_write(addr, value, chr, chr_is_ram),
            Self::Mmc3(m) => m.chr_write(addr, value, chr, chr_is_ram),
            Self::Axrom(m) => m.chr_write(addr, value, chr, chr_is_ram),
        }
    }

    fn mirroring(&self, header: Mirroring) -> Mirroring {
        match self {
            Self::Mmc1(m) => m.mirroring(),
            Self::Mmc3(m) => m.mirroring(header == Mirroring::FourScreen),
            Self::Axrom(m) => m.mirroring(),
            Self::Nrom(_) | Self::Uxrom(_) | Self::Cnrom(_) => header,
        }
    }

    fn irq_pending(&self) -> bool {
        match self {
            Self::Mmc3(m) => m.irq_pending(),
            _ => false,
        }
    }

    fn on_ppu_a12_rise(&mut self) {
        if let Self::Mmc3(m) = self {
            m.on_ppu_a12_rise();
        }
    }
}

/// Cartridge plugged into the console: PRG/CHR + mapper + mirroring.
///
/// CPU `$4020–$FFFF` and PPU pattern `$0000–$1FFF` go through here.
#[derive(Debug)]
pub struct Cartridge {
    /// Mirroring from the iNES header (fixed boards); ASIC mappers may override.
    header_mirroring: Mirroring,
    pub mapper_id: u8,
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    /// CHR-ROM, or CHR-RAM when the header had `chr_rom_banks == 0`.
    chr_mem: Vec<u8>,
    chr_is_ram: bool,
    mapper: MapperImpl,
}

impl Cartridge {
    /// Build from a parsed iNES [`Rom`].
    pub fn from_rom(rom: Rom) -> Result<Self, NesError> {
        if rom.prg_rom.is_empty() {
            return Err(NesError::GenericError);
        }

        let mapper_id = rom.header.mapper;
        let chr_is_ram = rom.header.has_chr_ram();
        let chr_mem = if chr_is_ram {
            vec![0; 0x2000]
        } else {
            rom.chr_rom
        };
        let chr_len = chr_mem.len();

        let mapper = MapperImpl::from_id(mapper_id, rom.prg_rom.len(), chr_len)?;
        let prg_ram = if MapperImpl::wants_prg_ram(mapper_id, rom.header.battery_backed) {
            vec![0; 0x2000]
        } else {
            Vec::new()
        };

        Ok(Self {
            header_mirroring: rom.header.mirroring,
            mapper_id,
            prg_rom: rom.prg_rom,
            prg_ram,
            chr_mem,
            chr_is_ram,
            mapper,
        })
    }

    /// NROM cart from raw PRG/CHR (unit tests without a full iNES file).
    pub fn nrom(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr_mem = if chr_is_ram {
            vec![0; 0x2000]
        } else {
            chr_rom
        };
        Self {
            header_mirroring: Mirroring::Horizontal,
            mapper_id: 0,
            prg_rom,
            prg_ram: Vec::new(),
            chr_mem,
            chr_is_ram,
            mapper: MapperImpl::Nrom(Nrom::new()),
        }
    }

    pub fn prg_rom(&self) -> &[u8] {
        &self.prg_rom
    }

    pub fn prg_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    pub fn chr_mem(&self) -> &[u8] {
        &self.chr_mem
    }

    pub fn chr_is_ram(&self) -> bool {
        self.chr_is_ram
    }

    /// Effective nametable mirroring (header or mapper-controlled).
    pub fn mirroring(&self) -> Mirroring {
        self.mapper.mirroring(self.header_mirroring)
    }

    pub fn irq_pending(&self) -> bool {
        self.mapper.irq_pending()
    }

    pub fn on_ppu_a12_rise(&mut self) {
        self.mapper.on_ppu_a12_rise();
    }

    /// CPU read from cartridge space (`$4020–$FFFF`).
    pub fn cpu_read(&self, addr: u16) -> u8 {
        self.mapper.cpu_read(addr, &self.prg_rom, &self.prg_ram)
    }

    /// CPU write (mapper registers / PRG-RAM). PRG-ROM itself stays read-only.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        self.mapper
            .cpu_write(addr, value, &mut self.prg_ram);
    }

    /// PPU pattern-table read (`$0000–$1FFF`).
    pub fn ppu_read(&self, addr: u16) -> u8 {
        self.mapper.chr_read(addr, &self.chr_mem)
    }

    /// PPU pattern-table write (CHR-RAM only).
    pub fn ppu_write(&mut self, addr: u16, value: u8) {
        self.mapper
            .chr_write(addr, value, &mut self.chr_mem, self.chr_is_ram);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cart::Loader;

    #[test]
    fn nrom_16k_mirrors_prg() {
        let mut prg = vec![0; 0x4000];
        prg[0] = 0xEA;
        let cart = Cartridge::nrom(prg, vec![]);
        assert_eq!(cart.cpu_read(0x8000), 0xEA);
        assert_eq!(cart.cpu_read(0xC000), 0xEA);
    }

    #[test]
    fn nrom_from_smb_rom() {
        let rom = Loader::load("roms/super-mario-bros.nes").unwrap();
        let cart = Cartridge::from_rom(rom).unwrap();
        assert_eq!(cart.mapper_id, 0);
        assert_eq!(cart.prg_rom().len(), 0x8000);
        assert_eq!(cart.chr_mem().len(), 0x2000);
        assert!(!cart.chr_is_ram());
        let lo = cart.cpu_read(0xFFFC) as u16;
        let hi = cart.cpu_read(0xFFFD) as u16;
        let reset = lo | (hi << 8);
        assert!(reset >= 0x8000);
    }

    #[test]
    fn uxrom_from_contra_rom() {
        let rom = Loader::load("roms/contra.nes").unwrap();
        assert_eq!(rom.header.mapper, 2);
        let cart = Cartridge::from_rom(rom).unwrap();
        assert_eq!(cart.mapper_id, 2);
        assert_eq!(cart.prg_rom().len(), 0x20000); // 128 KiB
        assert!(cart.chr_is_ram());
        let lo = cart.cpu_read(0xFFFC) as u16;
        let hi = cart.cpu_read(0xFFFD) as u16;
        let reset = lo | (hi << 8);
        assert!(reset >= 0x8000, "reset=${reset:04X}");
    }

    #[test]
    fn uxrom_bank_switch_changes_8000() {
        let mut prg = vec![0x11u8; 0x4000];
        prg.extend(std::iter::repeat_n(0x22u8, 0x4000));
        let mut cart = Cartridge {
            header_mirroring: Mirroring::Vertical,
            mapper_id: 2,
            prg_rom: prg,
            prg_ram: Vec::new(),
            chr_mem: vec![0; 0x2000],
            chr_is_ram: true,
            mapper: MapperImpl::Uxrom(Uxrom::new(2)),
        };
        assert_eq!(cart.cpu_read(0x8000), 0x11);
        assert_eq!(cart.cpu_read(0xC000), 0x22);
        cart.cpu_write(0x8000, 1);
        assert_eq!(cart.cpu_read(0x8000), 0x22);
        assert_eq!(cart.cpu_read(0xC000), 0x22);
    }

    #[test]
    fn unsupported_mapper_is_explicit() {
        let err = MapperImpl::from_id(5, 0x8000, 0x2000).unwrap_err();
        assert!(matches!(err, NesError::UnsupportedMapper(5)));
    }

    #[test]
    fn mmc1_gets_prg_ram() {
        // Minimal fake: mapper 1, 32K PRG, CHR-RAM
        let mut bytes = vec![0u8; 16 + 0x8000];
        bytes[0..4].copy_from_slice(b"NES\x1a");
        bytes[4] = 2; // 32 KiB PRG
        bytes[5] = 0; // CHR-RAM
        bytes[6] = 0x12; // mapper lo=1, battery
        bytes[7] = 0x00;
        let rom = Loader::parse(&bytes).unwrap();
        assert_eq!(rom.header.mapper, 1);
        let cart = Cartridge::from_rom(rom).unwrap();
        assert_eq!(cart.prg_ram().len(), 0x2000);
        // WRAM round-trip
        // (would need &mut — covered via cpu_write)
    }
}
