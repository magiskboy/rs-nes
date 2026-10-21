//! Pause menu + settings (keyboard / D-pad navigation).
//!
//! Screens are declared as [`ItemKind`] lists in [`screen`]; navigation is shared.

mod debug_hud;
mod draw;
mod input;
mod screen;
mod settings;

pub use debug_hud::draw_debug;
pub use screen::{ItemKind, ScreenId};
pub use settings::Settings;

use crate::emulator::controller::{
    GamepadMap, InputDevice, KeyboardMap, MapTarget, NesPadButton, held_gamepad_buttons,
};
use crate::emulator::menu::input::{NavAction, map_key};
use crate::emulator::menu::screen::{ItemKind as Kind, screen_def};
use crate::emulator::overlay::Overlay;
use glfw::{Action, GamepadButton, Glfw, Key};

/// Result of handling a UI key while paused / in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    None,
    /// Apply volume / fullscreen / debug toggles that changed.
    ApplySettings,
    Quit,
}

pub struct Menu {
    /// Empty = playing. Top of stack is the visible screen.
    stack: Vec<ScreenId>,
    /// Selection index per [`ScreenId`] (preserved when navigating back).
    sels: [usize; ScreenId::COUNT],
    pub settings: Settings,
    map_target: MapTarget,
    waiting_bind: Option<NesPadButton>,
    capture_baseline_ready: bool,
    capture_ignore: Vec<GamepadButton>,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            sels: [0; ScreenId::COUNT],
            settings: Settings::default(),
            map_target: MapTarget::Keyboard,
            waiting_bind: None,
            capture_baseline_ready: false,
            capture_ignore: Vec::new(),
        }
    }

    pub fn is_paused(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn is_waiting_bind(&self) -> bool {
        self.waiting_bind.is_some()
    }

    pub(crate) fn top_screen(&self) -> Option<ScreenId> {
        self.stack.last().copied()
    }

    pub(crate) fn sel(&self) -> usize {
        self.top_screen()
            .map(|id| self.sels[id.index()])
            .unwrap_or(0)
    }

    fn set_sel(&mut self, value: usize) {
        if let Some(id) = self.top_screen() {
            self.sels[id.index()] = value;
        }
    }

    pub(crate) fn map_target(&self) -> MapTarget {
        self.map_target
    }

    pub(crate) fn waiting_bind(&self) -> Option<NesPadButton> {
        self.waiting_bind
    }

    /// Cancel an in-progress rebind. Returns true if a capture was active.
    pub fn cancel_capture(&mut self) -> bool {
        if self.waiting_bind.is_some() {
            self.waiting_bind = None;
            self.capture_baseline_ready = false;
            self.capture_ignore.clear();
            true
        } else {
            false
        }
    }

    /// Toggle pause from the playing state, or step back one menu level.
    pub fn toggle_pause(&mut self) {
        if self.cancel_capture() {
            return;
        }
        if self.stack.is_empty() {
            self.push(ScreenId::Pause);
        } else {
            self.pop();
        }
    }

    fn push(&mut self, id: ScreenId) {
        self.sels[id.index()] = 0;
        self.stack.push(id);
    }

    fn pop(&mut self) {
        self.stack.pop();
    }

    /// Poll gamepad while waiting to capture a gamepad remapping.
    ///
    /// Uses edge detection: buttons already held when capture starts are ignored
    /// until released, so a stuck/phantom hold can't block remapping forever.
    pub fn poll_bind_capture(&mut self, glfw: &Glfw) {
        if self.map_target != MapTarget::Gamepad {
            return;
        }
        let Some(nes_btn) = self.waiting_bind else {
            return;
        };

        let held = held_gamepad_buttons(glfw);

        if !self.capture_baseline_ready {
            self.capture_ignore = held;
            self.capture_baseline_ready = true;
            return;
        }

        self.capture_ignore
            .retain(|b| held.iter().any(|h| h == b));

        if let Some(&pad_btn) = held
            .iter()
            .find(|b| !self.capture_ignore.iter().any(|i| i == *b))
        {
            self.settings.gamepad.set(nes_btn, pad_btn);
            self.waiting_bind = None;
            self.capture_baseline_ready = false;
            self.capture_ignore.clear();
        }
    }

    /// Handle a raw key press for menu navigation (only while paused).
    pub fn on_key(&mut self, key: Key, action: Action) -> UiAction {
        if self.stack.is_empty() || action != Action::Press {
            return UiAction::None;
        }

        if let Some(nes_btn) = self.waiting_bind {
            if key == Key::Escape {
                self.cancel_capture();
                return UiAction::None;
            }
            if self.map_target == MapTarget::Keyboard {
                self.settings.keyboard.set(nes_btn, key);
                self.waiting_bind = None;
                self.capture_baseline_ready = false;
                self.capture_ignore.clear();
            } else if matches!(key, Key::X | Key::Backspace) {
                self.cancel_capture();
            }
            return UiAction::None;
        }

        let Some(nav) = map_key(key) else {
            return UiAction::None;
        };
        self.on_nav(nav)
    }

    fn on_nav(&mut self, nav: NavAction) -> UiAction {
        let Some(id) = self.top_screen() else {
            return UiAction::None;
        };
        let items = screen_def(id).items;
        if items.is_empty() {
            return UiAction::None;
        }

        let sel = self.sel();
        match nav {
            NavAction::Up => {
                self.set_sel(sel.checked_sub(1).unwrap_or(items.len() - 1));
                UiAction::None
            }
            NavAction::Down => {
                self.set_sel((sel + 1) % items.len());
                UiAction::None
            }
            NavAction::Left => self.adjust_item(items[sel], -1),
            NavAction::Right => self.adjust_item(items[sel], 1),
            NavAction::Confirm => self.activate_item(items[sel]),
            NavAction::Back => {
                self.pop();
                UiAction::None
            }
        }
    }

    fn activate_item(&mut self, item: Kind) -> UiAction {
        match item {
            Kind::Resume => {
                self.stack.clear();
                UiAction::None
            }
            Kind::Quit => UiAction::Quit,
            Kind::Goto(id) => {
                self.push(id);
                UiAction::None
            }
            Kind::Back => {
                self.pop();
                UiAction::None
            }
            Kind::Volume => UiAction::None,
            Kind::Fullscreen => {
                self.settings.fullscreen = !self.settings.fullscreen;
                UiAction::ApplySettings
            }
            Kind::DebugInfo => {
                self.settings.debug_info = !self.settings.debug_info;
                UiAction::ApplySettings
            }
            Kind::Player1 | Kind::Player2 => self.adjust_item(item, 1),
            Kind::MapDevice => {
                self.map_target = self.map_target.cycle(1);
                UiAction::None
            }
            Kind::Bind(nes) => {
                self.waiting_bind = Some(nes);
                self.capture_baseline_ready = false;
                self.capture_ignore.clear();
                UiAction::None
            }
            Kind::ResetMap => {
                match self.map_target {
                    MapTarget::Keyboard => {
                        self.settings.keyboard = KeyboardMap::default();
                    }
                    MapTarget::Gamepad => {
                        self.settings.gamepad = GamepadMap::default();
                    }
                }
                UiAction::None
            }
        }
    }

    fn adjust_item(&mut self, item: Kind, dir: i32) -> UiAction {
        match item {
            Kind::Volume => {
                let step = 0.05 * dir as f32;
                self.settings.volume = (self.settings.volume + step).clamp(0.0, 1.0);
                UiAction::ApplySettings
            }
            Kind::Fullscreen => {
                self.settings.fullscreen = !self.settings.fullscreen;
                UiAction::ApplySettings
            }
            Kind::DebugInfo => {
                self.settings.debug_info = !self.settings.debug_info;
                UiAction::ApplySettings
            }
            Kind::Player1 => {
                self.settings.devices.player1 = self
                    .settings
                    .devices
                    .player1
                    .cycle(&InputDevice::PLAYER1_OPTIONS, dir);
                UiAction::None
            }
            Kind::Player2 => {
                self.settings.devices.player2 = self
                    .settings
                    .devices
                    .player2
                    .cycle(&InputDevice::PLAYER2_OPTIONS, dir);
                UiAction::None
            }
            Kind::MapDevice => {
                self.map_target = self.map_target.cycle(dir);
                UiAction::None
            }
            _ => UiAction::None,
        }
    }

    pub fn draw(&self, overlay: &mut Overlay, glfw: &Glfw, fb_w: i32, fb_h: i32) {
        draw::draw_menu(self, overlay, glfw, fb_w, fb_h);
    }
}
