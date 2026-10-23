pub mod audio;
pub mod cli;
pub mod config;
pub mod controller;
pub mod menu;
pub mod os;
pub mod overlay;
pub mod screenshot;
pub mod theme;
pub mod video;

use crate::cart::Loader;
use crate::clock::Region;
use crate::emulator::audio::AudioOut;
use crate::emulator::cli::{Cli, ParseOutcome};
use crate::emulator::controller::Controllers;
use crate::emulator::menu::draw_debug;
use crate::emulator::os::{Os, OsAction};
use crate::emulator::overlay::Overlay;
use crate::emulator::video::GlPresenter;
use crate::nes::Nes;
use glfw::{Action, Context, InitHint, Key, WindowEvent, WindowHint, WindowMode};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

/// Desktop / CLI entry used by the `rs-nes` binary.
///
/// See `rs-nes --help` for the full option list.
pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = match cli::parse(&args) {
        Ok(ParseOutcome::Help) => {
            cli::print_help();
            return;
        }
        Ok(ParseOutcome::Version) => {
            cli::print_version();
            return;
        }
        Ok(ParseOutcome::Run(cli)) => cli,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    if cli.headless {
        let rom_path = match cli.path.clone().or_else(pick_rom_dialog) {
            Some(p) if p.is_file() => p,
            Some(p) => {
                eprintln!("headless mode requires a ROM file, got {}", p.display());
                std::process::exit(2);
            }
            None => {
                eprintln!("headless mode requires a ROM path");
                std::process::exit(2);
            }
        };
        let rom = Loader::load(rom_path.to_str().unwrap_or_default())
            .unwrap_or_else(|e| panic!("load ROM {}: {e:?}", rom_path.display()));
        let mut nes = Nes::from_rom(rom).expect("build NES");
        run_headless(&mut nes);
        return;
    }

    run_windowed(cli);
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

/// Native folder picker for adding a directory of `.nes` ROMs to the library.
fn pick_folder_dialog() -> Option<PathBuf> {
    let path = rfd::FileDialog::new()
        .set_title("Add game folder")
        .pick_folder();
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

fn run_windowed(cli: Cli) {
    glfw::init_hint(InitHint::CocoaChdirResources(false));
    glfw::init_hint(InitHint::CocoaMenubar(true));

    let mut glfw = glfw::init(glfw::fail_on_errors).expect("glfw init");
    glfw.window_hint(WindowHint::ContextVersion(3, 3));
    glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    glfw.window_hint(WindowHint::Resizable(true));
    glfw.window_hint(WindowHint::Focused(true));

    let (mut window, events) = glfw
        .create_window(768, 720, "Rust NES", WindowMode::Windowed)
        .expect("create GLFW window");

    window.make_current();
    window.set_key_polling(true);
    window.set_focus_polling(true);
    macos_activate_for_input();
    window.focus();
    glfw.set_swap_interval(glfw::SwapInterval::Sync(1));

    gl::load_with(|s| window.get_proc_address(s) as *const _);
    let mut presenter = GlPresenter::new().expect("GL presenter");
    let mut overlay = Overlay::new().expect("UI overlay");

    let audio = match AudioOut::start() {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("audio disabled: {e}");
            None
        }
    };

    let mut controllers = Controllers::new();
    let mut os = Os::new();
    apply_cli_to_os(&mut os, &cli);

    // Remember windowed geometry for leaving fullscreen.
    let mut windowed_pos = window.get_pos();
    let mut windowed_size = window.get_size();
    apply_settings(
        &os.menu.settings,
        &audio,
        &mut glfw,
        &mut window,
        &mut windowed_pos,
        &mut windowed_size,
    );

    let mut nes: Option<Nes> = None;
    let mut rom_path: Option<PathBuf> = None;

    let mut frame_period = frame_duration(Region::Ntsc);
    let mut next_frame = Instant::now() + frame_period;
    let mut fps = 60.0_f32;
    let mut fps_timer = Instant::now();
    let mut fps_frames = 0u32;

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Key(Key::P, _, Action::Press, _)
                    if !os.in_shell() && !os.is_paused() =>
                {
                    os.toggle_pause();
                    if let Some(a) = audio.as_ref() {
                        a.clear_queue();
                    }
                }
                WindowEvent::Key(Key::O, _, Action::Press, mods)
                    if !os.in_shell()
                        && !os.is_paused()
                        && (mods.contains(glfw::Modifiers::Control)
                            || mods.contains(glfw::Modifiers::Super)) =>
                {
                    if let Some(path) = pick_rom_dialog() {
                        load_rom_into(
                            &path,
                            &mut nes,
                            &mut rom_path,
                            &audio,
                            &os,
                            &mut window,
                            &mut frame_period,
                        );
                        os.enter_playing();
                    }
                    macos_activate_for_input();
                    window.focus();
                }
                WindowEvent::Key(key, _, action, _) => {
                    // Always track keyboard so NES-mapped keys drive the OS shell.
                    controllers.on_key(key, action);
                    if os.is_waiting_bind() {
                        let act = os.on_key_for_bind(key, action);
                        handle_os_action(
                            act,
                            &mut os,
                            &mut nes,
                            &mut rom_path,
                            &audio,
                            &mut glfw,
                            &mut window,
                            &mut windowed_pos,
                            &mut windowed_size,
                            &mut frame_period,
                        );
                    }
                }
                WindowEvent::Focus(focused) => {
                    controllers.on_focus(focused);
                }
                _ => {}
            }
        }

        if os.is_waiting_bind() {
            let act = os.poll_bind_capture(&glfw);
            handle_os_action(
                act,
                &mut os,
                &mut nes,
                &mut rom_path,
                &audio,
                &mut glfw,
                &mut window,
                &mut windowed_pos,
                &mut windowed_size,
                &mut frame_period,
            );
        }

        os.tick_boot();
        os.tick_blink();

        let pads = controllers.poll(
            &glfw,
            &os.menu.settings.keyboard,
            &os.menu.settings.gamepad,
            &os.menu.settings.devices,
        );

        // Shell + pause UI, and Select-to-pause while playing.
        if !os.is_waiting_bind() {
            if os.in_shell() || os.is_paused() {
                let (act, sfx) = os.on_pad(pads[0]);
                handle_os_action(
                    act,
                    &mut os,
                    &mut nes,
                    &mut rom_path,
                    &audio,
                    &mut glfw,
                    &mut window,
                    &mut windowed_pos,
                    &mut windowed_size,
                    &mut frame_period,
                );
                // Play after ApplySettings so volume tweaks are audible at the new level.
                if let (Some(a), Some(sfx)) = (audio.as_ref(), sfx) {
                    a.play_sfx(sfx);
                }
            } else {
                // Playing: Select opens the pause menu (not forwarded to the cart).
                let _ = os.on_pad(pads[0]);
                if os.is_paused() {
                    if let Some(a) = audio.as_ref() {
                        a.clear_queue();
                    }
                }
            }
        }

        if !os.in_shell() && !os.is_paused() {
            if let Some(ref mut n) = nes {
                // Select is reserved for the pause menu while in-game.
                let select = crate::bus::Button::Select as u8;
                n.set_controller(0, pads[0] & !select);
                n.set_controller(1, pads[1] & !select);
                if let Err(e) = n.run_frame() {
                    eprintln!("emulation error: {e:?}");
                    break;
                }

                if let Some(a) = audio.as_ref() {
                    let samples = n.take_audio_samples();
                    a.push_samples(&samples);
                } else {
                    let _ = n.take_audio_samples();
                }
            }
        } else if let Some(ref mut n) = nes {
            // Frozen / in shell with a lingering cart: discard residual audio.
            let _ = n.take_audio_samples();
        }

        let (fb_w, fb_h) = window.get_framebuffer_size();
        let filter = os.menu.settings.video_filter;

        if os.in_shell() {
            presenter.ensure_ui_target(fb_w, fb_h);
            presenter.begin_ui([0.04, 0.10, 0.22, 1.0]);
            overlay.begin(fb_w, fb_h);
            os.draw(&mut overlay, &glfw, fb_w, fb_h);
            overlay.flush();
            presenter.present_ui(fb_w, fb_h, filter);
        } else if let Some(ref n) = nes {
            presenter.upload(n.framebuffer());
            presenter.draw(fb_w, fb_h, filter);

            overlay.begin(fb_w, fb_h);
            os.draw(&mut overlay, &glfw, fb_w, fb_h);
            if os.menu.settings.debug_info {
                draw_debug(&mut overlay, n, fps, pads, fb_w, fb_h);
            }
            overlay.flush();
        }

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

