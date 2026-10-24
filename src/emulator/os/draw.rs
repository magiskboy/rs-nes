//! Full-screen NES OS chrome (status / list / footer).

use crate::emulator::menu::screen::{
    hint_for, layout_scale, row_step, screen_def, LabelCtx,
};
use crate::emulator::menu::Menu;
use crate::emulator::os::cover::CoverRgba;
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

/// How many library rows fit between the title rule and the footer.
pub fn library_visible_rows(fb_w: i32, fb_h: i32) -> usize {
    let (_, _, line_h, list_top, list_bot) = library_list_metrics(fb_w, fb_h);
    ((list_bot - list_top) / line_h).floor().max(1.0) as usize
}

/// `(scale, pad, line_h, list_top, list_bot)` shared by layout + scroll.
fn library_list_metrics(fb_w: i32, fb_h: i32) -> (f32, f32, f32, f32, f32) {
    let h = fb_h as f32;
    let scale = ((fb_w.min(fb_h) as f32) / 320.0).clamp(1.75, 3.5);
    let pad = 12.0 * scale;
    let line_h = 12.0 * scale;
    let status_h = 14.0 * scale;
    let footer_h = 14.0 * scale;
    let title_h = 16.0 * scale;
    let title_y = status_h + pad * 0.6;
    let sep_y = title_y + title_h * 0.85;
    let list_top = sep_y + pad * 0.7;
    let list_bot = h - footer_h - pad * 0.5;
    (scale, pad, line_h, list_top, list_bot)
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
    cover: Option<&CoverRgba>,
    selected: Option<&RomEntry>,
    scan_hint: Option<&str>,
) {
    clear_bg(overlay, fb_w, fb_h);
    let w = fb_w as f32;
    let h = fb_h as f32;
    let (scale, pad, line_h, list_top, list_bot) = library_list_metrics(fb_w, fb_h);
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

    // Right cover panel (~38% width); list uses the remainder.
    let cover_w = (w * 0.38).clamp(120.0 * scale, 220.0 * scale);
    let cover_x = w - pad - cover_w;
    let list_right = cover_x - pad * 0.75;
    let list_width = (list_right - pad).max(80.0 * scale);

    // Cover frame sized to art + one meta line (no empty strip below).
    let cover_top = list_top;
    let max_cover_h = (list_bot - cover_top).max(40.0);
    let inner_pad = 6.0 * scale;
    let art_w = cover_w - inner_pad * 2.0;
    let ms = scale * 0.72;
    let meta_text_h = 8.0 * ms;
    let sep_h = (1.0 * scale).max(1.0);
    let meta_pad_y = 1.0 * scale;
    let meta_h = sep_h + meta_pad_y + meta_text_h + meta_pad_y;

    let art_h = if let Some(img) = cover {
        let (_, _, _, fitted_h) = fit_rect(art_w, max_cover_h, img.width as f32, img.height as f32);
        fitted_h
    } else {
        // Placeholder box when cover is missing.
        (art_w * 1.4).min(max_cover_h * 0.85)
    };
    let art_area_h = (art_h + inner_pad * 1.5).min(max_cover_h - meta_h).max(24.0 * scale);
    let cover_h = (art_area_h + meta_h).min(max_cover_h);

    let border = (2.0 * scale).max(2.0);
    overlay.fill_rect(
        cover_x - border,
        cover_top - border,
        cover_w + border * 2.0,
        cover_h + border * 2.0,
        theme::BORDER,
    );
    overlay.fill_rect(cover_x, cover_top, cover_w, cover_h, theme::BG_PANEL);

    let art_x = cover_x + inner_pad;
    let art_y = cover_top + inner_pad;
    let draw_art_h = art_area_h - inner_pad * 1.5;

    if let Some(img) = cover {
        overlay.set_cover_rgba(img.width, img.height, &img.pixels);
        let (dx, dy, dw, dh) = fit_rect(art_w, draw_art_h, img.width as f32, img.height as f32);
        overlay.draw_cover(art_x + dx, art_y + dy, dw, dh);
    } else if selected.is_some() {
        let msg = "NO COVER";
        let ns = scale * 0.85;
        let mw = Overlay::text_width(msg, ns);
        overlay.text(
            cover_x + (cover_w - mw) * 0.5,
            art_y + draw_art_h * 0.4,
            ns,
            theme::TEXT_DIM,
            msg,
        );
    }

    // Publisher - year under the cover (no title).
    let sep_y = cover_top + art_area_h;
    overlay.fill_rect(
        cover_x + 2.0 * scale,
        sep_y,
        cover_w - 4.0 * scale,
        sep_h,
        theme::BORDER_DIM,
    );
    let max_chars = ((cover_w - inner_pad * 2.0) / (8.0 * ms)).floor().max(6.0) as usize;
    if selected.is_some() {
        let developer = selected
            .and_then(|r| r.developer.as_deref())
            .filter(|s| !s.is_empty() && *s != "-")
            .unwrap_or("—");
        let year = selected
            .and_then(|r| r.year.as_deref())
            .filter(|s| !s.is_empty() && *s != "-")
            .unwrap_or("????");
        let meta = format!("{developer} - {year}");
        let meta_disp = ellipsize(&meta, max_chars);
        let mw = Overlay::text_width(&meta_disp, ms);
        overlay.text(
            cover_x + (cover_w - mw) * 0.5,
            sep_y + sep_h + meta_pad_y,
            ms,
            theme::TEXT,
            &meta_disp,
        );
    }

    let visible = ((list_bot - list_top) / line_h).floor().max(1.0) as usize;

    if rows.is_empty() {
        let msg = "No ROMs found";
        let mw = Overlay::text_width(msg, scale);
        overlay.text(
            pad + (list_width - mw) * 0.5,
            list_top + line_h,
            scale,
            theme::TEXT_DIM,
            msg,
        );
    } else {
        let end = (scroll + visible).min(rows.len());
        for (vis_i, row_i) in (scroll..end).enumerate() {
            let y = list_top + vis_i as f32 * line_h;
            let selected = row_i == sel;
            let label = rows[row_i].label(roms);
            let is_action = matches!(rows[row_i], LibRow::SelectFolder);

            if selected {
                overlay.fill_rect(
                    pad * 0.5,
                    y - 1.0 * scale,
                    list_width + pad * 0.5,
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
            let max_chars = (list_width / (8.0 * scale)).floor().max(8.0) as usize;
            let display = if text.chars().count() > max_chars {
                let t: String = text.chars().take(max_chars.saturating_sub(1)).collect();
                format!("{t}~")
            } else {
                text
            };
            overlay.text(pad, y, scale, color, &display);
        }

        if scroll > 0 {
            overlay.text(
                list_right - 8.0 * scale,
                list_top,
                scale,
                theme::TEXT_DIM,
                "^",
            );
        }
        if end < rows.len() {
            overlay.text(
                list_right - 8.0 * scale,
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
    let hint = scan_hint.unwrap_or("D-pad move  Start OK  Select menu");
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

fn ellipsize(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let t: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{t}~")
    }
}

/// Letterbox `src` into `dst` (preserve aspect). Returns (dx, dy, dw, dh).
fn fit_rect(dst_w: f32, dst_h: f32, src_w: f32, src_h: f32) -> (f32, f32, f32, f32) {
    if src_w <= 0.0 || src_h <= 0.0 || dst_w <= 0.0 || dst_h <= 0.0 {
        return (0.0, 0.0, dst_w, dst_h);
    }
    let scale = (dst_w / src_w).min(dst_h / src_h);
    let dw = src_w * scale;
    let dh = src_h * scale;
    ((dst_w - dw) * 0.5, (dst_h - dh) * 0.5, dw, dh)
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
        scan_covers_label: &menu.scan_covers_label,
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
