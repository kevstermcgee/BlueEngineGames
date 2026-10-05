//! Slapstick: two-player online air hockey on BlueEngine. The rules are a plain library (`sim.rs`,
//! `netgame.rs`, `bot.rs`) so they can be tested without a window; `main.rs` is the window, renderer,
//! sound and input built on top of it.
//!
//! `models` (the client meshes) is declared in `main.rs`, not here: it needs the `client` feature's
//! renderer types, and the library itself must build and test headlessly without them. See `AGENTS.md`
//! for the architecture this game must not break.
pub mod bot;
pub mod netgame;
pub mod sim;

pub use netgame::{AirHockeyGame, AirHockeySnapshot, AirHockeyView};
pub use sim::{
    Event, Input, Inputs, MatchReport, Paddle, Phase, Puck, Sim, GOAL_HALF_WIDTH, MATCH_TIME_LIMIT_TICKS, PADDLE_MASS,
    PADDLE_MAX_SPEED, PADDLE_PUCK_RESTITUTION, PADDLE_RADIUS, PUCK_MASS, PUCK_MAX_SPEED, PUCK_RADIUS, SERVE_TICKS,
    TABLE_HALF_LENGTH, TABLE_HALF_WIDTH, WALL_RESTITUTION, WIN_GOALS,
};

/// A number that changes whenever anything both peers must agree on changes: the table's dimensions, the
/// goal mouth, the physics constants and the rules' tick counts (a client predicts its own paddle from
/// the same constants the server plays with, so none of it may drift). Peers with a different
/// fingerprint are refused by the netplay kit, so a stale client never plays a new server.
pub fn content_fingerprint() -> u32 {
    let mut h: u32 = 0x811c_9dc5 ^ 4;
    let mut mix = |v: u32| h = (h ^ v).wrapping_mul(0x0100_0193);
    for v in [
        TABLE_HALF_WIDTH,
        TABLE_HALF_LENGTH,
        GOAL_HALF_WIDTH,
        PUCK_RADIUS,
        PADDLE_RADIUS,
        PUCK_MASS,
        PADDLE_MASS,
        PADDLE_MAX_SPEED,
        PUCK_MAX_SPEED,
        WALL_RESTITUTION,
        PADDLE_PUCK_RESTITUTION,
    ] {
        mix(v.to_bits());
    }
    mix(WIN_GOALS);
    mix(SERVE_TICKS);
    mix(MATCH_TIME_LIMIT_TICKS as u32);
    h
}
