//! Mapper 4 — MMC3 / TxROM ([nesdev](https://www.nesdev.org/wiki/MMC3)).
//!
//! ```text
//!   $6000–$7FFF  8 KiB PRG-RAM
//!   $8000–$9FFF  Bank select / bank data
//!   $A000–$BFFF  Mirroring / PRG-RAM protect
//!   $C000–$DFFF  IRQ latch / reload
//!   $E000–$FFFF  IRQ disable / enable
//! ```
//!
//! Scanline IRQ is clocked by rising edges on PPU A12 (CHR fetches that
//! cross into `$1000–$1FFF`). Call [`Mmc3::on_ppu_a12_rise`] from the PPU.

use crate::cart::Mirroring;

#[derive(Debug)]
pub struct Mmc3 {
    bank_select: u8,
    banks: [u8; 8],
    mirroring_bit: bool,
    prg_ram_enable: bool,
    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq_pending: bool,
    prg_banks_8k: usize,
    chr_banks_1k: usize,
}

impl Mmc3 {
    pub fn new(prg_len: usize, chr_len: usize) -> Self {
        Self {
            bank_select: 0,
            banks: [0; 8],
            mirroring_bit: false,
            prg_ram_enable: true,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_pending: false,
            prg_banks_8k: (prg_len / 0x2000).max(1),
            chr_banks_1k: (chr_len / 0x400).max(1),
        }
    }

    pub fn mirroring(&self, four_screen: bool) -> Mirroring {
        if four_screen {
            Mirroring::FourScreen
        } else if self.mirroring_bit {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        }
    }

    pub fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    /// Rising edge on PPU address line A12 (CHR `$1xxx`).
    pub fn on_ppu_a12_rise(&mut self) {
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
        }
        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }

    fn prg_mode_swap(&self) -> bool {
        self.bank_select & 0x40 != 0
    }

    fn chr_mode_swap(&self) -> bool {
        self.bank_select & 0x80 != 0
    }

    fn map_prg_bank(&self, slot: usize) -> usize {
        // slots: 0 = $8000, 1 = $A000, 2 = $C000, 3 = $E000 (fixed last-1 / last)
        let last = self.prg_banks_8k - 1;
        let last2 = last.saturating_sub(1);
        let r6 = (self.banks[6] as usize) % self.prg_banks_8k;
        let r7 = (self.banks[7] as usize) % self.prg_banks_8k;
        if !self.prg_mode_swap() {
            match slot {
                0 => r6,
                1 => r7,
                2 => last2,
                _ => last,
            }
        } else {
            match slot {
                0 => last2,
                1 => r7,
                2 => r6,
                _ => last,
            }
        }
    }

    fn map_chr_1k(&self, bank_1k: usize) -> usize {
        bank_1k % self.chr_banks_1k
    }

    fn chr_offset(&self, addr: u16) -> usize {
        let a = (addr as usize) & 0x1FFF;
        let page = a / 0x400; // 0..7
        let (r0, r1, r2, r3, r4, r5) = (
            self.banks[0] as usize & 0xFE,
            self.banks[1] as usize & 0xFE,
            self.banks[2] as usize,
            self.banks[3] as usize,
            self.banks[4] as usize,
            self.banks[5] as usize,
        );

        let bank = if !self.chr_mode_swap() {
            match page {
                0 => self.map_chr_1k(r0),
                1 => self.map_chr_1k(r0 + 1),
                2 => self.map_chr_1k(r1),
                3 => self.map_chr_1k(r1 + 1),
                4 => self.map_chr_1k(r2),
                5 => self.map_chr_1k(r3),
                6 => self.map_chr_1k(r4),
                _ => self.map_chr_1k(r5),
            }
        } else {
            match page {
                0 => self.map_chr_1k(r2),
                1 => self.map_chr_1k(r3),
                2 => self.map_chr_1k(r4),
                3 => self.map_chr_1k(r5),
                4 => self.map_chr_1k(r0),
                5 => self.map_chr_1k(r0 + 1),
                6 => self.map_chr_1k(r1),
                _ => self.map_chr_1k(r1 + 1),
            }
        };
        bank * 0x400 + (a & 0x3FF)
    }

    pub fn cpu_read(&self, addr: u16, prg: &[u8], prg_ram: &[u8]) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if !self.prg_ram_enable || prg_ram.is_empty() {
                    0
                } else {
                    prg_ram[(addr as usize - 0x6000) % prg_ram.len()]
                }
            }
            0x8000..=0xFFFF => {
                let slot = ((addr as usize) - 0x8000) / 0x2000;
                let bank = self.map_prg_bank(slot);
                let offset = bank * 0x2000 + ((addr as usize) & 0x1FFF);
                prg.get(offset).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8, prg_ram: &mut [u8]) {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enable && !prg_ram.is_empty() {
                    let i = (addr as usize - 0x6000) % prg_ram.len();
                    prg_ram[i] = value;
                }
            }
            0x8000..=0x9FFF => {
                if addr & 1 == 0 {
                    self.bank_select = value;
                } else {
                    let reg = (self.bank_select & 0x07) as usize;
                    self.banks[reg] = value;
                }
            }
            0xA000..=0xBFFF => {
                if addr & 1 == 0 {
                    self.mirroring_bit = value & 1 != 0;
                } else {
                    // Bit 7 = enable, bit 6 = write protect (ignored for learning).
                    self.prg_ram_enable = value & 0x80 != 0;
                }
            }
            0xC000..=0xDFFF => {
                if addr & 1 == 0 {
                    self.irq_latch = value;
                } else {
                    self.irq_reload = true;
                }
            }
            0xE000..=0xFFFF => {
                if addr & 1 == 0 {
                    self.irq_enabled = false;
                    self.irq_pending = false;
                } else {
                    self.irq_enabled = true;
                }
            }
            _ => {}
        }
    }

    pub fn chr_read(&self, addr: u16, chr: &[u8]) -> u8 {
        chr.get(self.chr_offset(addr)).copied().unwrap_or(0)
    }

    pub fn chr_write(&mut self, addr: u16, value: u8, chr: &mut [u8], chr_is_ram: bool) {
        if !chr_is_ram {
            return;
        }
        let i = self.chr_offset(addr);
        if let Some(slot) = chr.get_mut(i) {
            *slot = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irq_counts_down_on_a12() {
        let mut m = Mmc3::new(0x8000, 0x2000);
        let mut ram = [0u8; 0x2000];
        m.cpu_write(0xC000, 2, &mut ram); // latch = 2
        m.cpu_write(0xC001, 0, &mut ram); // reload
        m.cpu_write(0xE001, 0, &mut ram); // enable
        m.on_ppu_a12_rise(); // load 2
        assert!(!m.irq_pending());
        m.on_ppu_a12_rise(); // 2 -> 1
        assert!(!m.irq_pending());
        m.on_ppu_a12_rise(); // 1 -> 0 → IRQ
        assert!(m.irq_pending());
    }

    #[test]
    fn bank_select_writes() {
        let mut m = Mmc3::new(0x10000, 0x2000);
        let mut ram = [0u8; 0x2000];
        m.cpu_write(0x8000, 6, &mut ram);
        m.cpu_write(0x8001, 3, &mut ram);
        assert_eq!(m.banks[6], 3);
    }
}
