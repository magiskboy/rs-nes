//! NES OS shell — boot splash, game library, settings (OpenGL overlay UI).

mod boot;
mod cover;
mod draw;
mod gamelist;
mod library;
mod nav;
mod theme;

use crate::emulator::audio::UiSfx;
use crate::emulator::config;
use crate::emulator::menu::input::NavAction;
use crate::emulator::menu::{Menu, UiAction};
use crate::emulator::os::cover::{load_cover_rgba, CoverRgba, CoverService};
use crate::emulator::os::library::{
    build_rows, default_rom_dirs, scan_rom_dir, LibRow, RomEntry,
};
use crate::emulator::overlay::Overlay;
use glfw::{Action, Glfw, Key};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Boot,
    Library,
    /// Select-button popup: Settings / Quit.
    SystemMenu,
    Settings,
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SysItem {
    Settings,
    Quit,
}

impl SysItem {
    const ALL: [SysItem; 2] = [SysItem::Settings, SysItem::Quit];

    fn label(self) -> &'static str {
        match self {
            SysItem::Settings => "Settings",
            SysItem::Quit => "Quit",
        }
    }
}

/// Host-side shell that sits above the emulator core.
pub struct Os {
    mode: Mode,
    boot_ticks: u32,
    /// Shared settings + in-game pause navigation.
    pub menu: Menu,
    roms: Vec<RomEntry>,
    rows: Vec<LibRow>,
    sel: usize,
    scroll: usize,
    /// How many library rows fit on screen (updated each draw from framebuffer size).
    lib_visible: usize,
    blink_ticks: u32,
    sys_sel: usize,
    /// Previous P1 pad bitmask for edge detection in the shell / pause UI.
    prev_pad: u8,
    /// Library folders (from “Select folder…” / `--roms`). When non-empty these
    /// replace the default `roms/` scan paths.
    extra_dirs: Vec<PathBuf>,
    /// Extra single ROM paths (e.g. CLI arg).
    extra_roms: Vec<PathBuf>,
    covers: CoverService,
    /// RGBA pixels for the currently selected ROM cover (if any).
    selected_cover: Option<CoverRgba>,
    /// ROM path the cached cover belongs to.
    selected_cover_rom: Option<PathBuf>,
    /// Last `CoverService::generation` applied to `selected_cover`.
    cover_gen: u64,
}

/// Result of handling input while in the shell or pause overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OsAction {
    None,
    ApplySettings,
    Quit,
    LoadRom(PathBuf),
    /// Hard-reset the loaded cartridge; stay in gameplay.
    ResetGame,
    /// Save a BMP of the current NES frame.
    Screenshot,
    ExitToLibrary,
    PickFolder,
    /// Kick off (or no-op) a background cover download job.
    ScanCovers,
}

impl Os {
    pub fn new() -> Self {
        let loaded = config::load();
        let mut os = Self {
            mode: Mode::Boot,
            boot_ticks: 0,
            menu: Menu::with_settings(loaded.settings),
            roms: Vec::new(),
            rows: Vec::new(),
            sel: 0,
            scroll: 0,
            lib_visible: 12,
            blink_ticks: 0,
            sys_sel: 0,
            prev_pad: 0,
            extra_dirs: loaded.extra_dirs,
            extra_roms: Vec::new(),
            covers: CoverService::new(),
            selected_cover: None,
            selected_cover_rom: None,
            cover_gen: 0,
        };
        os.refresh_library();
        os.reload_selected_cover();
        os
    }

    /// Persist settings + added ROM folders to the user config file.
    pub fn save_config(&self) {
        if let Err(e) = config::save(&self.menu.settings, &self.extra_dirs) {
            eprintln!("warning: save config: {e}");
        }
    }

    pub fn in_shell(&self) -> bool {
        !matches!(self.mode, Mode::Playing)
    }

    pub fn is_paused(&self) -> bool {
        self.menu.is_paused()
    }

    pub fn is_waiting_bind(&self) -> bool {
        self.menu.is_waiting_bind()
    }

