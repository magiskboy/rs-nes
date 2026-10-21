pub mod audio;
pub mod controller;
pub mod menu;
pub mod overlay;
pub mod video;

use crate::cart::Loader;
use crate::clock::Region;
use crate::emulator::audio::AudioOut;
use crate::emulator::controller::Controllers;
use crate::emulator::menu::{Menu, UiAction, draw_debug};
use crate::emulator::overlay::Overlay;
use crate::emulator::video::GlPresenter;
use crate::nes::Nes;
use glfw::{Action, Context, InitHint, Key, WindowEvent, WindowHint, WindowMode};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

/// Desktop / CLI entry used by the `rs-nes` binary.
///
/// - No ROM arg → native file dialog (`.nes`)
/// - `rs-nes <rom.nes>` → open that path
/// - `rs-nes --headless [rom]` → run 60 frames, print stats (CI / smoke test)
pub fn run() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let headless = args.iter().any(|a| a == "--headless");
    args.retain(|a| a != "--headless");

    let rom_path = match resolve_rom_path(&args) {
        Some(p) => p,
        None => return, // user cancelled the dialog
    };

    let rom = Loader::load(rom_path.to_str().unwrap_or_default())
        .unwrap_or_else(|e| panic!("load ROM {}: {e:?}", rom_path.display()));
    let mut nes = Nes::from_rom(rom).expect("build NES");

    if headless {
        run_headless(&mut nes);
        return;
    }

    run_windowed(&mut nes, &rom_path);
}

fn resolve_rom_path(args: &[String]) -> Option<PathBuf> {
    if let Some(path) = args.first() {
        return Some(PathBuf::from(path));
    }
    pick_rom_dialog()
}

/// Native “Open NES ROM” dialog (macOS / Windows / Linux).
fn pick_rom_dialog() -> Option<PathBuf> {
    let path = rfd::FileDialog::new()
        .set_title("Open NES ROM")
        .add_filter("NES ROM", &["nes"])
        .add_filter("All files", &["*"])
        .pick_file();
    // rfd temporarily tweaks NSActivationPolicy; restore game focus afterward.
    macos_activate_for_input();
    path
}

/// Ensure the process is a normal UI app and can receive keyboard events.
///
/// On macOS, launching from Terminal (or opening an `rfd` dialog first) often
/// leaves the process as `Prohibited`/`Accessory`. GLFW then skips its own
/// `setActivationPolicy(Regular)` because `NSApp` already exists — keystrokes
/// stay in Terminal even when the game window is clicked.
#[cfg(target_os = "macos")]
fn macos_activate_for_input() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}

#[cfg(not(target_os = "macos"))]
fn macos_activate_for_input() {}

fn run_headless(nes: &mut Nes) {
    const FRAMES: u64 = 60;
    nes.run_frames(FRAMES).expect("run frames");
    let fb = nes.framebuffer();
    let lit = fb.chunks(3).filter(|p| p != &[0, 0, 0]).count();
    let samples = nes.take_audio_samples();
    println!(
        "ok: {} frames, cpu_cycles={}, pc=${:04X}, lit_pixels={}, audio_samples={}",
        nes.frame(),
        nes.cycles(),
        nes.pc(),
        lit,
        samples.len(),
    );
}

