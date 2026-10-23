//! Declarative menu screens (`ItemKind` + `ScreenDef`).

use crate::emulator::controller::{
    MapTarget, NesPadButton, device_status, gamepad_button_name, key_name,
};
use crate::emulator::menu::settings::Settings;
use glfw::Glfw;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenId {
    Pause,
    Settings,
    Controllers,
    ButtonMap,
}

impl ScreenId {
    pub const COUNT: usize = 4;

    pub fn index(self) -> usize {
        match self {
            ScreenId::Pause => 0,
            ScreenId::Settings => 1,
            ScreenId::Controllers => 2,
            ScreenId::ButtonMap => 3,
        }
    }
}

/// What a single menu row does when activated / adjusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Resume,
    ResetGame,
    Screenshot,
    ExitToLibrary,
    Quit,
    Goto(ScreenId),
    Back,
    Volume,
    Fullscreen,
    DebugInfo,
    VideoFilter,
    Player1,
    Player2,
    MapDevice,
    Bind(NesPadButton),
    ResetMap,
}

pub struct ScreenDef {
    pub title: &'static str,
    pub items: &'static [ItemKind],
}

pub struct LabelCtx<'a> {
    pub settings: &'a Settings,
    pub map_target: MapTarget,
    pub waiting_bind: Option<NesPadButton>,
    pub glfw: Option<&'a Glfw>,
}

impl ItemKind {
    pub fn label(self, ctx: &LabelCtx<'_>) -> String {
        match self {
            ItemKind::Resume => "Resume".into(),
            ItemKind::ResetGame => "Reset game".into(),
            ItemKind::Screenshot => "Screenshot".into(),
            ItemKind::ExitToLibrary => "Exit to OS".into(),
            ItemKind::Quit => "Quit".into(),
            ItemKind::Goto(ScreenId::Settings) => "Settings".into(),
            ItemKind::Goto(ScreenId::Controllers) => "Controllers".into(),
            ItemKind::Goto(ScreenId::ButtonMap) => "Button map".into(),
            ItemKind::Goto(ScreenId::Pause) => "Pause".into(),
            ItemKind::Back => "Back".into(),
            ItemKind::Volume => {
                let pct = (ctx.settings.volume * 100.0).round() as i32;
                format!("Volume        {pct:3}%")
            }
            ItemKind::Fullscreen => format!(
                "Fullscreen    {}",
                if ctx.settings.fullscreen { "On" } else { "Off" }
            ),
            ItemKind::DebugInfo => format!(
                "Debug info    {}",
                if ctx.settings.debug_info { "On" } else { "Off" }
            ),
            ItemKind::VideoFilter => {
                format!("Filter        {}", ctx.settings.video_filter.label())
            }
            ItemKind::Player1 => {
                let p1 = ctx.settings.devices.player1;
                let status = ctx
                    .glfw
                    .map(|g| device_status(g, p1))
                    .unwrap_or("");
                format!("Player 1   {}{}", p1.label(), status)
            }
            ItemKind::Player2 => {
                let p2 = ctx.settings.devices.player2;
                let status = ctx
                    .glfw
                    .map(|g| device_status(g, p2))
                    .unwrap_or("");
                format!("Player 2   {}{}", p2.label(), status)
            }
            ItemKind::MapDevice => format!("Device       {}", ctx.map_target.label()),
            ItemKind::Bind(nes) => {
                if ctx.waiting_bind == Some(nes) {
                    format!("{:<12} <press...>", nes.label())
                } else {
                    let bound = match ctx.map_target {
                        MapTarget::Keyboard => key_name(ctx.settings.keyboard.get(nes)),
                        MapTarget::Gamepad => {
                            gamepad_button_name(ctx.settings.gamepad.get(nes))
                        }
                    };
                    format!("{:<12} {}", nes.label(), bound)
                }
            }
            ItemKind::ResetMap => "Reset defaults".into(),
        }
    }

}

const PAUSE_ITEMS: &[ItemKind] = &[
    ItemKind::Resume,
    ItemKind::ResetGame,
    ItemKind::Screenshot,
    ItemKind::ExitToLibrary,
];

const SETTINGS_ITEMS: &[ItemKind] = &[
    ItemKind::Volume,
    ItemKind::Fullscreen,
    ItemKind::DebugInfo,
    ItemKind::VideoFilter,
    ItemKind::Goto(ScreenId::Controllers),
    ItemKind::Back,
];

const CONTROLLERS_ITEMS: &[ItemKind] = &[
    ItemKind::Player1,
    ItemKind::Player2,
    ItemKind::Goto(ScreenId::ButtonMap),
    ItemKind::Back,
];

const BUTTON_MAP_ITEMS: &[ItemKind] = &[
    ItemKind::MapDevice,
    ItemKind::Bind(NesPadButton::A),
    ItemKind::Bind(NesPadButton::B),
    ItemKind::Bind(NesPadButton::Start),
    ItemKind::Bind(NesPadButton::Select),
    ItemKind::Bind(NesPadButton::Up),
    ItemKind::Bind(NesPadButton::Down),
    ItemKind::Bind(NesPadButton::Left),
    ItemKind::Bind(NesPadButton::Right),
    ItemKind::ResetMap,
    ItemKind::Back,
];

pub fn screen_def(id: ScreenId) -> ScreenDef {
    match id {
        ScreenId::Pause => ScreenDef {
            title: "PAUSED",
            items: PAUSE_ITEMS,
        },
        ScreenId::Settings => ScreenDef {
            title: "SETTINGS",
            items: SETTINGS_ITEMS,
        },
        ScreenId::Controllers => ScreenDef {
            title: "CONTROLLERS",
            items: CONTROLLERS_ITEMS,
        },
        ScreenId::ButtonMap => ScreenDef {
            title: "BUTTON MAP",
            items: BUTTON_MAP_ITEMS,
        },
    }
}

pub fn hint_for(id: ScreenId, waiting_bind: bool, map_target: MapTarget) -> &'static str {
    if waiting_bind {
        return match map_target {
            MapTarget::Keyboard => "Press a key   Esc cancel",
            MapTarget::Gamepad => "Press a gamepad button   Esc cancel",
        };
    }
    match id {
        ScreenId::Pause => "D-pad  Start OK  Select/B back",
        ScreenId::Settings => "D-pad adjust  Start OK  B back",
        ScreenId::Controllers => "D-pad change  Start OK  B back",
        ScreenId::ButtonMap => "D-pad device  Start rebind  B back",
    }
}

/// Layout scale / title size tuned per screen.
pub fn layout_scale(id: ScreenId, fb_w: i32, fb_h: i32) -> (f32, f32) {
    // (scale, title_mul)
    match id {
        ScreenId::Pause => {
            let scale = ((fb_w.min(fb_h) as f32) / 320.0).clamp(2.0, 4.0);
            (scale, 1.5)
        }
        ScreenId::Settings => {
            let scale = ((fb_w.min(fb_h) as f32) / 320.0).clamp(2.0, 4.0);
            (scale, 1.5)
        }
        ScreenId::Controllers => {
            let scale = ((fb_w.min(fb_h) as f32) / 340.0).clamp(1.85, 3.75);
            (scale, 1.4)
        }
        ScreenId::ButtonMap => {
            let scale = ((fb_w.min(fb_h) as f32) / 360.0).clamp(1.75, 3.5);
            (scale, 1.4)
        }
    }
}

pub fn row_step(id: ScreenId, scale: f32) -> f32 {
    match id {
        ScreenId::ButtonMap => 11.5 * scale,
        _ => 14.0 * scale,
    }
}