    pub fn refresh_library(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut roms = Vec::new();
        // Selected / CLI folders replace the default `roms/` locations.
        let dirs: Vec<PathBuf> = if self.extra_dirs.is_empty() {
            default_rom_dirs()
        } else {
            self.extra_dirs.clone()
        };
        for dir in dirs {
            for entry in scan_rom_dir(&dir) {
                let key = entry.path.to_string_lossy().to_lowercase();
                if seen.insert(key) {
                    roms.push(entry);
                }
            }
        }
        for path in &self.extra_roms {
            if !path.is_file() {
                continue;
            }
            let key = path.to_string_lossy().to_lowercase();
            if seen.insert(key) {
                roms.push(RomEntry::from_path(path.clone()));
            }
        }
        gamelist::apply_to_roms(&mut roms);
        roms.sort_by(|a, b| {
            a.display_name()
                .to_lowercase()
                .cmp(&b.display_name().to_lowercase())
        });
        self.roms = roms;
        self.rows = build_rows(self.roms.len());
        if self.sel >= self.rows.len() && !self.rows.is_empty() {
            self.sel = self.rows.len() - 1;
        }
        self.ensure_scroll_visible(self.lib_visible);
        self.reload_selected_cover();
    }

    /// Add a folder of `.nes` files to the library (or a single ROM path from the CLI).
    pub fn add_path(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.add_folder(path);
        } else if path.is_file() {
            if !self.extra_roms.iter().any(|p| p == &path) {
                self.extra_roms.push(path.clone());
            }
            self.refresh_library();
            self.select_rom_path(&path);
        }
    }

    /// Replace the library with a single folder (NES OS “Select folder…”).
    pub fn set_folder(&mut self, dir: PathBuf) {
        if !dir.is_dir() {
            return;
        }
        self.extra_dirs = vec![dir.clone()];
        self.extra_roms.clear();
        self.sel = 0;
        self.scroll = 0;
        self.refresh_library();
        if let Some(entry) = self.roms.iter().find(|r| r.path.starts_with(&dir)) {
            let path = entry.path.clone();
            self.select_rom_path(&path);
        }
    }

    /// Append a ROM folder (CLI `--roms`, repeatable).
    pub fn add_folder(&mut self, dir: PathBuf) {
        if !dir.is_dir() {
            return;
        }
        if !self.extra_dirs.iter().any(|d| d == &dir) {
            self.extra_dirs.push(dir.clone());
        }
        self.refresh_library();
        if let Some(entry) = self.roms.iter().find(|r| r.path.starts_with(&dir)) {
            let path = entry.path.clone();
            self.select_rom_path(&path);
        }
    }

    fn select_rom_path(&mut self, path: &Path) {
        if let Some(i) = self.roms.iter().position(|r| r.path == path) {
            if let Some(row_i) = self.rows.iter().position(|r| *r == LibRow::Rom(i)) {
                self.sel = row_i;
                self.ensure_scroll_visible(self.lib_visible);
                self.reload_selected_cover();
            }
        }
    }

    pub fn enter_playing(&mut self) {
        self.mode = Mode::Playing;
        self.menu.close_all();
        // Treat all buttons as already held so a still-pressed Start/A
        // cannot immediately re-trigger UI on the next poll.
        self.prev_pad = u8::MAX;
    }

    pub fn return_to_library(&mut self) {
        self.menu.close_all();
        self.mode = Mode::Library;
        self.refresh_library();
        // Important: do NOT clear to 0. Exit is confirmed with Start/A, which is
        // often still held on the next frame — a zeroed prev_pad would look like
        // a fresh Confirm and instantly re-launch the selected ROM.
        self.prev_pad = u8::MAX;
    }

    pub fn tick_boot(&mut self) {
        if self.mode != Mode::Boot {
            return;
        }
        self.boot_ticks = self.boot_ticks.saturating_add(1);
        if self.boot_ticks >= boot::BOOT_FRAMES {
            self.mode = Mode::Library;
        }
    }

    /// Jump straight to the game library (CLI `--skip-boot`).
    pub fn skip_boot(&mut self) {
        if self.mode == Mode::Boot {
            self.mode = Mode::Library;
        }
    }

    pub fn tick_blink(&mut self) {
        self.blink_ticks = self.blink_ticks.wrapping_add(1);
    }

    /// Poll cover-scan worker and refresh settings label / selected cover.
    pub fn tick_covers(&mut self) {
        let before = self.covers.generation;
        self.covers.poll();
        self.menu.scan_covers_label = self.covers.status().label();
        if self.covers.generation != before {
            // Reload titles/devs/years written to gamelist.txt by the worker.
            // Keep ROM order (and LibRow indices) stable while browsing.
            gamelist::apply_to_roms(&mut self.roms);
            self.reload_selected_cover();
        } else if self.covers.generation != self.cover_gen {
            self.reload_selected_cover();
        }
    }

    fn reload_selected_cover(&mut self) {
        self.cover_gen = self.covers.generation;
        let rom = self.rows.get(self.sel).and_then(|row| match row {
            LibRow::Rom(i) => self.roms.get(*i),
            LibRow::SelectFolder => None,
        });
        match rom {
            Some(entry) => {
                self.selected_cover_rom = Some(entry.path.clone());
                self.selected_cover = gamelist::resolve_cover_path(entry).and_then(|p| {
                    load_cover_rgba(&p)
                });
            }
            None => {
                self.selected_cover = None;
                self.selected_cover_rom = None;
            }
        }
    }

    /// Metadata for the currently selected library ROM, if any.
    pub fn selected_meta(&self) -> Option<&RomEntry> {
        self.rows.get(self.sel).and_then(|row| match row {
            LibRow::Rom(i) => self.roms.get(*i),
            LibRow::SelectFolder => None,
        })
    }

    pub fn start_cover_scan(&mut self) {
        let _ = self.covers.start_scan(&self.roms);
        self.menu.scan_covers_label = self.covers.status().label();
    }

    fn blink_on(&self) -> bool {
        (self.blink_ticks / 16) % 2 == 0
    }

    pub fn poll_bind_capture(&mut self, glfw: &Glfw) -> OsAction {
        match self.menu.poll_bind_capture(glfw) {
            UiAction::ApplySettings => OsAction::ApplySettings,
            _ => OsAction::None,
        }
    }

    /// Select (or `P`) while a game is running — open/close the pause menu.
    pub fn toggle_pause(&mut self) {
        if self.mode == Mode::Playing {
            self.menu.toggle_pause();
            self.prev_pad = u8::MAX;
        }
    }

    /// Raw keys only for button-map capture (settings / pause menu).
    pub fn on_key_for_bind(&mut self, key: Key, action: Action) -> OsAction {
        if !self.menu.is_waiting_bind() {
            return OsAction::None;
        }
        match self.menu.on_key(key, action) {
            UiAction::None => OsAction::None,
            UiAction::ApplySettings => OsAction::ApplySettings,
            UiAction::Quit => OsAction::Quit,
            UiAction::ScanCovers => OsAction::ScanCovers,
            UiAction::ResetGame | UiAction::Screenshot | UiAction::ExitToLibrary => {
                OsAction::None
            }
        }
    }

    /// Drive shell / pause UI from player-1 NES pad (edge-triggered).
    ///
    /// Returns `(action, optional UI beep)`. Beeps are only emitted in the OS
    /// shell — not while the in-game pause menu is open.
    pub fn on_pad(&mut self, pad: u8) -> (OsAction, Option<UiSfx>) {
        let edges = nav::pressed_edges(pad, self.prev_pad);
        self.prev_pad = pad;
        if edges == 0 {
            return (OsAction::None, None);
        }

        // Select: system menu in the shell, pause menu in-game.
        if edges & (crate::bus::Button::Select as u8) != 0 {
            return self.on_select();
        }

        let mut action = OsAction::None;
        let mut sfx = None;
        for bit in nav::each_bit(edges) {
            if nav::is_select(bit) {
                continue;
            }
            if let Some(nav_act) = nav::nav_from_button(bit) {
                let (a, s) = self.dispatch_nav(nav_act);
                if s.is_some() {
                    sfx = s;
                }
                if a != OsAction::None {
                    action = a;
                    break;
                }
            }
        }
        (action, sfx)
    }

    fn on_select(&mut self) -> (OsAction, Option<UiSfx>) {
        match self.mode {
            Mode::Boot => {
                self.mode = Mode::Library;
                (OsAction::None, Some(UiSfx::Confirm))
            }
            Mode::Library => {
                self.sys_sel = 0;
                self.mode = Mode::SystemMenu;
                (OsAction::None, Some(UiSfx::Select))
            }
            Mode::SystemMenu => {
                self.mode = Mode::Library;
                (OsAction::None, Some(UiSfx::Back))
            }
            Mode::Settings => {
                // Select backs out of settings toward the library.
                self.menu.close_all();
                self.mode = Mode::Library;
                (OsAction::None, Some(UiSfx::Back))
            }
            Mode::Playing => {
                self.toggle_pause();
                (OsAction::None, None)
            }
        }
    }

    fn dispatch_nav(&mut self, nav_act: NavAction) -> (OsAction, Option<UiSfx>) {
        match self.mode {
            Mode::Boot => {
                if matches!(nav_act, NavAction::Confirm) {
                    self.mode = Mode::Library;
                    (OsAction::None, Some(UiSfx::Confirm))
                } else {
                    (OsAction::None, None)
                }
            }
            Mode::Library => self.on_library_nav(nav_act),
            Mode::SystemMenu => self.on_system_nav(nav_act),
            Mode::Settings => self.on_settings_nav(nav_act),
            Mode::Playing => {
                // Pause menu: actions only, no OS chrome SFX.
                if self.menu.is_paused() {
                    let action = match self.menu.on_nav(nav_act) {
                        UiAction::None => OsAction::None,
                        UiAction::ApplySettings => OsAction::ApplySettings,
                        UiAction::Quit => OsAction::Quit,
                        UiAction::ResetGame => OsAction::ResetGame,
                        UiAction::Screenshot => OsAction::Screenshot,
                        UiAction::ExitToLibrary => OsAction::ExitToLibrary,
                        UiAction::ScanCovers => OsAction::None,
                    };
                    (action, None)
                } else {
                    (OsAction::None, None)
                }
            }
        }
    }

    fn on_library_nav(&mut self, nav_act: NavAction) -> (OsAction, Option<UiSfx>) {
        if self.rows.is_empty() {
            return (OsAction::None, None);
        }
        match nav_act {
            NavAction::Up => {
                self.sel = self.sel.checked_sub(1).unwrap_or(self.rows.len() - 1);
                self.ensure_scroll_visible(self.lib_visible);
                self.reload_selected_cover();
                (OsAction::None, Some(UiSfx::Move))
            }
            NavAction::Down => {
                self.sel = (self.sel + 1) % self.rows.len();
                self.ensure_scroll_visible(self.lib_visible);
                self.reload_selected_cover();
                (OsAction::None, Some(UiSfx::Move))
            }
            NavAction::Confirm => {
                let action = self.activate_library_row();
                let sfx = if action == OsAction::None {
                    None
                } else {
                    Some(UiSfx::Confirm)
                };
                (action, sfx)
            }
            NavAction::Back | NavAction::Left | NavAction::Right => (OsAction::None, None),
        }
    }

    fn on_system_nav(&mut self, nav_act: NavAction) -> (OsAction, Option<UiSfx>) {
        let n = SysItem::ALL.len();
        match nav_act {
            NavAction::Up => {
                self.sys_sel = self.sys_sel.checked_sub(1).unwrap_or(n - 1);
                (OsAction::None, Some(UiSfx::Move))
            }
            NavAction::Down => {
                self.sys_sel = (self.sys_sel + 1) % n;
                (OsAction::None, Some(UiSfx::Move))
            }
            NavAction::Confirm => match SysItem::ALL[self.sys_sel] {
                SysItem::Settings => {
                    self.open_settings();
                    (OsAction::None, Some(UiSfx::Confirm))
                }
                SysItem::Quit => (OsAction::Quit, Some(UiSfx::Confirm)),
            },
            NavAction::Back => {
                self.mode = Mode::Library;
                (OsAction::None, Some(UiSfx::Back))
            }
            NavAction::Left | NavAction::Right => (OsAction::None, None),
        }
    }

    fn activate_library_row(&mut self) -> OsAction {
        match self.rows.get(self.sel).copied() {
            Some(LibRow::Rom(i)) => {
                if let Some(entry) = self.roms.get(i) {
                    OsAction::LoadRom(entry.path.clone())
                } else {
                    OsAction::None
                }
            }
            Some(LibRow::SelectFolder) => OsAction::PickFolder,
            None => OsAction::None,
        }
    }

    fn open_settings(&mut self) {
        self.menu.open_settings();
        self.mode = Mode::Settings;
    }

    fn on_settings_nav(&mut self, nav_act: NavAction) -> (OsAction, Option<UiSfx>) {
        if self.menu.is_waiting_bind() {
            // B cancels capture (via Back).
            if nav_act == NavAction::Back {
                self.menu.cancel_capture();
                return (OsAction::None, Some(UiSfx::Back));
            }
            return (OsAction::None, None);
        }

        let act = self.menu.on_nav(nav_act);
        if !self.menu.is_paused() {
            self.mode = Mode::Library;
        }

        let sfx = match nav_act {
            NavAction::Up | NavAction::Down | NavAction::Left | NavAction::Right => {
                Some(UiSfx::Move)
            }
            NavAction::Confirm => Some(UiSfx::Confirm),
            NavAction::Back => Some(UiSfx::Back),
        };

        let action = match act {
            UiAction::None => OsAction::None,
            UiAction::ApplySettings => OsAction::ApplySettings,
            UiAction::Quit => OsAction::Quit,
            UiAction::ResetGame | UiAction::Screenshot => OsAction::None,
            UiAction::ScanCovers => OsAction::ScanCovers,
            UiAction::ExitToLibrary => {
                self.menu.close_all();
                self.mode = Mode::Library;
                OsAction::None
            }
        };
        (action, sfx)
    }

    fn ensure_scroll_visible(&mut self, visible: usize) {
        if self.rows.is_empty() {
            self.scroll = 0;
            return;
        }
        let visible = visible.max(1);
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.sel >= self.scroll + visible {
            self.scroll = self.sel + 1 - visible;
        }
        // Keep the window packed to the bottom so tall screens stay full.
        let max_scroll = self.rows.len().saturating_sub(visible);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    pub fn draw(&mut self, overlay: &mut Overlay, glfw: &Glfw, fb_w: i32, fb_h: i32) {
        self.lib_visible = draw::library_visible_rows(fb_w, fb_h);
        self.ensure_scroll_visible(self.lib_visible);

        let blink = self.blink_on();
        let clock = draw::format_clock();
        let scan_hint = match self.covers.status() {
            cover::ScanStatus::Running { done, total } => {
                Some(format!("Scanning library {done}/{total}..."))
            }
            _ => None,
        };
        let scan_hint_ref = scan_hint.as_deref();
        let selected = self.selected_meta();
        match self.mode {
            Mode::Boot => boot::draw(overlay, fb_w, fb_h, self.boot_ticks),
            Mode::Library => {
                draw::draw_library(
                    overlay,
                    fb_w,
                    fb_h,
                    &self.roms,
                    &self.rows,
                    self.sel,
                    self.scroll,
                    blink,
                    &clock,
                    self.selected_cover.as_ref(),
                    selected,
                    scan_hint_ref,
                );
            }
            Mode::SystemMenu => {
                draw::draw_library(
                    overlay,
                    fb_w,
                    fb_h,
                    &self.roms,
                    &self.rows,
                    self.sel,
                    self.scroll,
                    blink,
                    &clock,
                    self.selected_cover.as_ref(),
                    selected,
                    scan_hint_ref,
                );
                let labels: Vec<&str> = SysItem::ALL.iter().map(|i| i.label()).collect();
                draw::draw_system_menu(overlay, fb_w, fb_h, self.sys_sel, blink, &labels);
            }
            Mode::Settings => {
                draw::draw_settings(overlay, &self.menu, glfw, fb_w, fb_h, blink, &clock);
            }
            Mode::Playing => {
                if self.menu.is_paused() {
                    self.menu.draw(overlay, glfw, fb_w, fb_h);
                }
            }
        }
    }
}
