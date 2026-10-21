# rs-nes

A NES emulator written in Rust — play classic `.nes` ROMs and explore how an emulator works.

## Purpose

Run NES games on a modern desktop with a simple flow: pick a ROM, play, pause, tweak settings. The project prioritizes playable classics over covering every rare cartridge.

## Features

### Play
- Open a ROM via a native file dialog, or pass a path on the command line
- Switch ROMs without quitting (`Cmd/Ctrl+O`)
- Video and audio kept in sync with the frame rate

### Pause menu
- `Esc` or `P` to pause / resume
- Volume control
- Fullscreen toggle
- On-screen debug info toggle

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
# Build and run (opens a ROM picker)
cargo run --release

# Or pass a ROM path
cargo run --release -- path/to/game.nes
```

Package a release for the current machine:

```bash
./scripts/build-release.sh
```

Artifacts land in `dist/`.

## Useful shortcuts

| Key | Action |
|-----|--------|
| `Esc` / `P` | Pause / resume |
| `Cmd/Ctrl+O` | Open another ROM |
| In menu | ↑↓ / WASD move · Enter / Z / Space confirm · Esc / X back |

Default keyboard (Player 1): **Z** = A, **X** = B, **Enter** = Start, **Right Shift** = Select, arrow keys = D-pad.
