mod input;
mod ram;

pub use input::{Button, Input};
pub use ram::Ram;

use crate::apu::Apu;
use crate::cart::Cartridge;
use crate::clock::{Clock, Region};
use crate::ppu::Ppu;

/// CPU memory bus: routes addresses to the correct device.
///
/// - `$0000–$1FFF` → WRAM  
/// - `$2000–$3FFF` → PPU regs  
/// - `$4000–$4013`, `$4015`, `$4017` → APU  
/// - `$4016` / `$4017` → controllers (write `$4016` strobe; read both)  
/// - `$4014` → OAM DMA  
/// - `$4020–$FFFF` → cartridge
///
/// Time advances through [`Bus::tick`], which uses [`Clock`] so PPU/APU stay
/// locked to the CPU (NTSC: 3 dots/cycle; PAL: 3.2 via fractional accumulator).
pub struct Bus {
    pub ram: Ram,
    pub ppu: Ppu,
    pub apu: Apu,
    pub input: Input,
    /// Master timing ledger (CPU cycle count + PPU/APU sync ratios).
    pub clock: Clock,
    cart: Option<Cartridge>,
    /// Extra CPU cycles owed after OAM DMA (handled by [`Bus::take_dma_cycles`]).
    dma_cycles: u32,
    /// Odd/even CPU cycle for DMA length (513 vs 514).
    pub cpu_odd_cycle: bool,
}

impl Bus {
    pub fn new() -> Self {
        Self::with_region(Region::Ntsc)
    }

    pub fn with_region(region: Region) -> Self {
        Self {
            ram: Ram::new(),
            ppu: Ppu::with_region(region),
            apu: Apu::with_region(region),
            input: Input::new(),
            clock: Clock::with_region(region),
            cart: None,
            dma_cycles: 0,
            cpu_odd_cycle: false,
        }
    }

    /// Switch NTSC/PAL; keeps RAM/cart, retargets clock + PPU + APU timing.
    pub fn set_region(&mut self, region: Region) {
        self.clock.set_region(region);
        self.ppu.set_region(region);
        self.apu.set_region(region);
    }

    pub fn region(&self) -> Region {
        self.clock.region()
    }

    pub fn cartridge(&self) -> Option<&Cartridge> {
        self.cart.as_ref()
    }

    pub fn cartridge_mut(&mut self) -> Option<&mut Cartridge> {
        self.cart.as_mut()
    }

    pub fn insert_cartridge(&mut self, cart: Cartridge) -> u16 {
        self.cart = Some(cart);
        self.read_u16(0xFFFC)
    }

    pub fn load_test_program(&mut self, program: &[u8]) -> u16 {
        self.cart = None;
        for (i, &byte) in program.iter().enumerate() {
            self.ram.write(i as u16, byte);
        }
        0
    }

    pub fn load_prg(&mut self, prg: Vec<u8>) -> u16 {
        if prg.is_empty() {
            self.cart = None;
            return 0;
        }
        if prg.len() <= 0x1000 {
            return self.load_test_program(&prg);
        }
        self.insert_cartridge(Cartridge::nrom(prg, Vec::new()))
    }

    /// Advance peripherals in lockstep with `cpu_cycles` (via [`Clock`]).
    ///
    /// Order matches hardware: PPU consumes the region's dots-per-cycle budget,
    /// then the APU frame sequencer advances by the same CPU time. Odd/even
    /// phase is tracked for OAM DMA length.
    pub fn tick(&mut self, cpu_cycles: u32) {
        let budget = self.clock.advance(cpu_cycles);
        for _ in 0..budget.ppu_dots {
            self.ppu.tick(&mut self.cart);
        }
        // DMC sample DMA (learning: one byte per request, +1 stall cycle).
        for _ in 0..budget.cpu_cycles {
            if let Some(addr) = self.apu.dmc_dma_request() {
                let byte = self.read_dma(addr);
                self.apu.dmc_dma_fill(byte);
                self.dma_cycles = self.dma_cycles.saturating_add(1);
            }
        }
        self.apu.tick(budget.cpu_cycles);
        if budget.cpu_cycles & 1 != 0 {
            self.cpu_odd_cycle = !self.cpu_odd_cycle;
        }
    }

