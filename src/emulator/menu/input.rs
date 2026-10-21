//! Raw keys → semantic menu navigation.

use glfw::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
}

pub fn map_key(key: Key) -> Option<NavAction> {
    match key {
        Key::Up | Key::W => Some(NavAction::Up),
        Key::Down | Key::S => Some(NavAction::Down),
        Key::Left | Key::A => Some(NavAction::Left),
        Key::Right | Key::D => Some(NavAction::Right),
        Key::Enter | Key::Z | Key::Space => Some(NavAction::Confirm),
        Key::Escape | Key::X | Key::Backspace => Some(NavAction::Back),
        _ => None,
    }
}
