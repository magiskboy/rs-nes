//! Mapper 5 — MMC5 / ExROM ([nesdev](https://www.nesdev.org/wiki/MMC5)).
//!
//! Game-compatible core for Castlevania III, Just Breed, Uncharted Waters, …
//!
//! ```text
//!   $5000–$5015  Expansion audio (ignored here)
//!   $5100–$5107  PRG/CHR mode, protect, ExRAM mode, NT map, fill
//!   $5113–$5117  PRG banks ($6000 + $8000–$FFFF)
//!   $5120–$5130  CHR banks (sprite set A / BG set B) + upper bits
//!   $5203–$5204  Scanline IRQ compare / status
//!   $5205–$5206  8×8 → 16 multiplier
//!   $5C00–$5FFF  1 KiB ExRAM
//! ```
//!
//! Not emulated: vertical split ($5200–$5202), MMC5 audio, MMC5A extras.

use crate::cart::Mirroring;

/// Where a nametable address maps for this board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NtSource {
    /// Offset into the console's CIRAM (`0x000` / `0x400`).
    Ciram(u16),
    /// Mapper supplies the byte (ExRAM or fill mode).
    Mapper,
}

#[derive(Debug)]
pub struct Mmc5 {
    prg_mode: u8,
    chr_mode: u8,
    ram_protect1: u8,
    ram_protect2: u8,
    exram_mode: u8,
    nt_map: u8,
    fill_tile: u8,
    fill_attr: u8,
    /// `$5113`–`$5117`.
    prg_banks: [u8; 5],
    /// Sprite / last-written set (`$5120`–`$5127`), 10-bit.
    chr_sprite: [u16; 8],
    /// Background set (`$5128`–`$512B`), 10-bit.
    chr_bg: [u16; 4],
    chr_upper: u16,
    /// Last CHR bank write targeted the BG set.
    last_set_bg: bool,
    sprites_8x16: bool,
    rendering: bool,
    /// ExGrafix: ExRAM byte latched on nametable tile fetch.
    ex_latch: u8,
    exram: [u8; 0x400],
    in_frame: bool,
    scanline_counter: u8,
    irq_compare: u8,
    irq_enabled: bool,
    irq_pending: bool,
    multiplicand: u8,
    multiplier: u8,
    prg_banks_8k: usize,
    chr_len: usize,
}

impl Mmc5 {
    pub fn new(prg_len: usize, chr_len: usize) -> Self {
        Self {
            // Power-on defaults match hardware / Koei games that never write $5100.
            prg_mode: 3,
            chr_mode: 3,
            ram_protect1: 0x02,
            ram_protect2: 0x01,
            exram_mode: 0,
            nt_map: 0,
            fill_tile: 0,
            fill_attr: 0,
            prg_banks: [0, 0, 0, 0, 0xFF],
            chr_sprite: [0; 8],
            chr_bg: [0; 4],
            chr_upper: 0,
            last_set_bg: false,
            sprites_8x16: false,
            rendering: false,
            ex_latch: 0,
            exram: [0; 0x400],
            in_frame: false,
            scanline_counter: 0,
            irq_compare: 0,
            irq_enabled: false,
            irq_pending: false,
            multiplicand: 0xFF,
            multiplier: 0xFF,
            prg_banks_8k: (prg_len / 0x2000).max(1),
            chr_len: chr_len.max(0x2000),
        }
    }

    pub fn mirroring(&self, _header: Mirroring) -> Mirroring {
        // Nametable routing goes through [`Self::nametable_source`].
        Mirroring::Vertical
    }

    pub fn irq_pending(&self) -> bool {
        self.irq_pending && self.irq_enabled
    }

    /// Snoop PPUCTRL / PPUMASK (fully decoded `$2000` / `$2001` only).
    pub fn notify_ppu_reg(&mut self, reg: u16, value: u8) {
        match reg & 7 {
            0 => self.sprites_8x16 = value & 0x20 != 0,
            1 => {
                let on = value & 0x18 != 0;
                if self.rendering && !on {
                    self.end_frame();
                }
                self.rendering = on;
            }
            _ => {}
        }
    }

