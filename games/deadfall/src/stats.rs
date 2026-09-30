//! What the game remembers about a player between sessions: kills, deaths, time played and so on, kept in one small
//! file on this machine. The client feeds a [`MatchTracker`] the events of a match; finishing it folds the match
//! into [`Stats`], which is stored atomically (a crash never tears the file).
use crate::sim::Event;
use crate::weapons;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use vesper3d::viewer::devkit::{load_or_default, store_atomic};

/// The folder for this game's files: `%APPDATA%\Deadfall` on Windows, `$XDG_DATA_HOME/deadfall` or
/// `~/.local/share/deadfall` elsewhere. `DEADFALL_DATA` overrides it (tests, portable installs).
pub fn data_dir() -> PathBuf {
    let dir = if let Some(d) = std::env::var_os("DEADFALL_DATA") {
        PathBuf::from(d)
    } else if let Some(d) = std::env::var_os("APPDATA") {
        PathBuf::from(d).join("Deadfall")
    } else if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(d).join("deadfall")
    } else if let Some(h) = std::env::var_os("HOME") {
        PathBuf::from(h).join(".local/share/deadfall")
    } else {
        PathBuf::from("deadfall-data")
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub kills: u64,
    pub deaths: u64,
    pub headshots: u64,
    pub damage: u64,
    pub shots_fired: u64,
    pub shots_hit: u64,
    pub matches: u64,
    pub wins: u64,
    pub losses: u64,
    pub draws: u64,
    pub seconds_played: u64,
    pub best_streak: u32,
    pub best_match_kills: u32,
    /// Kills per weapon key.
    pub weapon_kills: BTreeMap<String, u64>,
    pub first_played: u64,
    pub last_played: u64,
}

impl Stats {
    pub fn path() -> PathBuf {
        data_dir().join("stats.json")
    }
    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }
    pub fn load_from(path: &Path) -> Self {
        load_or_default(path)
    }
    pub fn store(&self) -> bool {
        self.store_to(&Self::path())
    }
    pub fn store_to(&self, path: &Path) -> bool {
        store_atomic(path, self)
    }

    /// Kills per death (kills when there are no deaths yet).
    pub fn kd(&self) -> f32 {
        self.kills as f32 / self.deaths.max(1) as f32
    }
    /// Fraction of shots that hit a player, 0..1.
    pub fn accuracy(&self) -> f32 {
        if self.shots_fired == 0 {
            0.
        } else {
            self.shots_hit as f32 / self.shots_fired as f32
        }
    }
    pub fn headshot_rate(&self) -> f32 {
        if self.kills == 0 {
            0.
        } else {
            self.headshots as f32 / self.kills as f32
        }
    }
    pub fn favourite_weapon(&self) -> Option<(&str, u64)> {
        self.weapon_kills.iter().max_by_key(|(_, n)| **n).map(|(k, n)| (k.as_str(), *n))
    }

    /// Fold a finished match in.
    pub fn add(&mut self, m: &MatchSummary, now_unix: u64) {
        self.kills += m.kills as u64;
        self.deaths += m.deaths as u64;
        self.headshots += m.headshots as u64;
        self.damage += m.damage as u64;
        self.shots_fired += m.shots_fired as u64;
        self.shots_hit += m.shots_hit as u64;
        self.seconds_played += m.seconds as u64;
        self.matches += 1;
        match m.won {
            Some(true) => self.wins += 1,
            Some(false) => self.losses += 1,
            None => self.draws += 1,
        }
        self.best_streak = self.best_streak.max(m.best_streak);
        self.best_match_kills = self.best_match_kills.max(m.kills);
        for (k, n) in &m.weapon_kills {
            *self.weapon_kills.entry(k.clone()).or_default() += *n as u64;
        }
        if self.first_played == 0 {
            self.first_played = now_unix;
        }
        self.last_played = now_unix;
    }
}

/// One match as this player lived it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatchSummary {
    pub kills: u32,
    pub deaths: u32,
    pub headshots: u32,
    pub damage: u32,
    pub shots_fired: u32,
    pub shots_hit: u32,
    pub seconds: f32,
    pub best_streak: u32,
    /// `Some(true)` won, `Some(false)` lost, `None` drawn (or left early).
    pub won: Option<bool>,
    pub weapon_kills: BTreeMap<String, u32>,
}

/// Watches one match's events for the local player.
#[derive(Clone, Debug, Default)]
pub struct MatchTracker {
    pub me: Option<u8>,
    pub summary: MatchSummary,
    streak: u32,
    counted: bool,
}

impl MatchTracker {
    pub fn new(me: Option<u8>) -> Self {
        Self { me, ..Default::default() }
    }

