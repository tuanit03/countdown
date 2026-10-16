//! Pure countdown state machine. Time is passed in as milliseconds since app start.

pub const MAX_SECS: u32 = 99 * 60 + 59;
pub const AUTO_HIDE_MS: u64 = 8000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Hidden,
    Setup,
    Running,
    Paused,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// Became visible in the setup phase.
    Shown,
    Hidden,
    Started { total: u32 },
    Paused,
    Resumed,
    /// The displayed second changed while running; `remaining` is the new value.
    Second { remaining: u32 },
    Finished,
    /// Typed digit (index into the 4-digit buffer that just appeared).
    Typed,
}

pub struct Timer {
    pub phase: Phase,
    buffer: String,
    /// Remaining ms when paused, or at the moment `started_at` was taken.
    remaining_ms: u64,
    started_at: u64,
    shown_secs: u32,
    done_at: u64,
}

impl Default for Timer {
    fn default() -> Self {
        Timer { phase: Phase::Hidden, buffer: String::new(), remaining_ms: 0, started_at: 0, shown_secs: 0, done_at: 0 }
    }
}

impl Timer {
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Seconds entered so far (microwave style: "425" = 04:25), clamped to 99:59.
    pub fn entered_secs(&self) -> u32 {
        let d = format!("{:0>4}", self.buffer);
        let m: u32 = d[..2].parse().unwrap_or(0);
        let s: u32 = d[2..].parse().unwrap_or(0);
        (m * 60 + s).min(MAX_SECS)
    }

    /// Whole seconds currently displayed while timing.
    pub fn remaining_secs(&self) -> u32 {
        self.shown_secs
    }