    /// Called once at the start of each rendered scanline (`0..=239`).
    ///
    /// Models the FCEUX / hardware “in-frame + up-counter” IRQ (Castlevania III).
    pub fn on_scanline(&mut self, scanline: i16) {
        if !self.rendering || !(0..240).contains(&scanline) {
            self.end_frame();
            return;
        }
        if !self.in_frame {
            self.in_frame = true;
            self.scanline_counter = 0;
            self.irq_pending = false;
        } else {
            self.scanline_counter = self.scanline_counter.wrapping_add(1);
            if self.irq_compare != 0 && self.scanline_counter == self.irq_compare {
                self.irq_pending = true;
            }
        }
    }

    fn end_frame(&mut self) {
        self.in_frame = false;
        self.scanline_counter = 0;
        // Pending bit stays until $5204 is read; in-frame bit clears.
    }

    fn ram_writable(&self) -> bool {
        self.ram_protect1 & 3 == 2 && self.ram_protect2 & 3 == 1
    }

    fn prg_rom_off(&self, bank_8k: usize, addr: u16) -> usize {
        let bank = bank_8k % self.prg_banks_8k;
        bank * 0x2000 + (addr as usize & 0x1FFF)
    }

    fn prg_ram_off(prg_ram: &[u8], bank: u8, addr: u16) -> Option<usize> {
        if prg_ram.is_empty() {
            return None;
        }
        let banks = (prg_ram.len() / 0x2000).max(1);
        let b = (bank as usize & 0x0F) % banks;
        Some(b * 0x2000 + (addr as usize & 0x1FFF))
    }

    /// `(is_rom, offset)` for `$6000–$FFFF`.
    fn prg_resolve(&self, addr: u16, prg_ram: &[u8]) -> (bool, usize) {
        let rom8 = |reg: u8, a: u16| {
            (true, self.prg_rom_off((reg & 0x7F) as usize, a))
        };
        let ram8 = |reg: u8, a: u16| {
            (
                false,
                Self::prg_ram_off(prg_ram, reg, a).unwrap_or(0),
            )
        };
        let split8 = |reg: u8, a: u16| {
            if reg & 0x80 != 0 {
                rom8(reg, a)
            } else {
                ram8(reg, a)
            }
        };
        let rom16 = |reg: u8, a: u16| {
            let banks = (self.prg_banks_8k / 2).max(1);
            let bank = ((reg as usize & 0x7F) >> 1) % banks;
            (true, bank * 0x4000 + (a as usize & 0x3FFF))
        };
        let ram16 = |reg: u8, a: u16| {
            let banks = (prg_ram.len() / 0x4000).max(1);
            let bank = ((reg as usize & 0x0F) >> 1) % banks;
            (false, bank * 0x4000 + (a as usize & 0x3FFF))
        };
        let split16 = |reg: u8, a: u16| {
            if reg & 0x80 != 0 {
                rom16(reg, a)
            } else {
                ram16(reg, a)
            }
        };

        if addr < 0x8000 {
            return ram8(self.prg_banks[0], addr);
        }

        match self.prg_mode & 3 {
            0 => {
                // 32 KiB from $5117 (bits 6–2 → 32K bank).
                let banks = (self.prg_banks_8k / 4).max(1);
                let bank = ((self.prg_banks[4] as usize & 0x7F) >> 2) % banks;
                (true, bank * 0x8000 + (addr as usize & 0x7FFF))
            }
            1 => match addr {
                0x8000..=0xBFFF => split16(self.prg_banks[2], addr),
                _ => rom16(self.prg_banks[4], addr),
            },
            2 => match addr {
                0x8000..=0xBFFF => split16(self.prg_banks[2], addr),
                0xC000..=0xDFFF => split8(self.prg_banks[3], addr),
                _ => rom8(self.prg_banks[4], addr),
            },
            _ => match addr {
                0x8000..=0x9FFF => split8(self.prg_banks[1], addr),
                0xA000..=0xBFFF => split8(self.prg_banks[2], addr),
                0xC000..=0xDFFF => split8(self.prg_banks[3], addr),
                _ => rom8(self.prg_banks[4], addr),
            },
        }
    }

    fn chr_banks_of(&self, unit: usize) -> usize {
        (self.chr_len / unit).max(1)
    }

