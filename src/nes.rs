use crate::cart::{Cartridge, Rom};
use crate::clock::Region;
use crate::cpu::Cpu;
use crate::error::NesError;

/// Top-level NES console: CPU + Bus (+ PPU/cart on the bus) kept in sync.
///
/// Run loop (timing via [`crate::clock::Clock`] on the bus):
/// 1. Optionally service NMI (inside [`Cpu::step`])
/// 2. Fetch → decode → execute via the Bus
/// 3. [`crate::bus::Bus::tick`]: PPU dots (NTSC ×3 / PAL ×3.2) + APU by the same CPU cycles
pub struct Nes {
    pub cpu: Cpu,
}

impl Nes {
    pub fn from_rom(rom: Rom) -> Result<Self, NesError> {
        Ok(Self {
            cpu: Cpu::from_rom(rom)?,
        })
    }

    pub fn from_cartridge(cart: Cartridge) -> Self {
        Self {
            cpu: Cpu::from_cartridge(cart),
        }
    }

    /// Switch NTSC / PAL timing on the bus (clock + PPU + APU).
    pub fn set_region(&mut self, region: Region) {
        self.cpu.bus.set_region(region);
    }

    pub fn region(&self) -> Region {
        self.cpu.bus.region()
    }

    /// One CPU instruction (or one NMI sequence), with PPU already ticked inside.
    pub fn step(&mut self) -> Result<(), NesError> {
        self.cpu.step()
    }

    /// Run until the PPU frame counter increments (one full video frame).
    pub fn run_frame(&mut self) -> Result<(), NesError> {
        let start = self.frame();
        // Safety bound: a hung CPU must not loop forever in tests/tools.
        let limit = self.cpu.bus.clock.cpu_cycles_per_frame() as u64 * 8;
        let cycles_at_start = self.cpu.cycles;

        while self.frame() == start {
            self.step()?;
            if self.cpu.cycles.wrapping_sub(cycles_at_start) > limit {
                return Err(NesError::GenericError);
            }
        }
        Ok(())
    }

    /// Run `count` PPU frames.
    pub fn run_frames(&mut self, count: u64) -> Result<(), NesError> {
        for _ in 0..count {
            self.run_frame()?;
        }
        Ok(())
    }

    pub fn frame(&self) -> u64 {
        self.cpu.bus.ppu.frame
    }

    pub fn cycles(&self) -> u64 {
        self.cpu.cycles
    }

    pub fn pc(&self) -> u16 {
        self.cpu.pc
    }

    pub fn a(&self) -> u8 {
        self.cpu.a
    }

    pub fn x(&self) -> u8 {
        self.cpu.x
    }

    pub fn y(&self) -> u8 {
        self.cpu.y
    }

    pub fn sp(&self) -> u8 {
        self.cpu.sp
    }

    pub fn status(&self) -> u8 {
        self.cpu.p
    }

    /// Peek CPU memory without advancing the clock (debug HUD).
    pub fn peek(&self, addr: u16) -> u8 {
        self.cpu.bus.peek(addr)
    }

    /// RGB framebuffer from the PPU (`256×240×3`).
    pub fn framebuffer(&self) -> &[u8] {
        self.cpu.bus.ppu.framebuffer()
    }

    /// Drain APU sample buffer (host-rate mono).
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.cpu.bus.apu.take_samples()
    }

    /// Set player controller button bitmask (`Button` flags).
    pub fn set_controller(&mut self, player: usize, buttons: u8) {
        self.cpu.bus.input.set_buttons(player, buttons);
    }

    /// Configure APU host PCM rate (call after opening the audio device).
    pub fn set_audio_sample_rate(&mut self, hz: u32) {
        self.cpu.bus.apu.set_output_sample_rate(hz);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cart::Loader;

    #[test]
    fn smb_run_frame_advances() {
        let rom = Loader::load("roms/super-mario-bros.nes").unwrap();
        let mut nes = Nes::from_rom(rom).unwrap();
        let f0 = nes.frame();
        nes.run_frame().unwrap();
        assert_eq!(nes.frame(), f0 + 1);
        nes.run_frames(2).unwrap();
        assert_eq!(nes.frame(), f0 + 3);
    }
}
