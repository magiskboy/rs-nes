//! Shared 80/90 CRT / Famicom-BIOS palette for NES OS + in-game menus.

pub type Rgba = [f32; 4];

pub const BG: Rgba = [0.04, 0.10, 0.22, 1.0];
pub const BG_PANEL: Rgba = [0.06, 0.14, 0.30, 1.0];
pub const BORDER: Rgba = [0.35, 0.55, 0.85, 1.0];
pub const BORDER_DIM: Rgba = [0.15, 0.28, 0.48, 1.0];
pub const TEXT: Rgba = [0.92, 0.94, 0.98, 1.0];
pub const TEXT_DIM: Rgba = [0.55, 0.65, 0.80, 1.0];
pub const ACCENT: Rgba = [0.36, 0.94, 1.0, 1.0];
pub const SELECT_BG: Rgba = [0.12, 0.35, 0.55, 1.0];
pub const SELECT_TEXT: Rgba = [1.0, 0.92, 0.35, 1.0];
pub const TITLE: Rgba = [1.0, 1.0, 1.0, 1.0];
