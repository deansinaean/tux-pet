# tux-pet

Desktop pet application (X11/Wayland pet overlay).

## Build

```bash
cargo build --release        # dev build
make build                   # same as above
make deb                     # Debian package (dpkg-buildpackage)
make appimage                # AppImage
make install                 # install to /usr/bin + assets
```

## Run

```bash
TUX_ASSETS=/path/to/assets   # override pet assets location (default:exe_dir/assets/pet)
```

Binary accepts no CLI args. First instance kills any old instance via `pgrep -x tux-pet`.

## Architecture

- Single Rust binary; X11 override-redirect shaped window renders pet on desktop
- D-Bus (`org.tux.WindowTracker`) provides window list on Wayland; falls back to X11 scan
- WebSocket server on `127.0.0.1:9872` accepts JSON `PetConfig` for runtime control
- Pet characters defined in `assets/pet/<name>/pet.json5` with animations (video or frames), behavior rules, and trigger conditions
- Pet state (hunger/mood/energy) stored at `~/.config/tux/pet_state.json`
- Pet position saved at `~/.config/tux/pet_pos.json`

## Key Files

- `src/main.rs` — entry, event loop, X11 window management, D-Bus watcher, WS server
- `src/shared.rs` — character loading, animation selection, pet state, `tux_log!` macro
- `src/settings.rs` — X11-based settings window (character/anim picker, scale slider)
- `src/menu.rs` — right-click context menu
- `src/video.rs` — FFmpeg video player
- `assets/pet/<name>/pet.json5` — character definition (name, animations, trigger rules)

## Dev Notes

- No tests, no clippy, no rustfmt config present
- `ffmpeg-sys-next` links against system FFmpeg (libavcodec, libavformat, libswscale)
- `TICK_MS = 40` in `main.rs:87` — 25 fps render loop; modifying this affects animation timing
- `pets_dir()` in `shared.rs` walks a specific search path order: `TUX_ASSETS` > exe-relative > `/usr/share/tux-pet/pet` > `~/.local/share/tux-pet/pet`
