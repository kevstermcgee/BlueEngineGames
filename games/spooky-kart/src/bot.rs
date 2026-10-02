//! AI drivers. A bot reads the simulation and returns the `KartInput` a human would press, so it needs no
//! state of its own: it replays identically, is saved with the race, and doubles as a load-test client.
use crate::character::Perk;
use crate::kart::{Driver, KartInput};
use crate::sim::Sim;
use crate::track::{wrap_angle, yaw_of};
use serde::{Deserialize, Serialize};

/// How hard the bots drive. Offline only: an online race always runs at `Medium`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    #[default]
    Medium,
    Hard,
}

/// Everything about a bot's driving that a difficulty changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// Metres aimed ahead: `look_base + speed * look_speed`.
    pub look_base: f32,
    pub look_speed: f32,
    /// Slowdown for a bend ahead: `min(bend * bend_gain, bend_cap)` of top speed.
    pub bend_gain: f32,
    pub bend_cap: f32,
    /// Multiplies the target speed (a bot never exceeds its kart's top speed whatever this says).
    pub pace: f32,
    /// A bot brakes when it is this much over its target speed.
    pub brake_ratio: f32,
    /// The range a bot's `skill` is rolled from at the start of a race.
    pub skill_range: (f32, f32),
    /// Drift when the bend ahead is sharper than this (radians) and the kart is above `drift_speed` of top
    /// speed and pointing `drift_error` radians off the line; `drift_skill` is the care level needed.
    pub drift_bend: f32,
    pub drift_speed: f32,
    pub drift_error: f32,
    pub drift_skill: f32,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % Self::ALL.len()]
    }

    /// The next harder (`+1`) or easier (`-1`) setting, stopping at the ends.
    pub fn step(self, by: i32) -> Self {
        Self::from_index((self.index() as i32 + by).clamp(0, Self::ALL.len() as i32 - 1) as usize)
    }

    /// One line for the select screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Difficulty::Easy => "Relaxed rivals: they brake early and never drift.",
            Difficulty::Medium => "Rivals that keep up the pace through the bends.",
            Difficulty::Hard => "Fast, sharp rivals that drift every corner.",
        }
    }

    pub fn tuning(self) -> Tuning {
        match self {
            // Slower than the original bots: heavy braking for every bend, no drifting.
            Difficulty::Easy => Tuning {
                look_base: 9.,
                look_speed: 0.6,
                bend_gain: 0.9,
                bend_cap: 0.55,
                pace: 0.97,
                brake_ratio: 1.25,
                skill_range: (0.80, 0.97),
                drift_bend: 0.5,
                drift_speed: 0.75,
                drift_error: 0.3,
                drift_skill: 2.,
            },
            Difficulty::Medium => Tuning {
                look_base: 10.,
                look_speed: 0.65,
                bend_gain: 0.7,
                bend_cap: 0.40,
                pace: 1.,
                brake_ratio: 1.25,
                skill_range: (0.85, 1.0),
                drift_bend: 0.5,
                drift_speed: 0.75,
                drift_error: 0.3,
                drift_skill: 0.9,
            },
            Difficulty::Hard => Tuning {
                look_base: 10.5,
                look_speed: 0.7,
                bend_gain: 0.65,
                bend_cap: 0.36,
                pace: 1.0,
                brake_ratio: 1.15,
                skill_range: (0.95, 1.0),
                drift_bend: 0.35,
                drift_speed: 0.65,
                drift_error: 0.2,
                drift_skill: 0.,
            },
        }
    }
}

impl std::str::FromStr for Difficulty {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_lowercase().as_str() {
            "easy" | "e" | "0" => Ok(Difficulty::Easy),
            "medium" | "med" | "m" | "normal" | "1" => Ok(Difficulty::Medium),
            "hard" | "h" | "2" => Ok(Difficulty::Hard),
            other => Err(format!("unknown difficulty '{other}' (use easy, medium or hard)")),
        }
    }
}

/// What kart `slot` would press this tick.
pub fn drive(sim: &Sim, slot: usize) -> KartInput {
    drive_as(sim, slot, sim.difficulty)
}

/// What kart `slot` would press if it drove at `difficulty` (the `--autopilot` flag uses this to hold the
/// human's kart at a fixed, Medium level whatever the rivals are set to).
pub fn drive_as(sim: &Sim, slot: usize, difficulty: Difficulty) -> KartInput {
    let tune = difficulty.tuning();
    let kart = &sim.karts[slot];
    let track = sim.track();
    let stats = kart.character.stats();
    let speed = kart.vel.length();

    // Aim at the road a little ahead, further the faster we go.
    let look = tune.look_base + speed * tune.look_speed;
    let target = track.point_at(kart.s + look);
    let error = wrap_angle(yaw_of(target - kart.pos) - kart.yaw);
    let steer = (error * 2.2).clamp(-1., 1.);

    // How sharply the road bends ahead sets how fast to take it.
    let bend = wrap_angle(yaw_of(track.tangent_at(kart.s + look * 1.8)) - yaw_of(track.tangent_at(kart.s))).abs();
    // A human on autopilot has no rolled skill: give it a fixed, steady, drifting one so the rivals' difficulty is
    // the only thing that changes between races.
    let skill = if kart.driver == Driver::Human { 0.95 } else { kart.skill };
    let want = stats.top_speed * (1. - (bend * tune.bend_gain).min(tune.bend_cap)) * (0.9 + 0.1 * skill) * tune.pace;
    let throttle = if speed < want {
        1.
    } else if speed > want * tune.brake_ratio {
        -0.5
    } else {
        0.
    };
    let drift = bend > tune.drift_bend
        && speed > tune.drift_speed * stats.top_speed
        && error.abs() > tune.drift_error
        && skill > tune.drift_skill;

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
