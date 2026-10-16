mod anim;
mod audio;
mod config;
mod ctl;
mod render;
mod timer;
mod wallpaper;
mod window;

use anim::Tween;
use audio::{Audio, Cmd, SOUNDS};
use config::{Config, Transition};
use render::{Leaving, Mode, Renderer, Scene};
use std::path::PathBuf;
use std::time::Instant;
use timer::{Event, Phase, Timer};
use window::{Input, Res, Window};

const FRAME_MS: u64 = 16;
const TOAST_MS: u64 = 2000;
const WALLPAPER_RECHECK_MS: u64 = 30_000;

mod keys {
    pub const RETURN: u32 = 0xff0d;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const BACKSPACE: u32 = 0xff08;
    pub const DELETE: u32 = 0xffff;
    pub const UP: u32 = 0xff52;
    pub const DOWN: u32 = 0xff54;
    pub const KP_DELETE: u32 = 0xff9f;
    pub const KP_0: u32 = 0xffb0;
    pub const KP_9: u32 = 0xffb9;
    pub const KP_ADD: u32 = 0xffab;
    pub const KP_SUBTRACT: u32 = 0xffad;

    /// The keypad always types digits, whether NumLock is on (KP_0…KP_9)
    /// or off (KP_Insert, KP_End, KP_Down, … — the same keys' navigation level).
    pub fn keypad_digit(sym: u32) -> Option<u8> {
        Some(match sym {
            KP_0..=KP_9 => (sym - KP_0) as u8,
            0xff9e => 0, // KP_Insert
            0xff9c => 1, // KP_End
            0xff99 => 2, // KP_Down
            0xff9b => 3, // KP_Next
            0xff96 => 4, // KP_Left
            0xff9d => 5, // KP_Begin
            0xff98 => 6, // KP_Right
            0xff95 => 7, // KP_Home
            0xff97 => 8, // KP_Up
            0xff9a => 9, // KP_Prior
            _ => return None,
        })
    }
}

struct Toast {
    text: String,
    start: u64,
    until: u64,
}

struct App {
    win: Window,
    renderer: Renderer,
    timer: Timer,
    audio: Audio,
    cfg: Config,
    cfg_path: PathBuf,
    ctl: ctl::Ctl,
    wallpaper: wallpaper::Wallpaper,
    epoch: Instant,
    light_auto: bool,
    invert: bool,
    wallpaper_at: u64,
    shown_at: u64,
    ss: String,
    enter_start: Option<u64>,
    leaving: Vec<Leaving>,
    tension: Tween,
    opacity: Tween,
    done_start: u64,
    pop_start: u64,
    toast: Option<Toast>,
    last_wheel: u64,
    input_rect: (i16, i16, u16, u16),
    dirty: bool,
}

fn pad(n: u32) -> String {
    format!("{n:02}")
}

impl App {
    fn new() -> Res<App> {
        let cfg_path = Config::default_path();
        let cfg = Config::load(&cfg_path);
        let ctl = ctl::Ctl::open(&ctl::default_path())?;
        Ok(App {
            win: Window::new()?,
            renderer: Renderer::new(),
            timer: Timer::default(),
            audio: Audio::spawn(),
            cfg,
            cfg_path,
            ctl,
            wallpaper: wallpaper::Wallpaper::default(),
            epoch: Instant::now(),
            light_auto: false,
            invert: false,
            wallpaper_at: 0,
            shown_at: 0,
            ss: "00".into(),
            enter_start: None,
            leaving: Vec::new(),
            tension: Tween::new(0.0, anim::TENSION_MS, anim::TENSION),
            opacity: Tween::new(1.0, anim::FADE_MS, anim::EASE),
            done_start: 0,
            pop_start: 0,
            toast: None,
            last_wheel: 0,
            input_rect: (0, 0, 0, 0),
            dirty: true,
        })
    }

    fn now(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    fn sound_name(&self) -> &'static str {
        SOUNDS[self.cfg.sound].1
    }

    fn target_gain(&self) -> f32 {
        if self.cfg.muted { 0.0 } else { self.cfg.volume }
    }

    /// Ambient gain while running, ducking through the last 10 seconds.
    fn running_gain(&self) -> f32 {
        let r = self.timer.remaining_secs();
        let duck = if (1..=10).contains(&r) { 0.15 + 0.85 * r as f32 / 10.0 } else { 1.0 };
        self.target_gain() * duck
    }

    fn flash(&mut self, text: String, ms: u64, now: u64) {
        self.toast = Some(Toast { text, start: now, until: now + ms });
        self.dirty = true;
    }

    fn save(&self) {
        if let Err(e) = self.cfg.save(&self.cfg_path) {
            eprintln!("countdown: cannot save {}: {e}", self.cfg_path.display());
        }
    }