fn handle_os_action(
    act: OsAction,
    os: &mut Os,
    nes: &mut Option<Nes>,
    rom_path: &mut Option<PathBuf>,
    audio: &Option<AudioOut>,
    glfw: &mut glfw::Glfw,
    window: &mut glfw::Window,
    windowed_pos: &mut (i32, i32),
    windowed_size: &mut (i32, i32),
    frame_period: &mut Duration,
) {
    match act {
        OsAction::None => {}
        OsAction::ApplySettings => {
            apply_settings(
                &os.menu.settings,
                audio,
                glfw,
                window,
                windowed_pos,
                windowed_size,
            );
            os.save_config();
        }
        OsAction::Quit => window.set_should_close(true),
        OsAction::LoadRom(path) => {
            load_rom_into(
                &path,
                nes,
                rom_path,
                audio,
                os,
                window,
                frame_period,
            );
            os.enter_playing();
        }
        OsAction::ResetGame => {
            let Some(path) = rom_path.clone() else {
                return;
            };
            load_rom_into(
                &path,
                nes,
                rom_path,
                audio,
                os,
                window,
                frame_period,
            );
            os.enter_playing();
        }
        OsAction::Screenshot => {
            if let Some(n) = nes.as_ref() {
                let path = screenshot::default_path();
                match screenshot::save_bmp(&path, n.framebuffer()) {
                    Ok(()) => eprintln!("screenshot saved: {}", path.display()),
                    Err(e) => eprintln!("screenshot failed: {e}"),
                }
            }
        }
        OsAction::ExitToLibrary => {
            if let Some(a) = audio.as_ref() {
                a.clear_queue();
            }
            *nes = None;
            *rom_path = None;
            window.set_title("Rust NES");
            os.return_to_library();
        }
        OsAction::PickFolder => {
            if let Some(path) = pick_folder_dialog() {
                os.add_folder(path);
                os.save_config();
            }
            macos_activate_for_input();
            window.focus();
        }
    }
}

