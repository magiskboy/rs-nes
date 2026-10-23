//! Persist settings under the per-user config directory.
//!
//! | OS      | Path |
//! |---------|------|
//! | Linux   | `$XDG_CONFIG_HOME/rs-nes/config.toml` or `~/.config/rs-nes/config.toml` |
//! | macOS   | `~/Library/Application Support/rs-nes/config.toml` |
//! | Windows | `%APPDATA%\rs-nes\config.toml` |

use crate::emulator::controller::{
    gamepad_button_name, key_name, DeviceAssign, GamepadMap, InputDevice, KeyboardMap,
};
use crate::emulator::menu::Settings;
use crate::emulator::video::VideoFilter;
use glfw::{GamepadButton, Key};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_VERSION: u32 = 1;
const APP_DIR: &str = "rs-nes";
const FILE_NAME: &str = "config.toml";

/// Loaded / saved user configuration.
#[derive(Debug, Clone, Default)]
pub struct UserConfig {
    pub settings: Settings,
    pub extra_dirs: Vec<PathBuf>,
}

/// Directory that holds `config.toml` (created on save).
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join(APP_DIR))
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|h| h.join("Library/Application Support").join(APP_DIR))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            Some(PathBuf::from(xdg).join(APP_DIR))
        } else {
            home_dir().map(|h| h.join(".config").join(APP_DIR))
        }
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        home_dir().map(|h| h.join(".config").join(APP_DIR))
    }
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(FILE_NAME))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Load config from disk. Missing file → defaults. Corrupt file → defaults + warning.
pub fn load() -> UserConfig {
    let Some(path) = config_path() else {
        return UserConfig::default();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return UserConfig::default();
    };
    match toml::from_str::<ConfigDto>(&text) {
        Ok(dto) => dto.into_user_config(),
        Err(e) => {
            eprintln!("warning: ignoring corrupt config {}: {e}", path.display());
            UserConfig::default()
        }
    }
}

