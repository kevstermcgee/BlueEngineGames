//! Player preferences: name, sensitivity, volume, and the last things typed into the menus.
use crate::stats::data_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use vesper3d::viewer::devkit::{load_or_default, store_atomic, ShadowQuality};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub name: String,
    /// Mouse look multiplier, 0.2-4.
    pub sensitivity: f32,
    /// Stick look speed multiplier, 0.3-3.
    pub stick_sensitivity: f32,
    pub invert_y: bool,
    /// Sound effects and ambience volume, 0-1.
    pub volume: f32,
    pub fullscreen: bool,
    /// Shadows: Off, Simple (soft contact shadows, the default) or Full (cast shadows). Files from before this
    /// existed, and unknown words, read as Simple.
    pub shadows: ShadowQuality,
    /// The Play Online hub the player chose on the command line (`--hub`) and then joined a room on: the next run uses it
    /// again unless `--hub` or a `server.txt` says otherwise. Never set for the built-in hub, so a new default still reaches
    /// everyone.
    pub last_hub: Option<String>,
    /// The last server typed in, and its key.
    pub address: String,
    pub key: String,
    /// What the last hosted or solo game used.
    pub kills_target: u16,
    pub minutes: u16,
    pub end_by_time: bool,
    pub bots: bool,
    pub bot_skill: u8,
    pub bot_count_fill: bool,
    /// Last team picked (0 Ironclad, 1 Nightwatch).
    pub team: u8,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            name: String::new(),
            sensitivity: 1.,
            stick_sensitivity: 1.,
            invert_y: false,
            volume: 0.8,
            fullscreen: false,
            shadows: ShadowQuality::default(),
            last_hub: None,
            address: String::new(),
            key: String::new(),
            kills_target: 40,
            minutes: 10,
            end_by_time: false,
            bots: false,
            bot_skill: 1,
            bot_count_fill: true,
            team: 0,
        }
    }
}

impl Prefs {
    pub fn path() -> PathBuf {
        data_dir().join("prefs.json")
    }
    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }
    pub fn load_from(path: &Path) -> Self {
        let mut p: Prefs = load_or_default(path);
        p.sanitize();
        p
    }
    pub fn store(&self) -> bool {
        store_atomic(&Self::path(), self)
    }
    /// Clamp everything into its valid range; a hand-edited file never breaks the game.
    pub fn sanitize(&mut self) {
        let fix = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.sensitivity = fix(self.sensitivity, 0.2, 4., 1.);
        self.stick_sensitivity = fix(self.stick_sensitivity, 0.3, 3., 1.);
        self.volume = fix(self.volume, 0., 1., 0.8);
        self.kills_target = self.kills_target.clamp(5, 500);
        self.minutes = self.minutes.clamp(1, 60);
        self.bot_skill = self.bot_skill.min(2);
        self.team = self.team.min(1);
        self.name = self.name.chars().filter(|c| !c.is_control()).take(16).collect();
        self.address = self.address.chars().filter(|c| !c.is_control()).take(80).collect();
        self.last_hub = self.last_hub.take().map(|h| h.chars().filter(|c| !c.is_control()).take(80).collect::<String>()).filter(|h| !h.trim().is_empty());
        self.key = self.key.chars().filter(|c| !c.is_control()).take(64).collect();
    }
    /// The name shown in matches: what was typed, else the computer's user name, else "Soldier".
    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.trim().to_string();
        }
        std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .ok()
            .map(|n| n.chars().take(16).collect::<String>())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Soldier".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonsense_in_the_file_is_repaired_not_trusted() {
        let dir = std::env::temp_dir().join(format!("deadfall-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefs.json");
        std::fs::write(&path, r#"{"sensitivity": 99, "volume": -3, "kills_target": 0, "minutes": 9999, "name": "a\u0007bcdefghijklmnopqrstuvwxyz", "bot_skill": 9}"#).unwrap();
        let p = Prefs::load_from(&path);
        assert_eq!((p.sensitivity, p.volume, p.kills_target, p.minutes, p.bot_skill), (4., 0., 5, 60, 2));
        assert!(p.name.len() <= 16 && !p.name.contains('\u{7}'));
        std::fs::write(&path, "garbage").unwrap();
        assert_eq!(Prefs::load_from(&path), Prefs::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn shadows_default_to_simple_survive_a_round_trip_and_old_files_still_load() {
        let dir = std::env::temp_dir().join(format!("deadfall-prefs-shadows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefs.json");
        // A file written before the setting existed keeps everything else and reads Simple.
        std::fs::write(&path, r#"{"name": "Kev", "sensitivity": 2.0, "volume": 0.3}"#).unwrap();
        let p = Prefs::load_from(&path);
        assert_eq!(p.shadows, ShadowQuality::Simple);
        assert_eq!((p.name.as_str(), p.sensitivity, p.volume), ("Kev", 2., 0.3));
        // An unknown word falls back to Simple without discarding the file.
        std::fs::write(&path, r#"{"name": "Kev", "shadows": "ultra"}"#).unwrap();
        let p = Prefs::load_from(&path);
        assert_eq!((p.shadows, p.name.as_str()), (ShadowQuality::Simple, "Kev"));
        // Each tier is remembered.
        for q in ShadowQuality::ALL {
            let mut p = Prefs::default();
            p.shadows = q;
            std::fs::write(&path, serde_json::to_string(&p).unwrap()).unwrap();
            assert_eq!(Prefs::load_from(&path).shadows, q);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_remembered_hub_is_optional_cleaned_and_absent_from_old_files() {
        let dir = std::env::temp_dir().join(format!("deadfall-prefs-hub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefs.json");
        std::fs::write(&path, r#"{"name": "Kev"}"#).unwrap();
        assert_eq!(Prefs::load_from(&path).last_hub, None);
        std::fs::write(&path, r#"{"last_hub": "play.example.com:4100\u0007"}"#).unwrap();
        assert_eq!(Prefs::load_from(&path).last_hub.as_deref(), Some("play.example.com:4100"));
        std::fs::write(&path, r#"{"last_hub": "  "}"#).unwrap();
        assert_eq!(Prefs::load_from(&path).last_hub, None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