fn run_windowed(nes: &mut Nes, rom_path: &Path) {
    let title = format!(
        "rs-nes — {}",
        rom_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("NES")
    );

    // Keep cwd stable inside a .app bundle (default GLFW chdir → Resources).
    glfw::init_hint(InitHint::CocoaChdirResources(false));
    glfw::init_hint(InitHint::CocoaMenubar(true));

    let mut glfw = glfw::init(glfw::fail_on_errors).expect("glfw init");
    glfw.window_hint(WindowHint::ContextVersion(3, 3));
    glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    glfw.window_hint(WindowHint::Resizable(true));
    glfw.window_hint(WindowHint::Focused(true));

    let (mut window, events) = glfw
        .create_window(768, 720, &title, WindowMode::Windowed)
        .expect("create GLFW window");

    window.make_current();
    window.set_key_polling(true);
    window.set_focus_polling(true);
    macos_activate_for_input();
    window.focus();
    glfw.set_swap_interval(glfw::SwapInterval::Sync(1));

    gl::load_with(|s| window.get_proc_address(s) as *const _);
    let presenter = GlPresenter::new().expect("GL presenter");
    let mut overlay = Overlay::new().expect("UI overlay");

    let audio = match AudioOut::start() {
        Ok(a) => {
            nes.set_audio_sample_rate(a.sample_rate);
            Some(a)
        }
        Err(e) => {
            eprintln!("audio disabled: {e}");
            None
        }
    };

    let mut controllers = Controllers::new();
    let mut menu = Menu::new();

    // Remember windowed geometry for leaving fullscreen.
    let mut windowed_pos = window.get_pos();
    let mut windowed_size = window.get_size();

    let frame_period = frame_duration(nes.region());
    let mut next_frame = Instant::now() + frame_period;
    let mut fps = 60.0_f32;
    let mut fps_timer = Instant::now();
    let mut fps_frames = 0u32;

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    menu.toggle_pause();
                    if menu.is_paused() {
                        if let Some(a) = audio.as_ref() {
                            a.clear_queue();
                        }
                    }
                }
                WindowEvent::Key(Key::P, _, Action::Press, _) if !menu.is_paused() => {
                    menu.toggle_pause();
                    if let Some(a) = audio.as_ref() {
                        a.clear_queue();
                    }
                }
                WindowEvent::Key(Key::O, _, Action::Press, mods)
                    if !menu.is_paused()
                        && (mods.contains(glfw::Modifiers::Control)
                            || mods.contains(glfw::Modifiers::Super)) =>
                {
                    if let Some(path) = pick_rom_dialog() {
                        let rom = Loader::load(path.to_str().unwrap_or_default())
                            .unwrap_or_else(|e| panic!("load ROM {}: {e:?}", path.display()));
                        *nes = Nes::from_rom(rom).expect("build NES");
                        if let Some(a) = audio.as_ref() {
                            nes.set_audio_sample_rate(a.sample_rate);
                            a.set_volume(menu.settings.volume);
                        }
                        let name = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("NES");
                        window.set_title(&format!("rs-nes — {name}"));
                    }
                    macos_activate_for_input();
                    window.focus();
                }
                WindowEvent::Key(key, _, action, _) if menu.is_paused() => {
                    match menu.on_key(key, action) {
                        UiAction::Quit => window.set_should_close(true),
                        UiAction::ApplySettings => {
                            apply_settings(
                                &menu,
                                &audio,
                                &mut glfw,
                                &mut window,
                                &mut windowed_pos,
                                &mut windowed_size,
                            );
                        }
                        UiAction::None => {}
                    }
                }
                WindowEvent::Key(key, _, action, _) => {
                    controllers.on_key(key, action);
                }
                WindowEvent::Focus(focused) => {
                    controllers.on_focus(focused);
                }
                _ => {}
            }
        }

        if menu.is_waiting_bind() {
            menu.poll_bind_capture(&glfw);
        }

        let pads = controllers.poll(
            &glfw,
            &menu.settings.keyboard,
            &menu.settings.gamepad,
            &menu.settings.devices,
        );

        if !menu.is_paused() {
            nes.set_controller(0, pads[0]);
            nes.set_controller(1, pads[1]);
            if let Err(e) = nes.run_frame() {
                eprintln!("emulation error: {e:?}");
                break;
            }

            if let Some(a) = audio.as_ref() {
                let samples = nes.take_audio_samples();
                a.push_samples(&samples);
            } else {
                let _ = nes.take_audio_samples();
            }
        } else {
            // Frozen frame: discard residual audio so the APU buffer doesn't grow.
            let _ = nes.take_audio_samples();
        }

        let (fb_w, fb_h) = window.get_framebuffer_size();
        presenter.upload(nes.framebuffer());
        presenter.draw(fb_w, fb_h);

        overlay.begin(fb_w, fb_h);
        // Menu first (dim + items), then debug on top so HUD stays readable while paused.
        menu.draw(&mut overlay, &glfw, fb_w, fb_h);
        if menu.settings.debug_info {
            draw_debug(&mut overlay, nes, fps, pads, fb_w, fb_h);
        }
        overlay.flush();

        window.swap_buffers();

        fps_frames += 1;
        let elapsed = fps_timer.elapsed();
        if elapsed >= Duration::from_millis(500) {
            fps = fps_frames as f32 / elapsed.as_secs_f32();
            fps_frames = 0;
            fps_timer = Instant::now();
        }

        let now = Instant::now();
        if now < next_frame {
            thread::sleep(next_frame - now);
            next_frame += frame_period;
        } else {
            next_frame = now + frame_period;
        }
    }
}

fn apply_settings(
    menu: &Menu,
    audio: &Option<AudioOut>,
    glfw: &mut glfw::Glfw,
    window: &mut glfw::Window,
    windowed_pos: &mut (i32, i32),
    windowed_size: &mut (i32, i32),
) {
    if let Some(a) = audio {
        a.set_volume(menu.settings.volume);
    }

    let want_fs = menu.settings.fullscreen;
    let is_fs = window.with_window_mode(|mode| matches!(mode, WindowMode::FullScreen(_)));

    if want_fs && !is_fs {
        *windowed_pos = window.get_pos();
        *windowed_size = window.get_size();
        glfw.with_primary_monitor(|_, monitor| {
            if let Some(monitor) = monitor {
                if let Some(mode) = monitor.get_video_mode() {
                    window.set_monitor(
                        WindowMode::FullScreen(monitor),
                        0,
                        0,
                        mode.width,
                        mode.height,
                        Some(mode.refresh_rate),
                    );
                }
            }
        });
    } else if !want_fs && is_fs {
        let (x, y) = *windowed_pos;
        let (w, h) = *windowed_size;
        window.set_monitor(WindowMode::Windowed, x, y, w as u32, h as u32, None);
    }
}

/// Wall-clock duration of one PPU frame for the region.
fn frame_duration(region: Region) -> Duration {
    let (num, den) = match region {
        Region::Ntsc => (
            crate::clock::ntsc::FRAME_HZ_NUM as u64,
            crate::clock::ntsc::FRAME_HZ_DEN as u64,
        ),
        Region::Pal => (
            crate::clock::pal::FRAME_HZ_NUM as u64,
            crate::clock::pal::FRAME_HZ_DEN as u64,
        ),
    };
    Duration::from_nanos(1_000_000_000u64.saturating_mul(den) / num)
}
