//! Simple AI: just enough to drive a hider or the seeker through a whole round, for `tests/net.rs`,
//! `tests/udp.rs` and the `prop-hunt-bots` load-test client. It reads only the same (possibly redacted)
//! [`PropHuntSnapshot`] a real client would see — a bot is not given any extra information — so a bot
//! playing the seeker is exactly as blind to a hider's disguise choice as a real seeker would be, and a
//! bot playing a hider learns nothing about where anyone else went.
use crate::disguise::DisguiseKind;
use crate::layout;
use crate::netgame::PropHuntSnapshot;
use crate::sim::{Input, CONFIRM_RADIUS, INSPECT_RADIUS, MAX_HIDERS};
use vesper3d::math::V;
use vesper3d::viewer::devkit::Rng;

fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn flat(v: V) -> V {
    V(v.0, 0., v.2)
}

/// The yaw (`Controller`'s convention: 0 faces -Z) that looks straight from `from` to `to`.
fn yaw_toward(from: V, to: V) -> f32 {
    let d = flat(to) - flat(from);
    d.0.atan2(-d.2)
}

/// Turn toward `target` and walk forward once roughly facing it: `(forward, right, look_yaw_delta)`.
fn steer(pos: V, yaw: f32, target: V) -> (f32, f32, f32) {
    let error = wrap_angle(yaw_toward(pos, target) - yaw);
    let turn = error.clamp(-0.08, 0.08);
    let forward = if error.abs() < 0.6 { 1. } else { 0. };
    (forward, 0., turn)
}

/// A hide-phase bot: walk to a spot it picked for itself and confirm a disguise it picked for itself,
/// once close enough. If its chosen spot is taken by the time it arrives (another bot got there first),
/// it simply idles — `Sim`'s end-of-hide-phase auto-placement (the same safety net a slow human benefits
/// from) picks it a free spot instead, so this never leaves it stuck.
#[derive(Clone, Copy)]
pub struct HiderBot {
    pub spot: usize,
    pub disguise: DisguiseKind,
}

impl HiderBot {
    /// Picks a spot and a disguise from the round seed and this bot's own seat, so bots in the same
    /// round do not all walk to the same place.
    pub fn new(seed: u64, participant: usize) -> Self {
        let mix = (participant as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut rng = Rng::new(seed ^ mix);
        let spot = rng.below(layout::DISGUISE_SPOTS.len());
        let disguise = *rng.pick(&DisguiseKind::ALL).expect("DisguiseKind::ALL is not empty");
        Self { spot, disguise }
    }

    pub fn drive(&self, snapshot: &PropHuntSnapshot, participant: usize) -> Input {
        if participant >= MAX_HIDERS {
            return Input::default();
        }
        let me = snapshot.hiders[participant];
        if me.confirmed {
            return Input::default();
        }
        let target = layout::DISGUISE_SPOTS[self.spot];
        let (forward, right, turn) = steer(me.pos, me.yaw, target.pos);
        let close = (flat(me.pos) - target.pos).length() < CONFIRM_RADIUS * 0.6;
        Input {
            forward,
            right,
            look: [turn, 0.],
            choose_disguise: Some(self.disguise.index()),
            confirm_placement: close,
            ..Default::default()
        }
    }
}

/// A seek-phase bot: walk toward the nearest hider visible in its own snapshot (every hider, for a real
/// seeker's view) and hold `inspect` once in range. With nobody left to find it does nothing.
pub fn seeker_drive(snapshot: &PropHuntSnapshot) -> Input {
    let seeker_pos = flat(snapshot.seeker.pos);
    let nearest = snapshot.hiders.iter().filter(|h| !h.tagged && h.confirmed).min_by(|a, b| {
        let da = (flat(a.pos) - seeker_pos).length();
        let db = (flat(b.pos) - seeker_pos).length();
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    match nearest {
        Some(h) => {
            let (forward, right, turn) = steer(snapshot.seeker.pos, snapshot.seeker.yaw, h.pos);
            let close = (flat(h.pos) - seeker_pos).length() < INSPECT_RADIUS;
            Input { forward, right, look: [turn, 0.], inspect: close, ..Default::default() }
        }
        None => Input::default(),
    }
}
