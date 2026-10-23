//! Baked "NITENDO" badge pixel buffer (RGBA8) for the OS boot splash.
//!
//! Classic red capsule + white lettering, built once from the 8×8 font.

use super::font8x8::FONT8X8;

/// Logo texture size — wide enough for 7× scaled glyphs + capsule padding.
pub const LOGO_W: usize = 360;
pub const LOGO_H: usize = 80;

/// Nintendo-red fill inside the capsule.
const RED: [u8; 4] = [0xE6, 0x00, 0x12, 0xFF];
const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

const WORD: &[u8] = b"NITENDO";
const SCALE: i32 = 4;
/// Tracking between glyphs (px at 1×; multiplied by SCALE below).
const GAP: i32 = 1;

/// Build an RGBA8 buffer (`LOGO_W * LOGO_H * 4` bytes).
pub fn bake_rgba() -> Vec<u8> {
    let mut px = vec![0u8; LOGO_W * LOGO_H * 4];

    let glyph_w = 8 * SCALE;
    let glyph_h = 8 * SCALE;
    let gap = GAP * SCALE;
    let text_w = WORD.len() as i32 * glyph_w + (WORD.len() as i32 - 1) * gap;
    let text_h = glyph_h;

    // Capsule wraps the word with clear side padding past the letterforms.
    let text_pad_x = 20i32;
    let text_pad_y = 12i32;
    let radius = text_h / 2 + text_pad_y;
    let straight = text_w + text_pad_x * 2;
    let capsule_w = straight + radius * 2;
    let capsule_h = radius * 2;

    let left = ((LOGO_W as i32 - capsule_w) / 2).max(2);
    let right = left + capsule_w - 1;
    let top = ((LOGO_H as i32 - capsule_h) / 2).max(2);
    let bot = top + capsule_h - 1;

    for y in 0..LOGO_H as i32 {
        for x in 0..LOGO_W as i32 {
            if inside_capsule(x, y, left, right, top, bot, radius) {
                put(&mut px, x as usize, y as usize, RED);
            } else {
                put(&mut px, x as usize, y as usize, CLEAR);
            }
        }
    }

    // Thin white rim just inside the capsule.
    let rim = 2;
    for y in 0..LOGO_H as i32 {
        for x in 0..LOGO_W as i32 {
            let in_outer = inside_capsule(x, y, left, right, top, bot, radius);
            let in_inner = inside_capsule(
                x,
                y,
                left + rim,
                right - rim,
                top + rim,
                bot - rim,
                (radius - rim).max(1),
            );
            if in_outer && !in_inner {
                put(&mut px, x as usize, y as usize, WHITE);
            }
        }
    }

    let text_x = left + radius + text_pad_x;
    let text_y = top + (capsule_h - text_h) / 2;

    for (i, &ch) in WORD.iter().enumerate() {
        let gx = text_x + i as i32 * (glyph_w + gap);
        blit_glyph(&mut px, ch, gx, text_y, SCALE, WHITE);
    }

    px
}

fn inside_capsule(x: i32, y: i32, left: i32, right: i32, top: i32, bot: i32, radius: i32) -> bool {
    let cy = (top + bot) / 2;
    if x >= left + radius && x <= right - radius {
        return y >= top && y <= bot;
    }
    let cx = if x < left + radius {
        left + radius
    } else {
        right - radius
    };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius
}

fn blit_glyph(px: &mut [u8], ch: u8, x0: i32, y0: i32, scale: i32, rgba: [u8; 4]) {
    if !(0x20..0x7F).contains(&ch) {
        return;
    }
    let glyph = &FONT8X8[(ch - 0x20) as usize];
    for row in 0..8 {
        let bits = glyph[row];
        for col in 0..8 {
            if bits & (1 << col) == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let x = x0 + col * scale + sx;
                    let y = y0 + row as i32 * scale + sy;
                    if x >= 0 && y >= 0 && (x as usize) < LOGO_W && (y as usize) < LOGO_H {
                        put(px, x as usize, y as usize, rgba);
                    }
                }
            }
        }
    }
}

fn put(px: &mut [u8], x: usize, y: usize, rgba: [u8; 4]) {
    let i = (y * LOGO_W + x) * 4;
    px[i..i + 4].copy_from_slice(&rgba);
}
