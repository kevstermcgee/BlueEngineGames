//! Wobble Tower's rules: a pure, deterministic, fixed-step simulation with no window, no sound device and no wall
//! clock. A crane swings the next crate over a small platform; you drop it and the crates are real rigid bodies
//! (the engine's `rapier`, see `physics.rs`) confined to the X-Y plane: they stack, slide, tip and topple under
//! gravity, friction and mass, while gusts of wind that grow with the tower lean on them. A crate that falls off
//! costs a life. Height and neat stacking score.
pub mod physics;

use physics::{Body, BodyState, Material, Physics};
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::devkit::{Rng, SavePolicy, Simulation, Snapshot, StateHasher, TICK};

/// Half the platform's width; its top surface is y = 0.
pub const PLATFORM_HALF: f32 = 2.2;
/// Half the depth (z) of the platform and every crate.
pub const DEPTH: f32 = 0.9;
/// How far the crane swings either way.
pub const CRANE_RANGE: f32 = 4.6;
/// The crane hangs this far above the tower's top.
pub const CRANE_ABOVE: f32 = 4.4;
pub const LIVES: u32 = 3;
const GRAVITY: V = V(0., -9.81, 0.);
/// Ticks a crate must rest for before it counts as stacked.
const SETTLE_TICKS: u32 = 45;
/// Ticks until the next crate hangs from the hook after a drop.
const RELOAD_TICKS: u32 = 50;
/// A drop this close to the crate beneath it is a perfect one.
const PERFECT: f32 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    /// Plain timber.
    Crate,
    /// Dense and grippy: it presses the stack down and hardly slides.
    Heavy,
    /// Light and slippery: easy to push around in the wind.
    Light,
}

