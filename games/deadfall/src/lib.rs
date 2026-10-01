//! Deadfall: team deathmatch for up to twelve players, rules as a plain library.
//!
//! The simulation, the armoury, the map, the bots and the network layouts live here and need no window, so
//! the server and the tests run headless. The window (renderer, menus, sound, input) is the `client` module,
//! behind the `client` feature.
pub mod bots;
pub mod hands;
pub mod input;
pub mod level;
pub mod nav;
pub mod netgame;
pub mod prefs;
pub mod sim;
pub mod slagworks;
pub mod stats;
pub mod team;
pub mod weapons;

pub use team::Team;

#[cfg(feature = "client")]
pub mod client;

/// The map every match is played on, built once.
pub fn level() -> &'static level::Level {
    static LEVEL: std::sync::OnceLock<level::Level> = std::sync::OnceLock::new();
    LEVEL.get_or_init(map)
}

/// Build the map afresh.
pub fn map() -> level::Level {
    slagworks::build()
}
