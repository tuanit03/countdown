//! Decide light/dark ink from the GNOME wallpaper behind the digits.
//! The screen behind an XWayland window can't be captured, so read the wallpaper file instead.

use image::GenericImageView;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// Above this mean luma (0..1) the wallpaper counts as light.
const LIGHT_THRESHOLD: f32 = 0.55;

#[derive(Default)]
pub struct Wallpaper {
    cache: Option<(PathBuf, Option<SystemTime>, (u32, u32), bool)>,
}

impl Wallpaper {
    /// `Some(true)` for a light wallpaper under the digit area of a `screen` (w, h) monitor.
    pub fn is_light(&mut self, screen: (u32, u32)) -> Option<bool> {
        let path = resolve(&uri_to_path(&current_uri()?)?)?;
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if let Some((p, m, s, light)) = &self.cache {
            if *p == path && *m == mtime && *s == screen {
                return Some(*light);
            }
        }
        let img = image::ImageReader::open(&path).ok()?.with_guessed_format().ok()?.decode().ok()?;
        let light = region_luma(&img, screen) > LIGHT_THRESHOLD;
        self.cache = Some((path, mtime, screen, light));
        Some(light)
    }
}

fn gsettings(schema: &str, key: &str) -> Option<String> {
    let out = Command::new("gsettings").args(["get", schema, key]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'').to_string())
}

fn current_uri() -> Option<String> {
    let dark = gsettings("org.gnome.desktop.interface", "color-scheme").is_some_and(|s| s == "prefer-dark");
    let key = if dark { "picture-uri-dark" } else { "picture-uri" };
    gsettings("org.gnome.desktop.background", key).filter(|s| !s.is_empty())
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    if !raw.starts_with('/') {
        return None;
    }
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

/// GNOME slideshow wallpapers are XML; use the first image they list.
fn resolve(path: &Path) -> Option<PathBuf> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")) {
        let xml = std::fs::read_to_string(path).ok()?;
        let start = xml.find("<file>")? + "<file>".len();
        let end = start + xml[start..].find("</file>")?;
        return Some(PathBuf::from(xml[start..end].trim()));
    }
    Some(path.to_path_buf())
}

/// Mean luma of the top-center band (where the digits sit), assuming GNOME's "zoom" scaling.
pub fn region_luma(img: &image::DynamicImage, (sw, sh): (u32, u32)) -> f32 {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 || sw == 0 || sh == 0 {
        return 0.0;
    }
    let scale = (sw as f32 / iw as f32).max(sh as f32 / ih as f32);
    let off_x = (iw as f32 * scale - sw as f32) / 2.0;
    let off_y = (ih as f32 * scale - sh as f32) / 2.0;
    let (x0, x1) = (sw as f32 / 2.0 - 220.0, sw as f32 / 2.0 + 220.0);
    let (y0, y1) = (0.0, 200.0f32.min(sh as f32));
    let (nx, ny) = (24, 10);
    let mut sum = 0.0;
    for j in 0..ny {
        for i in 0..nx {
            let sx = x0 + (x1 - x0) * (i as f32 + 0.5) / nx as f32;
            let sy = y0 + (y1 - y0) * (j as f32 + 0.5) / ny as f32;
            let px = (((sx + off_x) / scale) as u32).min(iw - 1);
            let py = (((sy + off_y) / scale) as u32).min(ih - 1);
            let p = img.get_pixel(px, py).0;
            sum += (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
        }
    }
    sum / (nx * ny) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    #[test]
    fn uri_decoding() {
        assert_eq!(uri_to_path("file:///usr/share/a%20b.jpg"), Some(PathBuf::from("/usr/share/a b.jpg")));
        assert_eq!(uri_to_path("file:///x/100%"), Some(PathBuf::from("/x/100%")));
        assert_eq!(uri_to_path("https://example.com/x.jpg"), None);
    }

    #[test]
    fn samples_only_the_top_band() {
        // Light top strip, dark everywhere else; 2:1 image on a 16:9 screen gets cropped at the sides.
        let mut img = RgbImage::from_pixel(2000, 1000, Rgb([10, 10, 10]));
        for y in 0..150 {
            for x in 0..2000 {
                img.put_pixel(x, y, Rgb([240, 240, 230]));
            }
        }
        let img = DynamicImage::ImageRgb8(img);
        assert!(region_luma(&img, (1920, 1080)) > LIGHT_THRESHOLD);
        let dark = DynamicImage::ImageRgb8(RgbImage::from_pixel(100, 100, Rgb([20, 30, 40])));
        assert!(region_luma(&dark, (1920, 1080)) < 0.2);
    }

    #[test]
    fn slideshow_xml_resolves_first_file() {
        let dir = std::env::temp_dir().join(format!("countdown-wp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let xml = dir.join("show.xml");
        std::fs::write(&xml, "<background><static><file>\n /a/b.png </file></static></background>").unwrap();
        assert_eq!(resolve(&xml), Some(PathBuf::from("/a/b.png")));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
