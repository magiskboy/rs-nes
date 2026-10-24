//! Ephemeral bottom-of-screen text notifications (toasts).

use crate::emulator::os::theme;
use crate::emulator::overlay::Overlay;

/// ~2.5s visibility at 60 Hz, then fade out.
const DEFAULT_TTL: u32 = 150;
const FADE_OUT: u32 = 36;

/// Single active toast; a new push replaces the previous message.
#[derive(Debug, Default)]
pub struct Notify {
    message: Option<String>,
    /// Frames remaining before the toast is cleared.
    ttl: u32,
}

impl Notify {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, msg: impl Into<String>) {
        let text = msg.into();
        if text.is_empty() {
            return;
        }
        self.message = Some(text);
        self.ttl = DEFAULT_TTL;
    }

    pub fn tick(&mut self) {
        if self.ttl == 0 {
            self.message = None;
            return;
        }
        self.ttl -= 1;
        if self.ttl == 0 {
            self.message = None;
        }
    }

    pub fn draw(&self, overlay: &mut Overlay, fb_w: i32, fb_h: i32) {
        let Some(msg) = self.message.as_deref() else {
            return;
        };
        if self.ttl == 0 {
            return;
        }

        let alpha = if self.ttl < FADE_OUT {
            (self.ttl as f32) / (FADE_OUT as f32)
        } else {
            1.0
        };

        // Match library footer metrics exactly.
        let scale = ((fb_w.min(fb_h) as f32) / 320.0).clamp(1.75, 3.5);
        let w = fb_w as f32;
        let h = fb_h as f32;
        let pad = 12.0 * scale;
        let footer_h = 14.0 * scale;
        let text_scale = scale * 0.7;
        let text_h = 8.0 * text_scale;
        let fy = h - footer_h;

        let max_chars = (((w - pad) / (8.0 * text_scale)).floor() as usize).max(8);
        let display = ellipsize(msg, max_chars);
        let tw = Overlay::text_width(&display, text_scale);
        let bar_w = (tw + pad).min(w);

        let mut bg = theme::BG_PANEL;
        bg[3] = 0.88 * alpha;
        let mut border = theme::BORDER_DIM;
        border[3] = 0.75 * alpha;
        let mut text = theme::ACCENT;
        text[3] = alpha;

        overlay.fill_rect(0.0, fy, bar_w, footer_h, bg);
        overlay.fill_rect(0.0, fy, bar_w, (1.0 * scale).max(1.0), border);
        overlay.text(
            pad * 0.5,
            fy + (footer_h - text_h) * 0.5,
            text_scale,
            text,
            &display,
        );
    }
}

fn ellipsize(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let t: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{t}~")
    }
}
