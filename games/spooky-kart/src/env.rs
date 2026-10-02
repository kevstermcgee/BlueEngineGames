//! A reinforcement-learning style interface to Spooky Kart: `reset(seed) -> obs`, `step(actions) -> obs,
//! reward, terminated, truncated, events, info`.
//!
//! It is a layer on top of the race, not a second implementation of it: [`Env`] owns a [`Sim`], hands the
//! agent's kart the agent's input through the same `Inputs` a human uses, and calls `Sim::step`. Everything
//! else (the other karts, bots, perks, hazards, standings) is the game's own code, so a race run through the
//! env is byte-identical to the same race run by the window or a test (the tests prove it by comparing the
//! `state_hash` and the `RaceReport`).
//!
//! * **Agents and bots.** `agent_slots` are the karts driven from outside; they are `Driver::Human` in the
//!   sim. Every other kart is a bot at [`EnvConfig::difficulty`], driven inside `Sim::step` like in the game.
//!   When every agent kart has finished, the sim ends the race (its own rule for humans); a finished agent's
//!   kart is put on the game's autopilot and its later actions are ignored.
//! * **Observation** ([`Obs`]): only what a driver could perceive; see [`obs`] for exactly what.
//! * **Action** ([`Action`]): a [`KartInput`], or four plain floats `[throttle, steer, drift, perk]`.
//! * **Reward**: [`reward`], a pure function of the agent's counters before and after the step.
//! * **Terminated** when the sim says the race is over (every agent finished, or the game's own finish
//!   rule fired); **truncated** when `max_ticks` is reached first. After either, `step` returns
//!   [`EnvError::EpisodeOver`] until `reset`.
//! * **Time.** One step is `frame_skip` ticks (60 Hz) holding the same action; the perk button is pressed on
//!   the first tick only (it is an edge). The 4-second countdown is skipped by default.
//! * **Determinism.** The whole episode is a pure function of `(config, seed, actions)`: the same binary on
//!   the same platform reproduces it exactly (f32 trig may differ across platforms; see `docs/ENV.md`).
mod obs;
mod validate;

pub use obs::{
    ranks, HazardView, Lookahead, Obs, OtherView, OwnView, HAZARD_FIELDS, HAZARD_RANGE, LAYOUT as OBS_LAYOUT,
    LEN as OBS_LEN, LOOKAHEAD_M, MAX_HAZARDS, MAX_OTHERS, OTHER_FIELDS, OTHER_RANGE,
};
pub use validate::{validate, Check, ValidationReport};

use crate::bot::Difficulty;
use crate::character::{Character, ALL, MAX_RACERS};
use crate::kart::{Driver, Kart, KartInput};
use crate::sim::{self, HazardKind, Inputs, Phase, RaceReport, Sim, COUNTDOWN_TICKS};
use crate::Perk;
use serde::{Deserialize, Serialize};
use vesper3d::viewer::devkit::Simulation;

/// Why an env call was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum EnvError {
    /// The configuration cannot make a race.
    BadConfig(String),
    /// `step` got the wrong number of actions (one per agent slot).
    ActionCount { expected: usize, got: usize },
    /// An action held NaN or infinity (the sim would be poisoned by it).
    NonFinite { agent: usize, field: &'static str },
    /// An action vector had the wrong length.
    ActionLength { expected: usize, got: usize },
    /// The episode ended (terminated or truncated); call `reset`.
    EpisodeOver,
}

impl std::fmt::Display for EnvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvError::BadConfig(why) => write!(f, "bad env config: {why}"),
            EnvError::ActionCount { expected, got } => {
                write!(f, "expected {expected} actions (one per agent), got {got}")
            }
            EnvError::NonFinite { agent, field } => write!(f, "agent {agent}: action field '{field}' is not finite"),
            EnvError::ActionLength { expected, got } => write!(f, "an action vector has {expected} floats, got {got}"),
            EnvError::EpisodeOver => write!(f, "the episode is over: call reset"),
        }
    }
}

impl std::error::Error for EnvError {}

/// The action encoding: a [`KartInput`] as four floats.
///
/// | index | name | range | meaning |
/// |---|---|---|---|
/// | 0 | `throttle` | -1..1 | gas (+) and brake or reverse (-) |
/// | 1 | `steer` | -1..1 | left (-) to right (+) |
/// | 2 | `drift` | 0..1 | drift button held when >= 0.5 |
/// | 3 | `perk` | 0..1 | perk button pressed when >= 0.5 (first tick of a step only) |
///
/// Out-of-range numbers are clamped; NaN and infinity are refused (never silently turned into a number).
pub struct Action;

