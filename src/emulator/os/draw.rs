//! Full-screen NES OS chrome (status / list / footer).

use crate::emulator::menu::screen::{
    hint_for, layout_scale, row_step, screen_def, LabelCtx,
};
use crate::emulator::menu::Menu;
use crate::emulator::os::library::{LibRow, RomEntry};
use crate::emulator::os::theme;
use crate::emulator::overlay::Overlay;
use chrono::Local;
use glfw::Glfw;

const STATUS: &str = "NES OS";

pub fn format_clock() -> String {
    Local::now().format("%Y-%m-%d  %H:%M:%S").to_string()
}

pub fn clear_bg(overlay: &mut Overlay, fb_w: i32, fb_h: i32) {
    overlay.fill_rect(0.0, 0.0, fb_w as f32, fb_h as f32, theme::BG);
}

fn draw_topbar(overlay: &mut Overlay, w: f32, status_h: f32, scale: f32, pad: f32, clock: &str) {
    overlay.fill_rect(0.0, 0.0, w, status_h, theme::BG_PANEL);
    overlay.fill_rect(0.0, status_h, w, (1.0 * scale).max(1.0), theme::BORDER_DIM);
    overlay.text(
        pad,
        (status_h - 8.0 * scale) * 0.5,
        scale,
        theme::ACCENT,
        STATUS,
    );
    let cs = scale * 0.85;
    let cw = Overlay::text_width(clock, cs);
    overlay.text(
        w - pad - cw,
        (status_h - 8.0 * cs) * 0.5,
        cs,
        theme::TEXT_DIM,
        clock,
    );
}

pub fn draw_library(
    overlay: &mut Overlay,
    fb_w: i32,
    fb_h: i32,
    roms: &[RomEntry],
    rows: &[LibRow],
    sel: usize,
    scroll: usize,
    blink: bool,
    clock: &str,
) {
    clear_bg(overlay, fb_w, fb_h);
    let w = fb_w as f32;
    let h = fb_h as f32;
    let scale = ((fb_w.min(fb_h) as f32) / 320.0).clamp(1.75, 3.5);
    let pad = 12.0 * scale;
    let line_h = 12.0 * scale;
    let status_h = 14.0 * scale;
    let footer_h = 14.0 * scale;
    let title_h = 16.0 * scale;

    draw_topbar(overlay, w, status_h, scale, pad, clock);

    let title_y = status_h + pad * 0.6;
    overlay.text(pad, title_y, scale * 1.2, theme::TITLE, "GAME LIBRARY");
    let count = format!("{} games", roms.len());
    let cw = Overlay::text_width(&count, scale * 0.85);
    overlay.text(
        w - pad - cw,
        title_y + 2.0 * scale,
        scale * 0.85,
        theme::TEXT_DIM,
        &count,
    );
    let sep_y = title_y + title_h * 0.85;
    overlay.fill_rect(pad, sep_y, w - pad * 2.0, (1.0 * scale).max(1.0), theme::BORDER);

    let list_top = sep_y + pad * 0.7;
    let list_bot = h - footer_h - pad * 0.5;
    let visible = ((list_bot - list_top) / line_h).floor().max(1.0) as usize;

    if rows.is_empty() {
        let msg = "No ROMs found";
        let mw = Overlay::text_width(msg, scale);
        overlay.text((w - mw) * 0.5, list_top + line_h, scale, theme::TEXT_DIM, msg);
    } else {
        let end = (scroll + visible).min(rows.len());
        for (vis_i, row_i) in (scroll..end).enumerate() {
            let y = list_top + vis_i as f32 * line_h;
            let selected = row_i == sel;
            let label = rows[row_i].label(roms);
            let is_action = matches!(rows[row_i], LibRow::AddFolder);

            if selected {
                overlay.fill_rect(
                    pad * 0.5,
                    y - 1.0 * scale,
                    w - pad,
                    line_h - 1.0 * scale,
                    theme::SELECT_BG,
                );
            }

            let prefix = if selected {
                if blink {
                    "> "
                } else {
                    "  "
                }
            } else {
                "  "
            };
            let color = if selected {
                theme::SELECT_TEXT
            } else if is_action {
                theme::ACCENT
            } else {
                theme::TEXT
            };
            let text = format!("{prefix}{label}");
            let max_chars = ((w - pad * 2.0) / (8.0 * scale)).floor().max(8.0) as usize;
            let display = if text.chars().count() > max_chars {
                let t: String = text.chars().take(max_chars.saturating_sub(1)).collect();
                format!("{t}~")
            } else {
                text
            };
            overlay.text(pad, y, scale, color, &display);
        }

        if scroll > 0 {
            overlay.text(w - pad - 8.0 * scale, list_top, scale, theme::TEXT_DIM, "^");
        }
        if end < rows.len() {
            overlay.text(
                w - pad - 8.0 * scale,
                list_bot - line_h,
                scale,
                theme::TEXT_DIM,
                "v",
            );
        }
    }

    let fy = h - footer_h;
    overlay.fill_rect(0.0, fy, w, footer_h, theme::BG_PANEL);
    overlay.fill_rect(0.0, fy, w, (1.0 * scale).max(1.0), theme::BORDER_DIM);
    let hint = "D-pad move  Start OK  Select menu";
    let hs = scale * 0.7;
    let hw = Overlay::text_width(hint, hs);
    overlay.text(
        ((w - hw) * 0.5).max(pad * 0.5),
        fy + (footer_h - 8.0 * hs) * 0.5,
        hs,
        theme::TEXT_DIM,
        hint,
    );
}

