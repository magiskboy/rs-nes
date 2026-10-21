//! Host input → NES controller bitmasks.
//!
//! Each NES player is assigned one host device (keyboard or a gamepad slot).
//! Defaults: P1 = keyboard, P2 = gamepad #2 (when connected).

use crate::bus::Button;
use glfw::{Action, GamepadAxis, GamepadButton, Glfw, JoystickId, Key};
use std::collections::HashSet;

const STICK_DEADZONE: f32 = 0.45;

/// Host device that feeds one NES controller port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDevice {
    /// No input (disconnected / unused port).
    None,
    Keyboard,
    Gamepad1,
    Gamepad2,
}

impl InputDevice {
    pub const PLAYER1_OPTIONS: [InputDevice; 3] = [
        InputDevice::Keyboard,
        InputDevice::Gamepad1,
        InputDevice::Gamepad2,
    ];

    pub const PLAYER2_OPTIONS: [InputDevice; 4] = [
        InputDevice::None,
        InputDevice::Keyboard,
        InputDevice::Gamepad1,
        InputDevice::Gamepad2,
    ];

    pub fn label(self) -> &'static str {
        match self {
            InputDevice::None => "None",
            InputDevice::Keyboard => "Keyboard",
            InputDevice::Gamepad1 => "Gamepad 1",
            InputDevice::Gamepad2 => "Gamepad 2",
        }
    }

    pub fn cycle(self, options: &[InputDevice], dir: i32) -> InputDevice {
        let Some(i) = options.iter().position(|&d| d == self) else {
            return options.first().copied().unwrap_or(InputDevice::None);
        };
        let n = options.len() as i32;
        let next = (i as i32 + dir).rem_euclid(n) as usize;
        options[next]
    }

    fn joystick_id(self) -> Option<JoystickId> {
        match self {
            InputDevice::Gamepad1 => Some(JoystickId::Joystick1),
            InputDevice::Gamepad2 => Some(JoystickId::Joystick2),
            InputDevice::None | InputDevice::Keyboard => None,
        }
    }
}

/// Which host devices feed NES ports 1 and 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceAssign {
    pub player1: InputDevice,
    pub player2: InputDevice,
}

impl Default for DeviceAssign {
    fn default() -> Self {
        Self {
            player1: InputDevice::Keyboard,
            player2: InputDevice::Gamepad2,
        }
    }
}

/// True when the given gamepad slot is present (gamepad or raw joystick).
pub fn device_connected(glfw: &Glfw, device: InputDevice) -> bool {
    match device.joystick_id() {
        Some(id) => glfw.get_joystick(id).is_present(),
        None => matches!(device, InputDevice::Keyboard),
    }
}

/// Short status suffix for the Controllers menu (e.g. " (off)").
pub fn device_status(glfw: &Glfw, device: InputDevice) -> &'static str {
    match device {
        InputDevice::None => "",
        InputDevice::Keyboard => "",
        InputDevice::Gamepad1 | InputDevice::Gamepad2 => {
            if device_connected(glfw, device) {
                ""
            } else {
                " (off)"
            }
        }
    }
}

/// NES face / d-pad buttons that can be remapped on a gamepad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NesPadButton {
    A,
    B,
    Start,
    Select,
    Up,
    Down,
    Left,
    Right,
}

impl NesPadButton {
    pub const ALL: [NesPadButton; 8] = [
        NesPadButton::A,
        NesPadButton::B,
        NesPadButton::Start,
        NesPadButton::Select,
        NesPadButton::Up,
        NesPadButton::Down,
        NesPadButton::Left,
        NesPadButton::Right,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NesPadButton::A => "A",
            NesPadButton::B => "B",
            NesPadButton::Start => "Start",
            NesPadButton::Select => "Select",
            NesPadButton::Up => "D-Pad Up",
            NesPadButton::Down => "D-Pad Down",
            NesPadButton::Left => "D-Pad Left",
            NesPadButton::Right => "D-Pad Right",
        }
    }

    fn nes_bit(self) -> u8 {
        match self {
            NesPadButton::A => Button::A as u8,
            NesPadButton::B => Button::B as u8,
            NesPadButton::Start => Button::Start as u8,
            NesPadButton::Select => Button::Select as u8,
            NesPadButton::Up => Button::Up as u8,
            NesPadButton::Down => Button::Down as u8,
            NesPadButton::Left => Button::Left as u8,
            NesPadButton::Right => Button::Right as u8,
        }
    }
}

