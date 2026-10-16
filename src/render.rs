//! Draws the digits with cairo + pango (Archivo variable font), mirroring the prototype's CSS.
//!
//! Every glyph is rasterized once (with its blurred shadow) into a sprite that lives on the X server;
//! frames are then just XRender composites with transforms, which keeps per-frame CPU near zero.

use crate::anim::{self, Pose};
use crate::config::Transition;
use cairo::Context;
use std::collections::HashMap;

pub const EM: f64 = 120.0;
pub const TOP: f64 = 28.0;
/// Width of the colon block: .07em dot + .05em margin either side.
const DOTS_W: f64 = 0.17 * EM;
const STROKE: f64 = 1.5;
/// Sprite margin around the 1em line box, room for the shadow blur.
const PAD: f64 = 30.0;
/// Box-blur radius; three passes ≈ Gaussian σ 9 px (CSS blur 18 px).
const BLUR_R: usize = 8;
const MAX_SPRITES: usize = 40;

pub type Rgb = [f64; 3];
pub const INK_DARK_BG: Rgb = [241.0 / 255.0, 237.0 / 255.0, 228.0 / 255.0];
pub const INK_LIGHT_BG: Rgb = [12.0 / 255.0, 12.0 / 255.0, 11.0 / 255.0];
pub const ACCENT: Rgb = [1.0, 77.0 / 255.0, 31.0 / 255.0];

pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn rgba8(c: Rgb, a: f64) -> [u8; 4] {
    [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8, (a * 255.0).round() as u8]
}

fn rgba_f(c: [u8; 4]) -> (f64, f64, f64, f64) {
    (c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0, c[3] as f64 / 255.0)
}

pub struct Leaving {
    pub text: String,
    pub start: u64,
}

pub enum Mode {
    Setup {
        digits: [u8; 4],
        typed: usize,
        /// Start of the pop animation of the newest digit.
        pop_start: u64,
        caret_on: bool,
    },
    Timing {
        mm: String,
        ss: String,
        /// When the current seconds started dropping in; `None` = already in place.
        enter_start: Option<u64>,
        leaving: Vec<Leaving>,
        transition: Transition,
        tension: f32,
        done: bool,
        done_start: u64,
        dots_on: bool,
        shake: (f32, f32, f32),
    },
}

pub struct Scene<'a> {
    pub now: u64,
    pub light_bg: bool,
    pub opacity: f32,
    pub mode: &'a Mode,
    /// (text, 0..1 visibility)
    pub toast: Option<(&'a str, f32)>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum Style {
    /// Filled glyph with a drop shadow.
    Solid { fill: [u8; 4], shadow: [u8; 4] },
    /// Hollow glyph: stroke, optionally filled.
    Outline { stroke: [u8; 4], fill: Option<[u8; 4]> },
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    text: String,
    wght: u16,
    wdth: u16,
    style: Style,
}

struct Sprite {
    surf: cairo::Surface,
    w: f64,
    h: f64,
    /// Logical (advance) width of the text.
    width: f64,
    used: u64,
}

struct Toast {
    text: String,
    light: bool,
    surf: cairo::Surface,
    w: f64,
    h: f64,
}

pub struct Renderer {
    sprites: HashMap<Key, Sprite>,
    toast: Option<Toast>,
    frame: u64,
}

/// Pango layout for the digits at the given variable-font axes.
fn digit_layout(cr: &Context, text: &str, wght: u16, wdth: u16) -> pango::Layout {
    let layout = pangocairo::functions::create_layout(cr);
    let mut fd = pango::FontDescription::from_string("Archivo");
    fd.set_absolute_size(EM * pango::SCALE as f64);
    fd.set_variations(Some(&format!("wght={wght},wdth={wdth}")));
    layout.set_font_description(Some(&fd));
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrInt::new_letter_spacing((-0.02 * EM * pango::SCALE as f64) as i32));
    attrs.insert(pango::AttrFontFeatures::new("tnum 1"));
    layout.set_attributes(Some(&attrs));
    layout.set_text(text);
    layout
}

