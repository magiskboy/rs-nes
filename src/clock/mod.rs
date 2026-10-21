//! Master timing — single source of truth for CPU / PPU / APU lockstep.
//!
//! On a real NES all chips share one crystal. Ratios differ by TV system
//! ([nesdev Cycle reference chart](https://www.nesdev.org/wiki/Cycle_reference_chart)):
//!
//! ```text
//! NTSC (2A03 + 2C02):
//!   master ≈ 21.477272 MHz
//!   PPU = master ÷ 4,  CPU = master ÷ 12  →  3 PPU dots / CPU cycle
//!   262 scanlines × 341 dots ≈ 29_780⅔ CPU cycles / frame ≈ 60.1 Hz
//!
//! PAL  (2A07 + 2C07):
//!   master ≈ 26.601712 MHz
//!   PPU = master ÷ 5,  CPU = master ÷ 16  →  3.2 PPU dots / CPU cycle
//!   312 scanlines × 341 dots = 33_247.5 CPU cycles / frame ≈ 50.0 Hz
//! ```
//!
//! Emulation advances in **CPU cycles**. Each [`Clock::advance`] yields a
//! [`TickBudget`] so the bus can tick PPU dots and APU time in lockstep.
//!
//! Region tables live in [`ntsc`] and [`pal`].

pub mod ntsc;
pub mod pal;

/// TV system / chipset timing profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Region {
    #[default]
    Ntsc,
    /// Ricoh 2A07 + 2C07 (European NES).
    Pal,
}

impl Region {
    /// CPU clock frequency (Hz).
    pub fn cpu_hz(self) -> u32 {
        match self {
            Self::Ntsc => ntsc::CPU_HZ,
            Self::Pal => pal::CPU_HZ,
        }
    }

    /// Master clock frequency (Hz).
    pub fn master_hz(self) -> u64 {
        match self {
            Self::Ntsc => ntsc::MASTER_HZ,
            Self::Pal => pal::MASTER_HZ,
        }
    }

    /// `(numerator, denominator)` for PPU dots per CPU cycle.
    /// NTSC = 3/1, PAL = 16/5 (= 3.2).
    pub fn ppu_dots_per_cpu(self) -> (u32, u32) {
        match self {
            Self::Ntsc => (ntsc::PPU_DOTS_PER_CPU_NUM, ntsc::PPU_DOTS_PER_CPU_DEN),
            Self::Pal => (pal::PPU_DOTS_PER_CPU_NUM, pal::PPU_DOTS_PER_CPU_DEN),
        }
    }

    /// Scanlines per frame (including pre-render).
    pub fn scanlines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => ntsc::SCANLINES_PER_FRAME,
            Self::Pal => pal::SCANLINES_PER_FRAME,
        }
    }

    /// PPU dots per frame (rendering off / no skipped dots).
    pub fn ppu_dots_per_frame(self) -> u32 {
        self.scanlines_per_frame() as u32 * DOTS_PER_SCANLINE as u32
    }

    /// Truncated CPU cycles per frame (safety bound / diagnostics).
    /// Exact NTSC off-render = 29_780⅔; PAL = 33_247.5.
    pub fn cpu_cycles_per_frame(self) -> u32 {
        match self {
            Self::Ntsc => ntsc::CPU_CYCLES_PER_FRAME,
            Self::Pal => pal::CPU_CYCLES_PER_FRAME,
        }
    }

    /// Approximate APU quarter-frame period in CPU cycles (stub sequencer).
    pub fn apu_quarter_frame_cpu(self) -> u32 {
        match self {
            Self::Ntsc => ntsc::APU_QUARTER_FRAME_CPU,
            Self::Pal => pal::APU_QUARTER_FRAME_CPU,
        }
    }

    /// Scanline where VBlank flag / NMI is set (cycle 1).
    pub fn vblank_scanline(self) -> i16 {
        match self {
            Self::Ntsc => 241,
            // PAL: 239 picture + 1 post-render → NMI at line 240.
            Self::Pal => 240,
        }
    }

    /// Pre-render scanline (VBlank cleared at cycle 1).
    pub fn prerender_scanline(self) -> i16 {
        match self {
            Self::Ntsc => 261,
            Self::Pal => 311,
        }
    }

    /// NTSC odd-frame shortens pre-render by 1 dot when rendering is on.
    pub fn odd_frame_skip(self) -> bool {
        matches!(self, Self::Ntsc)
    }

    /// APU Noise channel period lookup (`$400E` low nibble).
    pub fn noise_period(self, index: u8) -> u16 {
        let table = match self {
            Self::Ntsc => &ntsc::NOISE_PERIOD,
            Self::Pal => &pal::NOISE_PERIOD,
        };
        table[(index & 0x0F) as usize]
    }

    /// APU DMC rate lookup (`$4010` low nibble).
    pub fn dmc_period(self, index: u8) -> u16 {
        let table = match self {
            Self::Ntsc => &ntsc::DMC_PERIOD,
            Self::Pal => &pal::DMC_PERIOD,
        };
        table[(index & 0x0F) as usize]
    }
}

