//! NES palette + framebuffer helpers (PPU side; GL lives in frontend).

/// Screen width in pixels.
pub const SCREEN_WIDTH: usize = 256;
/// Visible height in pixels (NTSC picture; PAL crops similarly for buffer).
pub const SCREEN_HEIGHT: usize = 240;
/// Bytes per pixel (RGB).
pub const BYTES_PER_PIXEL: usize = 3;
/// Framebuffer size in bytes.
pub const FRAMEBUFFER_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT * BYTES_PER_PIXEL;

/// Classic 64-entry NES palette as RGB triplets (`$00`–`$3F`).
pub static NES_PALETTE: [[u8; 3]; 64] = [
    [84, 84, 84],
    [0, 30, 116],
    [8, 16, 144],
    [48, 0, 136],
    [68, 0, 100],
    [92, 0, 48],
    [84, 4, 0],
    [60, 24, 0],
    [32, 42, 0],
    [8, 58, 0],
    [0, 64, 0],
    [0, 60, 0],
    [0, 50, 60],
    [0, 0, 0],
    [0, 0, 0],
    [0, 0, 0],
    [152, 150, 152],
    [8, 76, 196],
    [48, 50, 236],
    [92, 30, 228],
    [136, 20, 176],
    [160, 20, 100],
    [152, 34, 32],
    [120, 60, 0],
    [84, 90, 0],
    [40, 114, 0],
    [8, 124, 0],
    [0, 118, 40],
    [0, 102, 120],
    [0, 0, 0],
    [0, 0, 0],
    [0, 0, 0],
    [236, 238, 236],
    [76, 154, 236],
    [120, 124, 236],
    [176, 98, 236],
    [228, 84, 236],
    [236, 88, 180],
    [236, 106, 100],
    [212, 136, 32],
    [160, 170, 0],
    [116, 196, 0],
    [76, 208, 32],
    [56, 204, 108],
    [56, 180, 204],
    [60, 60, 60],
    [0, 0, 0],
    [0, 0, 0],
    [236, 238, 236],
    [168, 204, 236],
    [188, 188, 236],
    [212, 178, 236],
    [236, 174, 236],
    [236, 174, 212],
    [236, 180, 176],
    [228, 196, 144],
    [204, 210, 120],
    [180, 222, 120],
    [168, 226, 144],
    [152, 226, 180],
    [160, 214, 228],
    [160, 162, 160],
    [0, 0, 0],
    [0, 0, 0],
];

/// Look up RGB for a NES palette index (`0..=63`).
#[inline]
pub fn nes_rgb(index: u8) -> [u8; 3] {
    NES_PALETTE[(index & 0x3F) as usize]
}

/// Write one RGB pixel into a packed framebuffer.
#[inline]
pub fn put_pixel(fb: &mut [u8], x: usize, y: usize, rgb: [u8; 3]) {
    if x >= SCREEN_WIDTH || y >= SCREEN_HEIGHT {
        return;
    }
    let i = (y * SCREEN_WIDTH + x) * BYTES_PER_PIXEL;
    fb[i] = rgb[0];
    fb[i + 1] = rgb[1];
    fb[i + 2] = rgb[2];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_64_entries() {
        assert_eq!(NES_PALETTE.len(), 64);
        assert_eq!(nes_rgb(0x0F), [0, 0, 0]);
    }

    #[test]
    fn put_pixel_writes_rgb() {
        let mut fb = [0u8; FRAMEBUFFER_SIZE];
        put_pixel(&mut fb, 1, 0, [1, 2, 3]);
        assert_eq!(&fb[3..6], &[1, 2, 3]);
    }
}

