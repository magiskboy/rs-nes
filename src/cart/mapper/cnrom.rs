//! Mapper 3 — CNROM ([nesdev](https://www.nesdev.org/wiki/CNROM)).
//!
//! ```text
//!   $8000–$FFFF  PRG-ROM fixed (16 KiB mirrored or 32 KiB flat)
//!   $0000–$1FFF  8 KiB CHR bank selected by writes to $8000–$FFFF
//! ```
//!
//! Bus conflicts exist on real hardware; we ignore them (emulators usually do).

#[derive(Debug)]
pub struct Cnrom {
    /// Number of 8 KiB CHR banks.
    chr_banks: usize,
    /// Selected 8 KiB CHR bank.
    chr_bank: usize,
}

impl Cnrom {
    pub fn new(chr_banks: usize) -> Self {
        Self {
            chr_banks: chr_banks.max(1),
            chr_bank: 0,
        }
    }

    fn bank_mask(&self) -> usize {
        self.chr_banks.next_power_of_two().saturating_sub(1)
    }

    pub fn cpu_read(&self, addr: u16, prg: &[u8], _prg_ram: &[u8]) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let offset = (addr as usize) - 0x8000;
                match prg.len() {
                    0x4000 => prg[offset % 0x4000],
                    n => prg[offset % n.min(0x8000)],
                }
            }
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8, _prg_ram: &mut [u8]) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.chr_bank = (value as usize) & self.bank_mask();
            if self.chr_bank >= self.chr_banks {
                self.chr_bank %= self.chr_banks;
            }
        }
    }

    pub fn chr_read(&self, addr: u16, chr: &[u8]) -> u8 {
        let bank = self.chr_bank % self.chr_banks;
        let offset = bank * 0x2000 + ((addr as usize) & 0x1FFF);
        chr.get(offset).copied().unwrap_or(0)
    }

    pub fn chr_write(&mut self, _addr: u16, _value: u8, _chr: &mut [u8], _chr_is_ram: bool) {
        // CNROM uses CHR-ROM.
    }

    pub fn export_state(&self) -> Vec<u8> {
        (self.chr_bank as u16).to_le_bytes().to_vec()
    }

    pub fn import_state(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            let bank = u16::from_le_bytes([data[0], data[1]]) as usize;
            self.chr_bank = bank % self.chr_banks;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chr_bank_select() {
        let mut m = Cnrom::new(4);
        let mut ram = [];
        m.cpu_write(0x8000, 2, &mut ram);
        assert_eq!(m.chr_bank, 2);

        let mut chr = vec![0u8; 0x8000];
        chr[0x4000] = 0xAB; // bank 2 start
        assert_eq!(m.chr_read(0x0000, &chr), 0xAB);
    }
}