/// Separable box blur of an 8-bit buffer (zero outside the edges).
pub fn box_blur(data: &mut [u8], w: usize, h: usize, stride: usize, r: usize) {
    let mut line = Vec::new();
    let mut prefix = Vec::new();
    let mut blur = |line: &mut Vec<u8>| {
        let n = line.len();
        prefix.clear();
        prefix.push(0u32);
        for &v in line.iter() {
            prefix.push(prefix.last().unwrap() + v as u32);
        }
        let div = (2 * r + 1) as u32;
        for i in 0..n {
            let a = i.saturating_sub(r);
            let b = (i + r + 1).min(n);
            line[i] = ((prefix[b] - prefix[a]) / div) as u8;
        }
    };
    for y in 0..h {
        line.clear();
        line.extend_from_slice(&data[y * stride..y * stride + w]);
        blur(&mut line);
        data[y * stride..y * stride + w].copy_from_slice(&line);
    }
    for x in 0..w {
        line.clear();
        line.extend((0..h).map(|y| data[y * stride + x]));
        blur(&mut line);
        for (y, v) in line.iter().enumerate() {
            data[y * stride + x] = *v;
        }
    }
}

fn rasterize(target: &cairo::Surface, key: &Key) -> Result<Sprite, cairo::Error> {
    // Measure and outline the text.
    let probe = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)?;
    let pcr = Context::new(&probe)?;
    let layout = digit_layout(&pcr, &key.text, key.wght, key.wdth);
    let (_, logical) = layout.pixel_extents();
    let width = logical.width() as f64;
    // CSS line-height: 1 centres the font's line box inside the 1em box.
    let gy = (EM - logical.height() as f64) / 2.0;
    pangocairo::functions::layout_path(&pcr, &layout);
    let path = pcr.copy_path()?;

    let iw = (width + 2.0 * PAD).ceil() as i32;
    let ih = (EM + 2.0 * PAD).ceil() as i32;
    let img = cairo::ImageSurface::create(cairo::Format::ARgb32, iw, ih)?;
    match &key.style {
        Style::Solid { fill, shadow } => {
            // Shadow: offset 3 px, blurred alpha mask (CSS `text-shadow: 0 3px 18px`).
            let mut mask = cairo::ImageSurface::create(cairo::Format::A8, iw, ih)?;
            {
                let m = Context::new(&mask)?;
                m.translate(PAD, PAD + gy + 3.0);
                m.append_path(&path);
                m.set_source_rgba(0.0, 0.0, 0.0, 1.0);
                m.fill()?;
            }
            mask.flush();
            let stride = mask.stride() as usize;
            {
                let mut data = mask.data().map_err(|_| cairo::Error::SurfaceFinished)?;
                for _ in 0..3 {
                    box_blur(&mut data, iw as usize, ih as usize, stride, BLUR_R);
                }
            }
            mask.mark_dirty();
            let cr = Context::new(&img)?;
            let (r, g, b, a) = rgba_f(*shadow);
            cr.set_source_rgba(r, g, b, a);
            cr.mask_surface(&mask, 0.0, 0.0)?;
            cr.translate(PAD, PAD + gy);
            cr.append_path(&path);
            let (r, g, b, a) = rgba_f(*fill);
            cr.set_source_rgba(r, g, b, a);
            cr.fill()?;
        }
        Style::Outline { stroke, fill } => {
            let cr = Context::new(&img)?;
            cr.translate(PAD, PAD + gy);
            cr.append_path(&path);
            if let Some(f) = fill {
                let (r, g, b, a) = rgba_f(*f);
                cr.set_source_rgba(r, g, b, a);
                cr.fill_preserve()?;
            }
            let (r, g, b, a) = rgba_f(*stroke);
            cr.set_source_rgba(r, g, b, a);
            cr.set_line_width(STROKE);
            cr.stroke()?;
        }
    }
    // Upload once to a server-side pixmap.
    let surf = target.create_similar(cairo::Content::ColorAlpha, iw, ih)?;
    {
        let cr = Context::new(&surf)?;
        cr.set_source_surface(&img, 0.0, 0.0)?;
        cr.set_operator(cairo::Operator::Source);
        cr.paint()?;
    }
    Ok(Sprite { surf, w: iw as f64, h: ih as f64, width, used: 0 })
}

