//! APU synthesis: frame counter, Pulse/Triangle/Noise/DMC, nonlinear mixer.
//!
//! Channel units live beside this facade for learning:
//! - [`dmc`] — delta modulation + DMA handshake with the bus
//!
//! Host playback lives in the frontend crate; samples are buffered here for `take_samples()`.

mod dmc;

use crate::clock::Region;
use dmc::Dmc;

const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];

const TRIANGLE_SEQUENCE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15,
];

/// Max buffered host-rate samples (~0.25 s at 48 kHz).
const SAMPLE_CAP: usize = 12_000;
/// Default host output rate until [`Apu::set_output_sample_rate`] is called.
const DEFAULT_OUTPUT_HZ: u32 = 48_000;

#[derive(Debug, Clone)]
struct Envelope {
    start: bool,
    loop_flag: bool,
    constant: bool,
    volume: u8,
    divider: u8,
    decay: u8,
}

impl Envelope {
    fn new() -> Self {
        Self {
            start: false,
            loop_flag: false,
            constant: false,
            volume: 0,
            divider: 0,
            decay: 0,
        }
    }

    fn write_ctrl(&mut self, value: u8) {
        self.loop_flag = value & 0x20 != 0;
        self.constant = value & 0x10 != 0;
        self.volume = value & 0x0F;
    }

    fn clock(&mut self) {
        if self.start {
            self.start = false;
            self.decay = 15;
            self.divider = self.volume;
        } else if self.divider == 0 {
            self.divider = self.volume;
            if self.decay > 0 {
                self.decay -= 1;
            } else if self.loop_flag {
                self.decay = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.constant {
            self.volume
        } else {
            self.decay
        }
    }
}

#[derive(Debug, Clone)]
struct Sweep {
    enabled: bool,
    negate: bool,
    shift: u8,
    period: u8,
    reload: bool,
    divider: u8,
    /// Pulse channel 1 uses one's complement negate; pulse 2 uses two's.
    ones_complement: bool,
}

impl Sweep {
    fn new(ones_complement: bool) -> Self {
        Self {
            enabled: false,
            negate: false,
            shift: 0,
            period: 0,
            reload: false,
            divider: 0,
            ones_complement,
        }
    }

    fn write(&mut self, value: u8) {
        self.enabled = value & 0x80 != 0;
        self.period = (value >> 4) & 0x07;
        self.negate = value & 0x08 != 0;
        self.shift = value & 0x07;
        self.reload = true;
    }

    fn target(&self, timer: u16) -> u16 {
        let change = timer >> self.shift;
        if self.negate {
            if self.ones_complement {
                timer.wrapping_sub(change).wrapping_sub(1)
            } else {
                timer.wrapping_sub(change)
            }
        } else {
            timer.wrapping_add(change)
        }
    }

    fn mute(&self, timer: u16) -> bool {
        timer < 8 || self.target(timer) > 0x7FF
    }

    fn clock(&mut self, timer: &mut u16) {
        let mut change = false;
        if self.divider == 0 && self.enabled && self.shift != 0 && !self.mute(*timer) {
            *timer = self.target(*timer) & 0x7FF;
            change = true;
        }
        let _ = change;
        if self.divider == 0 || self.reload {
            self.divider = self.period;
            self.reload = false;
        } else {
            self.divider -= 1;
        }
    }
}

#[derive(Debug, Clone)]
struct Pulse {
    enabled: bool,
    duty: u8,
    duty_step: u8,
    timer: u16,
    timer_period: u16,
    length: u8,
    envelope: Envelope,
    sweep: Sweep,
}

impl Pulse {
    fn new(ones_complement: bool) -> Self {
        Self {
            enabled: false,
            duty: 0,
            duty_step: 0,
            timer: 0,
            timer_period: 0,
            length: 0,
            envelope: Envelope::new(),
            sweep: Sweep::new(ones_complement),
        }
    }

    fn write(&mut self, reg: u8, value: u8) {
        match reg {
            0 => {
                self.duty = (value >> 6) & 0x03;
                self.envelope.write_ctrl(value);
            }
            1 => self.sweep.write(value),
            2 => {
                self.timer_period = (self.timer_period & 0xFF00) | value as u16;
            }
            3 => {
                self.timer_period = (self.timer_period & 0x00FF) | (((value as u16) & 0x07) << 8);
                if self.enabled {
                    self.length = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.duty_step = 0;
                self.envelope.start = true;
            }
            _ => {}
        }
    }

    fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.length = 0;
        }
    }

    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.duty_step = self.duty_step.wrapping_add(1) & 7;
        } else {
            self.timer -= 1;
        }
    }

    fn clock_length(&mut self) {
        if !self.envelope.loop_flag && self.length > 0 {
            self.length -= 1;
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled
            || self.length == 0
            || self.sweep.mute(self.timer_period)
            || DUTY_TABLE[self.duty as usize][self.duty_step as usize] == 0
        {
            return 0;
        }
        self.envelope.output()
    }
}

