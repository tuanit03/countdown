# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A frameless, transparent countdown timer for GNOME, written in Rust and run from Docker. It idles hidden in the background (autostarted by a systemd user service) and is summoned with Ctrl+Alt+Space. Product spec: `Countdown — Đồng hồ đếm ngược không khung.md` (Vietnamese). Implementation design and decisions: `docs/superpowers/specs/2026-10-16-countdown-design.md`. The original interactive prototype (HTML/CSS/Web Audio, the reference for every animation and sound) is frame 00 of https://claude.ai/artifact/46hUhzLAFeijWV4A2eoj2k. Read it with the Artifact tool, path `project/Main.dc.html`.

Low RAM/CPU is a primary requirement. Hidden: ~4 MB, 0% CPU. Counting: ~1.5% CPU for the app plus ~3% for Xwayland.

## Commands

The host has no Rust toolchain. Build and test inside Docker:

```sh
docker build --target builder -t countdown-dev .     # dev image with deps precompiled
docker run --rm -v "$PWD":/w -v countdown-target:/w/target -w /w countdown-dev cargo build --release
docker run --rm -v "$PWD":/w -v countdown-target:/w/target -w /w countdown-dev cargo test --release
#   single test: append e.g. `timer::tests::run_pause_resume_finish_autohide`
docker build --target test .                          # full test stage
./install.sh                                          # build image + install service + hotkey
docker compose build && systemctl --user restart countdown   # redeploy
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
- `docker-compose.yml` needs `XAUTHORITY`, `DISPLAY` and `XDG_RUNTIME_DIR` from the session. The systemd user unit inherits them from GNOME. Run the container with `network_mode: none` and `init: true` (PID 1 signal handling).
- Melt is an approximation (faded smears). cairo has no blur/goo filter.