impl Action {
    pub const LEN: usize = 4;
    pub const LAYOUT: &'static [(&'static str, usize)] = &[("throttle", 1), ("steer", 1), ("drift", 1), ("perk", 1)];
    /// `(low, high)` per element.
    pub const BOUNDS: [(f32, f32); 4] = [(-1., 1.), (-1., 1.), (0., 1.), (0., 1.)];

    /// A [`KartInput`] from four floats, clamped. Fails on the wrong length or a non-finite value.
    pub fn from_vec(v: &[f32]) -> Result<KartInput, EnvError> {
        if v.len() != Self::LEN {
            return Err(EnvError::ActionLength { expected: Self::LEN, got: v.len() });
        }
        for (i, x) in v.iter().enumerate() {
            if !x.is_finite() {
                return Err(EnvError::NonFinite { agent: 0, field: Self::LAYOUT[i].0 });
            }
        }
        Ok(KartInput {
            throttle: v[0].clamp(-1., 1.),
            steer: v[1].clamp(-1., 1.),
            drift: v[2] >= 0.5,
            perk: v[3] >= 0.5,
        })
    }

    /// The four floats of an input (booleans as 0 or 1).
    pub fn to_vec(a: &KartInput) -> Vec<f32> {
        vec![a.throttle, a.steer, if a.drift { 1. } else { 0. }, if a.perk { 1. } else { 0. }]
    }

    /// An input with its analog values clamped to -1..1; fails if one is not finite.
    pub fn clamp(a: &KartInput) -> Result<KartInput, EnvError> {
        for (field, x) in [("throttle", a.throttle), ("steer", a.steer)] {
            if !x.is_finite() {
                return Err(EnvError::NonFinite { agent: 0, field });
            }
        }
        Ok(KartInput { throttle: a.throttle.clamp(-1., 1.), steer: a.steer.clamp(-1., 1.), ..*a })
    }
}

/// The reward weights. See [`reward`] for the formula.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RewardConfig {
    /// Per metre of progress along the track (forward positive, backward negative).
    pub progress: f32,
    /// Subtracted per wall hit.
    pub wall_hit: f32,
    /// Subtracted per tick on the grass.
    pub offroad_tick: f32,
    /// Added once on finishing, whatever the place.
    pub finish_base: f32,
    /// Added once on finishing, scaled from 1 for first place down to 0 for last.
    pub finish_place: f32,
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self { progress: 0.01, wall_hit: 0.5, offroad_tick: 0.002, finish_base: 2., finish_place: 8. }
    }
}

/// One agent's counters at one moment: everything [`reward`] reads (all existing `Kart` fields).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Status {
    pub slot: usize,
    /// `Kart.progress`: metres driven along the loop from the start line.
    pub progress: f32,
    /// `Kart.stats.wall_hits`.
    pub wall_hits: u32,
    /// `Kart.stats.offroad_ticks` (racing ticks spent on the grass before finishing).
    pub offroad_ticks: u32,
    /// The kart has crossed the finish line.
    pub finished: bool,
    /// Karts in the race.
    pub racers: usize,
}

impl Status {
    pub fn of(slot: usize, kart: &Kart, racers: usize) -> Self {
        Self {
            slot,
            progress: kart.progress,
            wall_hits: kart.stats.wall_hits,
            offroad_ticks: kart.stats.offroad_ticks,
            finished: kart.finished_tick.is_some(),
            racers,
        }
    }
}