#[derive(Debug, Clone)]
struct Triangle {
    enabled: bool,
    timer: u16,
    timer_period: u16,
    length: u8,
    linear: u8,
    linear_reload: u8,
    linear_reload_flag: bool,
    control: bool,
    step: u8,
}

impl Triangle {
    fn new() -> Self {
        Self {
            enabled: false,
            timer: 0,
            timer_period: 0,
            length: 0,
            linear: 0,
            linear_reload: 0,
            linear_reload_flag: false,
            control: false,
            step: 0,
        }
    }

    fn write(&mut self, reg: u8, value: u8) {
        match reg {
            0 => {
                self.control = value & 0x80 != 0;
                self.linear_reload = value & 0x7F;
            }
            2 => {
                self.timer_period = (self.timer_period & 0xFF00) | value as u16;
            }
            3 => {
                self.timer_period = (self.timer_period & 0x00FF) | (((value as u16) & 0x07) << 8);
                if self.enabled {
                    self.length = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.linear_reload_flag = true;
            }
            _ => {}
        }
    }

    fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.length = 0;
        }
    }

    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            if self.length > 0 && self.linear > 0 {
                self.step = self.step.wrapping_add(1) & 31;
            }
        } else {
            self.timer -= 1;
        }
    }

    fn clock_linear(&mut self) {
        if self.linear_reload_flag {
            self.linear = self.linear_reload;
        } else if self.linear > 0 {
            self.linear -= 1;
        }
        if !self.control {
            self.linear_reload_flag = false;
        }
    }

    fn clock_length(&mut self) {
        if !self.control && self.length > 0 {
            self.length -= 1;
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled || self.length == 0 || self.linear == 0 || self.timer_period < 2 {
            return 0;
        }
        TRIANGLE_SEQUENCE[self.step as usize]
    }
}

#[derive(Debug, Clone)]
struct Noise {
    enabled: bool,
    timer: u16,
    timer_period: u16,
    length: u8,
    envelope: Envelope,
    shift: u16,
    mode: bool,
    region: Region,
}

impl Noise {
    fn new(region: Region) -> Self {
        Self {
            enabled: false,
            timer: 0,
            timer_period: 0,
            length: 0,
            envelope: Envelope::new(),
            shift: 1,
            mode: false,
            region,
        }
    }

    fn set_region(&mut self, region: Region) {
        self.region = region;
    }

    fn write(&mut self, reg: u8, value: u8) {
        match reg {
            0 => self.envelope.write_ctrl(value),
            2 => {
                self.mode = value & 0x80 != 0;
                self.timer_period = self.region.noise_period(value & 0x0F);
            }
            3 => {
                if self.enabled {
                    self.length = LENGTH_TABLE[(value >> 3) as usize];
                }
                self.envelope.start = true;
            }
            _ => {}
        }
    }

    fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.length = 0;
        }
    }

    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            let bit1 = self.shift & 1;
            let bit2 = if self.mode {
                (self.shift >> 6) & 1
            } else {
                (self.shift >> 1) & 1
            };
            let feedback = bit1 ^ bit2;
            self.shift >>= 1;
            self.shift |= feedback << 14;
        } else {
            self.timer -= 1;
        }
    }

    fn clock_length(&mut self) {
        if !self.envelope.loop_flag && self.length > 0 {
            self.length -= 1;
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled || self.length == 0 || self.shift & 1 != 0 {
            return 0;
        }
        self.envelope.output()
    }
}

/// APU register file + channel synthesis + sample buffer.
#[derive(Debug)]
pub struct Apu {
    region: Region,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
    status: u8,
    frame_counter: u8,
    irq_inhibit: bool,
    pub frame_irq: bool,
    /// CPU cycles into the current frame-sequencer period.
    cycle: u32,
    mode5: bool,
    /// Host-rate mono samples awaiting drain.
    samples: Vec<f32>,
    /// Target PCM rate for the sample accumulator.
    output_hz: u32,
    /// Accumulator toward the next host sample (`+= output_hz` each CPU cycle).
    sample_accum: u32,
    /// Even/odd CPU cycle for pulse/noise (APU clocks every other CPU cycle).
    odd: bool,
}

impl Apu {
    pub fn new() -> Self {
        Self::with_region(Region::Ntsc)
    }

