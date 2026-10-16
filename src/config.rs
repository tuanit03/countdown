//! Persisted user choices (sound, mute, volume, transition) as JSON.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transition {
    Shatter,
    Melt,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub sound: usize,
    pub muted: bool,
    pub volume: f32,
    pub transition: Transition,
}

impl Default for Config {
    fn default() -> Self {
        Config { sound: 0, muted: false, volume: 0.35, transition: Transition::Shatter }
    }
}

impl Config {
    pub fn default_path() -> PathBuf {
        if let Ok(p) = std::env::var("COUNTDOWN_CONFIG") {
            return p.into();
        }
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".config"));
        base.join("countdown/config.json")
    }

    pub fn load(path: &Path) -> Config {
        let mut c: Config = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        c.sound %= crate::audio::SOUNDS.len();
        c.volume = c.volume.clamp(0.0, 1.0);
        c
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_sanitize() {
        let dir = std::env::temp_dir().join(format!("countdown-test-{}", std::process::id()));
        let path = dir.join("config.json");
        let c = Config { sound: 3, muted: true, volume: 0.6, transition: Transition::Melt };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path), c);

        std::fs::write(&path, br#"{"sound": 42, "volume": 7}"#).unwrap();
        let c = Config::load(&path);
        assert_eq!(c.sound, 42 % crate::audio::SOUNDS.len());
        assert_eq!(c.volume, 1.0);
        assert_eq!(c.transition, Transition::Shatter);

        assert_eq!(Config::load(&dir.join("missing.json")), Config::default());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