/// The reward for one step of one agent, from its [`Status`] before and after and the events of the step.
///
/// ```text
/// r = progress   * (next.progress - prev.progress)             metres driven along the track (primary)
///   - wall_hit   * (wall hits this step)
///   - offroad_tick * (ticks on the grass this step)
///   + finish_base + finish_place * (racers - place) / (racers - 1)   once, on the step the kart finishes
/// r = 0 once the kart had already finished before the step
/// ```
///
/// With the default weights a full race of clean driving (two 900 m laps) is worth about 18 from progress, a
/// wall hit costs two seconds of top-speed progress, a second on the grass about 0.12 plus the lost speed, and
/// finishing pays 2 (last) to 10 (first). Not finishing (truncated, or the race ended around the kart) pays no
/// finish bonus. A pure function: same inputs, same output.
pub fn reward(cfg: &RewardConfig, prev: &Status, next: &Status, events: &[Event]) -> f32 {
    if prev.finished {
        return 0.;
    }
    let mut r = cfg.progress * (next.progress - prev.progress);
    r -= cfg.wall_hit * next.wall_hits.saturating_sub(prev.wall_hits) as f32;
    r -= cfg.offroad_tick * next.offroad_ticks.saturating_sub(prev.offroad_ticks) as f32;
    let finished_place = events.iter().find_map(|e| match e {
        Event::Finished { kart, place, .. } if *kart == next.slot => Some(*place),
        _ => None,
    });
    if let Some(place) = finished_place {
        let racers = next.racers.max(2) as f32;
        r += cfg.finish_base + cfg.finish_place * (racers - place as f32).max(0.) / (racers - 1.);
    }
    r
}

/// What happened during a step: the sim's own events as plain serialisable data (`sim::Event` is not
/// `Serialize`, and this keeps the file format ours). Indices are kart slots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    Count { n: u32 },
    Go,
    LapDone { kart: usize, lap: u32, ticks: u32 },
    Finished { kart: usize, place: u32, ticks: u32 },
    DriftBoost { kart: usize, tier: u32 },
    Bump { a: usize, b: usize, speed: f32 },
    WallHit { kart: usize, speed: f32 },
    PerkUsed { kart: usize, perk: Perk },
    HazardHit { kart: usize, kind: HazardKind },
    RaceOver,
}

impl From<&sim::Event> for Event {
    fn from(e: &sim::Event) -> Self {
        match *e {
            sim::Event::Count(n) => Event::Count { n },
            sim::Event::Go => Event::Go,
            sim::Event::LapDone { kart, lap, ticks } => Event::LapDone { kart, lap, ticks },
            sim::Event::Finished { kart, place, ticks } => Event::Finished { kart, place, ticks },
            sim::Event::DriftBoost { kart, tier } => Event::DriftBoost { kart, tier },
            sim::Event::Bump { a, b, speed } => Event::Bump { a, b, speed },
            sim::Event::WallHit { kart, speed } => Event::WallHit { kart, speed },
            sim::Event::PerkUsed { kart, perk } => Event::PerkUsed { kart, perk },
            sim::Event::HazardHit { kart, kind } => Event::HazardHit { kart, kind },
            sim::Event::RaceOver => Event::RaceOver,
        }
    }
}

/// How to build an episode. Serialisable: a recorded trajectory carries it in its header.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvConfig {
    /// The seed `Env::new` resets with (`reset(seed)` can use another).
    pub seed: u64,
    /// The characters on the grid, in grid order (1 to 8). Default: all eight, the game's grid.
    pub grid: Vec<Character>,
    /// Which grid slots are driven by the caller (at least one, no repeats). The rest are the game's bots.
    pub agent_slots: Vec<usize>,
    /// How the bots drive.
    pub difficulty: Difficulty,
    /// Truncate after this many ticks stepped since `reset` (the skipped countdown is not counted). A race
    /// is about 6,000 ticks; the default leaves room for a slow agent.
    pub max_ticks: u32,
    /// Ticks per `step`, holding the action (at least 1).
    pub frame_skip: u32,
    /// Run the 4-second countdown inside `reset` so the first observation is at the green light.
    pub skip_countdown: bool,
    pub reward: RewardConfig,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            grid: ALL.to_vec(),
            agent_slots: vec![0],
            difficulty: Difficulty::default(),
            max_ticks: 12_000,
            frame_skip: 1,
            skip_countdown: true,
            reward: RewardConfig::default(),
        }
    }
}

impl EnvConfig {
    pub fn check(&self) -> Result<(), EnvError> {
        let bad = |why: &str| Err(EnvError::BadConfig(why.into()));
        if self.grid.is_empty() || self.grid.len() > MAX_RACERS {
            return bad("the grid needs 1 to 8 characters");
        }
        if self.agent_slots.is_empty() {
            return bad("at least one agent slot is needed (a race of only bots ends by the game's own rule)");
        }
        let mut seen = vec![false; self.grid.len()];
        for &s in &self.agent_slots {
            if s >= self.grid.len() || std::mem::replace(&mut seen[s], true) {
                return bad("agent slots must be distinct and on the grid");
            }
        }
        if self.frame_skip == 0 || self.max_ticks == 0 {
            return bad("frame_skip and max_ticks must be at least 1");
        }
        Ok(())
    }
}