pub fn draw_system_menu(
    overlay: &mut Overlay,
    fb_w: i32,
    fb_h: i32,
    sel: usize,
    blink: bool,
    items: &[&str],
) {
    let w = fb_w as f32;
    let h = fb_h as f32;
    let scale = ((fb_w.min(fb_h) as f32) / 320.0).clamp(2.0, 4.0);
    let pad = 16.0 * scale;
    let line_h = 14.0 * scale;
    let title = "MENU";
    let title_scale = scale * 1.3;

    let mut content_w = Overlay::text_width(title, title_scale);
    for item in items {
        content_w = content_w.max(Overlay::text_width(&format!("> {item}"), scale));
    }
    let panel_w = content_w + pad * 2.0;
    let panel_h = pad + 10.0 * title_scale + pad * 0.5 + line_h * items.len() as f32 + pad;
    let panel_x = ((w - panel_w) * 0.5).round();
    let panel_y = ((h - panel_h) * 0.5).round();

    overlay.fill_rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
    let border = (2.0 * scale).max(2.0);
    overlay.fill_rect(
        panel_x - border,
        panel_y - border,
        panel_w + border * 2.0,
        panel_h + border * 2.0,
        theme::BORDER,
    );
    overlay.fill_rect(panel_x, panel_y, panel_w, panel_h, theme::BG_PANEL);

    let tw = Overlay::text_width(title, title_scale);
    let mut y = panel_y + pad * 0.7;
    overlay.text(
        panel_x + (panel_w - tw) * 0.5,
        y,
        title_scale,
        theme::TITLE,
        title,
    );
    y += 10.0 * title_scale + pad * 0.4;
    overlay.fill_rect(
        panel_x + pad * 0.5,
        y - pad * 0.2,
        panel_w - pad,
        (1.0 * scale).max(1.0),
        theme::BORDER_DIM,
    );

    for (i, item) in items.iter().enumerate() {
        let selected = i == sel;
        if selected {
            overlay.fill_rect(
                panel_x + pad * 0.4,
                y - 1.0 * scale,
                panel_w - pad * 0.8,
                line_h - 1.0 * scale,
                theme::SELECT_BG,
            );
        }
        let prefix = if selected {
            if blink {
                "> "
            } else {
                "  "
            }
        } else {
            "  "
        };
        let color = if selected {
            theme::SELECT_TEXT
        } else {
            theme::TEXT
        };
        let text = format!("{prefix}{item}");
        let lw = Overlay::text_width(&text, scale);
        overlay.text(panel_x + (panel_w - lw) * 0.5, y, scale, color, &text);
        y += line_h;
    }
}

pub fn draw_settings(
    overlay: &mut Overlay,
    menu: &Menu,
    glfw: &Glfw,
    fb_w: i32,
    fb_h: i32,
    blink: bool,
    clock: &str,
) {
    clear_bg(overlay, fb_w, fb_h);
    let Some(id) = menu.top_screen() else {
        return;
    };
    let def = screen_def(id);
    let w = fb_w as f32;
    let h = fb_h as f32;
    let (scale, title_mul) = layout_scale(id, fb_w, fb_h);
    let pad = 12.0 * scale;
    let status_h = 14.0 * scale;
    let footer_h = 14.0 * scale;

    draw_topbar(overlay, w, status_h, scale, pad, clock);

    let ctx = LabelCtx {
        settings: &menu.settings,
        map_target: menu.map_target(),
        waiting_bind: menu.waiting_bind(),
        glfw: Some(glfw),
    };

    let title_scale = scale * title_mul;
    let title_y = status_h + pad * 0.7;
    let tw = Overlay::text_width(def.title, title_scale);
    overlay.text((w - tw) * 0.5, title_y, title_scale, theme::TITLE, def.title);
    let sep_y = title_y + 10.0 * title_scale;
    overlay.fill_rect(pad, sep_y, w - pad * 2.0, (1.0 * scale).max(1.0), theme::BORDER);

    let step = row_step(id, scale);
    let mut y = sep_y + pad * 0.8;
    let sel = menu.sel();
    let list_bot = h - footer_h - pad;

    for (i, item) in def.items.iter().enumerate() {
        if y + step > list_bot {
            break;
        }
        let label = item.label(&ctx);
        let selected = i == sel;
        if selected {
            overlay.fill_rect(
                pad * 0.5,
                y - 1.0 * scale,
                w - pad,
                step - 1.0 * scale,
                theme::SELECT_BG,
            );
        }
        let prefix = if selected {
            if blink {
                "> "
            } else {
                "  "
            }
        } else {
            "  "
        };
        let color = if selected {
            theme::SELECT_TEXT
        } else {
            theme::TEXT
        };
        overlay.text(pad, y, scale, color, &format!("{prefix}{label}"));
        y += step;
    }

    let hint = hint_for(id, menu.is_waiting_bind(), menu.map_target());
    let fy = h - footer_h;
    overlay.fill_rect(0.0, fy, w, footer_h, theme::BG_PANEL);
    overlay.fill_rect(0.0, fy, w, (1.0 * scale).max(1.0), theme::BORDER_DIM);
    let hs = scale * 0.7;
    let hw = Overlay::text_width(hint, hs);
    overlay.text(
        ((w - hw) * 0.5).max(pad * 0.5),
        fy + (footer_h - 8.0 * hs) * 0.5,
        hs,
        theme::TEXT_DIM,
        hint,
    );
}