    pub fn with_region(region: Region) -> Self {
        Self {
            region,
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::new(),
            noise: Noise::new(region),
            dmc: Dmc::new(region),
            status: 0,
            frame_counter: 0,
            irq_inhibit: false,
            frame_irq: false,
            cycle: 0,
            mode5: false,
            samples: Vec::with_capacity(2048),
            output_hz: DEFAULT_OUTPUT_HZ,
            sample_accum: 0,
            odd: false,
        }
    }

    pub fn region(&self) -> Region {
        self.region
    }

    pub fn set_region(&mut self, region: Region) {
        self.region = region;
        self.noise.set_region(region);
        self.dmc.set_region(region);
    }

    /// Mirror of DMC IRQ flag (for status / CPU).
    pub fn dmc_irq(&self) -> bool {
        self.dmc.irq
    }

    /// Address the bus should DMA for the DMC sample buffer, if any.
    pub fn dmc_dma_request(&self) -> Option<u16> {
        self.dmc.needs_dma()
    }

    /// Complete one DMC DMA byte fetch.
    pub fn dmc_dma_fill(&mut self, byte: u8) {
        self.dmc.fill_sample(byte);
    }

    /// Set host PCM sample rate used by the downsample accumulator.
    pub fn set_output_sample_rate(&mut self, hz: u32) {
        self.output_hz = hz.max(1);
        self.sample_accum = 0;
    }

    pub fn output_sample_rate(&self) -> u32 {
        self.output_hz
    }

    /// Clear channel + sample state after a savestate load (keeps region / host rate).
    pub fn soft_reset_for_savestate(&mut self) {
        let hz = self.output_hz;
        let region = self.region;
        *self = Self::with_region(region);
        self.output_hz = hz;
    }