    fn light_bg(&self) -> bool {
        self.light_auto ^ self.invert
    }

    fn refresh_wallpaper(&mut self, now: u64) {
        self.wallpaper_at = now;
        let (_, _, w, h) = self.win.monitor;
        if let Some(light) = self.wallpaper.is_light((w as u32, h as u32)) {
            if light != self.light_auto {
                self.light_auto = light;
                self.dirty = true;
            }
        }
    }

    fn cycle_sound(&mut self, dir: i32, now: u64) {
        let n = SOUNDS.len() as i32;
        self.cfg.sound = ((self.cfg.sound as i32 + dir).rem_euclid(n)) as usize;
        self.cfg.muted = false;
        self.save();
        self.flash(format!("ÂM THANH — {}", self.sound_name()), TOAST_MS, now);
        match self.timer.phase {
            Phase::Running => self.audio.start(self.cfg.sound, self.running_gain(), 0.4),
            Phase::Setup => self.audio.preview(self.cfg.sound, self.cfg.volume * 0.6),
            _ => {}
        }
    }

    fn toggle_mute(&mut self, now: u64) {
        self.cfg.muted = !self.cfg.muted;
        self.save();
        let label = if self.cfg.muted { "TẮT" } else { self.sound_name() };
        self.flash(format!("ÂM THANH — {label}"), TOAST_MS, now);
        match self.timer.phase {
            Phase::Running => self.audio.fade(self.running_gain(), 0.5),
            Phase::Setup if self.cfg.muted => self.audio.fade(0.0, 0.3),
            _ => {}
        }
    }

    fn change_volume(&mut self, delta: f32, now: u64) {
        self.cfg.volume = ((self.cfg.volume + delta) * 20.0).round().clamp(0.0, 20.0) / 20.0;
        self.save();
        self.flash(format!("ÂM LƯỢNG {}%", (self.cfg.volume * 100.0).round()), TOAST_MS, now);
        match self.timer.phase {
            Phase::Running => self.audio.fade(self.running_gain(), 0.3),
            Phase::Setup if !self.cfg.muted => self.audio.preview(self.cfg.sound, self.cfg.volume * 0.6),
            _ => {}
        }
    }

    fn on_input(&mut self, input: Input, now: u64, ev: &mut Vec<Event>) {
        self.dirty = true;
        match input {
            Input::Click => self.timer.activate(now, ev),
            Input::Wheel(d) => {
                if now.saturating_sub(self.last_wheel) >= 300 {
                    self.last_wheel = now;
                    self.cycle_sound(d as i32, now);
                }
            }
            Input::Time(t) => {
                let _ = self.win.focus(t);
            }
            Input::Expose => {}
            Input::Key { sym, ctrl } => {
                if ctrl {
                    return;
                }
                if let Some(d) = keys::keypad_digit(sym) {
                    self.timer.digit(d, ev);
                    return;
                }
                match sym {
                    0x30..=0x39 => self.timer.digit((sym - 0x30) as u8, ev),
                    keys::RETURN | keys::KP_ENTER => self.timer.activate(now, ev),
                    keys::ESCAPE | 0x71 | 0x51 => self.timer.hide(ev),
                    keys::BACKSPACE | keys::DELETE | keys::KP_DELETE => self.timer.backspace(),
                    keys::UP => self.cycle_sound(-1, now),
                    keys::DOWN => self.cycle_sound(1, now),
                    0x6d | 0x4d => self.toggle_mute(now),
                    0x2b | 0x3d | keys::KP_ADD => self.change_volume(0.05, now),
                    0x2d | 0x5f | keys::KP_SUBTRACT => self.change_volume(-0.05, now),
                    0x73 | 0x53 => {
                        self.cfg.transition = match self.cfg.transition {
                            Transition::Shatter => Transition::Melt,
                            Transition::Melt => Transition::Shatter,
                        };
                        self.save();
                        let name = if self.cfg.transition == Transition::Melt { "TAN CHẢY" } else { "VỠ VỤN" };
                        self.flash(format!("CHUYỂN CẢNH — {name}"), TOAST_MS, now);
                    }
                    0x74 | 0x54 => {
                        self.invert = !self.invert;
                        let name = if self.light_bg() { "ĐEN" } else { "TRẮNG NGÀ" };
                        self.flash(format!("MÀU CHỮ — {name}"), TOAST_MS, now);
                    }
                    _ => {}
                }
            }
        }
    }

