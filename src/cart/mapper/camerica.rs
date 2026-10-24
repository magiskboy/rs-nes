//! Mapper 71 — Camerica / Codemasters BF9093 ([nesdev](https://www.nesdev.org/wiki/INES_Mapper_071)).
//!
//! ```text
//!   $8000–$BFFF  16 KiB switchable PRG bank
//!   $C000–$FFFF  16 KiB fixed to the last PRG bank
//! ```
//!
//! Bank select is written at `$C000–$FFFF` (unlike UxROM's `$8000–$FFFF`).
//! Most boards use hardwired H/V mirroring; Fire Hawk (BF9097) selects
//! single-screen CIRAM via writes to `$9000–$9FFF` (bit 4). For compatibility
//! without submappers we keep header mirroring until that range is written.
//! CHR is 8 KiB RAM. No bus conflicts.

use crate::cart::Mirroring;

#[derive(Debug, Clone)]
pub struct Camerica {
    bank_count: usize,
    prg_bank: usize,
    /// `None` = header mirroring; `Some(upper)` after Fire Hawk `$9000` write.
    single_screen_upper: Option<bool>,
}

impl Camerica {
    pub fn new(bank_count: usize) -> Self {
        Self {
            bank_count: bank_count.max(1),
            prg_bank: 0,
            single_screen_upper: None,
        }
    }

    fn bank_mask(&self) -> usize {
        self.bank_count.next_power_of_two().saturating_sub(1)
    }

    pub fn mirroring(&self, header: Mirroring) -> Mirroring {
        match self.single_screen_upper {
            Some(true) => Mirroring::SingleScreenUpper,
            Some(false) => Mirroring::SingleScreenLower,
            None => header,
        }
    }

    fn map_switchable(&self, addr: u16, prg: &[u8]) -> u8 {
        let bank = self.prg_bank % self.bank_count;
        let offset = bank * 0x4000 + ((addr as usize) & 0x3FFF);
        prg.get(offset).copied().unwrap_or(0)
    }

    fn map_fixed_last(&self, addr: u16, prg: &[u8]) -> u8 {
        let bank = self.bank_count - 1;
        let offset = bank * 0x4000 + ((addr as usize) & 0x3FFF);
        prg.get(offset).copied().unwrap_or(0)
    }

    pub fn cpu_read(&self, addr: u16, prg: &[u8], _prg_ram: &[u8]) -> u8 {
        match addr {
            0x8000..=0xBFFF => self.map_switchable(addr, prg),
            0xC000..=0xFFFF => self.map_fixed_last(addr, prg),
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8, _prg_ram: &mut [u8]) {
        match addr {
            // Fire Hawk mirroring; ignore `$8000–$8FFF` (other games write 0 there).
            0x9000..=0x9FFF => {
                self.single_screen_upper = Some(value & 0x10 != 0);
            }
            0xC000..=0xFFFF => {
                self.prg_bank = (value as usize) & self.bank_mask();
                if self.prg_bank >= self.bank_count {
                    self.prg_bank %= self.bank_count;
                }
            }
            _ => {}
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
        vec![
            self.prg_bank as u8,
            match self.single_screen_upper {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            },
        ]
    }

    pub fn import_state(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.prg_bank = (data[0] as usize) % self.bank_count;
        if data.len() >= 2 {
            self.single_screen_upper = match data[1] {
                1 => Some(false),
                2 => Some(true),
                _ => None,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_select_at_c000() {
        let mut m = Camerica::new(8);
        let mut ram = [];
        m.cpu_write(0xC000, 0x03, &mut ram);
        assert_eq!(m.prg_bank, 3);
        // Writes to $8000 must not change bank (unlike UxROM).
        m.cpu_write(0x8000, 0x07, &mut ram);
        assert_eq!(m.prg_bank, 3);
    }

    #[test]
    fn fire_hawk_mirroring_via_9000() {
        let mut m = Camerica::new(4);
        let mut ram = [];
        assert_eq!(m.mirroring(Mirroring::Vertical), Mirroring::Vertical);
        m.cpu_write(0x9000, 0x10, &mut ram);
        assert_eq!(m.mirroring(Mirroring::Vertical), Mirroring::SingleScreenUpper);
        m.cpu_write(0x9000, 0x00, &mut ram);
        assert_eq!(m.mirroring(Mirroring::Vertical), Mirroring::SingleScreenLower);
    }
}
