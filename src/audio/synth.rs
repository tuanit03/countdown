//! Sample-by-sample port of the prototype's Web Audio graphs.

use std::collections::VecDeque;
use std::f32::consts::TAU;

pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }
    pub fn next_f32(&mut self) -> f32 {
        // xorshift32 → [0, 1)
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn white(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

#[derive(Clone, Copy)]
enum NoiseKind {
    White,
    Pink,
    Brown,
}

/// Same generators the prototype used to fill its noise buffers.
struct Noise {
    kind: NoiseKind,
    b: [f32; 7],
}

impl Noise {
    fn new(kind: NoiseKind) -> Self {
        Noise { kind, b: [0.0; 7] }
    }
    fn next(&mut self, rng: &mut Rng) -> f32 {
        let w = rng.white();
        let b = &mut self.b;
        match self.kind {
            NoiseKind::White => w * 0.5,
            NoiseKind::Pink => {
                b[0] = 0.99886 * b[0] + w * 0.0555179;
                b[1] = 0.99332 * b[1] + w * 0.0750759;
                b[2] = 0.969 * b[2] + w * 0.153852;
                b[3] = 0.8665 * b[3] + w * 0.3104856;
                b[4] = 0.55 * b[4] + w * 0.5329522;
                b[5] = -0.7616 * b[5] - w * 0.016898;
                let out = (b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362) * 0.11;
                b[6] = w * 0.115926;
                out
            }
            NoiseKind::Brown => {
                b[0] = (b[0] + 0.02 * w) / 1.02;
                b[0] * 3.5
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum FilterKind {
    Lowpass,
    Highpass,
    Bandpass,
}

/// RBJ biquad. Lowpass/highpass Q is in dB like Web Audio (default 1 dB); bandpass Q is linear.
pub struct Biquad {
    kind: FilterKind,
    q: f32,
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    pub fn new(kind: FilterKind, freq: f32, q: f32, sr: f32) -> Self {
        let mut f = Biquad { kind, q, b: [0.0; 3], a: [0.0; 2], x: [0.0; 2], y: [0.0; 2] };
        f.set_freq(freq, sr);
        f
    }
    pub fn set_freq(&mut self, freq: f32, sr: f32) {
        let w0 = TAU * freq.clamp(10.0, sr * 0.49) / sr;
        let (s, c) = w0.sin_cos();
        let q = match self.kind {
            FilterKind::Bandpass => self.q.max(1e-3),
            _ => 10f32.powf(self.q / 20.0),
        };
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b = match self.kind {
            FilterKind::Lowpass => [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
            FilterKind::Highpass => [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0],
            FilterKind::Bandpass => [alpha, 0.0, -alpha],
        };
        self.b = [b[0] / a0, b[1] / a0, b[2] / a0];
        self.a = [-2.0 * c / a0, (1.0 - alpha) / a0];
    }
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

struct Lfo {
    freq: f32,
    depth: f32,
    phase: f32,
}

impl Lfo {
    fn value(&self) -> f32 {
        self.depth * (TAU * self.phase).sin()
    }
    fn advance(&mut self, n: usize, sr: f32) {
        self.phase = (self.phase + self.freq * n as f32 / sr).fract();
    }
}

/// Linear ramp of a parameter, in samples.
#[derive(Clone, Copy)]
pub struct Ramp {
    pub value: f32,
    target: f32,
    step: f32,
    left: u32,
}

impl Ramp {
    pub fn new(v: f32) -> Self {
        Ramp { value: v, target: v, step: 0.0, left: 0 }
    }
    pub fn to(&mut self, target: f32, samples: u32) {
        self.target = target;
        if samples == 0 {
            self.value = target;
            self.left = 0;
        } else {
            self.step = (target - self.value) / samples as f32;
            self.left = samples;
        }
    }
    pub fn tick(&mut self) -> f32 {
        if self.left > 0 {
            self.left -= 1;
            // Land exactly on the target so "silent" checks compare against 0.0.
            self.value = if self.left == 0 { self.target } else { self.value + self.step };
        }
        self.value
    }
    pub fn busy(&self) -> bool {
        self.left > 0
    }
}

struct Pop {
    age: u32,
    attack: u32,
    decay: u32,
    peak: f32,
}

const CONTROL_EVERY: usize = 32;

/// One ambient sound graph.
struct Voice {
    sound: usize,
    noise: Noise,
    filters: Vec<Biquad>,
    base_freq: f32,
    base_gain: f32,
    gain_lfo: Option<Lfo>,
    freq_lfo: Option<Lfo>,
    fade: Ramp,
    crackle: Option<(Biquad, Noise, Vec<Pop>, u32)>,
    ctl: usize,
    gain_now: f32,
}

impl Voice {
    fn new(sound: usize, sr: f32) -> Voice {
        use FilterKind::*;
        let lfo = |freq, depth| Some(Lfo { freq, depth, phase: 0.0 });
        let (kind, filters, base_freq, base_gain, gain_lfo, freq_lfo) = match sound {
            // rain: pink → highpass 500 → lowpass 6000, gain 0.9 ± 0.08 @ 0.17 Hz
            0 => (NoiseKind::Pink, vec![Biquad::new(Highpass, 500.0, 1.0, sr), Biquad::new(Lowpass, 6000.0, 1.0, sr)], 6000.0, 0.9, lfo(0.17, 0.08), None),
            // ocean: brown → lowpass 700 ± 400, gain 0.55 ± 0.45, both @ 0.08 Hz (~12 s swell)
            1 => (NoiseKind::Brown, vec![Biquad::new(Lowpass, 700.0, 1.0, sr)], 700.0, 0.55, lfo(0.08, 0.45), lfo(0.08, 400.0)),
            // brown noise: brown → lowpass 1000
            2 => (NoiseKind::Brown, vec![Biquad::new(Lowpass, 1000.0, 1.0, sr)], 1000.0, 0.8, None, None),
            // forest wind: pink → bandpass 500 ± 300 @ 0.05 Hz (Q 0.7), gain 1.6 ± 0.5 @ 0.13 Hz
            3 => (NoiseKind::Pink, vec![Biquad::new(Bandpass, 500.0, 0.7, sr)], 500.0, 1.6, lfo(0.13, 0.5), lfo(0.05, 300.0)),
            // campfire: brown → lowpass 450, plus crackles
            _ => (NoiseKind::Brown, vec![Biquad::new(Lowpass, 450.0, 1.0, sr)], 450.0, 0.7, None, None),
        };
        let crackle = (sound == 4).then(|| (Biquad::new(Highpass, 1800.0, 1.0, sr), Noise::new(NoiseKind::White), Vec::new(), 0));
        Voice { sound, noise: Noise::new(kind), filters, base_freq, base_gain, gain_lfo, freq_lfo, fade: Ramp::new(1.0), crackle, ctl: 0, gain_now: base_gain }
    }

    fn control(&mut self, sr: f32) {
        if let Some(l) = &mut self.freq_lfo {
            let f = self.base_freq + l.value();
            // Modulated filter is the last one in the chain.
            if let Some(last) = self.filters.last_mut() {
                last.set_freq(f, sr);
            }
            l.advance(CONTROL_EVERY, sr);
        }
        self.gain_now = self.base_gain + self.gain_lfo.as_ref().map_or(0.0, |l| l.value());
        if let Some(l) = &mut self.gain_lfo {
            l.advance(CONTROL_EVERY, sr);
        }
    }

    fn next(&mut self, rng: &mut Rng, sr: f32) -> f32 {
        if self.ctl == 0 {
            self.control(sr);
        }
        self.ctl = (self.ctl + 1) % CONTROL_EVERY;
        let mut x = self.noise.next(rng);
        for f in &mut self.filters {
            x = f.process(x);
        }
        x *= self.gain_now;
        if let Some((hp, white, pops, until_next)) = &mut self.crackle {
            // Every 110 ms, 40% chance of a short white-noise pop through a 1.8 kHz highpass.
            if *until_next == 0 {
                *until_next = (0.110 * sr) as u32;
                if rng.next_f32() < 0.4 {
                    pops.push(Pop {
                        age: 0,
                        attack: (0.004 * sr) as u32,
                        decay: ((0.03 + rng.next_f32() * 0.05) * sr) as u32,
                        peak: 0.15 + rng.next_f32() * 0.35,
                    });
                }
            }
            *until_next -= 1;
            let mut c = 0.0;
            let mut active = false;
            for p in pops.iter_mut() {
                let g = if p.age < p.attack {
                    p.peak * p.age as f32 / p.attack as f32
                } else {
                    // Exponential decay from peak to 0.001 over `decay` samples.
                    p.peak * (0.001 / p.peak).powf((p.age - p.attack) as f32 / p.decay as f32)
                };
                p.age += 1;
                if p.age < p.attack + p.decay {
                    active = true;
                }
                c += g;
            }
            pops.retain(|p| p.age < p.attack + p.decay);
            let n = white.next(rng);
            x += hp.process(if active { n * c } else { 0.0 });
        }
        x * self.fade.tick()
    }
}

/// Inharmonic modes of a small alarm bell: (frequency ratio, weight, decay seconds).
const BELL_MODES: [(f32, f32, f32); 4] = [(1.0, 1.0, 0.3), (2.76, 0.55, 0.2), (5.40, 0.3, 0.12), (8.93, 0.15, 0.07)];
/// The two bells of a twin-bell clock, struck alternately.
const BELLS_HZ: [f32; 2] = [1180.0, 1390.0];
const STRIKE_HZ: f32 = 24.0;
pub const RING_BURSTS: u32 = 1;
const RING_ON_S: f32 = 0.9;
const RING_OFF_S: f32 = 0.35;
const RING_GAIN: f32 = 0.22;

struct BellMode {
    bell: usize,
    freq: f32,
    weight: f32,
    decay: f32,
    phase: f32,
    amp: f32,
}

/// Mechanical alarm clock: a hammer rattles between two bells for one burst.
struct Alarm {
    age: u32,
    next_strike: u32,
    strikes: u32,
    modes: Vec<BellMode>,
}

impl Alarm {
    fn new(sr: f32) -> Alarm {
        let modes = BELLS_HZ
            .iter()
            .enumerate()
            .flat_map(|(bell, &f0)| {
                BELL_MODES.iter().map(move |&(ratio, weight, secs)| BellMode {
                    bell,
                    freq: f0 * ratio,
                    weight,
                    decay: (-1.0 / (secs * sr)).exp(),
                    phase: 0.0,
                    amp: 0.0,
                })
            })
            .collect();
        Alarm { age: 0, next_strike: 0, strikes: 0, modes }
    }

    fn next(&mut self, sr: f32) -> f32 {
        let on = (RING_ON_S * sr) as u32;
        let period = on + (RING_OFF_S * sr) as u32;
        if self.age < RING_BURSTS * period && self.age % period < on && self.age >= self.next_strike {
            self.next_strike = self.age + (sr / STRIKE_HZ) as u32;
            let bell = (self.strikes % 2) as usize;
            self.strikes += 1;
            for m in self.modes.iter_mut().filter(|m| m.bell == bell) {
                m.amp = (m.amp + 0.6 * m.weight).min(m.weight);
            }
        }
        self.age += 1;
        let mut x = 0.0;
        for m in &mut self.modes {
            x += m.amp * (TAU * m.phase).sin();
            m.phase = (m.phase + m.freq / sr).fract();
            m.amp *= m.decay;
        }
        x * RING_GAIN
    }

    fn finished(&self, sr: f32) -> bool {
        let period = ((RING_ON_S + RING_OFF_S) * sr) as u32;
        self.age >= RING_BURSTS * period && self.modes.iter().all(|m| m.amp < 1e-3)
    }
}

pub const CROSSFADE_S: f32 = 0.4;

pub struct Engine {
    pub sr: f32,
    master: Ramp,
    queue: VecDeque<(f32, f32)>,
    voices: Vec<Voice>,
    alarm: Option<Alarm>,
    rng: Rng,
}

impl Engine {
    pub fn new(sr: f32) -> Engine {
        Engine { sr, master: Ramp::new(0.0), queue: VecDeque::new(), voices: Vec::new(), alarm: None, rng: Rng::new(0x9e37_79b9) }
    }

    fn samples(&self, secs: f32) -> u32 {
        (secs.max(0.0) * self.sr) as u32
    }

    /// Switch to `sound`, crossfading from whatever is playing.
    pub fn play(&mut self, sound: usize) {
        if self.voices.last().is_some_and(|v| v.sound == sound && v.fade.value > 0.0 && !v.fade.busy()) {
            return;
        }
        let n = self.samples(CROSSFADE_S);
        for v in &mut self.voices {
            v.fade.to(0.0, n);
        }
        let mut v = Voice::new(sound, self.sr);
        if self.voices.is_empty() || self.master.value == 0.0 {
            self.voices.clear();
        } else {
            v.fade = Ramp::new(0.0);
            v.fade.to(1.0, n);
        }
        self.voices.push(v);
    }

    /// Replace pending master-gain ramps with `steps` of (target, seconds), run in order.
    pub fn envelope(&mut self, steps: &[(f32, f32)]) {
        self.queue = steps.iter().copied().collect();
        self.master.to(self.master.value, 0);
        self.advance_queue();
    }

    fn advance_queue(&mut self) {
        if let Some((target, secs)) = self.queue.pop_front() {
            let n = self.samples(secs);
            self.master.to(target, n);
        }
    }

    /// Start the end-of-timer alarm bell, bypassing the master gain.
    pub fn ring(&mut self) {
        self.alarm = Some(Alarm::new(self.sr));
    }

    pub fn stop_ring(&mut self) {
        self.alarm = None;
    }

    /// Nothing audible now or scheduled.
    pub fn is_idle(&self) -> bool {
        self.master.value <= 0.0 && !self.master.busy() && self.queue.is_empty() && self.alarm.is_none()
    }

    /// Drop the ambient graph once silent so a later play() starts fresh.
    pub fn release_if_idle(&mut self) {
        if self.is_idle() {
            self.voices.clear();
        }
    }

    pub fn render(&mut self, out: &mut [f32]) {
        let sr = self.sr;
        for s in out.iter_mut() {
            let m = self.master.tick();
            if !self.master.busy() && !self.queue.is_empty() {
                self.advance_queue();
            }
            let mut x = 0.0;
            if m > 0.0 || self.master.busy() {
                for v in &mut self.voices {
                    x += v.next(&mut self.rng, sr);
                }
                x *= m;
            }
            if let Some(alarm) = &mut self.alarm {
                x += alarm.next(sr);
                if alarm.finished(sr) {
                    self.alarm = None;
                }
            }
            self.voices.retain(|v| v.fade.value > 0.0 || v.fade.busy());
            *s = x.clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(e: &mut Engine, secs: f32) -> Vec<f32> {
        let mut buf = vec![0.0; (secs * e.sr) as usize];
        e.render(&mut buf);
        buf
    }

    #[test]
    fn all_sounds_bounded_and_audible() {
        for sound in 0..5 {
            let mut e = Engine::new(44100.0);
            e.play(sound);
            e.envelope(&[(0.35, 0.1)]);
            let buf = run(&mut e, 3.0);
            assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "sound {sound}");
            let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
            assert!(rms > 0.003, "sound {sound} too quiet: {rms}");
        }
    }

    #[test]
    fn envelope_runs_in_order_and_goes_idle() {
        let mut e = Engine::new(1000.0);
        e.play(2);
        e.envelope(&[(0.5, 0.1), (0.5, 0.1), (0.0, 0.1)]);
        assert!(!e.is_idle());
        run(&mut e, 0.1);
        assert!((e.master.value - 0.5).abs() < 1e-3);
        run(&mut e, 0.25);
        assert!(e.is_idle());
        e.release_if_idle();
        assert!(e.voices.is_empty());
    }

    #[test]
    fn alarm_rings_once_then_ends() {
        let mut e = Engine::new(8000.0);
        e.ring();
        let peak = |b: &[f32]| b.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        let burst = run(&mut e, 0.5);
        assert!(peak(&burst) > 0.1, "rings during the burst");
        run(&mut e, 0.75);
        let after = run(&mut e, 0.5); // no second burst
        assert!(peak(&after) < peak(&burst) * 0.5);
        run(&mut e, 2.0);
        assert!(e.is_idle());
    }

    #[test]
    fn alarm_can_be_stopped() {
        let mut e = Engine::new(8000.0);
        e.ring();
        run(&mut e, 0.2);
        e.stop_ring();
        assert!(e.is_idle());
    }

    #[test]
    fn switching_crossfades() {
        let mut e = Engine::new(1000.0);
        e.play(0);
        e.envelope(&[(1.0, 0.0)]);
        run(&mut e, 0.1);
        e.play(3);
        assert_eq!(e.voices.len(), 2);
        run(&mut e, 0.5);
        assert_eq!(e.voices.len(), 1);
        assert_eq!(e.voices[0].sound, 3);
    }
}
