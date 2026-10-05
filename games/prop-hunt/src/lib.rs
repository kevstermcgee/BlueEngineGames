//! Prop Hunt's rules: a pure, deterministic, fixed-step simulation of seven hiders and one seeker with
//! no window, no sound device and no wall clock. `main.rs` presents it; tests, bots and the server drive
//! it directly.
//!
//! - `disguise`: the curated subset of BlueEngine's prop catalog a hider may become.
//! - `layout`: Hollow Pine's old house (walls, legal disguise spots, decoy props, spawns).
//! - `sim`: the round (hide/seek phase FSM, inspection, win conditions, save states, `RoundReport`).
//! - `netgame`: the game as a BlueEngine `NetGame` (protocol layouts, seats to roles, the per-participant
//!   snapshot redaction that is the whole point of this game); the lobby, sessions, server and client come
//!   from the engine's netplay kit.
//! - `bot`: simple AI, reading only the same (possibly redacted) snapshot a real client would see, for
//!   tests and the `prop-hunt-bots` load-test client.
//!
//! `models` (the client meshes) is declared in `main.rs`, not here: it needs the `client` feature's
//! renderer types, and the library itself must build and test headlessly without them.
pub mod bot;
pub mod disguise;
pub mod layout;
pub mod netgame;
pub mod sim;

pub use disguise::DisguiseKind;
pub use layout::{Decoy, DisguiseSpot};
pub use netgame::{PropHuntGame, PropHuntSnapshot, PropHuntView};
pub use sim::{
    Event, Hider, HiderReport, HiderState, Input, Inputs, Outcome, Phase, RoundReport, Sim, SimState, CONFIRM_RADIUS,
    HIDE_PHASE_TICKS, INSPECT_HOLD_TICKS, INSPECT_RADIUS, MAX_HIDERS, MAX_SEATS, SEEKER_SLOT, SEEK_PHASE_TICKS,
};

/// A number that changes whenever anything both peers must agree on changes: the house's geometry, the
/// legal disguise spots, the disguise catalog's sizes and the round's timing constants (a client predicts
/// its own movement and renders decoys from the same data the server plays with, so none of it may drift).
pub fn content_fingerprint() -> u32 {
    let mut h: u32 = 0x811c_9dc5 ^ 3;
    let mut mix = |v: u32| h = (h ^ v).wrapping_mul(0x0100_0193);
    mix(MAX_HIDERS as u32);
    mix(HIDE_PHASE_TICKS);
    mix(SEEK_PHASE_TICKS);
    mix(INSPECT_RADIUS.to_bits());
    mix(INSPECT_HOLD_TICKS);
    mix(CONFIRM_RADIUS.to_bits());
    mix(layout::DISGUISE_SPOTS.len() as u32);
    for s in layout::DISGUISE_SPOTS {
        mix(s.pos.0.to_bits());
        mix(s.pos.2.to_bits());
        mix(s.yaw.to_bits());
    }
    for c in layout::colliders() {
        mix(c.min.0.to_bits());
        mix(c.min.1.to_bits());
        mix(c.min.2.to_bits());
        mix(c.max.0.to_bits());
        mix(c.max.1.to_bits());
        mix(c.max.2.to_bits());
    }
    for kind in DisguiseKind::ALL {
        let e = kind.half_extents();
        mix(e.0.to_bits());
        mix(e.1.to_bits());
        mix(e.2.to_bits());
    }
    h
}
