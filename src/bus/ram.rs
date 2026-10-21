/// Internal CPU WRAM: 2 KiB at `$0000–$07FF`, mirrored through `$1FFF`.
///
/// Stack (`$0100–$01FF`) and zero page (`$0000–$00FF`) live in this same array.
pub struct Ram {
    data: [u8; Self::SIZE],
}

impl Ram {
    pub const SIZE: usize = 0x800; // 2 KiB
    pub const ADDR_MASK: u16 = 0x07FF;

    pub fn new() -> Self {
        Self {
            data: [0; Self::SIZE],
        }
    }

    /// Map any address in `$0000–$1FFF` (and beyond, if misrouted) onto the 2 KiB array.
    #[inline]
    fn index(addr: u16) -> usize {
        (addr & Self::ADDR_MASK) as usize
    }

    pub fn read(&self, addr: u16) -> u8 {
        self.data[Self::index(addr)]
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        self.data[Self::index(addr)] = value;
    }
}

impl Default for Ram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_roundtrip() {
        let mut ram = Ram::new();
        ram.write(0x0010, 0x42);
        assert_eq!(ram.read(0x0010), 0x42);
    }

    #[test]
    fn mirrors_across_0x0000_0x1fff() {
        let mut ram = Ram::new();
        ram.write(0x0010, 0xAA);

        // Same physical cell via mirrors
        assert_eq!(ram.read(0x0810), 0xAA);
        assert_eq!(ram.read(0x1010), 0xAA);
        assert_eq!(ram.read(0x1810), 0xAA);

        ram.write(0x0810, 0x55);
        assert_eq!(ram.read(0x0010), 0x55);
    }

    #[test]
    fn stack_page_is_part_of_ram() {
        let mut ram = Ram::new();
        ram.write(0x01FD, 0x99);
        assert_eq!(ram.read(0x01FD), 0x99);
        assert_eq!(ram.read(0x09FD), 0x99); // mirror
    }
}