/// Which host map the Button Map screen is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapTarget {
    Keyboard,
    Gamepad,
}

impl MapTarget {
    pub const ALL: [MapTarget; 2] = [MapTarget::Keyboard, MapTarget::Gamepad];

    pub fn label(self) -> &'static str {
        match self {
            MapTarget::Keyboard => "Keyboard",
            MapTarget::Gamepad => "Gamepad",
        }
    }

    pub fn cycle(self, dir: i32) -> MapTarget {
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0) as i32;
        let n = Self::ALL.len() as i32;
        Self::ALL[(i + dir).rem_euclid(n) as usize]
    }
}

/// Maps each NES button to a keyboard key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardMap {
    pub a: Key,
    pub b: Key,
    pub start: Key,
    pub select: Key,
    pub up: Key,
    pub down: Key,
    pub left: Key,
    pub right: Key,
}

impl Default for KeyboardMap {
    fn default() -> Self {
        Self {
            a: Key::Z,
            b: Key::X,
            start: Key::Enter,
            select: Key::RightShift,
            up: Key::Up,
            down: Key::Down,
            left: Key::Left,
            right: Key::Right,
        }
    }
}

impl KeyboardMap {
    pub fn get(&self, nes: NesPadButton) -> Key {
        match nes {
            NesPadButton::A => self.a,
            NesPadButton::B => self.b,
            NesPadButton::Start => self.start,
            NesPadButton::Select => self.select,
            NesPadButton::Up => self.up,
            NesPadButton::Down => self.down,
            NesPadButton::Left => self.left,
            NesPadButton::Right => self.right,
        }
    }

    pub fn set(&mut self, nes: NesPadButton, key: Key) {
        match nes {
            NesPadButton::A => self.a = key,
            NesPadButton::B => self.b = key,
            NesPadButton::Start => self.start = key,
            NesPadButton::Select => self.select = key,
            NesPadButton::Up => self.up = key,
            NesPadButton::Down => self.down = key,
            NesPadButton::Left => self.left = key,
            NesPadButton::Right => self.right = key,
        }
    }
}

/// Maps each NES button to a GLFW gamepad button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamepadMap {
    pub a: GamepadButton,
    pub b: GamepadButton,
    pub start: GamepadButton,
    pub select: GamepadButton,
    pub up: GamepadButton,
    pub down: GamepadButton,
    pub left: GamepadButton,
    pub right: GamepadButton,
}

impl Default for GamepadMap {
    fn default() -> Self {
        Self {
            a: GamepadButton::ButtonA,
            b: GamepadButton::ButtonB,
            start: GamepadButton::ButtonStart,
            select: GamepadButton::ButtonBack,
            up: GamepadButton::ButtonDpadUp,
            down: GamepadButton::ButtonDpadDown,
            left: GamepadButton::ButtonDpadLeft,
            right: GamepadButton::ButtonDpadRight,
        }
    }
}

impl GamepadMap {
    pub fn get(&self, nes: NesPadButton) -> GamepadButton {
        match nes {
            NesPadButton::A => self.a,
            NesPadButton::B => self.b,
            NesPadButton::Start => self.start,
            NesPadButton::Select => self.select,
            NesPadButton::Up => self.up,
            NesPadButton::Down => self.down,
            NesPadButton::Left => self.left,
            NesPadButton::Right => self.right,
        }
    }

    pub fn set(&mut self, nes: NesPadButton, pad: GamepadButton) {
        match nes {
            NesPadButton::A => self.a = pad,
            NesPadButton::B => self.b = pad,
            NesPadButton::Start => self.start = pad,
            NesPadButton::Select => self.select = pad,
            NesPadButton::Up => self.up = pad,
            NesPadButton::Down => self.down = pad,
            NesPadButton::Left => self.left = pad,
            NesPadButton::Right => self.right = pad,
        }
    }
}