    fn chr_offset(&self, addr: u16, use_bg_set: bool) -> usize {
        let a = addr as usize & 0x1FFF;
        if use_bg_set {
            match self.chr_mode & 3 {
                0 => {
                    let b = self.chr_bg[3] as usize % self.chr_banks_of(0x2000);
                    b * 0x2000 + a
                }
                1 => {
                    let b = self.chr_bg[3] as usize % self.chr_banks_of(0x1000);
                    b * 0x1000 + (a & 0x0FFF)
                }
                2 => {
                    let reg = self.chr_bg[((a >> 11) & 1) * 2 + 1];
                    let b = reg as usize % self.chr_banks_of(0x800);
                    b * 0x800 + (a & 0x7FF)
                }
                _ => {
                    let reg = self.chr_bg[(a >> 10) & 3];
                    let b = reg as usize % self.chr_banks_of(0x400);
                    b * 0x400 + (a & 0x3FF)
                }
            }
        } else {
            match self.chr_mode & 3 {
                0 => {
                    let b = self.chr_sprite[7] as usize % self.chr_banks_of(0x2000);
                    b * 0x2000 + a
                }
                1 => {
                    let reg = self.chr_sprite[((a >> 12) & 1) * 4 + 3];
                    let b = reg as usize % self.chr_banks_of(0x1000);
                    b * 0x1000 + (a & 0x0FFF)
                }
                2 => {
                    let reg = self.chr_sprite[((a >> 11) & 3) * 2 + 1];
                    let b = reg as usize % self.chr_banks_of(0x800);
                    b * 0x800 + (a & 0x7FF)
                }
                _ => {
                    let reg = self.chr_sprite[(a >> 10) & 7];
                    let b = reg as usize % self.chr_banks_of(0x400);
                    b * 0x400 + (a & 0x3FF)
                }
            }
        }
    }

    pub fn nametable_source(&mut self, addr: u16) -> NtSource {
        let a = addr & 0x2FFF;
        if self.exram_mode == 1 && self.in_frame && self.rendering {
            if a & 0x3FF >= 0x3C0 {
                // ExGrafix substitutes the attribute byte from the latched tile.
                return NtSource::Mapper;
            }
            self.ex_latch = self.exram[(a & 0x3FF) as usize];
        }
        let q = (self.nt_map >> (((a >> 10) & 3) * 2)) & 3;
        match q {
            0 => NtSource::Ciram(a & 0x3FF),
            1 => NtSource::Ciram(0x400 | (a & 0x3FF)),
            _ => NtSource::Mapper,
        }
    }

    pub fn nt_read(&self, addr: u16) -> u8 {
        let a = addr & 0x2FFF;
        // ExGrafix substitutes attribute fetches during rendering.
        if self.exram_mode == 1 && self.in_frame && self.rendering && a & 0x3FF >= 0x3C0 {
            return (self.ex_latch >> 6) * 0x55;
        }
        let q = (self.nt_map >> (((a >> 10) & 3) * 2)) & 3;
        match q {
            2 => {
                if self.exram_mode < 2 {
                    self.exram[(a & 0x3FF) as usize]
                } else {
                    0
                }
            }
            3 => {
                if a & 0x3FF >= 0x3C0 {
                    self.fill_attr * 0x55
                } else {
                    self.fill_tile
                }
            }
            _ => 0,
        }
    }

    pub fn nt_write(&mut self, addr: u16, value: u8) {
        let a = addr & 0x2FFF;
        let q = (self.nt_map >> (((a >> 10) & 3) * 2)) & 3;
        if q == 2 && self.exram_mode < 2 {
            self.exram[(a & 0x3FF) as usize] = value;
        }
    }

