//! Time-based animation parameters, ported from the prototype's CSS keyframes.
//! Lengths are in em (1em = digit font size); angles in degrees.

/// CSS `cubic-bezier(x1, y1, x2, y2)` evaluated at progress `t`.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let bez = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    // Solve x(s) = t by bisection; x is monotonic for x1, x2 in [0, 1].
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if bez(x1, x2, mid) < t { lo = mid } else { hi = mid }
    }
    bez(y1, y2, 0.5 * (lo + hi))
}

pub type Ease = [f32; 4];
pub const EASE: Ease = [0.25, 0.1, 0.25, 1.0];
pub const FALL: Ease = [0.5, 0.0, 1.0, 0.5];
pub const DROP: Ease = [0.3, 0.7, 0.3, 1.0];
pub const MELT: Ease = [0.5, 0.0, 0.8, 0.4];
pub const POP: Ease = [0.3, 0.8, 0.3, 1.3];
pub const RELEASE: Ease = [0.2, 0.9, 0.3, 1.3];
pub const TENSION: Ease = [0.3, 0.7, 0.3, 1.0];

/// Interpolate keyframes `(offset, value)` with the timing function applied per segment, like CSS.
pub fn keyframes<const N: usize>(frames: &[(f32, [f32; N])], ease: Ease, p: f32) -> [f32; N] {
    let p = p.clamp(0.0, 1.0);
    for w in frames.windows(2) {
        let ((o0, v0), (o1, v1)) = (w[0], w[1]);
        if p <= o1 {
            let local = if o1 > o0 { (p - o0) / (o1 - o0) } else { 1.0 };
            let e = cubic_bezier(ease[0], ease[1], ease[2], ease[3], local);
            let mut out = [0.0; N];
            for i in 0..N {
                out[i] = v0[i] + (v1[i] - v0[i]) * e;
            }
            return out;
        }
    }
    frames[frames.len() - 1].1
}

pub fn progress(now: u64, start: u64, delay: u64, dur: u64) -> f32 {
    if now < start + delay { 0.0 } else { ((now - start - delay) as f32 / dur as f32).min(1.0) }
}

pub const SHATTER_MS: u64 = 1300;
pub const MELT_MS: u64 = 1100;
pub const DROP_MS: u64 = 620;
pub const DROP_DELAY_MS: u64 = 80;
pub const POP_MS: u64 = 260;
pub const RELEASE_MS: u64 = 700;
pub const FADE_MS: u64 = 300;
pub const TENSION_MS: u64 = 350;

/// Shard clip polygons as fractions of the 1em × 1em digit cell.
pub const SHARDS: [&[(f32, f32)]; 5] = [
    &[(0.0, 0.0), (0.55, 0.0), (0.40, 0.45), (0.0, 0.35)],
    &[(0.55, 0.0), (1.0, 0.0), (1.0, 0.40), (0.40, 0.45)],
    &[(0.0, 0.35), (0.40, 0.45), (0.30, 1.0), (0.0, 1.0)],
    &[(0.40, 0.45), (1.0, 0.40), (1.0, 1.0), (0.65, 1.0)],
    &[(0.40, 0.45), (0.65, 1.0), (0.30, 1.0)],
];

/// (12% nudge, 100% landing) as [dx, dy, rot] per shard.
const SHARD_PATH: [([f32; 3], [f32; 3]); 5] = [
    ([-0.02, -0.03, -4.0], [-0.40, 3.2, -75.0]),
    ([0.03, -0.04, 5.0], [0.35, 3.0, 62.0]),
    ([-0.03, -0.01, -3.0], [-0.70, 2.8, -40.0]),
    ([0.03, -0.02, 6.0], [0.20, 3.4, 90.0]),
    ([0.0, -0.03, -2.0], [-0.10, 3.6, 25.0]),
];

pub struct Pose {
    pub dx: f32,
    pub dy: f32,
    pub rot: f32,
    pub sx: f32,
    pub sy: f32,
    pub alpha: f32,
    pub blur: f32,
}

impl Pose {
    pub const REST: Pose = Pose { dx: 0.0, dy: 0.0, rot: 0.0, sx: 1.0, sy: 1.0, alpha: 1.0, blur: 0.0 };
}

pub fn shard(i: usize, p: f32) -> Pose {
    let (a, b) = SHARD_PATH[i];
    let [dx, dy, rot] = keyframes(&[(0.0, [0.0; 3]), (0.12, a), (1.0, b)], FALL, p);
    let [alpha] = keyframes(&[(0.0, [1.0]), (0.85, [1.0]), (1.0, [0.0])], FALL, p);
    Pose { dx, dy, rot, alpha, ..Pose::REST }
}

/// Melt pose; scale origin is 50% / 25% of the cell. `blur` in px.
pub fn melt(p: f32) -> Pose {
    let [dy, sy, sx, blur] = keyframes(
        &[(0.0, [0.0, 1.0, 1.0, 0.0]), (0.4, [0.15, 1.3, 0.96, 3.0]), (1.0, [1.6, 2.6, 0.7, 8.0])],
        MELT,
        p,
    );
    let [alpha] = keyframes(&[(0.0, [1.0]), (1.0, [0.0])], MELT, p);
    Pose { dy, sx, sy, blur, alpha, ..Pose::REST }
}