/// Human-readable key name (fits the bitmap font).
pub fn key_name(key: Key) -> &'static str {
    match key {
        Key::Space => "Space",
        Key::Apostrophe => "'",
        Key::Comma => ",",
        Key::Minus => "-",
        Key::Period => ".",
        Key::Slash => "/",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::Semicolon => ";",
        Key::Equal => "=",
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        Key::LeftBracket => "[",
        Key::Backslash => "\\",
        Key::RightBracket => "]",
        Key::GraveAccent => "`",
        Key::Escape => "Esc",
        Key::Enter => "Enter",
        Key::Tab => "Tab",
        Key::Backspace => "Bksp",
        Key::Insert => "Ins",
        Key::Delete => "Del",
        Key::Right => "Right",
        Key::Left => "Left",
        Key::Down => "Down",
        Key::Up => "Up",
        Key::PageUp => "PgUp",
        Key::PageDown => "PgDn",
        Key::Home => "Home",
        Key::End => "End",
        Key::CapsLock => "Caps",
        Key::LeftShift => "LShift",
        Key::RightShift => "RShift",
        Key::LeftControl => "LCtrl",
        Key::RightControl => "RCtrl",
        Key::LeftAlt => "LAlt",
        Key::RightAlt => "RAlt",
        Key::LeftSuper => "LSuper",
        Key::RightSuper => "RSuper",
        Key::Kp0 => "Num0",
        Key::Kp1 => "Num1",
        Key::Kp2 => "Num2",
        Key::Kp3 => "Num3",
        Key::Kp4 => "Num4",
        Key::Kp5 => "Num5",
        Key::Kp6 => "Num6",
        Key::Kp7 => "Num7",
        Key::Kp8 => "Num8",
        Key::Kp9 => "Num9",
        Key::KpEnter => "NumEnt",
        Key::KpAdd => "Num+",
        Key::KpSubtract => "Num-",
        Key::KpMultiply => "Num*",
        Key::KpDivide => "Num/",
        _ => "Key",
    }
}

/// Human-readable GLFW gamepad button name (fits the bitmap font).
pub fn gamepad_button_name(button: GamepadButton) -> &'static str {
    match button {
        GamepadButton::ButtonA => "A",
        GamepadButton::ButtonB => "B",
        GamepadButton::ButtonX => "X",
        GamepadButton::ButtonY => "Y",
        GamepadButton::ButtonLeftBumper => "LB",
        GamepadButton::ButtonRightBumper => "RB",
        GamepadButton::ButtonBack => "Back",
        GamepadButton::ButtonStart => "Start",
        GamepadButton::ButtonGuide => "Guide",
        GamepadButton::ButtonLeftThumb => "LStick",
        GamepadButton::ButtonRightThumb => "RStick",
        GamepadButton::ButtonDpadUp => "DUp",
        GamepadButton::ButtonDpadDown => "DDown",
        GamepadButton::ButtonDpadLeft => "DLeft",
        GamepadButton::ButtonDpadRight => "DRight",
    }
}

const ALL_GAMEPAD_BUTTONS: [GamepadButton; 15] = [
    GamepadButton::ButtonA,
    GamepadButton::ButtonB,
    GamepadButton::ButtonX,
    GamepadButton::ButtonY,
    GamepadButton::ButtonLeftBumper,
    GamepadButton::ButtonRightBumper,
    GamepadButton::ButtonBack,
    GamepadButton::ButtonStart,
    GamepadButton::ButtonGuide,
    GamepadButton::ButtonLeftThumb,
    GamepadButton::ButtonRightThumb,
    GamepadButton::ButtonDpadUp,
    GamepadButton::ButtonDpadDown,
    GamepadButton::ButtonDpadLeft,
    GamepadButton::ButtonDpadRight,
];

/// Tracks keyboard state and polls GLFW gamepads each frame.
#[derive(Debug, Default)]
pub struct Controllers {
    keys_down: HashSet<Key>,
}

impl Controllers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update keyboard tracking from a GLFW key event (Press / Repeat / Release).
    pub fn on_key(&mut self, key: Key, action: Action) {
        match action {
            Action::Press | Action::Repeat => {
                self.keys_down.insert(key);
            }
            Action::Release => {
                self.keys_down.remove(&key);
            }
        }
    }

    /// Clear held keys when the window loses focus (avoids stuck buttons).
    pub fn on_focus(&mut self, focused: bool) {
        if !focused {
            self.keys_down.clear();
        }
    }

    /// NES button masks for players 0 and 1 from the current device assignment.
    pub fn poll(
        &self,
        glfw: &Glfw,
        keyboard: &KeyboardMap,
        gamepad: &GamepadMap,
        assign: &DeviceAssign,
    ) -> [u8; 2] {
        [
            self.device_buttons(glfw, keyboard, gamepad, assign.player1),
            self.device_buttons(glfw, keyboard, gamepad, assign.player2),
        ]
    }

    fn device_buttons(
        &self,
        glfw: &Glfw,
        keyboard: &KeyboardMap,
        gamepad: &GamepadMap,
        device: InputDevice,
    ) -> u8 {
        match device {
            InputDevice::None => 0,
            InputDevice::Keyboard => buttons_from_keys(&self.keys_down, keyboard),
            InputDevice::Gamepad1 => {
                buttons_from_joystick(glfw, JoystickId::Joystick1, gamepad)
            }
            InputDevice::Gamepad2 => {
                buttons_from_joystick(glfw, JoystickId::Joystick2, gamepad)
            }
        }
    }
}

