//! Simple AI: just enough to drive a released paddle through a whole match, for `tests/net.rs`,
//! `tests/determinism.rs` and the `slapstick-bots` load-test client. It reads only the puck's position —
//! the same information a real player's screen shows — so a bot is exactly as blind as a human would be.
use crate::sim::{Input, TABLE_HALF_LENGTH, TABLE_HALF_WIDTH};
use vesper3d::math::V;

/// Paddle-tracks-puck AI for `slot` (0 defends -Z, 1 defends +Z): defends the goal mouth while the puck
/// is in the opponent's half, intercepts directly once the puck crosses into this paddle's own half.
/// Reads only `puck_pos` — the same information a real player's screen shows, never the puck's velocity
/// or either paddle's state.
pub fn paddle_ai(puck_pos: V, slot: usize) -> Input {
    let defending_sign: f32 = if slot == 0 { -1. } else { 1. };
    let depth_from_center = puck_pos.2 * defending_sign; // > 0 once the puck is in this paddle's own half
    let target_x = (puck_pos.0 / TABLE_HALF_WIDTH).clamp(-1., 1.);
    if depth_from_center > 0. {
        // Intercept: track the puck's depth and line, normalized within this paddle's own half (1 at
        // the centre line, -1 at the paddle's own backline).
        let depth_norm = (depth_from_center / TABLE_HALF_LENGTH).clamp(0., 1.);
        Input { target_x, target_z: 1. - 2. * depth_norm }
    } else {
        // Defend: hang back near the own backline, centred on goal.
        Input { target_x: 0., target_z: -0.85 }
    }
}
