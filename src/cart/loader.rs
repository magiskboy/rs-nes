use std::{fs::File, io::Read};

use crate::error::NesError;

/// iNES header fields (https://www.nesdev.org/wiki/INES).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NesHeader {
    /// PRG-ROM size in 16 KiB units (byte 4).
    pub prg_rom_banks: u8,
    /// CHR-ROM size in 8 KiB units (byte 5). `0` means the board uses CHR-RAM.
    pub chr_rom_banks: u8,

    pub mirroring: Mirroring,
    pub battery_backed: bool,
    pub has_trainer: bool,
    /// Mapper number (flags6 high nibble | flags7 high nibble).
    pub mapper: u8,

    pub vs_unisystem: bool,
    pub play_choice_10: bool,
    /// `true` when flags7 indicates the NES 2.0 extension (not fully parsed yet).
    pub nes20: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    /// Flags6 bit0 = 0: vertical arrangement / horizontally mirrored.
    Horizontal,
    /// Flags6 bit0 = 1: horizontal arrangement / vertically mirrored.
    Vertical,
    /// Flags6 bit3 set: four-screen VRAM (overrides bit0). Needs 4 KiB CIRAM.
    FourScreen,
    /// AxROM / MMC1: all nametable fetches use the lower 1 KiB of CIRAM.
    SingleScreenLower,
    /// AxROM / MMC1: all nametable fetches use the upper 1 KiB of CIRAM.
    SingleScreenUpper,
}

/// A loaded cartridge image: header + optional trainer + PRG/CHR payloads.
#[derive(Debug)]
pub struct Rom {
    pub header: NesHeader,
    /// 512-byte trainer at `$7000–$71FF` if present.
    pub trainer: Option<Vec<u8>>,
    /// PRG-ROM (`prg_rom_banks × 16 KiB`).
    pub prg_rom: Vec<u8>,
    /// CHR-ROM (`chr_rom_banks × 8 KiB`). Empty when the cart uses CHR-RAM.
    pub chr_rom: Vec<u8>,
}

pub struct Loader;

impl Loader {
    /// Step 1+2: read the file, parse the iNES header, split trainer/PRG/CHR.
    pub fn load(path: &str) -> Result<Rom, NesError> {
        let mut file = File::open(path).map_err(|_| NesError::GenericError)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|_| NesError::GenericError)?;
        Self::parse(&bytes)
    }

    /// Read only the iNES mapper number (16-byte header). `None` if not a valid NES file.
    pub fn peek_mapper(path: &std::path::Path) -> Option<u8> {
        let mut file = File::open(path).ok()?;
        let mut hdr = [0u8; 16];
        use std::io::Read;
        file.read_exact(&mut hdr).ok()?;
        if &hdr[0..4] != b"NES\x1a" {
            return None;
        }
        let mapper_lo = hdr[6] >> 4;
        let mapper_hi = hdr[7] & 0xF0;
        Some(mapper_hi | mapper_lo)
    }

    /// Parse an in-memory iNES image (useful for tests).
    pub fn parse(bytes: &[u8]) -> Result<Rom, NesError> {
        if bytes.len() < 16 {
            return Err(NesError::GenericError);
        }
        if &bytes[0..4] != b"NES\x1a" {
            return Err(NesError::GenericError);
        }

        let header = Self::parse_header(bytes)?;

        // Layout after 16-byte header:
        // [trainer 512?] [PRG banks×16KiB] [CHR banks×8KiB]
        let mut offset = 16usize;
        let trainer = if header.has_trainer {
            let end = offset + 512;
            let data = bytes.get(offset..end).ok_or(NesError::GenericError)?.to_vec();
            offset = end;
            Some(data)
        } else {
            None
        };

        let prg_len = header.prg_rom_size();
        let prg_end = offset + prg_len;
        let prg_rom = bytes
            .get(offset..prg_end)
            .ok_or(NesError::GenericError)?
            .to_vec();
        offset = prg_end;

        let chr_len = header.chr_rom_size();
        let chr_rom = if chr_len == 0 {
            Vec::new()
        } else {
            let chr_end = offset + chr_len;
            bytes
                .get(offset..chr_end)
                .ok_or(NesError::GenericError)?
                .to_vec()
        };

        Ok(Rom {
            header,
            trainer,
            prg_rom,
            chr_rom,
        })
    }

    fn parse_header(bytes: &[u8]) -> Result<NesHeader, NesError> {
        let prg_rom_banks = bytes[4];
        let chr_rom_banks = bytes[5];
        let flags6 = bytes[6];
        let flags7 = bytes[7];

        let has_trainer = flags6 & 0x04 != 0;
        let battery_backed = flags6 & 0x02 != 0;
        let four_screen = flags6 & 0x08 != 0;

        let mirroring = if four_screen {
            Mirroring::FourScreen
        } else if flags6 & 0x01 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        let mapper_lo = flags6 >> 4;
        let mapper_hi = flags7 & 0xF0;
        let mapper = mapper_hi | mapper_lo;

        let vs_unisystem = flags7 & 0x01 != 0;
        let play_choice_10 = flags7 & 0x02 != 0;
        // NES 2.0 when bits 3-2 of flags7 == 0b10
        let nes20 = flags7 & 0x0C == 0x08;

        if prg_rom_banks == 0 {
            return Err(NesError::GenericError);
        }

        Ok(NesHeader {
            prg_rom_banks,
            chr_rom_banks,
            mirroring,
            battery_backed,
            has_trainer,
            mapper,
            vs_unisystem,
            play_choice_10,
            nes20,
        })
    }
}

