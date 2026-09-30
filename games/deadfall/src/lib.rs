//! Deadfall: team deathmatch for up to twelve players, rules as a plain library.
//!
//! The simulation, the armoury, the map, the bots and the network layouts live here and need no window, so
//! the server and the tests run headless. The window (renderer, menus, sound, input) is the `client` module,
//! behind the `client` feature.
pub mod level;
pub mod weapons;

#[cfg(feature = "client")]
pub mod client;
