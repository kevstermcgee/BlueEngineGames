//! One kart: its state and the arcade driving model. Pure and fixed-step; `sim.rs` decides who does what
//! to whom, this module only moves a single kart over the road.
use crate::character::{Character, Perk, Stats};
use crate::track::{forward, right, wrap_angle, Track, HALF_WIDTH};
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::devkit::TICK;

/// One tick of one driver's intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KartInput {
    /// Gas (+1) and brake/reverse (-1).
    pub throttle: f32,
    /// Steer right (+1) or left (-1).
    pub steer: f32,
    /// Drift button held.
    pub drift: bool,
    /// The perk button went down this tick (a press edge).
    pub perk: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Driver {
    Human,
    Bot,
}

/// What a race teaches us about one kart; the raw numbers behind `RaceReport`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KartStats {
    pub drift_ticks: u32,
    pub wall_hits: u32,
    pub collisions: u32,
    pub perk_uses: u32,
    pub hazard_hits: u32,
    pub offroad_ticks: u32,
    pub racing_ticks: u32,
    pub top_speed: f32,
    pub distance: f32,
    /// Ticks since the start of the race at which each lap was completed.
    pub laps: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kart {
    pub character: Character,
    pub driver: Driver,
    pub pos: V,
    pub vel: V,
    pub yaw: f32,
    pub drifting: bool,
    pub drift_dir: f32,
    pub drift_charge: f32,
    pub boost_ticks: u32,
    pub boost_power: f32,
    pub slow_ticks: u32,
    pub slow_factor: f32,
    pub spin_ticks: u32,
    pub cooldown: u32,
    pub phase_ticks: u32,
    /// Nearest-centreline bookkeeping: segment hint, arc position, and signed distance from the centreline.
    pub hint: usize,
    pub s: f32,
    pub lateral: f32,
    /// Distance driven along the loop from the start line (negative on the grid).
    pub progress: f32,
    pub best_progress: f32,
    pub lap: u32,
    pub finished_tick: Option<u32>,
    /// 1-based finishing place; 0 while racing.
    pub place: u32,
    pub offroad: bool,
    /// A bot's care level, 0.85..1.
    pub skill: f32,
    pub stats: KartStats,
}

/// What `update` noticed, for `Sim` to turn into events.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Notes {
    pub boost_tier: Option<u32>,
    pub wall_impact: Option<f32>,
    pub laps_done: u32,
    pub finished: bool,
}

pub const BOOST_TIER_ONE: f32 = 1.0;
pub const BOOST_TIER_TWO: f32 = 2.0;

impl Kart {
    pub fn new(character: Character, driver: Driver, pos: V, yaw: f32, hint_from: &Track, skill: f32) -> Self {
        let near = hint_from.nearest_global(pos);
        Self {
            character,
            driver,
            pos,
            vel: V(0., 0., 0.),
            yaw,
            drifting: false,
            drift_dir: 0.,
            drift_charge: 0.,
            boost_ticks: 0,
            boost_power: 0.,
            slow_ticks: 0,
            slow_factor: 1.,
            spin_ticks: 0,
            cooldown: 0,
            phase_ticks: 0,
            hint: near.index,
            s: near.s,
            lateral: near.lateral,
            progress: hint_from.arc_delta(0., near.s),
            best_progress: hint_from.arc_delta(0., near.s),
            lap: 0,
            finished_tick: None,
            place: 0,
            offroad: false,
            skill,
            stats: KartStats::default(),
        }
    }