/// Facts about the episode that are NOT observations: they come from the privileged sim and are for
/// logging, evaluation and reward debugging. A policy that uses them is cheating.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    /// Sim tick (including a countdown that was not skipped).
    pub tick: u64,
    /// Ticks since the start signal.
    pub race_tick: u32,
    /// Ticks stepped since `reset`.
    pub ticks: u32,
    /// Steps since `reset`.
    pub steps: u64,
    /// Each agent's true race position, 1 = first (its final place once finished or the race is over).
    pub ranks: Vec<u32>,
    /// Each agent's kart has crossed the line.
    pub finished: Vec<bool>,
}

/// What a step returns. One entry per agent slot, in `agent_slots` order.
#[derive(Clone, Debug)]
pub struct StepResult {
    /// Observations after the step.
    pub obs: Vec<Obs>,
    pub reward: Vec<f32>,
    /// The race is over (see [`EnvConfig::agent_slots`]).
    pub terminated: bool,
    /// `max_ticks` reached while the race was still on.
    pub truncated: bool,
    /// Everything the sim reported during the step's ticks (all karts), oldest first.
    pub events: Vec<Event>,
    pub info: Info,
}

/// The environment: one race, reset by seed, stepped by the agent's actions.
pub struct Env {
    config: EnvConfig,
    sim: Sim,
    seed: u64,
    ticks: u32,
    steps: u64,
    terminated: bool,
    truncated: bool,
}

impl Env {
    /// Build the env and reset it with `config.seed`.
    pub fn new(config: EnvConfig) -> Result<Env, EnvError> {
        config.check()?;
        let seed = config.seed;
        let mut env = Env {
            sim: Self::make_sim(&config, seed),
            config,
            seed,
            ticks: 0,
            steps: 0,
            terminated: false,
            truncated: false,
        };
        env.reset(seed);
        Ok(env)
    }

    fn make_sim(config: &EnvConfig, seed: u64) -> Sim {
        let grid: Vec<(Character, Driver)> = config
            .grid
            .iter()
            .enumerate()
            .map(|(slot, c)| (*c, if config.agent_slots.contains(&slot) { Driver::Human } else { Driver::Bot }))
            .collect();
        Sim::with_difficulty(seed, &grid, config.difficulty)
    }

    /// Start a new race with `seed` and return the first observation of every agent.
    pub fn reset(&mut self, seed: u64) -> Vec<Obs> {
        self.sim = Self::make_sim(&self.config, seed);
        self.seed = seed;
        self.ticks = 0;
        self.steps = 0;
        self.terminated = false;
        self.truncated = false;
        if self.config.skip_countdown {
            let idle = Inputs::default();
            for _ in 0..COUNTDOWN_TICKS {
                self.sim.step(&idle);
            }
            self.sim.drain_events();
        }
        self.observe()
    }

    /// Advance `frame_skip` ticks (fewer if the race ends or `max_ticks` arrives) with one action per agent.
    ///
    /// Actions are clamped (see [`Action`]); a non-finite value or the wrong number of actions is an error
    /// and changes nothing. Rewards are computed from each agent's counters before and after all the ticks.
    pub fn step(&mut self, actions: &[KartInput]) -> Result<StepResult, EnvError> {
        if self.terminated || self.truncated {
            return Err(EnvError::EpisodeOver);
        }
        let slots = &self.config.agent_slots;
        if actions.len() != slots.len() {
            return Err(EnvError::ActionCount { expected: slots.len(), got: actions.len() });
        }
        let mut clean = Vec::with_capacity(actions.len());
        for (agent, a) in actions.iter().enumerate() {
            clean.push(Action::clamp(a).map_err(|e| match e {
                EnvError::NonFinite { field, .. } => EnvError::NonFinite { agent, field },
                other => other,
            })?);
        }
        let racers = self.sim.karts.len();
        let before: Vec<Status> = slots.iter().map(|&s| Status::of(s, &self.sim.karts[s], racers)).collect();
        let mut events = Vec::new();
        for k in 0..self.config.frame_skip {
            let mut inputs = Inputs::default();
            for (a, &slot) in clean.iter().zip(slots) {
                inputs.0[slot] = KartInput { perk: a.perk && k == 0, ..*a };
            }
            self.sim.step(&inputs);
            self.ticks += 1;
            events.extend(self.sim.drain_events().iter().map(Event::from));
            if self.sim.is_over() || self.ticks >= self.config.max_ticks {
                break;
            }
        }
        self.steps += 1;
        self.terminated = self.sim.is_over();
        self.truncated = !self.terminated && self.ticks >= self.config.max_ticks;
        let reward = before
            .iter()
            .map(|prev| {
                let next = Status::of(prev.slot, &self.sim.karts[prev.slot], racers);
                reward(&self.config.reward, prev, &next, &events)
            })
            .collect();
        Ok(StepResult {
            obs: self.observe(),
            reward,
            terminated: self.terminated,
            truncated: self.truncated,
            events,
            info: self.info(),
        })
    }