/// New digit dropping in; dy as a fraction of the cell height (= em).
pub fn drop(p: f32) -> Pose {
    let [dy, sy, sx] = keyframes(
        &[
            (0.0, [-1.3, 1.2, 1.0]),
            (0.55, [0.06, 0.9, 1.06]),
            (0.75, [-0.02, 1.03, 1.0]),
            (1.0, [0.0, 1.0, 1.0]),
        ],
        DROP,
        p,
    );
    Pose { dy, sy, sx, ..Pose::REST }
}

/// A freshly typed setup digit popping in.
pub fn pop(p: f32) -> Pose {
    let [dy, sy, alpha] = keyframes(&[(0.0, [-0.25, 1.15, 0.0]), (1.0, [0.0, 1.0, 1.0])], POP, p);
    Pose { dy, sy, alpha, ..Pose::REST }
}

/// Scale bounce when the clock reaches zero.
pub fn release(p: f32) -> f32 {
    keyframes(&[(0.0, [1.2]), (0.6, [0.97]), (1.0, [1.0])], RELEASE, p)[0]
}

/// Shake offset in px (dx, dy, rot°) for tension level 1..=3 (`steps(2)` keyframes).
pub fn shake(level: u8, now: u64) -> (f32, f32, f32) {
    let (period, a, b): (u64, (f32, f32, f32), (f32, f32, f32)) = match level {
        1 => (140, (1.0, 0.0, 0.0), (-1.0, 1.0, 0.0)),
        2 => (110, (2.0, -1.0, 0.0), (-2.0, 1.0, 0.0)),
        3 => (80, (4.0, -1.0, 0.6), (-4.0, 2.0, -0.6)),
        _ => return (0.0, 0.0, 0.0),
    };
    if now % period < period / 2 { a } else { b }
}

pub fn shake_level(t: f32) -> u8 {
    if t <= 0.0 { 0 } else if t < 0.34 { 1 } else if t < 0.67 { 2 } else { 3 }
}

/// Font axes for tension `t`: (wght, wdth, skew°).
pub fn tension_axes(t: f32) -> (f32, f32, f32) {
    (560.0 + 340.0 * t, 74.0 - 12.0 * t, -12.0 * t)
}

/// A value that eases toward a target over `dur` ms (CSS `transition`).
#[derive(Clone, Copy)]
pub struct Tween {
    from: f32,
    to: f32,
    start: u64,
    dur: u64,
    ease: Ease,
}

impl Tween {
    pub fn new(v: f32, dur: u64, ease: Ease) -> Self {
        Tween { from: v, to: v, start: 0, dur, ease }
    }
    pub fn set(&mut self, target: f32, now: u64) {
        if (target - self.to).abs() > f32::EPSILON {
            self.from = self.get(now);
            self.to = target;
            self.start = now;
        }
    }
    pub fn snap(&mut self, v: f32) {
        self.from = v;
        self.to = v;
    }
    pub fn get(&self, now: u64) -> f32 {
        let p = progress(now, self.start, 0, self.dur);
        let e = cubic_bezier(self.ease[0], self.ease[1], self.ease[2], self.ease[3], p);
        self.from + (self.to - self.from) * e
    }
    pub fn target(&self) -> f32 {
        self.to
    }
    pub fn active(&self, now: u64) -> bool {
        now < self.start + self.dur && (self.to - self.from).abs() > f32::EPSILON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_endpoints_and_linear() {
        assert!(cubic_bezier(0.5, 0.0, 1.0, 0.5, 0.0).abs() < 1e-4);
        assert!((cubic_bezier(0.5, 0.0, 1.0, 0.5, 1.0) - 1.0).abs() < 1e-4);
        let lin = cubic_bezier(1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 0.3);
        assert!((lin - 0.3).abs() < 1e-3);
    }

    #[test]
    fn shard_lands_and_fades() {
        let s = shard(3, 1.0);
        assert!((s.dy - 3.4).abs() < 1e-4 && (s.rot - 90.0).abs() < 1e-3 && s.alpha.abs() < 1e-4);
        let mid = shard(3, 0.5);
        assert_eq!(mid.alpha, 1.0);
        assert!(mid.dy > 0.0 && mid.dy < 3.4);
    }

    #[test]
    fn drop_starts_above_and_rests() {
        assert!((drop(0.0).dy + 1.3).abs() < 1e-4);
        let end = drop(1.0);
        assert!(end.dy.abs() < 1e-4 && (end.sy - 1.0).abs() < 1e-4);
    }

    #[test]
    fn tension_axes_range() {
        assert_eq!(tension_axes(0.0), (560.0, 74.0, 0.0));
        assert_eq!(tension_axes(1.0), (900.0, 62.0, -12.0));
        assert_eq!(shake_level(0.2), 1);
        assert_eq!(shake_level(0.5), 2);
        assert_eq!(shake_level(1.0), 3);
    }

    #[test]
    fn tween_moves_to_target() {
        let mut t = Tween::new(1.0, 300, EASE);
        t.set(0.38, 1000);
        assert!(t.active(1100));
        assert!((t.get(1000) - 1.0).abs() < 1e-4);
        assert!((t.get(1300) - 0.38).abs() < 1e-4);
        assert!(!t.active(1300));
    }
}
