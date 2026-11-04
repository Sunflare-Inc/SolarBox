//! Persistent console settings, stored as simple `key=value` lines.

use std::{fs, path::PathBuf};

/// Window scale choices. 0 means "fit the screen".
pub const SCALES: [u8; 5] = [1, 2, 4, 8, 0];

/// Frame caps for running carts, in fps. 0 means no cap.
pub const FPS_CAPS: [u8; 3] = [60, 30, 0];

#[derive(Clone, Debug)]
pub struct Settings {
    pub scale: u8,
    pub fps_cap: u8,
    pub show_fps: bool,
    pub carts_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { scale: 4, fps_cap: 60, show_fps: false, carts_dir: "carts".into() }
    }
}

impl Settings {
    /// `$XDG_CONFIG_HOME/solarbox/settings.cfg`, or `~/.config/solarbox/settings.cfg`.
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("solarbox").join("settings.cfg"))
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Some(text) = Self::path().and_then(|p| fs::read_to_string(p).ok()) else {
            return s;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let flag = |v: &str| matches!(v, "true" | "1" | "on");
            match k {
                "scale" => {
                    if let Some(n) = v.parse::<u8>().ok().filter(|n| SCALES.contains(n)) {
                        s.scale = n;
                    }
                }
                "fps_cap" => {
                    let cap = if v == "none" { Some(0) } else { v.parse::<u8>().ok() };
                    if let Some(c) = cap.filter(|c| FPS_CAPS.contains(c)) {
                        s.fps_cap = c;
                    }
                }
                // older settings files
                "fps_uncapped" if flag(v) => s.fps_cap = 0,
                "show_fps" => s.show_fps = flag(v),
                "carts_dir" if !v.is_empty() => s.carts_dir = v.to_string(),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else { return Ok(()) };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(
            path,
            format!(
                "# SolarBox settings (scale: 1, 2, 4, 8 or 0 for fit-screen)\n\
                 # fps_cap: 60, 30 or 0 (none); applies while a cart runs\n\
                 scale={}\nfps_cap={}\nshow_fps={}\ncarts_dir={}\n",
                self.scale, self.fps_cap, self.show_fps, self.carts_dir
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("solarbox-test-{}", std::process::id()));
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let s = Settings { scale: 8, fps_cap: 30, show_fps: true, carts_dir: "/games/carts".into() };
        s.save().unwrap();
        let l = Settings::load();
        assert_eq!((l.scale, l.fps_cap, l.show_fps, l.carts_dir.as_str()), (8, 30, true, "/games/carts"));
        // Older files used fps_uncapped=true; an invalid cap is ignored.
        std::fs::write(Settings::path().unwrap(), "fps_uncapped=true\n").unwrap();
        assert_eq!(Settings::load().fps_cap, 0);
        std::fs::write(Settings::path().unwrap(), "fps_cap=45\n").unwrap();
        assert_eq!(Settings::load().fps_cap, 60);
        // A bad scale value is ignored rather than crashing.
        std::fs::write(Settings::path().unwrap(), "scale=3\nshow_fps=on\n").unwrap();
        let l = Settings::load();
        assert_eq!((l.scale, l.show_fps), (4, true));
        let _ = std::fs::remove_dir_all(dir);
    }
}