    pub fn cpu_read(&mut self, addr: u16, prg: &[u8], prg_ram: &[u8]) -> u8 {
        match addr {
            0x5204 => {
                let v = (u8::from(self.irq_pending) << 7) | (u8::from(self.in_frame) << 6);
                self.irq_pending = false;
                v
            }
            0x5205 => {
                let p = self.multiplicand as u16 * self.multiplier as u16;
                p as u8
            }
            0x5206 => {
                let p = self.multiplicand as u16 * self.multiplier as u16;
                (p >> 8) as u8
            }
            0x5C00..=0x5FFF => {
                if self.exram_mode >= 2 {
                    self.exram[(addr & 0x3FF) as usize]
                } else {
                    0
                }
            }
            0x6000..=0xFFFF => {
                let (is_rom, off) = self.prg_resolve(addr, prg_ram);
                if is_rom {
                    prg.get(off).copied().unwrap_or(0)
                } else {
                    prg_ram.get(off).copied().unwrap_or(0)
                }
            }
            _ => 0,
        }
    }

    /// Debug peek — no IRQ acknowledge side effects.
    pub fn cpu_peek(&self, addr: u16, prg: &[u8], prg_ram: &[u8]) -> u8 {
        match addr {
            0x5204 => (u8::from(self.irq_pending) << 7) | (u8::from(self.in_frame) << 6),
            0x5205 => (self.multiplicand as u16 * self.multiplier as u16) as u8,
            0x5206 => ((self.multiplicand as u16 * self.multiplier as u16) >> 8) as u8,
            0x5C00..=0x5FFF if self.exram_mode >= 2 => self.exram[(addr & 0x3FF) as usize],
            0x6000..=0xFFFF => {
                let (is_rom, off) = self.prg_resolve(addr, prg_ram);
                if is_rom {
                    prg.get(off).copied().unwrap_or(0)
                } else {
                    prg_ram.get(off).copied().unwrap_or(0)
                }
            }
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8, prg_ram: &mut [u8]) {
        match addr {
            0x5100 => self.prg_mode = value & 3,
            0x5101 => self.chr_mode = value & 3,
            0x5102 => self.ram_protect1 = value & 3,
            0x5103 => self.ram_protect2 = value & 3,
            0x5104 => self.exram_mode = value & 3,
            0x5105 => self.nt_map = value,
            0x5106 => self.fill_tile = value,
            0x5107 => self.fill_attr = value & 3,
            0x5113..=0x5117 => self.prg_banks[addr as usize - 0x5113] = value,
            0x5120..=0x5127 => {
                self.chr_sprite[addr as usize - 0x5120] = value as u16 | self.chr_upper;
                self.last_set_bg = false;
            }
            0x5128..=0x512B => {
                self.chr_bg[addr as usize - 0x5128] = value as u16 | self.chr_upper;
                self.last_set_bg = true;
            }
            0x5130 => self.chr_upper = ((value as u16) & 3) << 8,
            0x5203 => self.irq_compare = value,
            0x5204 => self.irq_enabled = value & 0x80 != 0,
            0x5205 => self.multiplicand = value,
            0x5206 => self.multiplier = value,
            0x5C00..=0x5FFF => {
                if self.exram_mode != 3 {
                    // Modes 0/1 are only meant to be written while rendering;
                    // always allowing writes keeps games happy and is common.
                    self.exram[(addr & 0x3FF) as usize] = value;
                }
            }
            0x6000..=0xFFFF => {
                let (is_rom, off) = self.prg_resolve(addr, prg_ram);
                if !is_rom && self.ram_writable() {
                    if let Some(slot) = prg_ram.get_mut(off) {
                        *slot = value;
                    }
                }
            }
            _ => {}
        }
    }

    /// Pattern fetch. `sprite` selects the sprite CHR window when 8×16 + rendering.
    pub fn chr_read(&self, addr: u16, chr: &[u8], sprite: bool) -> u8 {
        let a = addr & 0x1FFF;
        if self.exram_mode == 1 && self.in_frame && self.rendering && !sprite {
            let bank =
                (self.ex_latch as usize & 0x3F) | (((self.chr_upper as usize) >> 8) << 6);
            let banks = self.chr_banks_of(0x1000);
            let off = (bank % banks) * 0x1000 + (a as usize & 0x0FFF);
            return chr.get(off).copied().unwrap_or(0);
        }
        let use_bg = if self.sprites_8x16 && self.in_frame && self.rendering {
            !sprite
        } else {
            self.last_set_bg
        };
        chr.get(self.chr_offset(a, use_bg)).copied().unwrap_or(0)
    }

    pub fn chr_write(&mut self, addr: u16, value: u8, chr: &mut [u8], chr_is_ram: bool) {
        if !chr_is_ram {
            return;
        }
        let use_bg = if self.sprites_8x16 && self.in_frame && self.rendering {
            false // pattern writes are rare; use sprite set
        } else {
            self.last_set_bg
        };
        let i = self.chr_offset(addr & 0x1FFF, use_bg);
        if let Some(slot) = chr.get_mut(i) {
            *slot = value;
        }
    }

    pub fn export_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + 0x400);
        out.push(self.prg_mode);
        out.push(self.chr_mode);
        out.push(self.ram_protect1);
        out.push(self.ram_protect2);
        out.push(self.exram_mode);
        out.push(self.nt_map);
        out.push(self.fill_tile);
        out.push(self.fill_attr);
        out.extend_from_slice(&self.prg_banks);
        for b in &self.chr_sprite {
            out.extend_from_slice(&b.to_le_bytes());
        }
        for b in &self.chr_bg {
            out.extend_from_slice(&b.to_le_bytes());
        }
        out.extend_from_slice(&self.chr_upper.to_le_bytes());
        out.push(u8::from(self.last_set_bg));
        out.push(u8::from(self.sprites_8x16));
        out.push(u8::from(self.rendering));
        out.push(self.ex_latch);
        out.push(u8::from(self.in_frame));
        out.push(self.scanline_counter);
        out.push(self.irq_compare);
        out.push(u8::from(self.irq_enabled));
        out.push(u8::from(self.irq_pending));
        out.push(self.multiplicand);
        out.push(self.multiplier);
        out.extend_from_slice(&self.exram);
        out
    }

    pub fn import_state(&mut self, data: &[u8]) {
        if data.len() < 48 + 0x400 {
            return;
        }
        let mut i = 0;
        let take = |data: &[u8], i: &mut usize| -> u8 {
            let v = data[*i];
            *i += 1;
            v
        };
        self.prg_mode = take(data, &mut i);
        self.chr_mode = take(data, &mut i);
        self.ram_protect1 = take(data, &mut i);
        self.ram_protect2 = take(data, &mut i);
        self.exram_mode = take(data, &mut i);
        self.nt_map = take(data, &mut i);
        self.fill_tile = take(data, &mut i);
        self.fill_attr = take(data, &mut i);
        self.prg_banks.copy_from_slice(&data[i..i + 5]);
        i += 5;
        for b in &mut self.chr_sprite {
            *b = u16::from_le_bytes([data[i], data[i + 1]]);
            i += 2;
        }
        for b in &mut self.chr_bg {
            *b = u16::from_le_bytes([data[i], data[i + 1]]);
            i += 2;
        }
        self.chr_upper = u16::from_le_bytes([data[i], data[i + 1]]);
        i += 2;
        self.last_set_bg = take(data, &mut i) != 0;
        self.sprites_8x16 = take(data, &mut i) != 0;
        self.rendering = take(data, &mut i) != 0;
        self.ex_latch = take(data, &mut i);
        self.in_frame = take(data, &mut i) != 0;
        self.scanline_counter = take(data, &mut i);
        self.irq_compare = take(data, &mut i);
        self.irq_enabled = take(data, &mut i) != 0;
        self.irq_pending = take(data, &mut i) != 0;
        self.multiplicand = take(data, &mut i);
        self.multiplier = take(data, &mut i);
        if data.len() >= i + 0x400 {
            self.exram.copy_from_slice(&data[i..i + 0x400]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mmc5() -> (Mmc5, Vec<u8>, Vec<u8>, Vec<u8>) {
        let prg: Vec<u8> = (0..16 * 0x2000).map(|i| (i / 0x2000) as u8).collect();
        let chr: Vec<u8> = (0..32 * 0x400).map(|i| (i / 0x400) as u8).collect();
        let ram = vec![0u8; 0x10000];
        (Mmc5::new(prg.len(), chr.len()), prg, chr, ram)
    }

    #[test]
    fn multiplier() {
        let (mut m, prg, _, ram) = mmc5();
        m.cpu_write(0x5205, 0x34, &mut []);
        m.cpu_write(0x5206, 0x56, &mut []);
        let prod = 0x34u16 * 0x56;
        assert_eq!(m.cpu_read(0x5205, &prg, &ram), prod as u8);
        assert_eq!(m.cpu_read(0x5206, &prg, &ram), (prod >> 8) as u8);
    }

    #[test]
    fn prg_mode_3_rom_and_ram() {
        let (mut m, prg, _, mut ram) = mmc5();
        m.cpu_write(0x5100, 3, &mut ram);
        m.cpu_write(0x5114, 0x82, &mut ram);
        m.cpu_write(0x5115, 0x01, &mut ram);
        m.cpu_write(0x5116, 0x85, &mut ram);
        m.cpu_write(0x5117, 0x0F, &mut ram);
        assert_eq!(m.cpu_read(0x8000, &prg, &ram), 2);
        assert_eq!(m.cpu_read(0xC000, &prg, &ram), 5);
        assert_eq!(m.cpu_read(0xE000, &prg, &ram), 15);
        m.cpu_write(0x5102, 2, &mut ram);
        m.cpu_write(0x5103, 1, &mut ram);
        m.cpu_write(0xA000, 0xCD, &mut ram);
        assert_eq!(m.cpu_read(0xA000, &prg, &ram), 0xCD);
    }

    #[test]
    fn prg_mode_2_castlevania_layout() {
        let (mut m, prg, _, mut ram) = mmc5();
        m.cpu_write(0x5100, 2, &mut ram);
        m.cpu_write(0x5115, 0x84, &mut ram); // 16K at $8000 → banks 4–5
        m.cpu_write(0x5116, 0x86, &mut ram);
        m.cpu_write(0x5117, 0x8F, &mut ram);
        assert_eq!(m.cpu_read(0x8000, &prg, &ram), 4);
        assert_eq!(m.cpu_read(0xA000, &prg, &ram), 5);
        assert_eq!(m.cpu_read(0xC000, &prg, &ram), 6);
        assert_eq!(m.cpu_read(0xE000, &prg, &ram), 15);
    }

    #[test]
    fn chr_1k_and_bg_set() {
        let (mut m, _, chr, mut ram) = mmc5();
        m.cpu_write(0x5101, 3, &mut ram);
        m.cpu_write(0x5120, 5, &mut ram);
        assert_eq!(m.chr_read(0x0000, &chr, false), 5);
        m.cpu_write(0x5128, 7, &mut ram);
        assert_eq!(m.chr_read(0x0000, &chr, false), 7);
    }

    #[test]
    fn nametable_exram_and_fill() {
        let (mut m, _, _, mut ram) = mmc5();
        m.cpu_write(0x5105, 0b11_10_01_00, &mut ram);
        assert_eq!(m.nametable_source(0x2010), NtSource::Ciram(0x010));
        assert_eq!(m.nametable_source(0x2410), NtSource::Ciram(0x410));
        assert_eq!(m.nametable_source(0x2810), NtSource::Mapper);
        assert_eq!(m.nametable_source(0x2C10), NtSource::Mapper);
        m.nt_write(0x2810, 0x42);
        assert_eq!(m.nt_read(0x2810), 0x42);
        m.cpu_write(0x5106, 0x7E, &mut ram);
        m.cpu_write(0x5107, 0x02, &mut ram);
        assert_eq!(m.nt_read(0x2C10), 0x7E);
        assert_eq!(m.nt_read(0x2FD0), 0x02 * 0x55);
    }

    #[test]
    fn scanline_irq() {
        let mut m = Mmc5::new(0x20000, 0x2000);
        m.notify_ppu_reg(1, 0x18);
        m.cpu_write(0x5203, 2, &mut []);
        m.cpu_write(0x5204, 0x80, &mut []);
        m.on_scanline(0); // enter frame
        assert!(!m.irq_pending());
        m.on_scanline(1); // counter = 1
        assert!(!m.irq_pending());
        m.on_scanline(2); // counter = 2 → pending
        assert!(m.irq_pending());
        let v = m.cpu_read(0x5204, &[], &[]);
        assert_eq!(v & 0x80, 0x80);
        assert!(!m.irq_pending());
    }
}
