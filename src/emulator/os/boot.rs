//! Boot splash: large baked Nitendo badge + "NES OS" subtitle.

use crate::emulator::os::theme;
use crate::emulator::overlay::Overlay;

/// Frames at ~60 Hz before auto-advancing to the library (~2.5 s).
pub const BOOT_FRAMES: u32 = 150;

const SUB: &str = "NES OS";

pub fn draw(overlay: &mut Overlay, fb_w: i32, fb_h: i32, _ticks: u32) {
    let w = fb_w as f32;
    let h = fb_h as f32;
    overlay.fill_rect(0.0, 0.0, w, h, theme::BG);

    let (logo_pw, logo_ph) = Overlay::logo_size();
    let aspect = logo_pw / logo_ph;

    // Large badge: dominate the first viewport (~70% of the shorter side).
    let draw_w = (fb_w.min(fb_h) as f32 * 0.78).clamp(220.0, w * 0.88);
    let draw_h = draw_w / aspect;
    let logo_x = ((w - draw_w) * 0.5).round();
    let logo_y = ((h - draw_h) * 0.42).round();
    overlay.draw_logo(logo_x, logo_y, draw_w, draw_h);

    let scale = ((fb_w.min(fb_h) as f32) / 320.0).clamp(2.0, 4.0);
    let sub_scale = scale * 1.0;
    let sw = Overlay::text_width(SUB, sub_scale);
    let sub_y = logo_y + draw_h + 16.0 * scale;
    overlay.text((w - sw) * 0.5, sub_y, sub_scale, theme::ACCENT, SUB);
}
