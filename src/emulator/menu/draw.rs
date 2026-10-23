//! Pause / settings overlay drawing — same CRT palette as NES OS.

use crate::emulator::menu::screen::{
    LabelCtx, ScreenId, hint_for, layout_scale, row_step, screen_def,
};
use crate::emulator::menu::Menu;
use crate::emulator::overlay::Overlay;
use crate::emulator::theme;
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
        measure_dialog(id, scale, title_mul, def.title, &labels, hint);

    let panel_x = ((fb_w as f32 - panel_w) * 0.5).round();
    let panel_y = ((fb_h as f32 - panel_h) * 0.5).round();

    // Dim backdrop
    overlay.fill_rect(0.0, 0.0, fb_w as f32, fb_h as f32, [0.0, 0.0, 0.0, 0.55]);

    // Dialog: OS-style cyan border + panel
    let border = (BORDER * scale).max(2.0);
    overlay.fill_rect(
        panel_x - border,
        panel_y - border,
        panel_w + border * 2.0,
        panel_h + border * 2.0,
        theme::BORDER,
    );
    overlay.fill_rect(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        theme::BG_PANEL,
    );

    // Title
    let title_scale = scale * title_mul;
    let tw = Overlay::text_width(def.title, title_scale);
    let mut y = panel_y + content.pad_top;
    overlay.text(
        panel_x + (panel_w - tw) * 0.5,
        y,
        title_scale,
        theme::TITLE,
        def.title,
    );
    y += 8.0 * title_scale + content.title_gap;

    let sep_w = panel_w - content.pad_x * 2.0;
    overlay.fill_rect(
        panel_x + content.pad_x,
        y - content.title_gap * 0.45,
        sep_w,
        (1.0 * scale).max(1.0),
        theme::BORDER_DIM,
    );

    let step = row_step(id, scale);
    let blink = true; // selection prefix always shown; OS blinks in shell only
    for (i, line) in labels.iter().enumerate() {
        draw_menu_line(
            overlay,
            panel_x,
            panel_w,
            y,
            scale,
            i == menu.sel(),
            blink,
            line,
        );
        y += step;
    }

    y += content.hint_gap;
    let hint_scale = scale * 0.7;
    let hw = Overlay::text_width(hint, hint_scale);
    overlay.text(
        panel_x + (panel_w - hw) * 0.5,
        y,
        hint_scale,
        theme::TEXT_DIM,
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
        let w = Overlay::text_width(&format!("> {line}"), scale);
        content_w = content_w.max(w);
    }

    let mut content_h = 8.0 * title_scale + title_gap;
    content_h += step * labels.len() as f32;
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

fn draw_menu_line(
    overlay: &mut Overlay,
    panel_x: f32,
    panel_w: f32,
    y: f32,
    scale: f32,
    selected: bool,
    blink: bool,
    label: &str,
) {
    if selected {
        overlay.fill_rect(
            panel_x + 4.0 * scale,
            y - 1.0 * scale,
            panel_w - 8.0 * scale,
            12.0 * scale,
            theme::SELECT_BG,
        );
    }
    let color = if selected {
        theme::SELECT_TEXT
    } else {
        theme::TEXT
    };
    let prefix = if selected {
        if blink {
            "> "
        } else {
            "  "
        }
    } else {
        "  "
    };
    let text = format!("{prefix}{label}");
    let lw = Overlay::text_width(&text, scale);
    overlay.text(panel_x + (panel_w - lw) * 0.5, y, scale, color, &text);
}