    fn on_event(&mut self, e: Event, now: u64) {
        self.dirty = true;
        match e {
            Event::Shown => {
                self.shown_at = now;
                self.leaving.clear();
                self.enter_start = None;
                self.opacity.snap(1.0);
                self.tension.snap(0.0);
                let text = if self.cfg.muted {
                    "ÂM THANH — TẮT · M ĐỂ BẬT".to_string()
                } else {
                    format!("ÂM THANH — {} · ↑ ↓ ĐỂ ĐỔI", self.sound_name())
                };
                self.flash(text, 3500, now);
            }
            Event::Hidden => {
                self.audio.send(Cmd::StopRing);
                self.audio.fade(0.0, 0.4);
                self.toast = None;
                self.leaving.clear();
            }
            Event::Typed => self.pop_start = now,
            Event::Started { total } => {
                self.ss = pad(total % 60);
                self.enter_start = None;
                self.leaving.clear();
                self.tension.snap(timer::tension(Phase::Running, total));
                self.audio.start(self.cfg.sound, self.running_gain(), 2.5);
                let label = if self.cfg.muted { "TẮT" } else { self.sound_name() };
                self.flash(format!("ÂM THANH — {label}"), 2200, now);
            }
            Event::Paused => {
                self.opacity.set(0.38, now);
                self.audio.fade(0.0, 0.6);
            }
            Event::Resumed => {
                self.opacity.set(1.0, now);
                self.audio.start(self.cfg.sound, self.running_gain(), 2.5);
            }
            Event::Second { remaining } => {
                let old = std::mem::replace(&mut self.ss, pad(remaining % 60));
                self.leaving.push(Leaving { text: old, start: now });
                self.enter_start = Some(now);
                self.tension.set(timer::tension(Phase::Running, remaining), now);
                if (1..=10).contains(&remaining) {
                    self.audio.fade(self.running_gain(), 0.9);
                }
            }
            Event::Finished => {
                self.done_start = now;
                self.tension.set(1.0, now);
                self.audio.fade(0.0, 1.2);
                if !self.cfg.muted {
                    self.audio.send(Cmd::Ring);
                }
            }
        }
    }

    fn leave_ms(&self) -> u64 {
        match self.cfg.transition {
            Transition::Shatter => anim::SHATTER_MS,
            Transition::Melt => anim::MELT_MS,
        }
    }

    fn animating(&self, now: u64) -> bool {
        let phase = self.timer.phase;
        !self.leaving.is_empty()
            || self.enter_start.is_some_and(|s| now < s + anim::DROP_DELAY_MS + anim::DROP_MS)
            || (phase == Phase::Setup && now < self.pop_start + anim::POP_MS)
            || self.tension.active(now)
            || self.opacity.active(now)
            || (phase == Phase::Done && now < self.done_start + anim::RELEASE_MS)
            || (phase == Phase::Running && self.tension.target() > 0.0)
            || self.toast.as_ref().is_some_and(|t| now < t.start + anim::FADE_MS || (now >= t.until && now < t.until + anim::FADE_MS))
    }

    fn toast_vis(&self, now: u64) -> Option<(String, f32)> {
        let t = self.toast.as_ref()?;
        let fade = anim::FADE_MS as f32;
        let vis = if now < t.until { ((now - t.start) as f32 / fade).min(1.0) } else { 1.0 - ((now - t.until) as f32 / fade).min(1.0) };
        Some((t.text.clone(), vis))
    }

    fn render(&mut self, now: u64) -> Res<()> {
        let leave_ms = self.leave_ms();
        self.leaving.retain(|l| now < l.start + leave_ms);
        if self.toast.as_ref().is_some_and(|t| now >= t.until + anim::FADE_MS) {
            self.toast = None;
        }
        let phase = self.timer.phase;
        let mode = if phase == Phase::Setup {
            let buf = self.timer.buffer();
            let mut digits = [0u8; 4];
            for (i, c) in format!("{buf:0>4}").bytes().enumerate() {
                digits[i] = c - b'0';
            }
            Mode::Setup { digits, typed: buf.len(), pop_start: self.pop_start, caret_on: (now - self.shown_at) % 1000 < 500 }
        } else {
            let r = self.timer.remaining_secs();
            let t = self.tension.get(now);
            let running = phase == Phase::Running;
            Mode::Timing {
                mm: pad(r / 60),
                ss: self.ss.clone(),
                enter_start: self.enter_start,
                leaving: std::mem::take(&mut self.leaving),
                transition: self.cfg.transition,
                tension: t,
                done: phase == Phase::Done,
                done_start: self.done_start,
                dots_on: !running || self.timer.sub_second(now) < 0.5,
                shake: if running { anim::shake(anim::shake_level(self.tension.target()), now) } else { (0.0, 0.0, 0.0) },
            }
        };
        let toast = self.toast_vis(now);
        let scene = Scene { now, light_bg: self.light_bg(), opacity: self.opacity.get(now), mode: &mode, toast: toast.as_ref().map(|(t, v)| (t.as_str(), *v)) };
        let cr = cairo::Context::new(&self.win.surface)?;
        let (x, y, w, h) = self.renderer.draw(&cr, window::WIDTH as f64, &scene);
        drop(cr);
        if let Mode::Timing { leaving, .. } = mode {
            self.leaving = leaving;
        }
        let rect = (x as i16, y.max(0.0) as i16, w as u16, h as u16);
        if rect != self.input_rect {
            self.input_rect = rect;
            self.win.set_input_rects(&[rect])?;
        }
        self.win.present()?;
        self.dirty = false;
        Ok(())
    }

