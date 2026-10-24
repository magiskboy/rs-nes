//! Mapper 2 — UxROM / UNROM ([nesdev](https://www.nesdev.org/wiki/UxROM)).
//!
//! ```text
//!   $8000–$BFFF  16 KiB switchable PRG bank
//!   $C000–$FFFF  16 KiB fixed to the *last* PRG bank (reset/NMI vectors live here)
//! ```
//!
//! Any CPU write to `$8000–$FFFF` selects the switchable bank (low bits of the
//! value). Commercial UNROM boards also AND that value with the ROM byte at the
//! address ([bus conflicts](https://www.nesdev.org/wiki/UxROM#Bus_conflicts));
//! [`Cartridge::cpu_write`](crate::cart::mapper::Cartridge::cpu_write) applies that
//! for mapper 2 (needed by Jackal when leaving bank 7).
//! CHR is almost always 8 KiB RAM (handled by [`crate::cart::mapper::Cartridge`]).

#[derive(Debug)]
pub struct Uxrom {
    /// Number of 16 KiB PRG banks on the cart.
    bank_count: usize,
    /// Currently selected bank for `$8000–$BFFF`.
    prg_bank: usize,
}

impl Uxrom {
    pub fn new(bank_count: usize) -> Self {
        Self {
            bank_count: bank_count.max(1),
            prg_bank: 0,
        }
    }

    fn bank_mask(&self) -> usize {
        // Hardware typically AND with (banks-1) when banks is a power of two.
        self.bank_count.next_power_of_two().saturating_sub(1)
    }

    fn map_switchable(&self, addr: u16, prg: &[u8]) -> u8 {
        let bank = self.prg_bank % self.bank_count;
        let offset = (bank * 0x4000) + ((addr as usize) & 0x3FFF);
        prg.get(offset).copied().unwrap_or(0)
    }

    fn map_fixed_last(&self, addr: u16, prg: &[u8]) -> u8 {
        let bank = self.bank_count - 1;
        let offset = (bank * 0x4000) + ((addr as usize) & 0x3FFF);
        prg.get(offset).copied().unwrap_or(0)
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
            0x8000..=0xBFFF => self.map_switchable(addr, prg),
            0xC000..=0xFFFF => self.map_fixed_last(addr, prg),
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8, prg_ram: &mut [u8]) {
        if (0x6000..=0x7FFF).contains(&addr) && !prg_ram.is_empty() {
            let i = (addr as usize - 0x6000) % prg_ram.len();
            prg_ram[i] = value;
            return;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            self.prg_bank = (value as usize) & self.bank_mask();
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

    pub fn export_state(&self) -> Vec<u8> {
        (self.prg_bank as u16).to_le_bytes().to_vec()
    }

    pub fn import_state(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            let bank = u16::from_le_bytes([data[0], data[1]]) as usize;
            self.prg_bank = bank % self.bank_count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_bank_with_mask() {
        let mut m = Uxrom::new(8);
        let mut ram = [];
        m.cpu_write(0x8000, 0xFF, &mut ram);
        assert_eq!(m.prg_bank, 7); // mask 7 for 8 banks
    }
}