    pub fn tick(&mut self, seconds: f32) {
        self.summary.seconds += seconds;
    }

    /// The local player pulled the trigger and a bullet left (counted from the predicted shot).
    pub fn shot_fired(&mut self) {
        self.summary.shots_fired += 1;
    }

    pub fn on_event(&mut self, e: &Event) {
        let Some(me) = self.me else { return };
        match e {
            Event::Kill { killer, victim, weapon, head } => {
                if *killer == me && *victim != me {
                    self.summary.kills += 1;
                    self.streak += 1;
                    self.summary.best_streak = self.summary.best_streak.max(self.streak);
                    if *head {
                        self.summary.headshots += 1;
                    }
                    if let Some(def) = weapons::get(*weapon) {
                        *self.summary.weapon_kills.entry(def.key.to_string()).or_default() += 1;
                    }
                }
                if *victim == me {
                    self.summary.deaths += 1;
                    self.streak = 0;
                }
            }
            Event::Hurt { attacker, victim, damage, .. } if *attacker == me && *victim != me => {
                self.summary.damage += *damage as u32;
            }
            Event::Shot { shooter, hit, .. } if *shooter == me && (*hit == crate::sim::hit::BODY || *hit == crate::sim::hit::HEAD) => {
                self.summary.shots_hit += 1;
            }
            _ => {}
        }
    }

    /// The match is over: fold it into `stats` (once). `my_team` and `winner` (0, 1 or 2 for a draw) say how it went.
    pub fn finish(&mut self, stats: &mut Stats, my_team: Option<u8>, winner: Option<u8>, now_unix: u64) -> bool {
        if self.counted || self.summary.seconds < 5. {
            return false;
        }
        self.counted = true;
        self.summary.won = match (my_team, winner) {
            (Some(t), Some(w)) if w < 2 => Some(t == w),
            _ => None,
        };
        stats.add(&self.summary, now_unix);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kill(killer: u8, victim: u8, head: bool) -> Event {
        Event::Kill { killer, victim, weapon: weapons::id_of("k47").unwrap(), head }
    }

    #[test]
    fn a_match_adds_up_and_survives_a_restart() {
        let mut t = MatchTracker::new(Some(1));
        for e in [kill(1, 2, true), kill(1, 3, false), kill(2, 1, false), kill(1, 4, false)] {
            t.on_event(&e);
        }
        t.on_event(&Event::Hurt { victim: 2, attacker: 1, damage: 40, head: false, weapon: 1, from: vesper3d::math::V::ZERO });
        t.shot_fired();
        t.shot_fired();
        t.on_event(&Event::Shot { shooter: 1, weapon: 1, from: vesper3d::math::V::ZERO, to: vesper3d::math::V::ZERO, hit: 3, material: 0 });
        t.tick(300.);
        let mut stats = Stats::default();
        assert!(t.finish(&mut stats, Some(0), Some(0), 1_700_000_000));
        assert!(!t.finish(&mut stats, Some(0), Some(0), 1_700_000_000), "a match counts once");
        assert_eq!((stats.kills, stats.deaths, stats.headshots, stats.damage), (3, 1, 1, 40));
        assert_eq!((stats.matches, stats.wins, stats.seconds_played, stats.best_streak), (1, 1, 300, 2));
        assert_eq!(stats.favourite_weapon(), Some(("k47", 3)));
        assert!((stats.accuracy() - 0.5).abs() < 1e-6);
        let dir = std::env::temp_dir().join(format!("deadfall-stats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stats.json");
        assert!(stats.store_to(&path));
        assert_eq!(Stats::load_from(&path), stats);
        let mut again = Stats::load_from(&path);
        let mut t2 = MatchTracker::new(Some(0));
        t2.tick(60.);
        t2.finish(&mut again, Some(1), Some(0), 1_700_000_100);
        assert_eq!((again.matches, again.losses, again.first_played, again.last_played), (2, 1, 1_700_000_000, 1_700_000_100));
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Stats::load_from(&path), Stats::default(), "a damaged file never stops the game");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_match_that_barely_started_does_not_count() {
        let mut t = MatchTracker::new(Some(0));
        t.tick(2.);
        let mut stats = Stats::default();
        assert!(!t.finish(&mut stats, Some(0), Some(0), 1));
        assert_eq!(stats.matches, 0);
    }

    #[test]
    fn kills_of_your_own_and_deaths_by_falling_are_counted_honestly() {
        let mut t = MatchTracker::new(Some(3));
        t.on_event(&kill(3, 3, false)); // a self-kill is not a kill
        t.on_event(&kill(255, 3, false)); // a fall is a death
        assert_eq!((t.summary.kills, t.summary.deaths), (0, 2));
    }
}
