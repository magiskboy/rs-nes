//! PAL timing constants (RP2A07 + RP2C07).

/// Master clock ≈ 26.601712 MHz.
pub const MASTER_HZ: u64 = 26_601_712;
/// CPU divisor: master ÷ 16.
pub const CPU_DIVISOR: u32 = 16;
/// PPU divisor: master ÷ 5.
pub const PPU_DIVISOR: u32 = 5;
/// CPU clock ≈ 1.662607 MHz.
pub const CPU_HZ: u32 = 1_662_607;
/// PPU dots / CPU cycle = 16/5 = 3.2.
pub const PPU_DOTS_PER_CPU_NUM: u32 = 16;
pub const PPU_DOTS_PER_CPU_DEN: u32 = 5;
/// 0–238 visible (239), 239 post, 240–310 vblank, 311 pre-render.
pub const SCANLINES_PER_FRAME: u16 = 312;
/// `341 × 312 / 3.2` truncated (= 33_247; exact is 33_247.5).
pub const CPU_CYCLES_PER_FRAME: u32 = 33_247;
/// ≈ 50.0070 Hz.
pub const FRAME_HZ_NUM: u32 = 50_0070;
pub const FRAME_HZ_DEN: u32 = 10_000;
/// ≈ 7459 CPU cycles (70 scanlines after NMI).
pub const CPU_CYCLES_NMI_TO_RENDER: u32 = 7459;
/// Mode 0 steps (CPU = APU×2): 4156→8312, 8313→16626, …
pub const APU_MODE0_STEPS_CPU: [u32; 4] = [8312, 16626, 24938, 33252];
/// Mode 1 steps.
pub const APU_MODE1_STEPS_CPU: [u32; 5] = [8312, 16626, 24938, 33252, 41564];
/// ≈ 8313 CPU cycles / quarter (PAL ~50 Hz frame IRQ / 4).
pub const APU_QUARTER_FRAME_CPU: u32 = 8313;
/// Noise timer periods (PAL).
pub const NOISE_PERIOD: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
/// DMC rate periods (PAL).
pub const DMC_PERIOD: [u16; 16] = [
    398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
];
