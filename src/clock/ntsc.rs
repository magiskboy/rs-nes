//! NTSC timing constants (RP2A03 + RP2C02).

/// Master clock ≈ 21.477272 MHz (236.25 MHz ÷ 11).
pub const MASTER_HZ: u64 = 21_477_272;
/// CPU divisor: master ÷ 12.
pub const CPU_DIVISOR: u32 = 12;
/// PPU divisor: master ÷ 4.
pub const PPU_DIVISOR: u32 = 4;
/// CPU clock ≈ 1.789773 MHz.
pub const CPU_HZ: u32 = 1_789_773;
/// PPU dots / CPU cycle = 3/1.
pub const PPU_DOTS_PER_CPU_NUM: u32 = 3;
pub const PPU_DOTS_PER_CPU_DEN: u32 = 1;
/// 0–239 visible, 240 post, 241–260 vblank, 261 pre-render.
pub const SCANLINES_PER_FRAME: u16 = 262;
/// `341 × 262 / 3` truncated (= 29_780; exact is 29_780⅔).
pub const CPU_CYCLES_PER_FRAME: u32 = 29_780;
/// ≈ 60.0988 Hz (num/den × 10⁻⁴).
pub const FRAME_HZ_NUM: u32 = 60_0988;
pub const FRAME_HZ_DEN: u32 = 10_000;
/// CPU cycles from NMI to rendering ≈ 2273⅓.
pub const CPU_CYCLES_NMI_TO_RENDER: u32 = 2273;
/// Mode 0 step boundaries (CPU cycles).
pub const APU_MODE0_STEPS_CPU: [u32; 4] = [7456, 14914, 22370, 29828];
/// Mode 1 step boundaries (CPU cycles).
pub const APU_MODE1_STEPS_CPU: [u32; 5] = [7456, 14914, 22370, 29828, 37280];
/// ≈ 7457 CPU cycles / quarter frame.
pub const APU_QUARTER_FRAME_CPU: u32 = 7457;
/// Noise timer periods ([nesdev APU Noise](https://www.nesdev.org/wiki/APU_Noise)).
pub const NOISE_PERIOD: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
/// DMC rate periods ([nesdev APU DMC](https://www.nesdev.org/wiki/APU_DMC)).
pub const DMC_PERIOD: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];