fn load_rom_into(
    path: &Path,
    nes: &mut Option<Nes>,
    rom_path: &mut Option<PathBuf>,
    audio: &Option<AudioOut>,
    os: &Os,
    window: &mut glfw::Window,
    frame_period: &mut Duration,
) {
    let rom = match Loader::load(path.to_str().unwrap_or_default()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("load ROM {}: {e:?}", path.display());
            return;
        }
    };
    let mut n = match Nes::from_rom(rom) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("build NES: {e:?}");
            return;
        }
    };
    if let Some(a) = audio.as_ref() {
        n.set_audio_sample_rate(a.sample_rate);
        a.set_volume(os.menu.settings.volume);
        a.clear_queue();
    }
    *frame_period = frame_duration(n.region());
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("NES");
    window.set_title(&format!("Rust NES — {name}"));
    *rom_path = Some(path.to_path_buf());
    *nes = Some(n);
}

fn apply_cli_to_os(os: &mut Os, cli: &Cli) {
    if cli.skip_boot {
        os.skip_boot();
    }
    if let Some(filter) = cli.filter {
        os.menu.settings.video_filter = filter;
    }
    if let Some(volume) = cli.volume {
        os.menu.settings.volume = volume;
    }
    if cli.fullscreen {
        os.menu.settings.fullscreen = true;
    }
    for dir in &cli.roms_dirs {
        os.add_folder(dir.clone());
    }
    if let Some(ref path) = cli.path {
        os.add_path(path.clone());
    }
}

fn apply_settings(
    settings: &crate::emulator::menu::Settings,
    audio: &Option<AudioOut>,
    glfw: &mut glfw::Glfw,
    window: &mut glfw::Window,
    windowed_pos: &mut (i32, i32),
    windowed_size: &mut (i32, i32),
) {
    if let Some(a) = audio {
        a.set_volume(settings.volume);
    }

    let want_fs = settings.fullscreen;
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