    /// The agents' current observations (what `reset` and `step` return).
    pub fn observe(&self) -> Vec<Obs> {
        let order = ranks(&self.sim);
        self.config.agent_slots.iter().map(|&s| Obs::observe_with(&self.sim, s, &order)).collect()
    }

    pub fn info(&self) -> Info {
        let order = ranks(&self.sim);
        let slots = &self.config.agent_slots;
        Info {
            tick: self.sim.tick,
            race_tick: self.sim.race_tick,
            ticks: self.ticks,
            steps: self.steps,
            ranks: slots
                .iter()
                .map(|&s| if self.sim.karts[s].place > 0 { self.sim.karts[s].place } else { order[s] })
                .collect(),
            finished: slots.iter().map(|&s| self.sim.karts[s].finished_tick.is_some()).collect(),
        }
    }

    pub fn config(&self) -> &EnvConfig {
        &self.config
    }

    pub fn agent_slots(&self) -> &[usize] {
        &self.config.agent_slots
    }

    /// The seed of the current episode.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Ticks stepped since `reset`.
    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// The episode is over (terminated or truncated): `step` needs a `reset` first.
    pub fn is_done(&self) -> bool {
        self.terminated || self.truncated
    }

    /// The whole race, privileged. Reading it is how the reference bot (`bot::drive_as`) works and how the
    /// tests check things; a learning policy must use [`Obs`] only.
    pub fn sim(&self) -> &Sim {
        &self.sim
    }

    /// Mutable access to the race, for tests and scenario setup only. Anything changed through it breaks
    /// replay equivalence with a recorded trajectory.
    #[doc(hidden)]
    pub fn sim_mut(&mut self) -> &mut Sim {
        &mut self.sim
    }

    /// The sim's state hash (the same one saves and the determinism tests use).
    pub fn state_hash(&self) -> u64 {
        self.sim.state_hash()
    }

    /// The race report as it stands (final once `terminated`).
    pub fn report(&self) -> RaceReport {
        self.sim.report()
    }

