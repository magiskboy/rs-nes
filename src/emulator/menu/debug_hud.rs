//! In-game debug HUD (top-left).

use crate::emulator::overlay::Overlay;
use crate::nes::Nes;

/// Draw the in-game debug HUD (top-left).
pub fn draw_debug(
    overlay: &mut Overlay,
    nes: &Nes,
    fps: f32,
    buttons: [u8; 2],
    fb_w: i32,
    _fb_h: i32,
) {
    let scale = ((fb_w as f32) / 480.0).clamp(1.5, 3.0);
    let mut y = 8.0 * scale;
    let x = 8.0 * scale;
    let color = [0.2, 1.0, 0.45, 1.0];
    let shadow = [0.0, 0.0, 0.0, 0.75];

    let lines = debug_lines(nes, fps, buttons);
    let line_h = 10.0 * scale;
    let max_w = lines
        .iter()
        .map(|l| Overlay::text_width(l, scale))
        .fold(0.0_f32, f32::max);
    overlay.fill_rect(
        x - 4.0,
        y - 4.0,
        max_w + 8.0,
        line_h * lines.len() as f32 + 8.0,
        [0.0, 0.0, 0.0, 0.45],
    );

    for line in &lines {
        overlay.text(x + 1.0, y + 1.0, scale, shadow, line);
        overlay.text(x, y, scale, color, line);
        y += line_h;
    }
}

fn debug_lines(nes: &Nes, fps: f32, buttons: [u8; 2]) -> Vec<String> {
    let p = nes.status();
    let flags = format!(
        "{}{}{}{}{}{}{}{}",
        if p & 0x80 != 0 { 'N' } else { 'n' },
        if p & 0x40 != 0 { 'V' } else { 'v' },
        '-',
        if p & 0x10 != 0 { 'B' } else { 'b' },
        if p & 0x08 != 0 { 'D' } else { 'd' },
        if p & 0x04 != 0 { 'I' } else { 'i' },
        if p & 0x02 != 0 { 'Z' } else { 'z' },
        if p & 0x01 != 0 { 'C' } else { 'c' },
    );

    let zp: String = (0u16..16)
        .map(|i| format!("{:02X}", nes.peek(i)))
        .collect::<Vec<_>>()
        .join(" ");

    vec![
        format!("FPS  {fps:5.1}"),
        format!("FRM  {}", nes.frame()),
        format!(
            "CPU  PC=${:04X} A=${:02X} X=${:02X} Y=${:02X} SP=${:02X}",
            nes.pc(),
            nes.a(),
            nes.x(),
            nes.y(),
            nes.sp()
        ),
        format!("     P=${:02X} [{flags}]  CYC={}", p, nes.cycles()),
        format!("RAM  {zp}"),
        format!("P1   {}", format_buttons(buttons[0])),
        format!("P2   {}", format_buttons(buttons[1])),
    ]
}

fn format_buttons(buttons: u8) -> String {
    use crate::bus::Button;
    let bits = [
        (Button::A, "A"),
        (Button::B, "B"),
        (Button::Select, "Se"),
        (Button::Start, "St"),
        (Button::Up, "U"),
        (Button::Down, "D"),
        (Button::Left, "L"),
        (Button::Right, "R"),
    ];
    let held: Vec<&str> = bits
        .iter()
        .filter(|(b, _)| buttons & (*b as u8) != 0)
        .map(|(_, name)| *name)
        .collect();
    if held.is_empty() {
        "-".into()
    } else {
        held.join(" ")
    }
}