    pub fn stats_table(&self) -> Stats {
        self.character.stats()
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    /// Speed along the kart's heading (negative when reversing).
    pub fn forward_speed(&self) -> f32 {
        self.vel.dot(forward(self.yaw))
    }

    pub fn phased(&self) -> bool {
        self.phase_ticks > 0
    }

    /// Slow this kart to `factor` of its speed for `ticks` (a stronger slow replaces a weaker one).
    pub fn apply_slow(&mut self, factor: f32, ticks: u32) {
        if self.slow_ticks == 0 || factor <= self.slow_factor {
            self.slow_factor = factor;
        }
        self.slow_ticks = self.slow_ticks.max(ticks);
    }

    fn end_drift(&mut self, st: &Stats) -> Option<u32> {
        let tier = if self.drift_charge >= BOOST_TIER_TWO {
            2
        } else if self.drift_charge >= BOOST_TIER_ONE {
            1
        } else {
            0
        };
        self.drifting = false;
        self.drift_charge = 0.;
        if tier == 0 {
            return None;
        }
        self.boost_ticks = 30 + 30 * tier;
        self.boost_power = st.boost_power * if tier == 2 { 1.5 } else { 1. };
        Some(tier)
    }

    /// Advance this kart one tick. `racing` is false during the countdown (the kart sits still).
    pub fn update(&mut self, track: &Track, input: &KartInput, racing: bool, race_tick: u32, laps: u32) -> Notes {
        let mut notes = Notes::default();
        let st = self.character.stats();
        let undead = self.character.perk() == Perk::Undead;
        // Wear-off: the Zombie's stuns and slowdowns end twice as fast.
        let decay = if undead { 2 } else { 1 };
        self.slow_ticks = self.slow_ticks.saturating_sub(decay);
        self.spin_ticks = self.spin_ticks.saturating_sub(decay);
        self.boost_ticks = self.boost_ticks.saturating_sub(1);
        self.cooldown = self.cooldown.saturating_sub(1);
        self.phase_ticks = self.phase_ticks.saturating_sub(1);
        let phased = self.phased();

        let control = racing && self.spin_ticks == 0;
        let throttle = if control { input.throttle.clamp(-1., 1.) } else { 0. };
        let steer = if control { input.steer.clamp(-1., 1.) } else { 0. };

        self.offroad = self.lateral.abs() > HALF_WIDTH && !phased;
        let surface = if self.offroad { st.offroad } else { 1. };
        let slow = if self.slow_ticks > 0 { self.slow_factor } else { 1. };
        let boost = if self.boost_ticks > 0 { 1. + self.boost_power } else { 1. };
        let target = st.top_speed * surface * slow * boost;

        let mut vf = self.vel.dot(forward(self.yaw));

        // Drifting: hold the button through a corner at speed to charge a boost, release to cash it in.
        let holding = control && input.drift;
        if !self.drifting && holding && steer.abs() > 0.25 && vf > 0.55 * st.top_speed {
            self.drifting = true;
            self.drift_dir = steer.signum();
            self.drift_charge = 0.;
        }
        if self.drifting {
            if !holding || vf < 0.4 * st.top_speed {
                notes.boost_tier = self.end_drift(&st);
            } else {
                self.stats.drift_ticks += 1;
                if steer.abs() > 0.2 {
                    self.drift_charge += st.drift_rate * TICK;
                }
            }
        }

        // Steering: none when stopped, reversed when backing up, tighter and biased through a drift.
        let speed_factor = if vf.abs() < 0.01 { 0. } else { (vf.abs() / 8.).clamp(0., 1.) * vf.signum() };
        let turn = if self.spin_ticks > 0 {
            9.
        } else if self.drifting {
            (self.drift_dir * 0.55 + steer * 0.65) * st.handling * 1.25 * speed_factor
        } else {
            steer * st.handling * speed_factor
        };
        self.yaw = wrap_angle(self.yaw + turn * TICK);

        // Re-read the velocity in the new heading: the part along it is thrust, the part across it is slide.
        let (f, r) = (forward(self.yaw), right(self.yaw));
        vf = self.vel.dot(f);
        let mut vl = self.vel.dot(r);
        if throttle > 0. {
            if vf < target {
                vf += st.accel * throttle * TICK * (1. - 0.5 * (vf.max(0.) / target.max(1.)));
            }
        } else if throttle < 0. {
            if vf > 0. {
                vf -= st.accel * 2.5 * TICK;
            } else {
                vf = (vf - st.accel * 0.5 * TICK).max(-0.35 * st.top_speed);
            }
        } else {
            vf -= vf * 0.5 * TICK;
        }
        if self.boost_ticks > 0 && control {
            vf += 20. * TICK;
        }
        if vf > target {
            vf -= (vf - target) * 3. * TICK;
        }
        if self.spin_ticks > 0 {
            vf -= vf * 2. * TICK;
        }
        let grip = st.grip * if self.drifting { 0.2 } else { 1. };
        vl *= (-grip * TICK).exp();
        self.vel = f * vf + r * vl;
        self.pos = self.pos + self.vel * TICK;
        self.pos.1 = 0.;

        // The wall stands beyond the grass.
        let mut near = track.nearest(self.pos, self.hint);
        let out = self.pos - near.center;
        let distance = out.length();
        if distance > track.wall() {
            let normal = out * (1. / distance.max(1e-4));
            self.pos = near.center + normal * track.wall();
            let impact = self.vel.dot(normal);
            if impact > 0. {
                self.vel = self.vel - normal * (impact * 1.3);
                self.vel = self.vel * 0.92;
                if impact > 4. {
                    notes.wall_impact = Some(impact);
                    self.stats.wall_hits += 1;
                }
            }
            near = track.nearest(self.pos, near.index);
        }

        // Progress along the loop, never trusting a jump of more than a few metres.
        let delta = track.arc_delta(self.s, near.s);
        if delta.abs() < 40. {
            self.progress += delta;
        }
        self.hint = near.index;
        self.s = near.s;
        self.lateral = near.lateral;
        if self.progress > self.best_progress {
            self.best_progress = self.progress;
        }
        while self.finished_tick.is_none() && self.best_progress >= (self.lap + 1) as f32 * track.length {
            self.lap += 1;
            notes.laps_done += 1;
            self.stats.laps.push(race_tick);
            if self.lap >= laps {
                self.finished_tick = Some(race_tick);
                notes.finished = true;
            }
        }

        if racing && self.finished_tick.is_none() {
            self.stats.racing_ticks += 1;
            if self.offroad {
                self.stats.offroad_ticks += 1;
            }
        }
        let speed = self.vel.length();
        if speed > self.stats.top_speed {
            self.stats.top_speed = speed;
        }
        if racing {
            self.stats.distance += speed * TICK;
        }
        notes
    }
}
