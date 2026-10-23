//! Pause / settings overlay drawing (centered dialog panel).

use crate::emulator::menu::screen::{
    ItemKind, LabelCtx, ScreenId, hint_for, layout_scale, row_step, screen_def,
};
use crate::emulator::menu::Menu;
use crate::emulator::overlay::Overlay;
use glfw::Glfw;

const PAD_X: f32 = 18.0;
const PAD_TOP: f32 = 14.0;
const PAD_BOTTOM: f32 = 12.0;
const TITLE_GAP: f32 = 12.0;
const HINT_GAP: f32 = 10.0;
const BORDER: f32 = 2.0;
const MIN_CONTENT_W: f32 = 180.0;

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
    let (scale, title_mul) = layout_scale(id, fb_w, fb_h);
    let hint = hint_for(id, menu.is_waiting_bind(), menu.map_target());

    let ctx = LabelCtx {
        settings: &menu.settings,
        map_target: menu.map_target(),
        waiting_bind: menu.waiting_bind(),
        glfw: Some(glfw),
    };

    let labels: Vec<String> = def.items.iter().map(|item| item.label(&ctx)).collect();
    let (panel_w, panel_h, content) =
        measure_dialog(id, scale, title_mul, def.title, def.items, &labels, hint);

    let panel_x = ((fb_w as f32 - panel_w) * 0.5).round();
    let panel_y = ((fb_h as f32 - panel_h) * 0.5).round();

    // Dim backdrop
    overlay.fill_rect(0.0, 0.0, fb_w as f32, fb_h as f32, [0.0, 0.0, 0.0, 0.55]);

    // Dialog: border + solid panel
    let border = (BORDER * scale).max(2.0);
    overlay.fill_rect(
        panel_x - border,
        panel_y - border,
        panel_w + border * 2.0,
        panel_h + border * 2.0,
        [0.55, 0.55, 0.58, 0.95],
    );
    overlay.fill_rect(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        [0.08, 0.09, 0.12, 0.96],
    );

    // Title
    let title_scale = scale * title_mul;
    let tw = Overlay::text_width(def.title, title_scale);
    let mut y = panel_y + content.pad_top;
    overlay.text(
        panel_x + (panel_w - tw) * 0.5,
        y,
        title_scale,
        [1.0, 1.0, 1.0, 1.0],
        def.title,
    );
    y += 8.0 * title_scale + content.title_gap;

    // Thin separator under title
    let sep_w = panel_w - content.pad_x * 2.0;
    overlay.fill_rect(
        panel_x + content.pad_x,
        y - content.title_gap * 0.45,
        sep_w,
        (1.0 * scale).max(1.0),
        [0.35, 0.35, 0.40, 0.9],
    );

    let step = row_step(id, scale);
    for (i, (item, line)) in def.items.iter().zip(labels.iter()).enumerate() {
        draw_menu_line(overlay, panel_x, panel_w, y, scale, i == menu.sel(), line);

        if matches!(item, ItemKind::Volume) {
            draw_volume_bar(overlay, panel_x, panel_w, y, scale, menu.settings.volume);
            y += 8.0 * scale;
        }
        y += step;
    }

    y += content.hint_gap;
    let hint_scale = scale * 0.7;
    let hw = Overlay::text_width(hint, hint_scale);
    overlay.text(
        panel_x + (panel_w - hw) * 0.5,
        y,
        hint_scale,
        [0.65, 0.65, 0.68, 1.0],
        hint,
    );
}

struct DialogMetrics {
    pad_x: f32,
    pad_top: f32,
    title_gap: f32,
    hint_gap: f32,
}

fn measure_dialog(
    id: ScreenId,
    scale: f32,
    title_mul: f32,
    title: &str,
    items: &[ItemKind],
    labels: &[String],
    hint: &str,
) -> (f32, f32, DialogMetrics) {
    let pad_x = PAD_X * scale;
    let pad_top = PAD_TOP * scale;
    let pad_bottom = PAD_BOTTOM * scale;
    let title_gap = TITLE_GAP * scale;
    let hint_gap = HINT_GAP * scale;
    let title_scale = scale * title_mul;
    let hint_scale = scale * 0.7;
    let step = row_step(id, scale);

    let mut content_w = Overlay::text_width(title, title_scale)
        .max(Overlay::text_width(hint, hint_scale))
        .max(MIN_CONTENT_W * scale);

    for line in labels {
        // "> " prefix for selected row
        let w = Overlay::text_width(&format!("> {line}"), scale);
        content_w = content_w.max(w);
    }
    if items.iter().any(|i| matches!(i, ItemKind::Volume)) {
        content_w = content_w.max(160.0 * scale);
    }

    let mut content_h = 8.0 * title_scale + title_gap;
    for item in items {
        content_h += step;
        if matches!(item, ItemKind::Volume) {
            content_h += 8.0 * scale;
        }
    }
    content_h += hint_gap + 8.0 * hint_scale;

    let panel_w = content_w + pad_x * 2.0;
    let panel_h = content_h + pad_top + pad_bottom;

    (
        panel_w,
        panel_h,
        DialogMetrics {
            pad_x,
            pad_top,
            title_gap,
            hint_gap,
        },
    )
}

fn draw_volume_bar(
    overlay: &mut Overlay,
    panel_x: f32,
    panel_w: f32,
    y: f32,
    scale: f32,
    volume: f32,
) {
    let bar_w = (160.0 * scale).min(panel_w - PAD_X * scale * 2.0);
    let bar_x = panel_x + (panel_w - bar_w) * 0.5;
    let bar_y = y + 10.0 * scale;
    overlay.fill_rect(bar_x, bar_y, bar_w, 4.0 * scale, [0.22, 0.22, 0.26, 1.0]);
    overlay.fill_rect(
        bar_x,
        bar_y,
        bar_w * volume,
        4.0 * scale,
        [0.3, 0.85, 0.4, 1.0],
    );
}

fn draw_menu_line(
    overlay: &mut Overlay,
    panel_x: f32,
    panel_w: f32,
    y: f32,
    scale: f32,
    selected: bool,
    label: &str,
) {
    let color = if selected {
        [1.0, 0.85, 0.2, 1.0]
    } else {
        [0.9, 0.9, 0.9, 1.0]
    };
    let prefix = if selected { "> " } else { "  " };
    let text = format!("{prefix}{label}");
    let lw = Overlay::text_width(&text, scale);
    overlay.text(panel_x + (panel_w - lw) * 0.5, y, scale, color, &text);
}