/// Every currently held gamepad button across all joystick slots (for rebind UI).
///
/// Prefers GLFW gamepad state; falls back to raw joystick button indices so
/// controllers without a gamepad mapping can still be remapped.
pub fn held_gamepad_buttons(glfw: &Glfw) -> Vec<GamepadButton> {
    let mut held = Vec::new();
    for id in all_joystick_ids() {
        let joy = glfw.get_joystick(id);
        if !joy.is_present() {
            continue;
        }
        if joy.is_gamepad() {
            if let Some(state) = joy.get_gamepad_state() {
                for &b in &ALL_GAMEPAD_BUTTONS {
                    if pressed(&state, b) {
                        push_unique(&mut held, b);
                    }
                }
                continue;
            }
        }
        // Raw fallback: button index → GamepadButton when possible.
        for (i, &down) in joy.get_buttons().iter().enumerate() {
            if down != 0 {
                if let Some(b) = GamepadButton::from_i32(i as i32) {
                    push_unique(&mut held, b);
                }
            }
        }
    }
    held
}

fn push_unique(held: &mut Vec<GamepadButton>, button: GamepadButton) {
    if !held.contains(&button) {
        held.push(button);
    }
}

fn all_joystick_ids() -> [JoystickId; 16] {
    [
        JoystickId::Joystick1,
        JoystickId::Joystick2,
        JoystickId::Joystick3,
        JoystickId::Joystick4,
        JoystickId::Joystick5,
        JoystickId::Joystick6,
        JoystickId::Joystick7,
        JoystickId::Joystick8,
        JoystickId::Joystick9,
        JoystickId::Joystick10,
        JoystickId::Joystick11,
        JoystickId::Joystick12,
        JoystickId::Joystick13,
        JoystickId::Joystick14,
        JoystickId::Joystick15,
        JoystickId::Joystick16,
    ]
}

fn buttons_from_keys(keys: &HashSet<Key>, map: &KeyboardMap) -> u8 {
    let mut m = 0u8;
    for nes in NesPadButton::ALL {
        if keys.contains(&map.get(nes)) {
            m |= nes.nes_bit();
        }
    }
    m
}

fn buttons_from_joystick(glfw: &Glfw, id: JoystickId, map: &GamepadMap) -> u8 {
    let joy = glfw.get_joystick(id);
    if !joy.is_present() {
        return 0;
    }
    if joy.is_gamepad() {
        if let Some(state) = joy.get_gamepad_state() {
            return buttons_from_gamepad(&state, map);
        }
    }
    buttons_from_raw_joystick(&joy)
}

fn buttons_from_gamepad(state: &glfw::GamepadState, map: &GamepadMap) -> u8 {
    let mut m = 0u8;

    for nes in NesPadButton::ALL {
        if pressed(state, map.get(nes)) {
            m |= nes.nes_bit();
        }
    }

    // Left stick always contributes to D-pad (in addition to remapped buttons).
    let lx = state.get_axis(GamepadAxis::AxisLeftX);
    let ly = state.get_axis(GamepadAxis::AxisLeftY);
    let mut up = ly <= -STICK_DEADZONE;
    let mut down = ly >= STICK_DEADZONE;
    let mut left = lx <= -STICK_DEADZONE;
    let mut right = lx >= STICK_DEADZONE;

    up |= m & Button::Up as u8 != 0;
    down |= m & Button::Down as u8 != 0;
    left |= m & Button::Left as u8 != 0;
    right |= m & Button::Right as u8 != 0;

    m &= !(Button::Up as u8
        | Button::Down as u8
        | Button::Left as u8
        | Button::Right as u8);
    if up && !down {
        m |= Button::Up as u8;
    }
    if down && !up {
        m |= Button::Down as u8;
    }
    if left && !right {
        m |= Button::Left as u8;
    }
    if right && !left {
        m |= Button::Right as u8;
    }

    m
}