    /// Whether the countdown phase is still running (only when `skip_countdown` is off).
    pub fn in_countdown(&self) -> bool {
        matches!(self.sim.phase, Phase::Countdown(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(progress: f32, walls: u32, offroad: u32, finished: bool) -> Status {
        Status { slot: 2, progress, wall_hits: walls, offroad_ticks: offroad, finished, racers: 8 }
    }

    #[test]
    fn progress_is_the_primary_reward_and_driving_backwards_costs() {
        let cfg = RewardConfig::default();
        let fwd = reward(&cfg, &status(10., 0, 0, false), &status(10.4, 0, 0, false), &[]);
        assert!((fwd - 0.004).abs() < 1e-6, "{fwd}");
        let back = reward(&cfg, &status(10., 0, 0, false), &status(9.6, 0, 0, false), &[]);
        assert!((back + 0.004).abs() < 1e-6, "{back}");
        assert_eq!(reward(&cfg, &status(10., 0, 0, false), &status(10., 0, 0, false), &[]), 0.);
    }

    #[test]
    fn walls_and_grass_are_small_penalties() {
        let cfg = RewardConfig::default();
        let wall = reward(&cfg, &status(5., 3, 0, false), &status(5., 4, 0, false), &[]);
        assert!((wall + 0.5).abs() < 1e-6);
        let grass = reward(&cfg, &status(5., 0, 100, false), &status(5., 0, 160, false), &[]);
        assert!((grass + 0.12).abs() < 1e-6, "{grass}");
        // A step with both: the penalties add, and counters never run backwards.
        let both = reward(&cfg, &status(5., 0, 0, false), &status(6., 1, 2, false), &[]);
        assert!((both - (0.01 - 0.5 - 0.004)).abs() < 1e-6, "{both}");
        assert_eq!(reward(&cfg, &status(5., 4, 9, false), &status(5., 3, 8, false), &[]), 0.);
    }

    #[test]
    fn finishing_pays_by_place_once_and_only_to_the_finisher() {
        let cfg = RewardConfig::default();
        let done = |place| vec![Event::Finished { kart: 2, place, ticks: 6000 }];
        let step = |place| reward(&cfg, &status(1000., 0, 0, false), &status(1000., 0, 0, true), &done(place));
        assert!((step(1) - 10.).abs() < 1e-5, "{}", step(1));
        assert!((step(8) - 2.).abs() < 1e-5, "{}", step(8));
        assert!(step(1) > step(4) && step(4) > step(8));
        // Somebody else finishing pays nothing, and a kart that already finished earns nothing more.
        let other = [Event::Finished { kart: 5, place: 1, ticks: 1 }];
        assert_eq!(reward(&cfg, &status(1., 0, 0, false), &status(1., 0, 0, false), &other), 0.);
        assert_eq!(reward(&cfg, &status(1000., 0, 0, true), &status(1010., 5, 5, true), &done(1)), 0.);
    }

    #[test]
    fn a_race_of_one_does_not_divide_by_zero() {
        let cfg = RewardConfig::default();
        let solo = Status { racers: 1, ..status(0., 0, 0, false) };
        let ev = [Event::Finished { kart: 2, place: 1, ticks: 1 }];
        let r = reward(&cfg, &solo, &Status { finished: true, ..solo }, &ev);
        assert!(r.is_finite() && r > 0.);
    }

    #[test]
    fn actions_clamp_and_refuse_nan() {
        let a = Action::from_vec(&[3., -9., 0.7, 0.2]).unwrap();
        assert_eq!(a, KartInput { throttle: 1., steer: -1., drift: true, perk: false });
        assert_eq!(Action::to_vec(&a), vec![1., -1., 1., 0.]);
        assert!(matches!(Action::from_vec(&[0., f32::NAN, 0., 0.]), Err(EnvError::NonFinite { field: "steer", .. })));
        assert!(Action::from_vec(&[f32::INFINITY, 0., 0., 0.]).is_err());
        assert!(matches!(Action::from_vec(&[0., 0.]), Err(EnvError::ActionLength { .. })));
        assert_eq!(Action::LAYOUT.iter().map(|l| l.1).sum::<usize>(), Action::LEN);
        let nan = KartInput { throttle: f32::NAN, ..Default::default() };
        assert!(Action::clamp(&nan).is_err());
    }

    #[test]
    fn the_obs_layout_names_bounds_and_vector_agree() {
        assert_eq!(OBS_LAYOUT.iter().map(|l| l.1).sum::<usize>(), OBS_LEN);
        assert_eq!(Obs::names().len(), OBS_LEN);
        assert_eq!(Obs::bounds().len(), OBS_LEN);
        let env = Env::new(EnvConfig::default()).unwrap();
        let v = env.observe()[0].to_vec();
        assert_eq!(v.len(), OBS_LEN);
        assert!(v.iter().all(|x| x.is_finite()));
        for (x, (lo, hi)) in v.iter().zip(Obs::bounds()) {
            assert!((lo..=hi).contains(x));
        }
        let names = Obs::names();
        assert_eq!(names[0], "speed");
        assert!(names.contains(&"others.3.rank_delta".to_string()));
        assert!(names.contains(&"hazards.0.bone".to_string()));
        assert_eq!(names.iter().collect::<std::collections::HashSet<_>>().len(), OBS_LEN, "names are unique");
    }

    #[test]
    fn config_checks_refuse_what_cannot_race() {
        let ok = EnvConfig::default();
        assert!(ok.check().is_ok());
        assert!(EnvConfig { agent_slots: vec![], ..ok.clone() }.check().is_err());
        assert!(EnvConfig { agent_slots: vec![8], ..ok.clone() }.check().is_err());
        assert!(EnvConfig { agent_slots: vec![1, 1], ..ok.clone() }.check().is_err());
        assert!(EnvConfig { grid: vec![], ..ok.clone() }.check().is_err());
        assert!(EnvConfig { frame_skip: 0, ..ok.clone() }.check().is_err());
        assert!(Env::new(EnvConfig { max_ticks: 0, ..ok }).is_err());
    }
}