    fn step_table(&self) -> &'static [u32] {
        match self.region {
            Region::Ntsc => {
                if self.mode5 {
                    &crate::clock::ntsc::APU_MODE1_STEPS_CPU
                } else {
                    &crate::clock::ntsc::APU_MODE0_STEPS_CPU
                }
            }
            Region::Pal => {
                if self.mode5 {
                    &crate::clock::pal::APU_MODE1_STEPS_CPU
                } else {
                    &crate::clock::pal::APU_MODE0_STEPS_CPU
                }
            }
        }
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x4015 => {
                let mut v = 0u8;
                if self.pulse1.length > 0 {
                    v |= 0x01;
                }
                if self.pulse2.length > 0 {
                    v |= 0x02;
                }
                if self.triangle.length > 0 {
                    v |= 0x04;
                }
                if self.noise.length > 0 {
                    v |= 0x08;
                }
                if self.dmc.bytes_remaining() > 0 {
                    v |= 0x10;
                }
                if self.frame_irq {
                    v |= 0x40;
                }
                if self.dmc.irq {
                    v |= 0x80;
                }
                self.frame_irq = false;
                v
            }
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x4000..=0x4003 => self.pulse1.write((addr - 0x4000) as u8, value),
            0x4004..=0x4007 => self.pulse2.write((addr - 0x4004) as u8, value),
            0x4008..=0x400B => self.triangle.write((addr - 0x4008) as u8, value),
            0x400C..=0x400F => self.noise.write((addr - 0x400C) as u8, value),
            0x4010..=0x4013 => self.dmc.write((addr - 0x4010) as u8, value),
            0x4015 => {
                self.status = value & 0x1F;
                self.pulse1.set_enabled(value & 0x01 != 0);
                self.pulse2.set_enabled(value & 0x02 != 0);
                self.triangle.set_enabled(value & 0x04 != 0);
                self.noise.set_enabled(value & 0x08 != 0);
                self.dmc.set_enabled(value & 0x10 != 0);
            }
            0x4017 => {
                self.frame_counter = value;
                self.mode5 = value & 0x80 != 0;
                self.irq_inhibit = value & 0x40 != 0;
                if self.irq_inhibit {
                    self.frame_irq = false;
                }
                self.cycle = 0;
                // Mode 1: immediately clock quarter+half (simplified; hardware delays 3–4 cycles).
                if self.mode5 {
                    self.clock_quarter();
                    self.clock_half();
                }
            }
            _ => {}
        }
    }

    fn clock_quarter(&mut self) {
        self.pulse1.envelope.clock();
        self.pulse2.envelope.clock();
        self.noise.envelope.clock();
        self.triangle.clock_linear();
    }

    fn clock_half(&mut self) {
        self.pulse1.clock_length();
        self.pulse2.clock_length();
        self.triangle.clock_length();
        self.noise.clock_length();
        self.pulse1.sweep.clock(&mut self.pulse1.timer_period);
        self.pulse2.sweep.clock(&mut self.pulse2.timer_period);
    }

    fn mix_sample(&self) -> f32 {
        let p1 = self.pulse1.output() as f32;
        let p2 = self.pulse2.output() as f32;
        let t = self.triangle.output() as f32;
        let n = self.noise.output() as f32;
        let d = self.dmc.output() as f32;

        let pulse_out = if p1 + p2 == 0.0 {
            0.0
        } else {
            95.88 / ((8128.0 / (p1 + p2)) + 100.0)
        };
        let tnd_out = if t + n + d == 0.0 {
            0.0
        } else {
            159.79 / (1.0 / ((t / 8227.0) + (n / 12241.0) + (d / 22638.0)) + 100.0)
        };
        pulse_out + tnd_out
    }

    fn push_host_sample(&mut self) {
        if self.samples.len() >= SAMPLE_CAP {
            let keep = SAMPLE_CAP / 2;
            self.samples.drain(0..self.samples.len() - keep);
        }
        // Mild gain to leave headroom for the nonlinear mixer peaks.
        self.samples.push(self.mix_sample() * 0.5);
    }

    /// Drain buffered host-rate samples.
    pub fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.samples)
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Advance APU by CPU cycles: timers, frame sequencer, host-rate samples.
    pub fn tick(&mut self, cpu_cycles: u32) {
        let steps = self.step_table();
        let period = *steps.last().unwrap_or(&29828);
        let cpu_hz = self.region.cpu_hz();

        for _ in 0..cpu_cycles {
            self.cycle = self.cycle.wrapping_add(1);

            for (i, &boundary) in steps.iter().enumerate() {
                if self.cycle == boundary {
                    let is_half = match (self.mode5, i) {
                        (false, 1) | (false, 3) => true,
                        (true, 1) | (true, 4) => true,
                        _ => false,
                    };
                    let is_quarter = match (self.mode5, i) {
                        (false, 0) | (false, 1) | (false, 2) | (false, 3) => true,
                        (true, 0) | (true, 1) | (true, 2) | (true, 4) => true,
                        _ => false,
                    };
                    if is_quarter {
                        self.clock_quarter();
                    }
                    if is_half {
                        self.clock_half();
                    }
                    if !self.mode5 && i == steps.len() - 1 && !self.irq_inhibit {
                        self.frame_irq = true;
                    }
                }
            }
            if self.cycle >= period {
                self.cycle = 0;
            }

            self.triangle.clock_timer();
            self.dmc.clock_timer();
            if self.odd {
                self.pulse1.clock_timer();
                self.pulse2.clock_timer();
                self.noise.clock_timer();
            }
            self.odd = !self.odd;

            // Bresenham-style downsample: emit when accum crosses cpu_hz.
            self.sample_accum = self.sample_accum.saturating_add(self.output_hz);
            while self.sample_accum >= cpu_hz {
                self.sample_accum -= cpu_hz;
                self.push_host_sample();
            }
        }
    }

    pub fn irq_pending(&self) -> bool {
        self.frame_irq || self.dmc.irq
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_read_clears_frame_irq() {
        let mut apu = Apu::new();
        apu.frame_irq = true;
        let v = apu.read(0x4015);
        assert!(v & 0x40 != 0);
        assert!(!apu.frame_irq);
    }

    #[test]
    fn write_4017_inhibits_irq() {
        let mut apu = Apu::new();
        apu.frame_irq = true;
        apu.write(0x4017, 0x40);
        assert!(!apu.frame_irq);
        assert!(apu.irq_inhibit);
    }

    #[test]
    fn length_counter_silences_pulse() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0xBF); // duty, constant vol 15, halt
        apu.write(0x4002, 0x08);
        apu.write(0x4003, 0x00); // length index 0 → 10
        assert!(apu.pulse1.length > 0);
        apu.write(0x4015, 0x00);
        assert_eq!(apu.pulse1.length, 0);
        assert_eq!(apu.pulse1.output(), 0);
    }

    #[test]
    fn mode0_sets_frame_irq() {
        let mut apu = Apu::new();
        apu.write(0x4017, 0x00); // mode 0, IRQ enabled
        // Step past last boundary (29828).
        apu.tick(29828);
        assert!(apu.frame_irq);
    }

    #[test]
    fn mixer_silent_when_disabled() {
        let mut apu = Apu::new();
        apu.set_output_sample_rate(48_000);
        // Enough CPU cycles to emit several host samples.
        apu.tick(apu.region().cpu_hz() / 100);
        let samples = apu.take_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn mixer_nonzero_with_pulse() {
        let mut apu = Apu::new();
        apu.set_output_sample_rate(48_000);
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0x9F); // duty 2, constant 15
        apu.write(0x4002, 0x10);
        apu.write(0x4003, 0x00);
        apu.tick(apu.region().cpu_hz() / 50);
        let samples = apu.take_samples();
        assert!(samples.iter().any(|&s| s > 0.0));
    }
}