    fn remaining_at(&self, now: u64) -> u64 {
        match self.phase {
            Phase::Running => self.remaining_ms.saturating_sub(now.saturating_sub(self.started_at)),
            _ => self.remaining_ms,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.phase != Phase::Hidden
    }

    /// Hotkey: hidden → setup, setup → hidden, otherwise stays (caller refocuses).
    pub fn toggle(&mut self, out: &mut Vec<Event>) {
        match self.phase {
            Phase::Hidden => {
                self.buffer.clear();
                self.phase = Phase::Setup;
                out.push(Event::Shown);
            }
            Phase::Setup => self.hide(out),
            _ => {}
        }
    }

    pub fn hide(&mut self, out: &mut Vec<Event>) {
        if self.phase != Phase::Hidden {
            self.phase = Phase::Hidden;
            out.push(Event::Hidden);
        }
    }

    pub fn digit(&mut self, d: u8, out: &mut Vec<Event>) {
        if self.phase != Phase::Setup || self.buffer.len() >= 4 || (self.buffer.is_empty() && d == 0) {
            return;
        }
        self.buffer.push((b'0' + d) as char);
        out.push(Event::Typed);
    }

    pub fn backspace(&mut self) {
        if self.phase == Phase::Setup {
            self.buffer.pop();
        }
    }

    /// Click / Enter.
    pub fn activate(&mut self, now: u64, out: &mut Vec<Event>) {
        match self.phase {
            Phase::Setup => {
                let total = self.entered_secs();
                if total == 0 {
                    return;
                }
                self.remaining_ms = total as u64 * 1000;
                self.started_at = now;
                self.shown_secs = total;
                self.phase = Phase::Running;
                out.push(Event::Started { total });
            }
            Phase::Running => {
                self.remaining_ms = self.remaining_at(now);
                self.phase = Phase::Paused;
                out.push(Event::Paused);
            }
            Phase::Paused => {
                self.started_at = now;
                self.phase = Phase::Running;
                out.push(Event::Resumed);
            }
            Phase::Done => self.hide(out),
            Phase::Hidden => {}
        }
    }

    /// Advance the clock. Emits at most one `Second` per call (the latest), plus `Finished`.
    pub fn update(&mut self, now: u64, out: &mut Vec<Event>) {
        match self.phase {
            Phase::Running => {
                let rem = self.remaining_at(now);
                let secs = rem.div_ceil(1000) as u32;
                if secs != self.shown_secs {
                    self.shown_secs = secs;
                    out.push(Event::Second { remaining: secs });
                }
                if rem == 0 {
                    self.remaining_ms = 0;
                    self.phase = Phase::Done;
                    self.done_at = now;
                    out.push(Event::Finished);
                }
            }
            Phase::Done if now >= self.done_at + AUTO_HIDE_MS => self.hide(out),
            _ => {}
        }
    }

    /// Next instant (ms) at which `update` would emit something, if any.
    pub fn next_deadline(&self, now: u64) -> Option<u64> {
        match self.phase {
            Phase::Running => {
                let rem = self.remaining_at(now);
                let into = rem % 1000;
                Some(now + if into == 0 { 1000.min(rem.max(1)) } else { into })
            }
            Phase::Done => Some(self.done_at + AUTO_HIDE_MS),
            _ => None,
        }
    }

    /// Fraction 0..1 of the current second that has elapsed (for blinking colon).
    pub fn sub_second(&self, now: u64) -> f32 {
        let rem = self.remaining_at(now);
        let into = rem % 1000;
        if into == 0 { 0.0 } else { 1.0 - into as f32 / 1000.0 }
    }
}

/// Last-10-seconds tension 0..1: 0 at 10 s left, 1 at 1 s left (and when done).
pub fn tension(phase: Phase, remaining: u32) -> f32 {
    match phase {
        Phase::Done => 1.0,
        Phase::Running | Phase::Paused if (1..=10).contains(&remaining) => (10 - remaining) as f32 / 9.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Timer, Vec<Event>) {
        let mut t = Timer::default();
        let mut ev = vec![];
        t.toggle(&mut ev);
        (t, ev)
    }

    #[test]
    fn microwave_entry() {
        let (mut t, mut ev) = setup();
        t.digit(0, &mut ev); // leading zero ignored
        for d in [4, 2, 5] {
            t.digit(d, &mut ev);
        }
        assert_eq!(t.buffer(), "425");
        assert_eq!(t.entered_secs(), 4 * 60 + 25);
        t.digit(9, &mut ev);
        t.digit(9, &mut ev); // 5th digit ignored
        assert_eq!(t.buffer(), "4259");
        t.backspace();
        assert_eq!(t.buffer(), "425");
    }

    #[test]
    fn entry_clamps_to_max() {
        let (mut t, mut ev) = setup();
        for d in [9, 9, 9, 9] {
            t.digit(d, &mut ev);
        }
        assert_eq!(t.entered_secs(), MAX_SECS);
    }

    #[test]
    fn zero_does_not_start() {
        let (mut t, mut ev) = setup();
        t.activate(0, &mut ev);
        assert_eq!(t.phase, Phase::Setup);
    }

    #[test]
    fn run_pause_resume_finish_autohide() {
        let (mut t, mut ev) = setup();
        t.digit(3, &mut ev);
        ev.clear();
        t.activate(1000, &mut ev);
        assert_eq!(ev, vec![Event::Started { total: 3 }]);
        assert_eq!(t.remaining_secs(), 3);

        ev.clear();
        t.update(1500, &mut ev);
        assert!(ev.is_empty());
        t.update(2000, &mut ev);
        assert_eq!(ev, vec![Event::Second { remaining: 2 }]);

        ev.clear();
        t.activate(2500, &mut ev); // pause with 1.5 s left
        assert_eq!(t.phase, Phase::Paused);
        t.update(9000, &mut ev);
        assert_eq!(ev, vec![Event::Paused]);

        ev.clear();
        t.activate(10_000, &mut ev); // resume
        t.update(10_500, &mut ev);
        assert_eq!(ev, vec![Event::Resumed, Event::Second { remaining: 1 }]);

        ev.clear();
        t.update(11_500, &mut ev);
        assert_eq!(ev, vec![Event::Second { remaining: 0 }, Event::Finished]);
        assert_eq!(t.phase, Phase::Done);

        ev.clear();
        t.update(11_500 + AUTO_HIDE_MS, &mut ev);
        assert_eq!(ev, vec![Event::Hidden]);
    }

    #[test]
    fn toggle_hides_only_from_setup() {
        let (mut t, mut ev) = setup();
        t.digit(5, &mut ev);
        t.activate(0, &mut ev);
        t.toggle(&mut ev);
        assert_eq!(t.phase, Phase::Running);
        let (mut t, mut ev) = setup();
        t.toggle(&mut ev);
        assert_eq!(t.phase, Phase::Hidden);
        t.toggle(&mut ev);
        assert_eq!(t.buffer(), "");
    }

    #[test]
    fn next_deadline_hits_second_boundary() {
        let (mut t, mut ev) = setup();
        t.digit(5, &mut ev);
        t.activate(0, &mut ev);
        assert_eq!(t.next_deadline(0), Some(1000));
        assert_eq!(t.next_deadline(300), Some(1000));
    }

    #[test]
    fn tension_ramp() {
        assert_eq!(tension(Phase::Running, 11), 0.0);
        assert_eq!(tension(Phase::Running, 10), 0.0);
        assert_eq!(tension(Phase::Running, 1), 1.0);
        assert!((tension(Phase::Running, 5) - 5.0 / 9.0).abs() < 1e-6);
        assert_eq!(tension(Phase::Done, 0), 1.0);
        assert_eq!(tension(Phase::Setup, 3), 0.0);
    }
}