/// Composite a sprite so that its 1em line box's top-left lands at (x, y) in the current space.
fn blit(cr: &Context, s: &Sprite, x: f64, y: f64, alpha: f64) {
    if alpha <= 0.0 {
        return;
    }
    let _ = cr.save();
    cr.translate(x - PAD, y - PAD);
    cr.rectangle(0.0, 0.0, s.w, s.h);
    cr.clip();
    let _ = cr.set_source_surface(&s.surf, 0.0, 0.0);
    let _ = cr.paint_with_alpha(alpha);
    let _ = cr.restore();
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer { sprites: HashMap::new(), toast: None, frame: 0 }
    }

    /// Fetch the sprite for `text`, rasterizing it on a miss.
    fn sprite(&mut self, target: &cairo::Surface, text: &str, wght: f32, wdth: f32, style: Style) -> Option<&Sprite> {
        let key = Key { text: text.to_string(), wght: ((wght / 10.0).round() * 10.0) as u16, wdth: wdth.round() as u16, style };
        if !self.sprites.contains_key(&key) {
            match rasterize(target, &key) {
                Ok(s) => {
                    self.sprites.insert(key.clone(), s);
                }
                Err(e) => {
                    eprintln!("countdown: rasterize failed: {e}");
                    return None;
                }
            }
        }
        let s = self.sprites.get_mut(&key)?;
        s.used = self.frame;
        Some(s)
    }

    /// Draw the scene; returns the rectangle (x, y, w, h) that should receive pointer input.
    pub fn draw(&mut self, cr: &Context, width: f64, s: &Scene) -> (f64, f64, f64, f64) {
        self.frame += 1;
        let target = cr.target();
        cr.set_operator(cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        let _ = cr.paint();
        cr.set_operator(cairo::Operator::Over);

        let ink = if s.light_bg { INK_LIGHT_BG } else { INK_DARK_BG };
        let shadow = if s.light_bg { rgba8([1.0; 3], 0.45) } else { rgba8([0.0; 3], 0.35) };
        let group = s.opacity < 0.999;
        if group {
            cr.push_group();
        }
        let rect = match s.mode {
            Mode::Setup { digits, typed, pop_start, caret_on } => {
                self.draw_setup(cr, &target, width, s.now, ink, shadow, digits, *typed, *pop_start, *caret_on)
            }
            Mode::Timing { .. } => self.draw_timing(cr, &target, width, s, ink, shadow),
        };
        if group {
            let _ = cr.pop_group_to_source();
            let _ = cr.paint_with_alpha(s.opacity as f64);
        }
        if let Some((text, vis)) = s.toast {
            if vis > 0.0 {
                self.draw_toast(cr, &target, width, text, vis, ink, s.light_bg);
            }
        }
        if self.sprites.len() > MAX_SPRITES {
            let frame = self.frame;
            self.sprites.retain(|_, sp| sp.used == frame);
        }
        rect
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_setup(
        &mut self,
        cr: &Context,
        target: &cairo::Surface,
        width: f64,
        now: u64,
        ink: Rgb,
        shadow: [u8; 4],
        digits: &[u8; 4],
        typed: usize,
        pop_start: u64,
        caret_on: bool,
    ) -> (f64, f64, f64, f64) {
        let slot = 0.5 * EM;
        let total = 4.0 * slot + DOTS_W;
        let left = ((width - total) / 2.0).round();
        let first_typed = 4 - typed;
        let empty_alpha = if ink == INK_LIGHT_BG { 0.5 } else { 0.55 };
        for (i, d) in digits.iter().enumerate() {
            let x = left + slot * i as f64 + if i >= 2 { DOTS_W } else { 0.0 };
            let text = ((b'0' + d) as char).to_string();
            let is_typed = i >= first_typed;
            let style = if is_typed {
                Style::Solid { fill: rgba8(ink, 1.0), shadow }
            } else {
                Style::Outline { stroke: rgba8(ink, empty_alpha), fill: None }
            };
            let Some(sp) = self.sprite(target, &text, 560.0, 74.0, style) else { continue };
            // The newest digit (rightmost) pops in.
            let pose = if is_typed && i == 3 { anim::pop(anim::progress(now, pop_start, 0, anim::POP_MS)) } else { Pose::REST };
            let _ = cr.save();
            cr.translate(x + (slot - sp.width) / 2.0, TOP);
            apply_pose(cr, &pose, sp.width / 2.0, EM / 2.0);
            blit(cr, sp, 0.0, 0.0, pose.alpha as f64);
            let _ = cr.restore();
        }
        draw_dots(cr, left + 2.0 * slot, ink, 1.0);
        if caret_on {
            cr.rectangle(left + total + 0.06 * EM, TOP + 0.12 * EM, 0.03 * EM, 0.76 * EM);
            cr.set_source_rgb(ACCENT[0], ACCENT[1], ACCENT[2]);
            let _ = cr.fill();
        }
        (left - 10.0, TOP - 10.0, total + 0.1 * EM + 20.0, EM + 20.0)
    }

    fn draw_timing(&mut self, cr: &Context, target: &cairo::Surface, width: f64, s: &Scene, ink: Rgb, shadow: [u8; 4]) -> (f64, f64, f64, f64) {
        let Mode::Timing { mm, ss, enter_start, leaving, transition, tension, done, done_start, dots_on, shake } = s.mode else {
            unreachable!()
        };
        let now = s.now;
        // Quantize tension so colour/axes changes reuse a small set of sprites.
        let t = ((*tension * 20.0).round() / 20.0) as f64;
        let color = mix(ink, ACCENT, t);
        let (wght, wdth, _) = anim::tension_axes(t as f32);
        // The lean uses the unquantized tension: it is a transform, so it can animate smoothly.
        let skew = if *done { 0.0 } else { anim::tension_axes(*tension).2 as f64 };
        let solid = Style::Solid { fill: rgba8(color, 1.0), shadow };

        // Minutes: hollow outline (filled with the accent when done).
        let mm_style = Style::Outline { stroke: rgba8(ink, 1.0), fill: done.then(|| rgba8(ACCENT, 1.0)) };
        let mm_w = self.sprite(target, mm, 560.0, 74.0, mm_style.clone()).map_or(EM, |sp| sp.width);
        let total = mm_w + DOTS_W + EM;
        let left = ((width - total) / 2.0).round();
        if let Some(sp) = self.sprite(target, mm, 560.0, 74.0, mm_style) {
            blit(cr, sp, left, TOP, 1.0);
        }

        let dot_color = if t > 0.0 { ACCENT } else { ink };
        draw_dots(cr, left + mm_w, dot_color, if *dots_on { 1.0 } else { 0.2 });

        // Seconds cell (1em × 1em) with tension transform: skew, release bounce, shake.
        let cx = left + mm_w + DOTS_W;
        let _ = cr.save();
        cr.translate(cx + EM / 2.0 + shake.0 as f64, TOP + EM / 2.0 + shake.1 as f64);
        cr.rotate((shake.2 as f64).to_radians());
        if *done {
            let k = anim::release(anim::progress(now, *done_start, 0, anim::RELEASE_MS)) as f64;
            cr.scale(k, k);
        }
        cr.transform(cairo::Matrix::new(1.0, 0.0, skew.to_radians().tan(), 1.0, 0.0, 0.0));
        cr.translate(-EM / 2.0, -EM / 2.0);

        for l in leaving {
            let Some(sp) = self.sprite(target, &l.text, wght, wdth, solid.clone()) else { continue };
            let gx = (EM - sp.width) / 2.0;
            match transition {
                Transition::Shatter => {
                    let p = anim::progress(now, l.start, 0, anim::SHATTER_MS);
                    for (i, poly) in anim::SHARDS.iter().enumerate() {
                        let pose = anim::shard(i, p);
                        let _ = cr.save();
                        apply_pose(cr, &pose, EM / 2.0, EM / 2.0);
                        cr.move_to(poly[0].0 as f64 * EM, poly[0].1 as f64 * EM);
                        for &(x, y) in &poly[1..] {
                            cr.line_to(x as f64 * EM, y as f64 * EM);
                        }
                        cr.close_path();
                        cr.clip();
                        blit(cr, sp, gx, 0.0, pose.alpha as f64);
                        let _ = cr.restore();
                    }
                }
                Transition::Melt => {
                    let pose = anim::melt(anim::progress(now, l.start, 0, anim::MELT_MS));
                    let _ = cr.save();
                    apply_pose(cr, &pose, EM / 2.0, EM / 4.0);
                    // No blur filter: smear a few faded copies downward instead.
                    let b = pose.blur as f64;
                    let a = pose.alpha as f64;
                    for k in 1..=3 {
                        blit(cr, sp, gx, b * k as f64 * 0.8, a * 0.25 * (b / 8.0));
                    }
                    blit(cr, sp, gx, 0.0, a * (1.0 - b / 16.0));
                    let _ = cr.restore();
                }
            }
        }

        if let Some(sp) = self.sprite(target, ss, wght, wdth, solid) {
            let pose = match enter_start {
                Some(start) => anim::drop(anim::progress(now, *start, anim::DROP_DELAY_MS, anim::DROP_MS)),
                None => Pose::REST,
            };
            let _ = cr.save();
            apply_pose(cr, &pose, EM / 2.0, EM / 2.0);
            blit(cr, sp, (EM - sp.width) / 2.0, 0.0, 1.0);
            let _ = cr.restore();
        }
        let _ = cr.restore();

        (left - 10.0, TOP - 10.0, total + 20.0, EM + 20.0)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_toast(&mut self, cr: &Context, target: &cairo::Surface, width: f64, text: &str, vis: f32, ink: Rgb, light: bool) {
        if !self.toast.as_ref().is_some_and(|t| t.text == text && t.light == light) {
            match rasterize_toast(target, text, ink, light) {
                Ok(t) => self.toast = Some(t),
                Err(_) => return,
            }
        }
        let Some(t) = &self.toast else { return };
        let x = ((width - t.w) / 2.0).round();
        let y = TOP + EM + 14.0 - 4.0 * (1.0 - vis as f64) - 4.0;
        let _ = cr.save();
        cr.rectangle(x, y, t.w, t.h);
        cr.clip();
        let _ = cr.set_source_surface(&t.surf, x, y);
        let _ = cr.paint_with_alpha(vis as f64);
        let _ = cr.restore();
    }
}

fn rasterize_toast(target: &cairo::Surface, text: &str, ink: Rgb, light: bool) -> Result<Toast, cairo::Error> {
    let probe = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)?;
    let pcr = Context::new(&probe)?;
    let layout = pangocairo::functions::create_layout(&pcr);
    let mut fd = pango::FontDescription::from_string("JetBrains Mono");
    fd.set_absolute_size(11.0 * pango::SCALE as f64);
    fd.set_weight(pango::Weight::Medium);
    layout.set_font_description(Some(&fd));
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrInt::new_letter_spacing((0.16 * 11.0 * pango::SCALE as f64) as i32));
    layout.set_attributes(Some(&attrs));
    layout.set_text(text);
    let (_, logical) = layout.pixel_extents();
    let pad = 4.0;
    let (w, h) = (logical.width() as f64 + 2.0 * pad, logical.height() as f64 + 2.0 * pad);
    let surf = target.create_similar(cairo::Content::ColorAlpha, w.ceil() as i32, h.ceil() as i32)?;
    let cr = Context::new(&surf)?;
    // Soft halo for legibility (CSS `text-shadow: 0 1px 8px`).
    let (sc, sa) = if light { ([1.0, 1.0, 1.0], 0.6) } else { ([0.0, 0.0, 0.0], 0.5) };
    cr.move_to(pad, pad + 1.0);
    pangocairo::functions::layout_path(&cr, &layout);
    cr.set_line_width(3.0);
    cr.set_line_join(cairo::LineJoin::Round);
    cr.set_source_rgba(sc[0], sc[1], sc[2], sa * 0.35);
    cr.stroke()?;
    cr.move_to(pad, pad);
    cr.set_source_rgba(ink[0], ink[1], ink[2], if light { 0.72 } else { 0.78 });
    pangocairo::functions::show_layout(&cr, &layout);
    drop(cr);
    Ok(Toast { text: text.to_string(), light, surf, w, h })
}