/// PPU dots per scanline (same on NTSC and PAL).
pub const DOTS_PER_SCANLINE: u16 = 341;

/// OAM DMA base cost (CPU cycles). +1 if DMA starts on an odd CPU cycle.
pub const OAM_DMA_CYCLES_BASE: u32 = 513;

// ─── Backward-compatible NTSC aliases ─────────────────────────────────────────

/// CPU clock (NTSC), Hz.
pub const CPU_HZ: u32 = ntsc::CPU_HZ;
/// PPU dots per CPU cycle (NTSC integer ratio).
pub const PPU_DOTS_PER_CPU_CYCLE: u32 = ntsc::PPU_DOTS_PER_CPU_NUM;
/// Scanlines per NTSC frame.
pub const SCANLINES_PER_FRAME: u16 = ntsc::SCANLINES_PER_FRAME;
/// PPU dots per NTSC frame (rendering off).
pub const PPU_DOTS_PER_FRAME: u32 = ntsc::SCANLINES_PER_FRAME as u32 * DOTS_PER_SCANLINE as u32;
/// Truncated CPU cycles per NTSC frame.
pub const CPU_CYCLES_PER_FRAME: u32 = ntsc::CPU_CYCLES_PER_FRAME;
/// NTSC APU quarter-frame stub period.
pub const APU_QUARTER_FRAME_CPU: u32 = ntsc::APU_QUARTER_FRAME_CPU;
/// NTSC mode-0 APU steps.
pub const APU_MODE0_STEPS_CPU: [u32; 4] = ntsc::APU_MODE0_STEPS_CPU;
/// NTSC mode-1 APU steps.
pub const APU_MODE1_STEPS_CPU: [u32; 5] = ntsc::APU_MODE1_STEPS_CPU;

// ─── Sync budget ──────────────────────────────────────────────────────────────

/// How many peripheral ticks to run after the CPU spends `cpu_cycles`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickBudget {
    /// CPU cycles just elapsed (input).
    pub cpu_cycles: u32,
    /// Whole PPU dots to advance this step.
    pub ppu_dots: u32,
}

/// Master clock that records elapsed CPU time and produces sync budgets.
///
/// PAL uses a fractional accumulator (`16/5` dots per CPU cycle) so long-run
/// averages stay exact (5 CPU cycles → exactly 16 PPU dots).
#[derive(Clone, Debug)]
pub struct Clock {
    region: Region,
    /// Total CPU cycles since power-on / reset.
    pub cpu_cycles: u64,
    /// Total whole PPU dots advanced.
    pub ppu_dots: u64,
    /// Fractional PPU-dot remainder in `[0, den)`.
    ppu_frac: u32,
}

impl Clock {
    pub fn new() -> Self {
        Self::with_region(Region::Ntsc)
    }

    pub fn with_region(region: Region) -> Self {
        Self {
            region,
            cpu_cycles: 0,
            ppu_dots: 0,
            ppu_frac: 0,
        }
    }

    pub fn region(&self) -> Region {
        self.region
    }

    /// Switch TV system; resets the fractional PPU phase (keeps cycle counters).
    pub fn set_region(&mut self, region: Region) {
        self.region = region;
        self.ppu_frac = 0;
    }

    pub fn cpu_cycles_per_frame(&self) -> u32 {
        self.region.cpu_cycles_per_frame()
    }

    /// Record `n` CPU cycles and return the peripheral work for this step.
    pub fn advance(&mut self, n: u32) -> TickBudget {
        let (num, den) = self.region.ppu_dots_per_cpu();
        // dots = (ppu_frac + n * num) / den ; keep remainder.
        let acc = self.ppu_frac as u64 + n as u64 * num as u64;
        let ppu_dots = (acc / den as u64) as u32;
        self.ppu_frac = (acc % den as u64) as u32;

        self.cpu_cycles = self.cpu_cycles.wrapping_add(n as u64);
        self.ppu_dots = self.ppu_dots.wrapping_add(ppu_dots as u64);

        TickBudget {
            cpu_cycles: n,
            ppu_dots,
        }
    }