fn pressed(state: &glfw::GamepadState, button: GamepadButton) -> bool {
    matches!(
        state.get_button_state(button),
        Action::Press | Action::Repeat
    )
}

fn buttons_from_raw_joystick(joy: &glfw::Joystick) -> u8 {
    let mut m = 0u8;
    let buttons = joy.get_buttons();
    let axes = joy.get_axes();
    let hats = joy.get_hats();

    if buttons.first().is_some_and(|&b| b != 0) {
        m |= Button::A as u8;
    }
    if buttons.get(1).is_some_and(|&b| b != 0) {
        m |= Button::B as u8;
    }
    if buttons.get(7).is_some_and(|&b| b != 0) || buttons.get(9).is_some_and(|&b| b != 0) {
        m |= Button::Start as u8;
    }
    if buttons.get(6).is_some_and(|&b| b != 0) || buttons.get(8).is_some_and(|&b| b != 0) {
        m |= Button::Select as u8;
    }

    let mut up = false;
    let mut down = false;
    let mut left = false;
    let mut right = false;

    if let Some(hat) = hats.first() {
        up |= hat.contains(glfw::JoystickHats::Up);
        down |= hat.contains(glfw::JoystickHats::Down);
        left |= hat.contains(glfw::JoystickHats::Left);
        right |= hat.contains(glfw::JoystickHats::Right);
    }

    if let (Some(&lx), Some(&ly)) = (axes.first(), axes.get(1)) {
        if lx <= -STICK_DEADZONE {
            left = true;
        }
        if lx >= STICK_DEADZONE {
            right = true;
        }
        if ly <= -STICK_DEADZONE {
            up = true;
        }
        if ly >= STICK_DEADZONE {
            down = true;
        }
    }

    if up && !down {
        m |= Button::Up as u8;
    }
    if down && !up {
        m |= Button::Down as u8;
    }
    if left && !right {
        m |= Button::Left as u8;
    }
    if right && !left {
        m |= Button::Right as u8;
    }

    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_maps_default_face_and_dpad() {
        let map = KeyboardMap::default();
        let mut keys = HashSet::new();
        keys.insert(Key::Z);
        keys.insert(Key::Up);
        keys.insert(Key::Left);
        let m = buttons_from_keys(&keys, &map);
        assert_ne!(m & Button::A as u8, 0);
        assert_ne!(m & Button::Up as u8, 0);
        assert_ne!(m & Button::Left as u8, 0);
        assert_eq!(m & Button::B as u8, 0);
    }

    #[test]
    fn keyboard_map_remap_applies() {
        let mut map = KeyboardMap::default();
        map.set(NesPadButton::A, Key::J);
        let m = buttons_from_keys(&HashSet::from([Key::J]), &map);
        assert_ne!(m & Button::A as u8, 0);
        assert_eq!(buttons_from_keys(&HashSet::from([Key::Z]), &map) & Button::A as u8, 0);
    }

    #[test]
    fn keyboard_or_merge_is_additive() {
        let map = KeyboardMap::default();
        let a = buttons_from_keys(&HashSet::from([Key::Z]), &map);
        let b = buttons_from_keys(&HashSet::from([Key::X]), &map);
        assert_eq!(a | b, (Button::A as u8) | (Button::B as u8));
    }

    #[test]
    fn gamepad_map_roundtrip() {
        let mut map = GamepadMap::default();
        map.set(NesPadButton::A, GamepadButton::ButtonX);
        assert_eq!(map.get(NesPadButton::A), GamepadButton::ButtonX);
        assert_eq!(map.get(NesPadButton::B), GamepadButton::ButtonB);
    }

    #[test]
    fn device_cycle_wraps() {
        let d = InputDevice::Keyboard;
        assert_eq!(
            d.cycle(&InputDevice::PLAYER1_OPTIONS, 1),
            InputDevice::Gamepad1
        );
        assert_eq!(
            InputDevice::Gamepad2.cycle(&InputDevice::PLAYER1_OPTIONS, 1),
            InputDevice::Keyboard
        );
        assert_eq!(
            InputDevice::None.cycle(&InputDevice::PLAYER2_OPTIONS, -1),
            InputDevice::Gamepad2
        );
    }

    #[test]
    fn default_assign_is_keyboard_and_pad2() {
        let a = DeviceAssign::default();
        assert_eq!(a.player1, InputDevice::Keyboard);
        assert_eq!(a.player2, InputDevice::Gamepad2);
    }
}