impl NesHeader {
    pub fn prg_rom_size(&self) -> usize {
        self.prg_rom_banks as usize * 16 * 1024
    }

    pub fn chr_rom_size(&self) -> usize {
        self.chr_rom_banks as usize * 8 * 1024
    }

    /// Board has CHR-RAM when CHR-ROM size is 0.
    pub fn has_chr_ram(&self) -> bool {
        self.chr_rom_banks == 0
    }
}

impl Rom {
    /// Step 2 accessor: PRG-ROM bytes.
    pub fn prg(&self) -> &[u8] {
        &self.prg_rom
    }

    /// Step 2 accessor: CHR-ROM bytes (empty if CHR-RAM).
    pub fn chr(&self) -> &[u8] {
        &self.chr_rom
    }

    pub fn trainer(&self) -> Option<&[u8]> {
        self.trainer.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_ines(prg_banks: u8, chr_banks: u8, flags6: u8, flags7: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; 16];
        bytes[0..4].copy_from_slice(b"NES\x1a");
        bytes[4] = prg_banks;
        bytes[5] = chr_banks;
        bytes[6] = flags6;
        bytes[7] = flags7;

        let trainer = if flags6 & 0x04 != 0 { 512 } else { 0 };
        let prg_len = prg_banks as usize * 16 * 1024;
        let chr_len = chr_banks as usize * 8 * 1024;
        bytes.resize(16 + trainer + prg_len + chr_len, 0xAA);

        if trainer != 0 {
            bytes[16..16 + trainer].fill(0x71);
        }
        let prg_start = 16 + trainer;
        bytes[prg_start] = 0xEA; // distinctive PRG byte
        if chr_len != 0 {
            bytes[prg_start + prg_len] = 0xC1; // distinctive CHR byte
        }
        bytes
    }

    #[test]
    fn parse_header_mapper_and_mirroring() {
        // mapper = 0x03 << 4 | 0x01 = 0x31? flags6 high=1, flags7 high=3 → mapper 0x31
        let bytes = minimal_ines(1, 1, 0x11, 0x30); // vertical mirror, mapper lo=1, hi=3
        let rom = Loader::parse(&bytes).unwrap();
        assert_eq!(rom.header.mirroring, Mirroring::Vertical);
        assert_eq!(rom.header.mapper, 0x31);
        assert!(!rom.header.nes20);
        assert_eq!(rom.prg().len(), 16 * 1024);
        assert_eq!(rom.chr().len(), 8 * 1024);
        assert_eq!(rom.prg()[0], 0xEA);
        assert_eq!(rom.chr()[0], 0xC1);
    }

    #[test]
    fn parse_four_screen_and_trainer() {
        let bytes = minimal_ines(1, 0, 0x0C, 0x00); // trainer + four-screen, CHR-RAM
        let rom = Loader::parse(&bytes).unwrap();
        assert_eq!(rom.header.mirroring, Mirroring::FourScreen);
        assert!(rom.header.has_trainer);
        assert!(rom.header.has_chr_ram());
        assert_eq!(rom.trainer().unwrap().len(), 512);
        assert!(rom.chr().is_empty());
        assert_eq!(rom.prg().len(), 16 * 1024);
    }

    #[test]
    fn reject_bad_magic() {
        let mut bytes = minimal_ines(1, 1, 0, 0);
        bytes[0] = b'X';
        assert!(Loader::parse(&bytes).is_err());
    }

    #[test]
    fn smb_header_prg_chr() {
        let rom = Loader::load("roms/super-mario-bros.nes").expect("load");
        assert_eq!(rom.header.prg_rom_banks, 2);
        assert_eq!(rom.header.chr_rom_banks, 1);
        assert_eq!(rom.header.mapper, 0);
        assert_eq!(rom.header.mirroring, Mirroring::Vertical);
        assert_eq!(rom.prg().len(), 0x8000);
        assert_eq!(rom.chr().len(), 0x2000);
        assert!(rom.trainer().is_none());
    }

}
