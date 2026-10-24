//! Mapper 1 — MMC1 / SxROM ([nesdev](https://www.nesdev.org/wiki/MMC1)).
//!
//! ```text
//!   $6000–$7FFF  8 KiB PRG-RAM
//!   $8000–$FFFF  PRG banks (16 KiB ×2 or 32 KiB) via shift register
//!   $0000–$1FFF  CHR banks (8 KiB or 4 KiB ×2)
//! ```
//!
//! Writes to `$8000–$FFFF` load a 5-bit shift register. Bit 7 resets it.
//! After 5 consecutive writes, the value is copied into the selected register
//! (control / CHR0 / CHR1 / PRG) based on address bits 13–14.

use crate::cart::Mirroring;

#[derive(Debug)]
pub struct Mmc1 {
    shift: u8,
    shift_count: u8,
    /// Control register ($8000–$9FFF): mirroring, PRG mode, CHR mode.
    control: u8,
    chr_bank0: u8,
    chr_bank1: u8,
    prg_bank: u8,
    prg_banks_16k: usize,
    chr_banks_4k: usize,
}

impl Mmc1 {
    pub fn new(prg_len: usize, chr_len: usize) -> Self {
        Self {
            shift: 0,
            shift_count: 0,
            // Power-on: PRG mode 3 (fix last bank at $C000), vertical mirroring-ish.
            control: 0x0C,
            chr_bank0: 0,
            chr_bank1: 0,
            prg_bank: 0,
            prg_banks_16k: (prg_len / 0x4000).max(1),
            chr_banks_4k: (chr_len / 0x1000).max(1),
        }
    }

    pub fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            _ => Mirroring::Horizontal,
        }
    }

    fn prg_mode(&self) -> u8 {
        (self.control >> 2) & 0x03
    }

    fn chr_4k_mode(&self) -> bool {
        self.control & 0x10 != 0
    }

    fn load_register(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            self.shift = 0;
            self.shift_count = 0;
            self.control |= 0x0C; // reset PRG mode to 3
            return;
        }

        self.shift |= (value & 1) << self.shift_count;
        self.shift_count += 1;
        if self.shift_count < 5 {
            return;
        }

        let data = self.shift & 0x1F;
        self.shift = 0;
        self.shift_count = 0;

        match addr & 0xE000 {
            0x8000 => self.control = data,
            0xA000 => self.chr_bank0 = data,
            0xC000 => self.chr_bank1 = data,
            0xE000 => self.prg_bank = data & 0x0F,
            _ => {}
        }
    }

    fn map_prg(&self, addr: u16, prg: &[u8]) -> u8 {
        let bank_16k = match self.prg_mode() {
            0 | 1 => {
                // 32 KiB mode: ignore low bit of prg_bank
                let bank = ((self.prg_bank as usize & 0x0E) >> 1) % (self.prg_banks_16k.max(2) / 2).max(1);
                let half = if addr >= 0xC000 { 1 } else { 0 };
                bank * 2 + half
            }
            2 => {
                // Fix first bank at $8000
                if addr < 0xC000 {
                    0
                } else {
                    (self.prg_bank as usize) % self.prg_banks_16k
                }
            }
            _ => {
                // Mode 3: fix last bank at $C000
                if addr < 0xC000 {
                    (self.prg_bank as usize) % self.prg_banks_16k
                } else {
                    self.prg_banks_16k - 1
                }
            }
        };
        let offset = (bank_16k % self.prg_banks_16k) * 0x4000 + ((addr as usize) & 0x3FFF);
        prg.get(offset).copied().unwrap_or(0)
    }

    fn map_chr(&self, addr: u16) -> usize {
        let a = (addr as usize) & 0x1FFF;
        if self.chr_4k_mode() {
            let bank = if a < 0x1000 {
                self.chr_bank0 as usize
            } else {
                self.chr_bank1 as usize
            } % self.chr_banks_4k;
            bank * 0x1000 + (a & 0x0FFF)
        } else {
            let bank = ((self.chr_bank0 as usize) & 0x1E) % self.chr_banks_4k.max(2);
            // 8 KiB mode: bank is even 4K index
            (bank & !1) * 0x1000 + a
        }
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
            0x8000..=0xFFFF => self.map_prg(addr, prg),
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8, prg_ram: &mut [u8]) {
        match addr {
            0x6000..=0x7FFF => {
                if !prg_ram.is_empty() {
                    let i = (addr as usize - 0x6000) % prg_ram.len();
                    prg_ram[i] = value;
                }
            }
            0x8000..=0xFFFF => self.load_register(addr, value),
            _ => {}
        }
    }

    pub fn chr_read(&self, addr: u16, chr: &[u8]) -> u8 {
        chr.get(self.map_chr(addr)).copied().unwrap_or(0)
    }

    pub fn chr_write(&mut self, addr: u16, value: u8, chr: &mut [u8], chr_is_ram: bool) {
        if !chr_is_ram {
            return;
        }
        let i = self.map_chr(addr);
        if let Some(slot) = chr.get_mut(i) {
            *slot = value;
        }
    }

    pub fn export_state(&self) -> Vec<u8> {
        vec![
            self.shift,
            self.shift_count,
            self.control,
            self.chr_bank0,
            self.chr_bank1,
            self.prg_bank,
        ]
    }

    pub fn import_state(&mut self, data: &[u8]) {
        if data.len() < 6 {
            return;
        }
        self.shift = data[0];
        self.shift_count = data[1].min(5);
        self.control = data[2];
        self.chr_bank0 = data[3];
        self.chr_bank1 = data[4];
        self.prg_bank = data[5];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write5(m: &mut Mmc1, addr: u16, value: u8) {
        let mut ram = [0u8; 0x2000];
        // LSB first
        for i in 0..5 {
            m.cpu_write(addr, (value >> i) & 1, &mut ram);
        }
    }

    #[test]
    fn shift_loads_control() {
        let mut m = Mmc1::new(0x8000, 0x2000);
        write5(&mut m, 0x8000, 0x12); // horizontal + chr 4k + prg mode 0-ish
        assert_eq!(m.control, 0x12);
        assert_eq!(m.mirroring(), Mirroring::Vertical); // bits 0-1 = 2
    }

    #[test]
    fn bit7_resets_shift() {
        let mut m = Mmc1::new(0x8000, 0);
        let mut ram = [0u8; 0x2000];
        m.cpu_write(0x8000, 1, &mut ram);
        m.cpu_write(0x8000, 0x80, &mut ram);
        assert_eq!(m.shift_count, 0);
        assert_eq!(m.control & 0x0C, 0x0C);
    }
}
