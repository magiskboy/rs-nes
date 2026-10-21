//! Mapper 0 — NROM ([nesdev](https://www.nesdev.org/wiki/NROM)).
//!
//! ```text
//!   $6000–$7FFF  PRG-RAM (optional; rare battery boards)
//!   $8000–$BFFF  PRG-ROM (16 KiB mirrored, or low half of 32 KiB)
//!   $C000–$FFFF  PRG-ROM (mirror of low, or high half of 32 KiB)
//!   $0000–$1FFF  CHR 8 KiB flat (ROM or RAM)
//! ```

#[derive(Debug, Default)]
pub struct Nrom;

impl Nrom {
    pub fn new() -> Self {
        Self
    }

    pub fn cpu_read(&self, addr: u16, prg: &[u8], prg_ram: &[u8]) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if prg_ram.is_empty() {
                    0
                } else {
                    prg_ram[(addr as usize - 0x6000) % prg_ram.len()]
                }
            }
            0x8000..=0xFFFF => {
                let offset = (addr as usize) - 0x8000;
                match prg.len() {
                    0x4000 => prg[offset % 0x4000],
                    n => prg[offset % n.min(0x8000)],
                }
            }
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8, prg_ram: &mut [u8]) {
        if (0x6000..=0x7FFF).contains(&addr) && !prg_ram.is_empty() {
            let i = (addr as usize - 0x6000) % prg_ram.len();
            prg_ram[i] = value;
        }
    }

    pub fn chr_read(&self, addr: u16, chr: &[u8]) -> u8 {
        chr.get((addr as usize) & 0x1FFF).copied().unwrap_or(0)
    }

    pub fn chr_write(&mut self, addr: u16, value: u8, chr: &mut [u8], chr_is_ram: bool) {
        if !chr_is_ram {
            return;
        }
        let i = (addr as usize) & 0x1FFF;
        if let Some(slot) = chr.get_mut(i) {
            *slot = value;
        }
    }
}