    /// OAM DMA stall length for the current odd/even CPU phase.
    pub fn oam_dma_cycles(cpu_odd_cycle: bool) -> u32 {
        if cpu_odd_cycle {
            OAM_DMA_CYCLES_BASE + 1
        } else {
            OAM_DMA_CYCLES_BASE
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntsc_ratio_is_three_ppu_per_cpu() {
        assert_eq!(
            ntsc::PPU_DIVISOR * ntsc::PPU_DOTS_PER_CPU_NUM,
            ntsc::CPU_DIVISOR * ntsc::PPU_DOTS_PER_CPU_DEN
        );
        let mut clk = Clock::with_region(Region::Ntsc);
        let b = clk.advance(7);
        assert_eq!(b.ppu_dots, 21);
        assert_eq!(b.cpu_cycles, 7);
        assert_eq!(clk.ppu_frac, 0);
    }

    #[test]
    fn pal_ratio_is_sixteen_ppu_per_five_cpu() {
        assert_eq!(
            pal::PPU_DIVISOR * pal::PPU_DOTS_PER_CPU_NUM,
            pal::CPU_DIVISOR * pal::PPU_DOTS_PER_CPU_DEN
        );
        let mut clk = Clock::with_region(Region::Pal);
        // 1 CPU → 3 dots, remainder 1/5.
        assert_eq!(clk.advance(1).ppu_dots, 3);
        assert_eq!(clk.ppu_frac, 1);
        // +1 → 3 more (frac 2); keep going until 5 CPU = 16 dots total.
        assert_eq!(clk.advance(1).ppu_dots, 3);
        assert_eq!(clk.advance(1).ppu_dots, 3);
        assert_eq!(clk.advance(1).ppu_dots, 3);
        assert_eq!(clk.advance(1).ppu_dots, 4); // 3+3+3+3+4 = 16
        assert_eq!(clk.ppu_frac, 0);
        assert_eq!(clk.ppu_dots, 16);
        assert_eq!(clk.cpu_cycles, 5);
    }

    #[test]
    fn frame_sizes() {
        assert_eq!(Region::Ntsc.ppu_dots_per_frame(), 89_342);
        assert_eq!(Region::Ntsc.cpu_cycles_per_frame(), 29_780);
        assert_eq!(Region::Pal.ppu_dots_per_frame(), 106_392);
        assert_eq!(Region::Pal.cpu_cycles_per_frame(), 33_247);
        // PAL exact: 106392 / 3.2 = 33247.5 → remainder half-cycle.
        assert_eq!(
            Region::Pal.ppu_dots_per_frame() * pal::PPU_DOTS_PER_CPU_DEN
                % pal::PPU_DOTS_PER_CPU_NUM,
            pal::PPU_DOTS_PER_CPU_NUM / 2
        );
    }

    #[test]
    fn region_scanline_layout() {
        assert_eq!(Region::Ntsc.vblank_scanline(), 241);
        assert_eq!(Region::Ntsc.prerender_scanline(), 261);
        assert!(Region::Ntsc.odd_frame_skip());
        assert_eq!(Region::Pal.vblank_scanline(), 240);
        assert_eq!(Region::Pal.prerender_scanline(), 311);
        assert!(!Region::Pal.odd_frame_skip());
    }

    #[test]
    fn clock_accumulates_ntsc() {
        let mut clk = Clock::new();
        let _ = clk.advance(2);
        let _ = clk.advance(5);
        assert_eq!(clk.cpu_cycles, 7);
        assert_eq!(clk.ppu_dots, 21);
    }

    #[test]
    fn oam_dma_parity() {
        assert_eq!(Clock::oam_dma_cycles(false), 513);
        assert_eq!(Clock::oam_dma_cycles(true), 514);
    }

    #[test]
    fn set_region_clears_frac() {
        let mut clk = Clock::with_region(Region::Pal);
        let _ = clk.advance(1);
        assert_ne!(clk.ppu_frac, 0);
        clk.set_region(Region::Ntsc);
        assert_eq!(clk.ppu_frac, 0);
        assert_eq!(clk.region(), Region::Ntsc);
    }
}
