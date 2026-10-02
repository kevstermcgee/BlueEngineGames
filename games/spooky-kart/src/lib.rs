//! Spooky Kart's rules: a pure, deterministic, fixed-step kart race with no window, no sound device and no
//! wall clock. `main.rs` presents it; tests, bots and the server drive it directly.
//!
//! - `character`: the eight drivers and their numbers.
//! - `track`: Haunted Hollow, the one map.
//! - `kart`: one kart's driving model.
//! - `sim`: the race (perks, hazards, bumping, standings, save states, `RaceReport`).
//! - `border`: where the drawn road edge, kerbs, verges and walls stand (presentation only).
//! - `bot`: AI drivers and their `Difficulty`.
//! - `music`: the race music spec and which layers play when.
//! - `controls`: keyboard and controller mapping.
//! - `netgame`: the game as a BlueEngine `NetGame` (protocol layouts, seats to karts, client prediction); the
//!   lobby, sessions, server and client come from the engine's netplay kit.
pub mod border;
pub mod bot;
pub mod character;
pub mod controls;
pub mod kart;
pub mod music;
pub mod netgame;
pub mod sim;
pub mod track;

pub use bot::Difficulty;
pub use character::{Character, Perk, ALL, MAX_RACERS};
pub use kart::{Driver, Kart, KartInput, KartStats};
pub use netgame::{KartGame, KartSnapshot, KartView};
pub use sim::{Event, Hazard, HazardKind, Inputs, Phase, RaceReport, RacerReport, Sim, SimState, LAPS};
pub use track::Track;

/// A number that changes whenever anything both peers must agree on changes: the layouts, the track, the
/// lap count and every character's numbers (a client predicts its own kart, so stats must match).
pub fn content_fingerprint() -> u32 {
    let track = Track::haunted_hollow();
    let mut h: u32 = 0x811c_9dc5 ^ 2;
    let mut mix = |v: u32| h = (h ^ v).wrapping_mul(0x0100_0193);
    mix(track.length.to_bits());
    mix(track.samples().len() as u32);
    mix(LAPS);
    for c in ALL {
        let s = c.stats();
        for v in [s.top_speed, s.accel, s.handling, s.grip, s.mass, s.offroad, s.drift_rate, s.boost_power] {
            mix(v.to_bits());
        }
    }
    h
}
