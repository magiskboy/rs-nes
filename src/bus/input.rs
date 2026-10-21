/// Standard controller button bits (NES report order).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    A = 1 << 0,
    B = 1 << 1,
    Select = 1 << 2,
    Start = 1 << 3,
    Up = 1 << 4,
    Down = 1 << 5,
    Left = 1 << 6,
    Right = 1 << 7,
}

#[derive(Debug, Clone, Copy, Default)]
struct Controller {
    /// Latched button state (1 = pressed).
    buttons: u8,
    /// Shift register for serial reads.
    shift: u8,
}

impl Controller {
    fn set(&mut self, buttons: u8) {
        self.buttons = buttons;
    }

    fn press(&mut self, button: Button) {
        self.buttons |= button as u8;
    }

    fn release(&mut self, button: Button) {
        self.buttons &= !(button as u8);
    }

    fn strobe(&mut self) {
        self.shift = self.buttons;
    }

    fn read_bit(&mut self, strobe: bool) -> u8 {
        if strobe {
            // While strobe high, continuously reload and report A.
            self.shift = self.buttons;
            return self.buttons & 1;
        }
        let bit = self.shift & 1;
        self.shift = (self.shift >> 1) | 0x80; // after 8 reads, return 1s
        bit
    }
}

/// Two controllers on `$4016` / `$4017`.
#[derive(Debug, Default)]
pub struct Input {
    pads: [Controller; 2],
    strobe: bool,
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_buttons(&mut self, player: usize, buttons: u8) {
        if let Some(pad) = self.pads.get_mut(player) {
            pad.set(buttons);
            if self.strobe {
                pad.strobe();
            }
        }
    }

    pub fn press(&mut self, player: usize, button: Button) {
        if let Some(pad) = self.pads.get_mut(player) {
            pad.press(button);
            if self.strobe {
                pad.strobe();
            }
        }
    }

    pub fn release(&mut self, player: usize, button: Button) {
        if let Some(pad) = self.pads.get_mut(player) {
            pad.release(button);
            if self.strobe {
                pad.strobe();
            }
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        if addr == 0x4016 {
            let new_strobe = value & 1 != 0;
            if new_strobe {
                self.pads[0].strobe();
                self.pads[1].strobe();
            }
            self.strobe = new_strobe;
        }
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x4016 => 0x40 | self.pads[0].read_bit(self.strobe),
            0x4017 => 0x40 | self.pads[1].read_bit(self.strobe),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_read_a_then_b() {
        let mut input = Input::new();
        input.set_buttons(0, (Button::A as u8) | (Button::B as u8));
        input.write(0x4016, 1);
        input.write(0x4016, 0);

        assert_eq!(input.read(0x4016) & 1, 1); // A
        assert_eq!(input.read(0x4016) & 1, 1); // B
        assert_eq!(input.read(0x4016) & 1, 0); // Select
    }

    #[test]
    fn default_unpressed() {
        let mut input = Input::new();
        input.write(0x4016, 1);
        input.write(0x4016, 0);
        for _ in 0..8 {
            assert_eq!(input.read(0x4016) & 1, 0);
        }
    }
}
