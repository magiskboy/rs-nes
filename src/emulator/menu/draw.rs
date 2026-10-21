//! Pause / settings overlay drawing.

use crate::emulator::menu::screen::{
    ItemKind, LabelCtx, ScreenId, hint_for, layout_scale, row_step, screen_def,
};
use crate::emulator::menu::Menu;
use crate::emulator::overlay::Overlay;
use glfw::Glfw;

pub fn draw_menu(
    menu: &Menu,
    overlay: &mut Overlay,
    glfw: &Glfw,
    fb_w: i32,
    fb_h: i32,
) {
    let Some(id) = menu.top_screen() else {
        return;
    };
    let def = screen_def(id);
    let (scale, title_mul, title_y) = layout_scale(id, fb_w, fb_h);

    overlay.fill_rect(0.0, 0.0, fb_w as f32, fb_h as f32, [0.0, 0.0, 0.0, 0.55]);

    let title = def.title;
    let tw = Overlay::text_width(title, scale * title_mul);
    let mut y = fb_h as f32 * title_y;
    overlay.text(
        (fb_w as f32 - tw) * 0.5,
        y,
        scale * title_mul,
        [1.0, 1.0, 1.0, 1.0],
        title,
    );
    y += if id == ScreenId::Pause {
        16.0 * scale * title_mul
    } else if id == ScreenId::ButtonMap {
        15.0 * scale * title_mul
    } else {
        18.0 * scale * title_mul
    };

    let ctx = LabelCtx {
        settings: &menu.settings,
        map_target: menu.map_target(),
        waiting_bind: menu.waiting_bind(),
        glfw: Some(glfw),
    };

    let step = row_step(id, scale);
    for (i, &item) in def.items.iter().enumerate() {
        let line = item.label(&ctx);
        draw_menu_line(overlay, fb_w, y, scale, i == menu.sel(), &line);

        if matches!(item, ItemKind::Volume) {
            draw_volume_bar(overlay, fb_w, y, scale, menu.settings.volume);
            y += 8.0 * scale;
        }
        y += step;
    }

    draw_hint(
        overlay,
        fb_w,
        fb_h,
        scale,
        hint_for(id, menu.is_waiting_bind(), menu.map_target()),
    );
}

fn draw_volume_bar(overlay: &mut Overlay, fb_w: i32, y: f32, scale: f32, volume: f32) {
    let bar_w = 160.0 * scale;
    let bar_x = (fb_w as f32 - bar_w) * 0.5;
    let bar_y = y + 10.0 * scale;
    overlay.fill_rect(bar_x, bar_y, bar_w, 4.0 * scale, [0.25, 0.25, 0.25, 1.0]);
    overlay.fill_rect(
        bar_x,
        bar_y,
        bar_w * volume,
        4.0 * scale,
        [0.3, 0.85, 0.4, 1.0],
    );
}

fn draw_menu_line(overlay: &mut Overlay, fb_w: i32, y: f32, scale: f32, selected: bool, label: &str) {
    let color = if selected {
        [1.0, 0.85, 0.2, 1.0]
    } else {
        [0.9, 0.9, 0.9, 1.0]
    };
    let prefix = if selected { "> " } else { "  " };
    let text = format!("{prefix}{label}");
    let lw = Overlay::text_width(&text, scale);
    overlay.text((fb_w as f32 - lw) * 0.5, y, scale, color, &text);
}

fn draw_hint(overlay: &mut Overlay, fb_w: i32, fb_h: i32, scale: f32, hint: &str) {
    let hw = Overlay::text_width(hint, scale * 0.7);
    overlay.text(
        (fb_w as f32 - hw) * 0.5,
        fb_h as f32 - 22.0 * scale,
        scale * 0.7,
        [0.7, 0.7, 0.7, 1.0],
        hint,
    );
}
