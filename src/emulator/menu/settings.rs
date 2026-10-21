//! Emulator UI settings (volume, display, input maps).

use crate::emulator::controller::{DeviceAssign, GamepadMap, KeyboardMap};

#[derive(Debug, Clone)]
pub struct Settings {
    pub volume: f32,
    pub fullscreen: bool,
    pub debug_info: bool,
    pub devices: DeviceAssign,
    pub keyboard: KeyboardMap,
    pub gamepad: GamepadMap,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            fullscreen: false,
            debug_info: false,
            devices: DeviceAssign::default(),
            keyboard: KeyboardMap::default(),
            gamepad: GamepadMap::default(),
        }
    }
}