/// Write settings + extra ROM folders. Creates the config directory if needed.
pub fn save(settings: &Settings, extra_dirs: &[PathBuf]) -> Result<(), String> {
    let dir = config_dir().ok_or_else(|| "cannot resolve config directory".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join(FILE_NAME);
    let dto = ConfigDto::from_user(settings, extra_dirs);
    let text = toml::to_string_pretty(&dto).map_err(|e| format!("serialize config: {e}"))?;
    fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigDto {
    version: u32,
    volume: f32,
    fullscreen: bool,
    debug_info: bool,
    video_filter: String,
    #[serde(default)]
    extra_dirs: Vec<String>,
    devices: DevicesDto,
    keyboard: ButtonMapDto,
    gamepad: ButtonMapDto,
}

#[derive(Debug, Serialize, Deserialize)]
struct DevicesDto {
    player1: String,
    player2: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ButtonMapDto {
    a: String,
    b: String,
    start: String,
    select: String,
    up: String,
    down: String,
    left: String,
    right: String,
}

impl ConfigDto {
    fn from_user(settings: &Settings, extra_dirs: &[PathBuf]) -> Self {
        Self {
            version: CONFIG_VERSION,
            volume: settings.volume.clamp(0.0, 1.0),
            fullscreen: settings.fullscreen,
            debug_info: settings.debug_info,
            video_filter: filter_to_str(settings.video_filter).into(),
            extra_dirs: extra_dirs
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            devices: DevicesDto {
                player1: device_to_str(settings.devices.player1).into(),
                player2: device_to_str(settings.devices.player2).into(),
            },
            keyboard: ButtonMapDto::from_keyboard(&settings.keyboard),
            gamepad: ButtonMapDto::from_gamepad(&settings.gamepad),
        }
    }

    fn into_user_config(self) -> UserConfig {
        let defaults = Settings::default();
        let settings = Settings {
            volume: self.volume.clamp(0.0, 1.0),
            fullscreen: self.fullscreen,
            debug_info: self.debug_info,
            video_filter: filter_from_str(&self.video_filter).unwrap_or(defaults.video_filter),
            devices: DeviceAssign {
                player1: device_from_str(&self.devices.player1)
                    .unwrap_or(defaults.devices.player1),
                player2: device_from_str(&self.devices.player2)
                    .unwrap_or(defaults.devices.player2),
            },
            keyboard: self.keyboard.into_keyboard(&defaults.keyboard),
            gamepad: self.gamepad.into_gamepad(&defaults.gamepad),
        };
        let extra_dirs = self
            .extra_dirs
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .collect();
        UserConfig {
            settings,
            extra_dirs,
        }
    }
}

impl ButtonMapDto {
    fn from_keyboard(map: &KeyboardMap) -> Self {
        Self {
            a: key_name(map.a).into(),
            b: key_name(map.b).into(),
            start: key_name(map.start).into(),
            select: key_name(map.select).into(),
            up: key_name(map.up).into(),
            down: key_name(map.down).into(),
            left: key_name(map.left).into(),
            right: key_name(map.right).into(),
        }
    }

    fn from_gamepad(map: &GamepadMap) -> Self {
        Self {
            a: gamepad_button_name(map.a).into(),
            b: gamepad_button_name(map.b).into(),
            start: gamepad_button_name(map.start).into(),
            select: gamepad_button_name(map.select).into(),
            up: gamepad_button_name(map.up).into(),
            down: gamepad_button_name(map.down).into(),
            left: gamepad_button_name(map.left).into(),
            right: gamepad_button_name(map.right).into(),
        }
    }

    fn into_keyboard(self, fallback: &KeyboardMap) -> KeyboardMap {
        KeyboardMap {
            a: key_from_name(&self.a).unwrap_or(fallback.a),
            b: key_from_name(&self.b).unwrap_or(fallback.b),
            start: key_from_name(&self.start).unwrap_or(fallback.start),
            select: key_from_name(&self.select).unwrap_or(fallback.select),
            up: key_from_name(&self.up).unwrap_or(fallback.up),
            down: key_from_name(&self.down).unwrap_or(fallback.down),
            left: key_from_name(&self.left).unwrap_or(fallback.left),
            right: key_from_name(&self.right).unwrap_or(fallback.right),
        }
    }

    fn into_gamepad(self, fallback: &GamepadMap) -> GamepadMap {
        GamepadMap {
            a: pad_from_name(&self.a).unwrap_or(fallback.a),
            b: pad_from_name(&self.b).unwrap_or(fallback.b),
            start: pad_from_name(&self.start).unwrap_or(fallback.start),
            select: pad_from_name(&self.select).unwrap_or(fallback.select),
            up: pad_from_name(&self.up).unwrap_or(fallback.up),
            down: pad_from_name(&self.down).unwrap_or(fallback.down),
            left: pad_from_name(&self.left).unwrap_or(fallback.left),
            right: pad_from_name(&self.right).unwrap_or(fallback.right),
        }
    }
}

fn filter_to_str(f: VideoFilter) -> &'static str {
    match f {
        VideoFilter::Sharp => "sharp",
        VideoFilter::Smooth => "smooth",
        VideoFilter::Scanlines => "scanlines",
        VideoFilter::Crt => "crt",
    }
}

fn filter_from_str(s: &str) -> Option<VideoFilter> {
    match s.trim().to_ascii_lowercase().as_str() {
        "sharp" | "nearest" => Some(VideoFilter::Sharp),
        "smooth" | "linear" => Some(VideoFilter::Smooth),
        "scanlines" | "scanline" => Some(VideoFilter::Scanlines),
        "crt" => Some(VideoFilter::Crt),
        _ => None,
    }
}

fn device_to_str(d: InputDevice) -> &'static str {
    match d {
        InputDevice::None => "none",
        InputDevice::Keyboard => "keyboard",
        InputDevice::Gamepad1 => "gamepad1",
        InputDevice::Gamepad2 => "gamepad2",
    }
}

fn device_from_str(s: &str) -> Option<InputDevice> {
    match s.trim().to_ascii_lowercase().as_str() {
        "none" => Some(InputDevice::None),
        "keyboard" => Some(InputDevice::Keyboard),
        "gamepad1" | "pad1" => Some(InputDevice::Gamepad1),
        "gamepad2" | "pad2" => Some(InputDevice::Gamepad2),
        _ => None,
    }
}

fn key_from_name(name: &str) -> Option<Key> {
    // Prefer exact match against the same labels we write.
    const KEYS: &[Key] = &[
        Key::Space,
        Key::Apostrophe,
        Key::Comma,
        Key::Minus,
        Key::Period,
        Key::Slash,
        Key::Num0,
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
        Key::Semicolon,
        Key::Equal,
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
        Key::LeftBracket,
        Key::Backslash,
        Key::RightBracket,
        Key::GraveAccent,
        Key::Escape,
        Key::Enter,
        Key::Tab,
        Key::Backspace,
        Key::Insert,
        Key::Delete,
        Key::Right,
        Key::Left,
        Key::Down,
        Key::Up,
        Key::PageUp,
        Key::PageDown,
        Key::Home,
        Key::End,
        Key::CapsLock,
        Key::LeftShift,
        Key::RightShift,
        Key::LeftControl,
        Key::RightControl,
        Key::LeftAlt,
        Key::RightAlt,
        Key::LeftSuper,
        Key::RightSuper,
        Key::Kp0,
        Key::Kp1,
        Key::Kp2,
        Key::Kp3,
        Key::Kp4,
        Key::Kp5,
        Key::Kp6,
        Key::Kp7,
        Key::Kp8,
        Key::Kp9,
        Key::KpEnter,
        Key::KpAdd,
        Key::KpSubtract,
        Key::KpMultiply,
        Key::KpDivide,
    ];
    let name = name.trim();
    KEYS.iter().copied().find(|&k| key_name(k) == name).or_else(|| {
        // Aliases for a few common spellings.
        match name.to_ascii_lowercase().as_str() {
            "shift" | "rshift" => Some(Key::RightShift),
            "lshift" => Some(Key::LeftShift),
            "return" => Some(Key::Enter),
            "esc" => Some(Key::Escape),
            _ => None,
        }
    })
}

fn pad_from_name(name: &str) -> Option<GamepadButton> {
    const PADS: &[GamepadButton] = &[
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
    let name = name.trim();
    PADS.iter()
        .copied()
        .find(|&b| gamepad_button_name(b) == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::controller::NesPadButton;
    use std::path::Path;

    #[test]
    fn roundtrip_default_settings() {
        let settings = Settings::default();
        let dirs = vec![PathBuf::from("/tmp/roms")];
        let dto = ConfigDto::from_user(&settings, &dirs);
        let text = toml::to_string_pretty(&dto).unwrap();
        let back: ConfigDto = toml::from_str(&text).unwrap();
        let cfg = back.into_user_config();
        assert_eq!(cfg.settings.volume, settings.volume);
        assert_eq!(cfg.settings.video_filter, settings.video_filter);
        assert_eq!(cfg.settings.keyboard, settings.keyboard);
        assert_eq!(cfg.settings.gamepad, settings.gamepad);
        assert_eq!(cfg.settings.devices, settings.devices);
        // Missing /tmp/roms on disk is filtered out — only check serialize path list in dto.
        assert_eq!(dto.extra_dirs, vec!["/tmp/roms".to_string()]);
    }

    #[test]
    fn key_name_roundtrip_defaults() {
        for nes in NesPadButton::ALL {
            let key = KeyboardMap::default().get(nes);
            assert_eq!(key_from_name(key_name(key)), Some(key));
        }
    }

    #[test]
    fn config_path_is_under_app_dir() {
        let path = config_path().expect("home/config available in tests");
        assert!(path.ends_with(Path::new(APP_DIR).join(FILE_NAME)));
    }
}
