//! Mapper 7 — AxROM ([nesdev](https://www.nesdev.org/wiki/AxROM)).
//!
//! ```text
//!   $8000–$FFFF  32 KiB switchable PRG bank
//!   $0000–$1FFF  8 KiB CHR-RAM
//! ```
//!
//! Bit 4 of the bank register selects single-screen nametable mirroring
//! (lower vs upper CIRAM page).

use crate::cart::Mirroring;

#[derive(Debug)]
pub struct Axrom {
    prg_banks_32k: usize,
    prg_bank: usize,
    /// false = SingleScreenLower, true = SingleScreenUpper
    upper_screen: bool,
}

impl Axrom {
    pub fn new(prg_len: usize) -> Self {
        Self {
            prg_banks_32k: (prg_len / 0x8000).max(1),
            prg_bank: 0,
            upper_screen: false,
        }
    }

    fn bank_mask(&self) -> usize {
        self.prg_banks_32k.next_power_of_two().saturating_sub(1)
    }

    pub fn mirroring(&self) -> Mirroring {
        if self.upper_screen {
            Mirroring::SingleScreenUpper
        } else {
            Mirroring::SingleScreenLower
        }
    }

    pub fn cpu_read(&self, addr: u16, prg: &[u8], _prg_ram: &[u8]) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = self.prg_bank % self.prg_banks_32k;
                let offset = bank * 0x8000 + ((addr as usize) & 0x7FFF);
                prg.get(offset).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8, _prg_ram: &mut [u8]) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.prg_bank = (value as usize) & self.bank_mask();
            if self.prg_bank >= self.prg_banks_32k {
                self.prg_bank %= self.prg_banks_32k;
            }
            self.upper_screen = value & 0x10 != 0;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_and_mirroring() {
        let mut m = Axrom::new(0x10000); // 2 × 32 KiB
        let mut ram = [];
        m.cpu_write(0x8000, 0x11, &mut ram);
        assert_eq!(m.prg_bank, 1);
        assert!(m.upper_screen);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
    }
}
