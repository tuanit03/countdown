//! Audio thread: renders the synth into a PulseAudio stream, closing it whenever silent.

pub mod synth;

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// (id, on-screen name) in cycling order; index 0 is the default.
pub const SOUNDS: [(&str, &str); 5] =
    [("rain", "MƯA"), ("ocean", "SÓNG BIỂN"), ("brown", "TIẾNG ỒN NÂU"), ("wind", "GIÓ RỪNG"), ("fire", "LỬA TRẠI")];

const RATE: u32 = 44100;
const BLOCK: usize = 512;

pub enum Cmd {
    Play(usize),
    /// Master gain steps of (target, seconds).
    Envelope(Vec<(f32, f32)>),
    /// End-of-timer alarm bell.
    Ring,
    StopRing,
}

#[derive(Clone)]
pub struct Audio {
    tx: Sender<Cmd>,
}

impl Audio {
    pub fn spawn() -> Audio {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("audio".into())
            .stack_size(256 * 1024)
            .spawn(move || run(rx))
            .expect("spawn audio thread");
        Audio { tx }
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    pub fn fade(&self, target: f32, secs: f32) {
        self.send(Cmd::Envelope(vec![(target, secs)]));
    }

    /// Start (or switch to) `sound` and fade up to `gain`.
    pub fn start(&self, sound: usize, gain: f32, secs: f32) {
        self.send(Cmd::Play(sound));
        self.fade(gain, secs);
    }

    /// A short, quiet audition while choosing a sound.
    pub fn preview(&self, sound: usize, gain: f32) {
        self.send(Cmd::Play(sound));
        self.send(Cmd::Envelope(vec![(gain, 0.3), (gain, 2.0), (0.0, 0.8)]));
    }
}

fn apply(e: &mut synth::Engine, c: Cmd) {
    match c {
        Cmd::Play(s) => e.play(s),
        Cmd::Envelope(steps) => e.envelope(&steps),
        Cmd::Ring => e.ring(),
        Cmd::StopRing => e.stop_ring(),
    }
}

fn open() -> Option<libpulse_simple_binding::Simple> {
    use libpulse_binding::{def::BufferAttr, sample, stream::Direction};
    let spec = sample::Spec { format: sample::Format::F32le, channels: 1, rate: RATE };
    let bytes = |secs: f32| (secs * RATE as f32) as u32 * 4;
    // ~80 ms target latency keeps fades responsive without waking too often.
    let attr = BufferAttr { maxlength: u32::MAX, tlength: bytes(0.08), prebuf: u32::MAX, minreq: bytes(0.02), fragsize: u32::MAX };
    match libpulse_simple_binding::Simple::new(None, "Countdown", Direction::Playback, None, "focus sound", &spec, None, Some(&attr)) {
        Ok(s) => Some(s),
        Err(err) => {
            eprintln!("countdown: audio unavailable: {err}");
            None
        }
    }
}

fn run(rx: Receiver<Cmd>) {
    let mut engine = synth::Engine::new(RATE as f32);
    let mut buf = vec![0.0f32; BLOCK];
    let mut bytes = vec![0u8; BLOCK * 4];
    loop {
        // Idle: no stream, block until told to make sound.
        match rx.recv() {
            Ok(c) => apply(&mut engine, c),
            Err(_) => return,
        }
        while let Ok(c) = rx.try_recv() {
            apply(&mut engine, c);
        }
        if engine.is_idle() {
            engine.release_if_idle();
            continue;
        }
        let Some(stream) = open() else {
            // Keep state consistent without output; avoid spinning.
            std::thread::sleep(Duration::from_millis(200));
            engine.envelope(&[(0.0, 0.0)]);
            engine.release_if_idle();
            continue;
        };
        let mut silent_blocks = 0;
        loop {
            loop {
                match rx.try_recv() {
                    Ok(c) => {
                        apply(&mut engine, c);
                        silent_blocks = 0;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            engine.render(&mut buf);
            for (i, s) in buf.iter().enumerate() {
                bytes[i * 4..i * 4 + 4].copy_from_slice(&s.to_le_bytes());
            }
            if stream.write(&bytes).is_err() {
                break;
            }
            if engine.is_idle() {
                silent_blocks += 1;
                // ~0.3 s of trailing silence, then release the device.
                if silent_blocks > (0.3 * RATE as f32) as usize / BLOCK {
                    let _ = stream.drain();
                    engine.release_if_idle();
                    break;
                }
            }
        }
    }
}
