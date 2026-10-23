# rs-nes

A NES emulator written in Rust — play classic `.nes` ROMs and explore how an emulator works.

## Purpose

Run NES games on a modern desktop through a small “NES OS” shell: boot splash, game library, settings — then play. The emulator core stays focused on accuracy; the shell is the fancy host UI (OpenGL overlay, 80/90 look).

## Features

### NES OS
- Boot splash → **Game Library** (scans `roms/` for `.nes` files)
- Top bar shows date/time; video **Filter** applies to the OS shell as well as games
- Shell input uses only NES buttons: **D-pad**, **A**, **B**, **Select**, **Start**
  - **Start** / **A** — confirm
  - **Select** — system menu (Settings / Quit)
  - **B** — back
- Add a folder of ROMs via “Add folder…” in the library (or a CLI path / folder)
- In-game: **Select** (or `P`) → pause (**Resume** / **Reset game** / **Screenshot** / **Exit to OS**)

### Play
- Start on a library row to load a game
- Switch ROMs while playing (`Cmd/Ctrl+O`)
- Video and audio kept in sync with the frame rate

### Controllers
- Two-player support
- Keyboard and gamepads
- Assign a device per player
- Remap each button (A, B, Start, Select, D-pad) for keyboard or gamepad
- Reset mappings to defaults

### Compatible ROMs
Many well-known titles work thanks to common mappers, for example:
- Super Mario Bros.
- The Legend of Zelda, Metroid
- Contra, Mega Man
- Super Mario Bros. 3
- Battletoads
- and other games that use the same mapper families

### Cross-platform
- **macOS** — `.app` bundle and `.dmg` installer
- **Linux / Windows** — portable standalone binary

## Quick start

```bash
# Build and run (NES OS → game library)
cargo run --release

# Seed the library with a ROM or folder
cargo run --release -- path/to/game.nes
cargo run --release -- --roms path/to/roms --skip-boot
```

### CLI

```text
rs-nes [OPTIONS] [ROM_OR_FOLDER]

  --roms <DIR>        Add/scan a ROM folder (repeatable)
  --skip-boot         Skip the NES OS splash
  --filter <NAME>     sharp | smooth | scanlines | crt  (alias: nearest)
  --volume <0.0-1.0>  Master volume
  --fullscreen        Start in fullscreen
  --headless          Smoke test without a window (needs a ROM)
  -h, --help
  -V, --version
```

Package a release for the current machine:

```bash
./scripts/build-release.sh
```

Artifacts land in `dist/`.

## Config file

Settings (volume, filter, fullscreen, debug HUD, controllers, button maps) and folders added via **Add folder…** are saved automatically to:

| OS | Path |
|----|------|
| Linux | `~/.config/rs-nes/config.toml` (or `$XDG_CONFIG_HOME/rs-nes/`) |
| macOS | `~/Library/Application Support/rs-nes/config.toml` |
| Windows | `%APPDATA%\rs-nes\config.toml` |

CLI flags (`--filter`, `--volume`, …) override the file for the current session only until you change a setting in the UI.

## Useful shortcuts

| Input | Action |
|-------|--------|
| Select / `P` | Pause / resume (in-game); system menu in OS library |
| `Cmd/Ctrl+O` | Open another ROM (in-game) |
| D-pad | Move in OS / menus |
| Start / A | Confirm |
| B | Back |

Default keyboard (Player 1): **N** = A, **M** = B, **Enter** = Start, **Right Shift** = Select, **WASD** = D-pad.