impl Kind {
    pub fn material(self) -> Material {
        match self {
            Kind::Crate => Material { friction: 0.8, restitution: 0.05, density: 1.0, ccd: true, ..Default::default() },
            Kind::Heavy => Material { friction: 0.95, restitution: 0.0, density: 2.5, ccd: true, ..Default::default() },
            Kind::Light => Material { friction: 0.5, restitution: 0.1, density: 0.4, ccd: true, ..Default::default() },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrateState {
    /// Falling or still moving.
    Dropped,
    /// Counted as part of the stack.
    Settled,
    /// Gone over the edge (its body is removed).
    Fell,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrateInfo {
    pub body: usize,
    pub half: V,
    pub kind: Kind,
    pub state: CrateState,
    /// Ticks spent nearly still while touching something.
    pub resting: u32,
    /// Where it was released from, for rebuilding a save.
    pub spawn: V,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    /// Drop button held (a press drops; holding does not drop again).
    pub drop: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Drop { at: V, kind: Kind },
    Land { at: V, kind: Kind },
    Settled { at: V, perfect: bool, points: u32 },
    Fell { at: V, lives_left: u32 },
    Gust { strength: f32 },
    GameOver { score: u64, height: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Playing,
    Over,
}

pub struct Sim {
    pub tick: u64,
    pub phys: Physics,
    pub crates: Vec<CrateInfo>,
    pub carriage_x: f32,
    carriage_dir: f32,
    /// The crate hanging on the hook: half extents and kind.
    pub carried: Option<(V, Kind)>,
    reload: u32,
    prev_drop: bool,
    pub score: u64,
    pub lives: u32,
    /// Crates counted into the stack.
    pub stacked: u32,
    pub perfects: u32,
    /// Top surface of the highest settled crate (0 = the platform).
    pub height: f32,
    /// Current horizontal wind acceleration (m/s^2, +x is to the right) and where it is heading.
    pub wind: f32,
    wind_target: f32,
    wind_timer: u32,
    pub phase: Phase,
    rng: Rng,
    events: Vec<Event>,
}

fn build_platform() -> Physics {
    let mut phys = Physics::new(GRAVITY);
    phys.fixed_box(
        V(0., -0.6, 0.),
        V(PLATFORM_HALF, 0.6, DEPTH + 0.1),
        0.,
        Material { friction: 0.9, restitution: 0., ..Default::default() },
    );
    phys
}

fn spawn_body(phys: &mut Physics, c: &CrateInfo) -> Body {
    phys.dynamic_box_planar(c.spawn, c.half, 0., c.kind.material())
}

impl Sim {
    /// A fresh game; the same seed always plays out the same way.
    pub fn new(seed: u64) -> Self {
        let mut sim = Self {
            tick: 0,
            phys: build_platform(),
            crates: Vec::new(),
            carriage_x: 0.,
            carriage_dir: 1.,
            carried: None,
            reload: 30,
            prev_drop: false,
            score: 0,
            lives: LIVES,
            stacked: 0,
            perfects: 0,
            height: 0.,
            wind: 0.,
            wind_target: 0.,
            wind_timer: 60 * 5,
            phase: Phase::Playing,
            rng: Rng::new(seed),
            events: Vec::new(),
        };
        sim.carried = Some(sim.next_crate());
        sim
    }

    fn next_crate(&mut self) -> (V, Kind) {
        let widths = [0.5, 0.7, 0.9];
        let heights = [0.4, 0.5];
        let hx = widths[self.rng.below(3)];
        let hy = heights[self.rng.below(2)];
        let roll = self.rng.f32();
        let kind = if roll < 0.7 {
            Kind::Crate
        } else if roll < 0.85 {
            Kind::Heavy
        } else {
            Kind::Light
        };
        (V(hx, hy, DEPTH), kind)
    }

    /// Where the crane hangs (x, y): the carriage follows the tower up.
    pub fn crane_at(&self) -> V {
        V(self.carriage_x, self.height + CRANE_ABOVE, 0.)
    }

    pub fn crane_speed(&self) -> f32 {
        (2.2 + 0.14 * self.stacked as f32).min(5.5)
    }

    /// The x of the middle of the highest settled crate (0 on the bare platform).
    pub fn top_center(&self) -> f32 {
        self.crates
            .iter()
            .filter(|c| c.state == CrateState::Settled)
            .map(|c| (self.phys.position(Body(c.body)), c.half))
            .max_by(|a, b| (a.0 .1 + a.1 .1).total_cmp(&(b.0 .1 + b.1 .1)))
            .map_or(0., |(p, _)| p.0)
    }

    /// Where a careful player aims: the average x of the top few settled crates (their balance point), so a crate
    /// that has slid to one side gets corrected instead of piled on.
    pub fn balance_point(&self) -> f32 {
        let mut top: Vec<(f32, f32)> = self
            .crates
            .iter()
            .filter(|c| c.state == CrateState::Settled)
            .map(|c| {
                let p = self.phys.position(Body(c.body));
                (p.1 + c.half.1, p.0)
            })
            .collect();
        top.sort_by(|a, b| b.0.total_cmp(&a.0));
        let few: Vec<f32> = top.iter().take(3).map(|t| t.1).collect();
        if few.is_empty() {
            0.
        } else {
            few.iter().sum::<f32>() / few.len() as f32
        }
    }

    fn drop_crate(&mut self) {
        let Some((half, kind)) = self.carried.take() else { return };
        let at = V(self.carriage_x, self.crane_at().1 - 1.0, 0.);
        let info = CrateInfo { body: 0, half, kind, state: CrateState::Dropped, resting: 0, spawn: at };
        let body = spawn_body(&mut self.phys, &info);
        self.crates.push(CrateInfo { body: body.0, ..info });
        self.reload = RELOAD_TICKS;
        self.events.push(Event::Drop { at, kind });
    }

    /// Advance exactly one 60 Hz tick. A finished game ignores further input.
    pub fn step(&mut self, input: &Input) {
        if self.phase == Phase::Over {
            return;
        }
        self.tick += 1;
        // The crane swings; a new crate hangs on the hook after a short reload.
        self.carriage_x += self.carriage_dir * self.crane_speed() * TICK;
        if self.carriage_x.abs() > CRANE_RANGE {
            self.carriage_x = self.carriage_x.clamp(-CRANE_RANGE, CRANE_RANGE);
            self.carriage_dir = -self.carriage_dir;
        }
        if self.carried.is_none() {
            self.reload = self.reload.saturating_sub(1);
            if self.reload == 0 {
                self.carried = Some(self.next_crate());
            }
        }
        if input.drop && !self.prev_drop {
            self.drop_crate();
        }
        self.prev_drop = input.drop;

        // Wind: none until the tower has some height, then gusts whose strength grows with the stack.
        if self.stacked >= 3 {
            self.wind_timer = self.wind_timer.saturating_sub(1);
            if self.wind_timer == 0 {
                let strength = (0.22 * (self.stacked as f32 - 2.)).min(2.4);
                self.wind_target = if self.wind_target == 0. { strength * self.rng.sign() } else { 0. };
                self.wind_timer = 60 * self.rng.range(2., 4.5) as u32;
                if self.wind_target != 0. {
                    self.events.push(Event::Gust { strength: self.wind_target });
                }
            }
        }
        self.wind += (self.wind_target - self.wind).clamp(-1.2 * TICK, 1.2 * TICK);
        if self.wind != 0. {
            for c in self.crates.iter().filter(|c| c.state != CrateState::Fell) {
                let body = Body(c.body);
                let mass = self.phys.mass(body);
                self.phys.push(body, V(self.wind * mass, 0., 0.));
            }
        }

        self.phys.step();
        self.bookkeeping();
    }

    /// Landings, settling, falls and the score: the rules the physics does not decide.
    fn bookkeeping(&mut self) {
        let top_before = self.top_center();
        for i in 0..self.crates.len() {
            let c = self.crates[i];
            if c.state == CrateState::Fell {
                continue;
            }
            let body = Body(c.body);
            let p = self.phys.position(body);
            if p.1 < -1.0 || p.0.abs() > 9. {
                self.phys.remove(body);
                self.crates[i].state = CrateState::Fell;
                self.lives = self.lives.saturating_sub(1);
                self.events.push(Event::Fell { at: p, lives_left: self.lives });
                if self.lives == 0 {
                    self.phase = Phase::Over;
                    self.events.push(Event::GameOver { score: self.score, height: self.height });
                }
                continue;
            }
            if c.state != CrateState::Dropped {
                continue;
            }
            let touching = !self.phys.touching(body).is_empty();
            if touching && c.resting == 0 && self.phys.speed(body) > 0.5 {
                self.events.push(Event::Land { at: p, kind: c.kind });
            }
            if touching && self.phys.speed(body) < 0.3 {
                self.crates[i].resting += 1;
            } else {
                self.crates[i].resting = 0;
            }
            if self.crates[i].resting >= SETTLE_TICKS {
                self.crates[i].state = CrateState::Settled;
                self.stacked += 1;
                let perfect = (p.0 - top_before).abs() < PERFECT && self.stacked > 1;
                let old = self.height;
                self.height = self
                    .crates
                    .iter()
                    .filter(|c| c.state == CrateState::Settled)
                    .map(|c| self.phys.position(Body(c.body)).1 + c.half.1)
                    .fold(0., f32::max);
                let gained = (10. * (self.height - old).max(0.)).round() as u32;
                let points = 100 + gained + if perfect { 150 } else { 0 };
                if perfect {
                    self.perfects += 1;
                }
                self.score += points as u64;
                self.events.push(Event::Settled { at: p, perfect, points });
            }
        }
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, input: &Input) {
        Sim::step(self, input);
    }
    fn state_hash(&self) -> u64 {
        self.hash_parts()
            .iter()
            .fold(StateHasher::new(), |mut h, (_, part)| {
                h.u64(*part);
                h
            })
            .finish()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        let one = |f: &dyn Fn(&mut StateHasher)| {
            let mut h = StateHasher::new();
            f(&mut h);
            h.finish()
        };
        vec![
            (
                "rules",
                one(&|h| {
                    h.u64(self.tick).u64(self.score).u32(self.lives).u32(self.stacked).u32(self.perfects);
                    h.f32(self.height).f32(self.wind).f32(self.wind_target).u32(self.wind_timer);
                    h.f32(self.carriage_x).f32(self.carriage_dir).u32(self.reload).bool(self.prev_drop);
                    match self.carried {
                        Some((half, kind)) => h.bool(true).f32(half.0).f32(half.1).u32(kind as u32),
                        None => h.bool(false),
                    };
                    h.bool(self.phase == Phase::Over);
                }),
            ),
            (
                "crates",
                one(&|h| {
                    for c in &self.crates {
                        h.u64(c.body as u64).u32(c.kind as u32).u32(c.state as u32).u32(c.resting);
                        h.f32(c.half.0).f32(c.half.1);
                    }
                }),
            ),
            ("bodies", one(&|h| self.phys.hash_into(h))),
            (
                "rng",
                one(&|h| {
                    h.u64(self.rng.state());
                }),
            ),
        ]
    }
}

/// Everything that decides where the game goes next. The bodies carry poses and velocities only (the physics
/// library's contact cache is not saved), so a load is a fair continuation, not a bit-exact one
/// (`SavePolicy::PhysicsContinuation`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub bodies: Vec<BodyState>,
    pub crates: Vec<CrateInfo>,
    pub carriage_x: f32,
    pub carriage_dir: f32,
    pub carried: Option<(V, Kind)>,
    pub reload: u32,
    pub prev_drop: bool,
    pub score: u64,
    pub lives: u32,
    pub stacked: u32,
    pub perfects: u32,
    pub height: f32,
    pub wind: f32,
    pub wind_target: f32,
    pub wind_timer: u32,
    pub over: bool,
    pub rng: Rng,
}

impl Snapshot for Sim {
    const KIND: &'static str = "wobble-tower";
    const POLICY: SavePolicy = SavePolicy::PhysicsContinuation;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            bodies: self.phys.states(),
            crates: self.crates.clone(),
            carriage_x: self.carriage_x,
            carriage_dir: self.carriage_dir,
            carried: self.carried,
            reload: self.reload,
            prev_drop: self.prev_drop,
            score: self.score,
            lives: self.lives,
            stacked: self.stacked,
            perfects: self.perfects,
            height: self.height,
            wind: self.wind,
            wind_target: self.wind_target,
            wind_timer: self.wind_timer,
            over: self.phase == Phase::Over,
            rng: self.rng.clone(),
        }
    }
    /// Refuse a state this game could not have produced; the caller then keeps the running game.
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        let finite = |v: f32| v.is_finite();
        if s.lives > LIVES
            || s.carriage_x.abs() > CRANE_RANGE + 0.01
            || !(s.carriage_dir == 1. || s.carriage_dir == -1.)
            || !finite(s.height)
            || !finite(s.wind)
            || !finite(s.wind_target)
            || s.crates.len() > 400
        {
            return Err("the save has values this game cannot reach".into());
        }
        if s.crates.iter().any(|c| {
            !c.half.0.is_finite() || !c.half.1.is_finite() || c.half.0 <= 0. || c.half.1 <= 0. || c.half.0 > 2.
        }) {
            return Err("the save has a crate this game could not have made".into());
        }
        // Rebuild the world from scratch (the library's contact cache is not in the save, so nothing of the
        // old world may survive a load): the platform, then every crate in the order it was dropped, removing
        // the ones that had fallen so slot numbers line up; then put the saved poses back.
        let mut phys = build_platform();
        for (i, c) in s.crates.iter().enumerate() {
            let body = spawn_body(&mut phys, c);
            if body.0 != c.body || body.0 != i + 1 {
                return Err("the save's crates do not line up with its bodies".into());
            }
            if c.state == CrateState::Fell {
                phys.remove(body);
            }
        }
        if !phys.restore(&s.bodies) {
            return Err("the save's bodies do not match this tower".into());
        }
        self.phys = phys;
        self.tick = s.tick;
        self.crates = s.crates;
        self.carriage_x = s.carriage_x;
        self.carriage_dir = s.carriage_dir;
        self.carried = s.carried;
        self.reload = s.reload;
        self.prev_drop = s.prev_drop;
        self.score = s.score;
        self.lives = s.lives;
        self.stacked = s.stacked;
        self.perfects = s.perfects;
        self.height = s.height;
        self.wind = s.wind;
        self.wind_target = s.wind_target;
        self.wind_timer = s.wind_timer;
        self.phase = if s.over { Phase::Over } else { Phase::Playing };
        self.rng = s.rng;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}

/// A simple player: drops each crate when it is over the tower's balance point.
pub fn autoplay(sim: &Sim) -> Input {
    let lead = sim.crane_speed() * TICK;
    let over_target = (sim.carriage_x - sim.balance_point()).abs() < 0.08 + lead;
    Input { drop: sim.carried.is_some() && over_target && sim.phase == Phase::Playing }
}
