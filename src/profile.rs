//! Best score and control preferences kept between launches.
use crate::game::Game;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub best: u32,
    pub sensitivity: f32,
    pub mouse: bool,
    pub sound: bool,
    pub minimap: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            best: 0,
            sensitivity: 0.025,
            mouse: true,
            sound: false,
            minimap: true,
        }
    }
}

impl Profile {
    /// `TERMINALSHOOTER_PROFILE` overrides the platform's config directory.
    pub fn path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("TERMINALSHOOTER_PROFILE") {
            return Some(path.into());
        }
        let env = |name| std::env::var_os(name).filter(|value| !value.is_empty());
        let base = if cfg!(target_os = "windows") {
            PathBuf::from(env("APPDATA")?)
        } else if cfg!(target_os = "macos") {
            PathBuf::from(env("HOME")?).join("Library/Application Support")
        } else if let Some(config) = env("XDG_CONFIG_HOME") {
            PathBuf::from(config)
        } else {
            PathBuf::from(env("HOME")?).join(".config")
        };
        Some(base.join("terminalshooter").join("profile.txt"))
    }

    /// Missing files, unknown keys and invalid values fall back to defaults,
    /// so a damaged profile never stops the game from starting.
    pub fn load(path: &Path) -> Self {
        let mut profile = Self::default();
        let Ok(text) = fs::read_to_string(path) else {
            return profile;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "best" => profile.best = value.parse().unwrap_or(profile.best),
                "sensitivity" => {
                    if let Ok(sensitivity) = value.parse::<f32>()
                        && sensitivity.is_finite()
                    {
                        profile.sensitivity = sensitivity.clamp(0.005, 0.1);
                    }
                }
                "mouse" => profile.mouse = value.parse().unwrap_or(profile.mouse),
                "sound" => profile.sound = value.parse().unwrap_or(profile.sound),
                "minimap" => profile.minimap = value.parse().unwrap_or(profile.minimap),
                _ => {}
            }
        }
        profile
    }

    /// Write through a temporary file so an interrupted save keeps the old profile.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        fs::write(
            &temporary,
            format!(
                "best={}\nsensitivity={}\nmouse={}\nsound={}\nminimap={}\n",
                self.best, self.sensitivity, self.mouse, self.sound, self.minimap
            ),
        )?;
        fs::rename(&temporary, path)
    }

    pub fn apply(&self, game: &mut Game) {
        game.best = self.best;
        game.sensitivity = self.sensitivity;
        game.mouse = self.mouse;
        game.sound = self.sound;
        game.minimap = self.minimap;
    }

    pub fn update(&mut self, game: &Game) {
        self.best = self.best.max(game.best);
        self.sensitivity = game.sensitivity;
        self.mouse = game.mouse;
        self.sound = game.sound;
        self.minimap = game.minimap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("terminalshooter-profile-{}", std::process::id()))
            .join(name)
    }

    #[test]
    fn round_trips_through_a_new_directory() {
        let path = scratch("round-trip/profile.txt");
        let profile = Profile {
            best: 4210,
            sensitivity: 0.04,
            mouse: false,
            sound: true,
            minimap: false,
        };
        profile.save(&path).unwrap();
        assert_eq!(Profile::load(&path), profile);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn damaged_or_missing_profiles_fall_back_to_defaults() {
        assert_eq!(Profile::load(&scratch("missing.txt")), Profile::default());
        let path = scratch("damaged.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "best=12\nsensitivity=NaN\nmouse=maybe\njunk\nminimap = false\n",
        )
        .unwrap();
        let profile = Profile::load(&path);
        assert_eq!(profile.best, 12);
        assert_eq!(profile.sensitivity, Profile::default().sensitivity);
        assert!(profile.mouse);
        assert!(!profile.minimap);
        fs::write(&path, "sensitivity=9\n").unwrap();
        assert_eq!(Profile::load(&path).sensitivity, 0.1);
        let _ = fs::remove_file(&path);
    }
}