    /// Milliseconds until something needs doing, or -1 to sleep until input.
    fn timeout(&self, now: u64) -> i32 {
        if !self.win.mapped {
            return -1;
        }
        if self.animating(now) {
            return FRAME_MS as i32;
        }
        let mut next = self.wallpaper_at + WALLPAPER_RECHECK_MS;
        if let Some(d) = self.timer.next_deadline(now) {
            next = next.min(d);
        }
        match self.timer.phase {
            Phase::Setup => next = next.min(now + 500 - (now - self.shown_at) % 500),
            Phase::Running => {
                let into = (self.timer.sub_second(now) * 1000.0) as u64;
                next = next.min(now + if into < 500 { 500 - into } else { 1000 - into }.max(1));
            }
            _ => {}
        }
        if let Some(t) = &self.toast {
            next = next.min(if now < t.until { t.until } else { t.until + anim::FADE_MS });
        }
        next.saturating_sub(now).clamp(1, 60_000) as i32
    }

    fn run(&mut self) -> Res<()> {
        let mut inputs = Vec::new();
        let mut events = Vec::new();
        loop {
            let now = self.now();
            self.win.poll_events(&mut inputs)?;
            for i in inputs.drain(..) {
                self.on_input(i, now, &mut events);
            }
            for c in self.ctl.read() {
                match c {
                    ctl::Command::Toggle => {
                        self.timer.toggle(&mut events);
                        if matches!(self.timer.phase, Phase::Running | Phase::Paused | Phase::Done) {
                            // Already visible: just bring it back to front and take the keyboard.
                            self.win.show()?;
                        }
                    }
                    ctl::Command::Hide => self.timer.hide(&mut events),
                    ctl::Command::Quit => return Ok(()),
                    ctl::Command::Type(digits) => {
                        for d in digits.bytes().filter(u8::is_ascii_digit) {
                            self.timer.digit(d - b'0', &mut events);
                        }
                    }
                    ctl::Command::Enter => self.timer.activate(now, &mut events),
                }
            }
            self.timer.update(now, &mut events);
            for e in std::mem::take(&mut events) {
                self.on_event(e, now);
            }

            let visible = self.timer.is_visible();
            if visible && !self.win.mapped {
                self.win.show()?;
                self.refresh_wallpaper(now);
            } else if !visible && self.win.mapped {
                self.win.hide()?;
            } else if visible && now >= self.wallpaper_at + WALLPAPER_RECHECK_MS {
                self.refresh_wallpaper(now);
            }
            // We only wake for input, animation frames or blink/second edges, so every wake redraws.
            if self.win.mapped {
                self.render(now)?;
            }
            self.win.flush()?;

            // Events may have been queued while waiting for replies; handle them before sleeping.
            self.win.poll_events(&mut inputs)?;
            if !inputs.is_empty() {
                continue;
            }
            let mut fds = [
                libc::pollfd { fd: self.win.fd(), events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: self.ctl.fd(), events: libc::POLLIN, revents: 0 },
            ];
            let timeout = self.timeout(self.now());
            unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
            if fds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                return Err("display connection closed".into());
            }
        }
    }
}

fn main() {
    let mut app = match App::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("countdown: {e}");
            std::process::exit(1);
        }
    };
    if std::env::args().any(|a| a == "--show") {
        let mut ev = Vec::new();
        app.timer.toggle(&mut ev);
        for e in ev {
            app.on_event(e, 0);
        }
    }
    if let Err(e) = app.run() {
        eprintln!("countdown: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::keys::keypad_digit;

    #[test]
    fn keypad_types_digits_with_or_without_numlock() {
        // NumLock on: KP_1, KP_3, KP_4, KP_6, KP_8
        let on: Vec<_> = [0xffb1, 0xffb3, 0xffb4, 0xffb6, 0xffb8].into_iter().filter_map(keypad_digit).collect();
        assert_eq!(on, vec![1, 3, 4, 6, 8]);
        // NumLock off: KP_End, KP_Next, KP_Left, KP_Right, KP_Up
        let off: Vec<_> = [0xff9c, 0xff9b, 0xff96, 0xff98, 0xff97].into_iter().filter_map(keypad_digit).collect();
        assert_eq!(off, vec![1, 3, 4, 6, 8]);
        // Main arrow keys are not digits (they change the sound).
        assert_eq!(keypad_digit(0xff52), None);
        assert_eq!(keypad_digit(0xff54), None);
    }
}
