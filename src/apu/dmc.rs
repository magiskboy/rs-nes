//! Delta Modulation Channel ([nesdev](https://www.nesdev.org/wiki/APU_DMC)).
//!
//! Plays 1-bit DPCM samples from CPU memory. The bus fills bytes via
//! [`Dmc::needs_dma`] / [`Dmc::fill_sample`] (see [`crate::bus::Bus::tick`]).

use crate::clock::Region;

#[derive(Debug, Clone)]
pub struct Dmc {
    enabled: bool,
    irq_enabled: bool,
    loop_flag: bool,
    pub irq: bool,
    /// Output level 0–127 (`$4011` direct load / delta unit).
    pub level: u8,
    rate_index: u8,
    timer: u16,
    timer_period: u16,
    sample_addr: u16,
    sample_length: u16,
    current_addr: u16,
    bytes_remaining: u16,
    /// Buffered sample byte awaiting the output unit.
    sample_buffer: Option<u8>,
    /// True when empty and bytes remain — bus should DMA.
    dma_request: bool,
    shift: u8,
    bits_remaining: u8,
    silence: bool,
    region: Region,
}

impl Dmc {
    pub fn new(region: Region) -> Self {
        Self {
            enabled: false,
            irq_enabled: false,
            loop_flag: false,
            irq: false,
            level: 0,
            rate_index: 0,
            timer: 0,
            timer_period: region.dmc_period(0),
            sample_addr: 0xC000,
            sample_length: 1,
            current_addr: 0xC000,
            bytes_remaining: 0,
            sample_buffer: None,
            dma_request: false,
            shift: 0,
            bits_remaining: 0,
            silence: true,
            region,
        }
    }

    pub fn set_region(&mut self, region: Region) {
        self.region = region;
        self.timer_period = region.dmc_period(self.rate_index);
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        match reg {
            0 => {
                // $4010: I L - - RRRR
                self.irq_enabled = value & 0x80 != 0;
                self.loop_flag = value & 0x40 != 0;
                if !self.irq_enabled {
                    self.irq = false;
                }
                self.rate_index = value & 0x0F;
                self.timer_period = self.region.dmc_period(self.rate_index);
            }
            1 => {
                // $4011: direct load
                self.level = value & 0x7F;
            }
            2 => {
                // $4012: sample address = $C000 + value*64
                self.sample_addr = 0xC000 | ((value as u16) << 6);
            }
            3 => {
                // $4013: sample length = value*16 + 1
                self.sample_length = ((value as u16) << 4) + 1;
            }
            _ => {}
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.bytes_remaining = 0;
            self.dma_request = false;
            self.irq = false;
        } else if self.bytes_remaining == 0 {
            self.restart_sample();
        }
    }

    pub fn bytes_remaining(&self) -> u16 {
        self.bytes_remaining
    }

    fn restart_sample(&mut self) {
        self.current_addr = self.sample_addr;
        self.bytes_remaining = self.sample_length;
        if self.sample_buffer.is_none() {
            self.dma_request = true;
        }
    }

    /// Address the bus should fetch, if a DMA byte is needed.
    pub fn needs_dma(&self) -> Option<u16> {
        if self.dma_request && self.bytes_remaining > 0 {
            Some(self.current_addr)
        } else {
            None
        }
    }

    /// Fill the sample buffer with one DMA byte.
    pub fn fill_sample(&mut self, byte: u8) {
        if !self.dma_request {
            return;
        }
        self.sample_buffer = Some(byte);
        self.dma_request = false;
        self.current_addr = self.current_addr.wrapping_add(1);
        if self.current_addr == 0 {
            self.current_addr = 0x8000;
        }
        if self.bytes_remaining > 0 {
            self.bytes_remaining -= 1;
        }
        if self.bytes_remaining == 0 {
            if self.loop_flag {
                self.restart_sample();
            } else if self.irq_enabled {
                self.irq = true;
            }
        }
    }

    /// Clock once per CPU cycle.
    pub fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.clock_output();
        } else {
            self.timer -= 1;
        }
    }

    fn clock_output(&mut self) {
        if !self.silence {
            let bit = self.shift & 1;
            if bit != 0 {
                if self.level <= 125 {
                    self.level += 2;
                }
            } else if self.level >= 2 {
                self.level -= 2;
            }
            self.shift >>= 1;
        }

        if self.bits_remaining == 0 {
            self.bits_remaining = 8;
            if let Some(byte) = self.sample_buffer.take() {
                self.silence = false;
                self.shift = byte;
                if self.bytes_remaining > 0 {
                    self.dma_request = true;
                }
            } else {
                self.silence = true;
            }
        } else {
            self.bits_remaining -= 1;
        }
    }

    pub fn output(&self) -> u8 {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_load_sets_level() {
        let mut d = Dmc::new(Region::Ntsc);
        d.write(1, 0x40);
        assert_eq!(d.level, 0x40);
    }

    #[test]
    fn enable_starts_sample() {
        let mut d = Dmc::new(Region::Ntsc);
        d.write(2, 0);
        d.write(3, 0); // length = 1
        d.set_enabled(true);
        assert_eq!(d.bytes_remaining(), 1);
        assert!(d.needs_dma().is_some());
    }
}
