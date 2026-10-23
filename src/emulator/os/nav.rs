//! Map NES controller bits → OS navigation (D-pad / A / B / Select / Start only).

use crate::bus::Button;
use crate::emulator::menu::input::NavAction;

/// Newly pressed NES buttons (edge: in `pad` but not in `prev`).
pub fn pressed_edges(pad: u8, prev: u8) -> u8 {
    pad & !prev
}

/// Translate a single newly pressed NES button into a menu [`NavAction`].
///
/// - Start / A → Confirm  
/// - B → Back  
/// - D-pad → direction  
/// - Select is handled separately (system menu).
pub fn nav_from_button(bit: u8) -> Option<NavAction> {
    if bit == Button::Up as u8 {
        Some(NavAction::Up)
    } else if bit == Button::Down as u8 {
        Some(NavAction::Down)
    } else if bit == Button::Left as u8 {
        Some(NavAction::Left)
    } else if bit == Button::Right as u8 {
        Some(NavAction::Right)
    } else if bit == Button::Start as u8 || bit == Button::A as u8 {
        Some(NavAction::Confirm)
    } else if bit == Button::B as u8 {
        Some(NavAction::Back)
    } else {
        None
    }
}

pub fn is_select(bit: u8) -> bool {
    bit == Button::Select as u8
}

/// Iterate set bits in a mask (low bit first).
pub fn each_bit(mask: u8) -> impl Iterator<Item = u8> {
    (0..8).filter_map(move |i| {
        let b = 1u8 << i;
        if mask & b != 0 { Some(b) } else { None }
    })
}
