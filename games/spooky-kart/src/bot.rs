//! AI drivers. A bot reads the simulation and returns the `KartInput` a human would press, so it needs no
//! state of its own: it replays identically, is saved with the race, and doubles as a load-test client.
use crate::character::Perk;
use crate::kart::KartInput;
use crate::sim::Sim;
use crate::track::{wrap_angle, yaw_of};

/// What kart `slot` would press this tick.
pub fn drive(sim: &Sim, slot: usize) -> KartInput {
    let kart = &sim.karts[slot];
    let track = sim.track();
    let stats = kart.character.stats();
    let speed = kart.vel.length();

    // Aim at the road a little ahead, further the faster we go.
    let look = 9. + speed * 0.6;
    let target = track.point_at(kart.s + look);
    let error = wrap_angle(yaw_of(target - kart.pos) - kart.yaw);
    let steer = (error * 2.2).clamp(-1., 1.);

    // How sharply the road bends ahead sets how fast to take it.
    let bend = wrap_angle(yaw_of(track.tangent_at(kart.s + look * 1.8)) - yaw_of(track.tangent_at(kart.s))).abs();
    let want = stats.top_speed * (1. - (bend * 0.9).min(0.55)) * (0.9 + 0.1 * kart.skill);
    let throttle = if speed < want {
        1.
    } else if speed > want * 1.25 {
        -0.5
    } else {
        0.
    };
    let drift = bend > 0.5 && speed > 0.75 * stats.top_speed && error.abs() > 0.3 && kart.skill > 0.9;

    // Use the perk when it would matter.
    let ahead_within = |limit: f32| {
        sim.karts
            .iter()
            .enumerate()
            .any(|(j, o)| j != slot && !o.phased() && (0. ..limit).contains(&track.arc_delta(kart.s, o.s)))
    };
    let behind_within = |limit: f32| {
        sim.karts.iter().enumerate().any(|(j, o)| j != slot && (2. ..limit).contains(&track.arc_delta(o.s, kart.s)))
    };
    let near = |limit: f32| {
        sim.karts.iter().enumerate().any(|(j, o)| j != slot && !o.phased() && (o.pos - kart.pos).length() < limit)
    };
    let perk = sim.racing()
        && kart.cooldown == 0
        && match kart.character.perk() {
            Perk::BandageTrail | Perk::Rattle => behind_within(25.),
            Perk::CrowSwarm => ahead_within(30.),
            Perk::Honk => near(8.),
            Perk::Phase => kart.offroad || near(3.),
            Perk::BatBoost | Perk::Ram | Perk::Undead => false,
        };
    KartInput { throttle, steer, drift, perk }
}
