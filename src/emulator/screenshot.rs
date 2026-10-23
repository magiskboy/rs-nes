//! Save the NES RGB framebuffer as a 24-bit BMP (no extra deps).

use crate::ppu::{BYTES_PER_PIXEL, SCREEN_HEIGHT, SCREEN_WIDTH};
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Write `framebuffer` (`256×240` RGB) to `path` as BMP.
pub fn save_bmp(path: &Path, framebuffer: &[u8]) -> io::Result<()> {
    let expected = SCREEN_WIDTH * SCREEN_HEIGHT * BYTES_PER_PIXEL;
    if framebuffer.len() < expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "framebuffer too small",
        ));
    }

    // BMP rows are bottom-up and padded to 4 bytes.
    let row_stride = (SCREEN_WIDTH * 3 + 3) & !3;
    let pixel_size = row_stride * SCREEN_HEIGHT;
    let file_size = 14 + 40 + pixel_size;

    let mut f = File::create(path)?;

    // BITMAPFILEHEADER
    f.write_all(b"BM")?;
    f.write_all(&(file_size as u32).to_le_bytes())?;
    f.write_all(&0u16.to_le_bytes())?; // reserved
    f.write_all(&0u16.to_le_bytes())?;
    f.write_all(&54u32.to_le_bytes())?; // offset to pixels

    // BITMAPINFOHEADER
    f.write_all(&40u32.to_le_bytes())?; // header size
    f.write_all(&(SCREEN_WIDTH as i32).to_le_bytes())?;
    f.write_all(&(SCREEN_HEIGHT as i32).to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // planes
    f.write_all(&24u16.to_le_bytes())?; // bpp
    f.write_all(&0u32.to_le_bytes())?; // compression
    f.write_all(&(pixel_size as u32).to_le_bytes())?;
    f.write_all(&2835u32.to_le_bytes())?; // ~72 DPI
    f.write_all(&2835u32.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;

    let mut row = vec![0u8; row_stride];
    for y in (0..SCREEN_HEIGHT).rev() {
        row.fill(0);
        for x in 0..SCREEN_WIDTH {
            let i = (y * SCREEN_WIDTH + x) * BYTES_PER_PIXEL;
            let r = framebuffer[i];
            let g = framebuffer[i + 1];
            let b = framebuffer[i + 2];
            let o = x * 3;
            // BMP is BGR
            row[o] = b;
            row[o + 1] = g;
            row[o + 2] = r;
        }
        f.write_all(&row)?;
    }
    Ok(())
}

/// Default path: `./screenshots/nes-YYYYMMDD-HHMMSS.bmp` (creates dir if needed).
pub fn default_path() -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dir = PathBuf::from("screenshots");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("nes-{stamp}.bmp"))
}