    pub fn take_dma_cycles(&mut self) -> u32 {
        let n = self.dma_cycles;
        self.dma_cycles = 0;
        n
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram.read(addr),
            0x2000..=0x3FFF => {
                let reg = addr & 0x7;
                self.ppu.cpu_read(reg, &mut self.cart)
            }
            0x4015 => self.apu.read(0x4015),
            0x4016 | 0x4017 => self.input.read(addr),
            0x4000..=0x4014 | 0x4018..=0x401F => 0,
            0x4020..=0xFFFF => self
                .cart
                .as_ref()
                .map(|c| c.cpu_read(addr))
                .unwrap_or(0),
        }
    }

    /// Side-effect-free read for debug HUD (PPU/APU/controller ports return 0).
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram.read(addr),
            0x4020..=0xFFFF => self
                .cart
                .as_ref()
                .map(|c| c.cpu_read(addr))
                .unwrap_or(0),
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram.write(addr, value),
            0x2000..=0x3FFF => {
                let reg = addr & 0x7;
                self.ppu.cpu_write(reg, value, &mut self.cart);
            }
            0x4014 => self.oam_dma(value),
            0x4016 => self.input.write(addr, value),
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, value),
            0x4020..=0xFFFF => {
                if let Some(cart) = self.cart.as_mut() {
                    cart.cpu_write(addr, value);
                }
            }
            _ => {}
        }
    }

    /// OAM DMA: copy 256 bytes from CPU page `value` into OAM.
    fn oam_dma(&mut self, page: u8) {
        let base = (page as u16) << 8;
        for i in 0u16..256 {
            let byte = self.read_dma(base | i);
            self.ppu.oam_dma_write(byte);
        }
        self.dma_cycles = Clock::oam_dma_cycles(self.cpu_odd_cycle);
    }

    /// DMA source read without PPU/APU side effects.
    fn read_dma(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram.read(addr),
            0x8000..=0xFFFF => self
                .cart
                .as_ref()
                .map(|c| c.cpu_read(addr))
                .unwrap_or(0),
            _ => 0,
        }
    }

    pub fn read_u16(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cart::{Cartridge, Loader};

    #[test]
    fn routes_ram_region() {
        let mut bus = Bus::new();
        bus.write(0x0000, 0x11);
        bus.write(0x0800, 0x22);
        assert_eq!(bus.read(0x0000), 0x22);
    }

    #[test]
    fn oam_dma_copies_page() {
        let mut bus = Bus::new();
        for i in 0..256u16 {
            bus.ram.write(0x0200 + i, i as u8);
        }
        bus.write(0x4014, 0x02);
        assert_eq!(bus.take_dma_cycles(), 513);
        assert_eq!(bus.ppu.oam[0], 0);
        assert_eq!(bus.ppu.oam[1], 1);
        assert_eq!(bus.ppu.oam[255], 255);
    }

    #[test]
    fn maps_16k_prg_mirrored_via_cartridge() {
        let mut prg = vec![0; 0x4000];
        prg[0x3FFC] = 0x00;
        prg[0x3FFD] = 0x80;
        prg[0] = 0xEA;

        let mut bus = Bus::new();
        let pc = bus.load_prg(prg);
        assert_eq!(pc, 0x8000);
        assert_eq!(bus.read(0x8000), 0xEA);
        assert_eq!(bus.read(0xC000), 0xEA);
    }

    #[test]
    fn insert_smb_cartridge() {
        let rom = Loader::load("roms/super-mario-bros.nes").unwrap();
        let cart = Cartridge::from_rom(rom).unwrap();
        let mut bus = Bus::new();
        let pc = bus.insert_cartridge(cart);
        assert!(pc >= 0x8000);
        let via_bus = bus.read(pc);
        let via_cart = bus.cartridge().unwrap().cpu_read(pc);
        assert_eq!(via_bus, via_cart);
    }

    #[test]
    fn ppu_register_mirror_and_vblank_tick() {
        let mut bus = Bus::new();
        bus.write(0x2000, 0x80);
        bus.write(0x2008, 0x90);
        assert_eq!(bus.ppu.ctrl(), 0x90);

        bus.ppu.scanline = 241;
        bus.ppu.cycle = 1;
        bus.tick(1);
        assert!(bus.ppu.status() & 0x80 != 0);
        assert!(bus.ppu.take_nmi());
    }

    #[test]
    fn pal_region_syncs_clock_ppu_apu() {
        let mut bus = Bus::with_region(Region::Pal);
        assert_eq!(bus.region(), Region::Pal);
        assert_eq!(bus.clock.region(), Region::Pal);
        assert_eq!(bus.ppu.region(), Region::Pal);
        assert_eq!(bus.apu.region(), Region::Pal);

        // 5 CPU cycles → exactly 16 PPU dots on PAL.
        let before = bus.ppu.cycle;
        bus.ppu.scanline = 0;
        bus.ppu.cycle = 0;
        bus.tick(5);
        assert_eq!(bus.clock.ppu_dots, 16);
        assert_eq!(bus.ppu.cycle, before.wrapping_add(16) % 341);
    }
}
