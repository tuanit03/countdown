# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A frameless, transparent countdown timer for GNOME, written in Rust and run from Docker. It idles hidden in the background (started at login by XDG autostart) and is summoned with Ctrl+Alt+Space. Users install it with `docker run --rm pythonking11/countdown install | sh`. Product spec: `Countdown — Đồng hồ đếm ngược không khung.md` (Vietnamese). Implementation design and decisions: `docs/superpowers/specs/2026-10-16-countdown-design.md`. The original interactive prototype (HTML/CSS/Web Audio, the reference for every animation and sound) is frame 00 of https://claude.ai/artifact/46hUhzLAFeijWV4A2eoj2k. Read it with the Artifact tool, path `project/Main.dc.html`.

Low RAM/CPU is a primary requirement. Hidden: ~4 MB, 0% CPU. Counting: ~1.5% CPU for the app plus ~3% for Xwayland.

## Commands

The host has no Rust toolchain. Build and test inside Docker:

```sh
docker build --target builder -t countdown-dev .     # dev image with deps precompiled
docker run --rm -v "$PWD":/w -v countdown-target:/w/target -w /w countdown-dev cargo build --release
docker run --rm -v "$PWD":/w -v countdown-target:/w/target -w /w countdown-dev cargo test --release
#   single test: append e.g. `timer::tests::run_pause_resume_finish_autohide`
docker build --target test .                          # full test stage
./install.sh                                          # build local image and (re)install it, replacing the running one
docker run --rm countdown:latest install              # print the host installer without running it
```

Base images come from `mirror.gcr.io` because Docker Hub has been flaky from this network. Don't add `# syntax=` lines, since those pull from Docker Hub.

Drive the running app through its control FIFO (`$XDG_RUNTIME_DIR/countdown/ctl`, which the container sees as `/run/countdown/ctl`). Commands are `show` (toggle), `hide`, `quit`, `type <digits>` and `enter`. `~/.local/bin/countdown-toggle "type 5" enter` starts a 5 s run.

To see what was drawn, `ffmpeg -f x11grab -window_id <id> -i :0.0 -frames:v 1 out.png` captures the window contents (alpha is dropped). Find `<id>` with `xwininfo -root -tree | grep Countdown`. Capturing the root shows black under Wayland. Note that `show` focuses the window, so keys the user types land in it while a test runs.

## Architecture

There is one binary with two threads. The main thread (`main.rs`) runs a `poll()` loop over the X connection and the control FIFO. Its timeout comes from `App::timeout`: 16 ms frames only while something animates, otherwise the next second/blink/toast edge, and infinite while hidden. The audio thread (`audio/mod.rs`) takes `Cmd`s over a channel and opens a PulseAudio stream only while something is audible.

- `timer.rs`: pure state machine (Hidden → Setup → Running ⇄ Paused → Done → Hidden). Time is injected as ms since start, so it is unit-tested. It emits `Event`s, which `App::on_event` turns into animation and audio actions.
- `anim.rs`: the prototype's CSS keyframes and cubic-beziers as pure functions (shards, melt, drop, pop, release, shake, tension axes) plus `Tween` for CSS transitions. Lengths are in em (120 px).
- `render.rs`: each glyph is rasterized once into a server-side sprite (pango outline, then a box-blurred shadow, then upload via `create_similar`). Frames are cairo-xcb composites of sprites with transforms and shard clip polygons. Keep per-frame work on sprites. Stroking or filling glyph paths every frame made cairo-xcb tessellate on the CPU, which cost about 14% CPU. Tension is quantized to 1/20 so color/axis changes hit the cache.
- `window.rs`: x11rb over libxcb (shared with cairo). It creates a 32-bit ARGB window with no decorations and UTILITY + ABOVE/STICKY/SKIP_* states, centered on the RandR primary monitor at y=0. The XShape input region limits clicks to the digits. Focus goes through `_NET_ACTIVE_WINDOW` with source=2, using a server timestamp obtained via a PropertyNotify round-trip (`Input::Time`).
- `audio/synth.rs`: a sample-level port of the prototype's Web Audio graphs (Kellet pink, leaky brown noise, RBJ biquads where lowpass/highpass Q is in dB like Web Audio, LFOs, gain-ramp queue, crossfading voices) and a twin-bell alarm (modal synthesis, hammer at 24 Hz, one ~0.9 s burst) when time is up.
- `wallpaper.rs`: Xwayland can't capture what is behind the window, so ink color comes from the GNOME wallpaper file. It reads `gsettings` through a read-only dconf mount and measures the luma of the top-center band under "zoom" scaling.
- `config.rs` / `ctl.rs`: JSON settings on the `countdown-data` volume, and the FIFO (recreated if a stale regular file is in its place).

## Platform constraints

- The app must stay an X11 client (via Xwayland). Under Wayland a client can't position itself or stay on top, and Xwayland can't grab global keys. That is why the hotkey is a GNOME custom keybinding that writes to the FIFO.
- Host integration is shell, embedded in the binary (`src/host.rs` uses `include_str!`). `install`/`uninstall` subcommands print `scripts/install.sh` / `scripts/uninstall.sh`. The installer writes `bin/countdown-run` (the single source of truth for `docker run` flags and mounts), `bin/countdown-toggle` (hotkey target) and an XDG autostart entry. There is no compose file or systemd unit. Placeholders: `@DEFAULT_IMAGE@` is filled by Rust from `COUNTDOWN_IMAGE_REF` (build arg `IMAGE_REF`), `@IMAGE@` in the launcher by the installer (honoring `COUNTDOWN_IMAGE`).
- The container is locked down: `--network none`, `--read-only`, `--cap-drop ALL`, `no-new-privileges`, and `--init` for PID 1 signals. It runs as the host user's uid so it can read XAUTHORITY. Anything that must write goes to the tmpfs mounts or `/data`.
- The GNOME `custom-keybindings` list is shared with the user's own shortcuts. Edit it only through `other_bindings` in the scripts, which parses quoted entries. The 0.1.x installer used `tr -d "[]@as'"`, which mangled paths, and the scripts still clean up those leftovers.
- Font downloads in the Dockerfile are pinned to google/fonts commits and verified by sha256. When bumping a font, update the revision and the checksum together.
- `.dockerignore` is a whitelist (`Cargo.*`, `src/`, `scripts/`, `bin/`). Add new build inputs there explicitly.
- Melt is an approximation (faded smears). cairo has no blur/goo filter.