/// Apply a pose around the origin (ox, oy) in the current space; offsets are in em.
fn apply_pose(cr: &Context, p: &Pose, ox: f64, oy: f64) {
    cr.translate(p.dx as f64 * EM + ox, p.dy as f64 * EM + oy);
    cr.rotate((p.rot as f64).to_radians());
    cr.scale(p.sx as f64, p.sy as f64);
    cr.translate(-ox, -oy);
}

/// Colon: two .07em squares, .22em apart, vertically centred in the 1em line.
fn draw_dots(cr: &Context, x: f64, color: Rgb, alpha: f64) {
    let d = 0.07 * EM;
    let y0 = TOP + (EM - (2.0 * d + 0.22 * EM)) / 2.0;
    cr.set_source_rgba(color[0], color[1], color[2], alpha);
    cr.rectangle(x + 0.05 * EM, y0, d, d);
    cr.rectangle(x + 0.05 * EM, y0 + d + 0.22 * EM, d, d);
    let _ = cr.fill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_blur_spreads_and_conserves() {
        let (w, h) = (21, 21);
        let mut d = vec![0u8; w * h];
        d[10 * w + 10] = 255;
        d[10 * w + 9] = 255;
        d[10 * w + 11] = 255;
        box_blur(&mut d, w, h, w, 1);
        assert!(d[10 * w + 10] > 0 && d[10 * w + 10] < 255);
        assert!(d[9 * w + 12] > 0, "spreads diagonally");
        assert_eq!(d[0], 0);
    }

    #[test]
    fn mix_endpoints() {
        let close = |a: Rgb, b: Rgb| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
        assert!(close(mix(INK_DARK_BG, ACCENT, 0.0), INK_DARK_BG));
        assert!(close(mix(INK_DARK_BG, ACCENT, 1.0), ACCENT));
    }
}
