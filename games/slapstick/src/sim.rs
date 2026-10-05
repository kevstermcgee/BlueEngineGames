//! The match: a pure, deterministic, fixed-step simulation of two paddles and a puck on a flat table.
//! Humans supply one [`Input`] a tick; a released (bot-driven) paddle is driven from inside
//! (`bot::paddle_ai`), so it replays identically and is part of a save.
//!
//! No rigid-body physics library is used here (see `AGENTS.md`'s "No rapier3d, deliberately" and
//! `~/BlueEngine/docs/adr/0018-physics-save-contract.md`): the puck and paddles are simple hand-rolled
//! vectors, adapted from Spooky Kart's `collide_karts`, so a save can promise [`SavePolicy::Exact`] — a
//! resumed match is bit-identical to the one that was never interrupted, which is exactly what netplay's
//! client-side prediction demands of every tick, not just the ones around a save.
use crate::bot;
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::devkit::{Rng, SavePolicy, Simulation, Snapshot, StateHasher, TICK};

/// Table half-width: X spans `[-TABLE_HALF_WIDTH, TABLE_HALF_WIDTH]`.
pub const TABLE_HALF_WIDTH: f32 = 0.6;
/// Table half-length: Z spans `[-TABLE_HALF_LENGTH, TABLE_HALF_LENGTH]`. Paddle 0 defends -Z, paddle 1
/// defends +Z.
pub const TABLE_HALF_LENGTH: f32 = 1.2;
/// Half-width of the goal mouth cut into each end wall, centred on X = 0.
pub const GOAL_HALF_WIDTH: f32 = 0.2;
pub const PUCK_RADIUS: f32 = 0.05;
pub const PADDLE_RADIUS: f32 = 0.11;
pub const PUCK_MASS: f32 = 1.0;
pub const PADDLE_MASS: f32 = 6.0;
pub const PADDLE_MAX_SPEED: f32 = 7.0;
pub const PUCK_MAX_SPEED: f32 = 18.0;
pub const WALL_RESTITUTION: f32 = 0.92;
pub const PADDLE_PUCK_RESTITUTION: f32 = 1.6;
/// First to this many goals wins (unless the match ends in sudden death first).
pub const WIN_GOALS: u32 = 7;
/// Ticks the puck stays frozen at the start of a serve.
pub const SERVE_TICKS: u32 = 60;
/// Soft time cap (8 minutes at 60 Hz): the leader wins immediately if untied, otherwise sudden death.
pub const MATCH_TIME_LIMIT_TICKS: u64 = 28_800;

/// One paddle: its real position/velocity, the target the last [`Input`] asked for, and whether the
/// bot AI (not a human) is driving it (see [`NetGame::release`](crate::netgame::AirHockeyGame::release)).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paddle {
    pub pos: V,
    pub vel: V,
    pub target: V,
    pub ai: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Puck {
    pub pos: V,
    pub vel: V,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    /// The puck is frozen at `server`'s end; paddles may still move. Resumes `Playing` at 0.
    Serve {
        ticks_left: u32,
        server: usize,
    },
    Playing,
    GameOver {
        winner: usize,
    },
}

/// One tick of a player's intent: an **absolute**, normalized `[-1, 1]` target within the paddle's own
/// half (not a delta), so a resent or duplicated input packet is harmless. See `apply_paddle_input`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub target_x: f32,
    pub target_z: f32,
}

/// One tick of input for both paddles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Inputs(pub [Input; 2]);

/// Something that happened this tick; the window reacts with sound and effects, and the server logs them.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A new serve has begun (the very first one, or after a goal).
    Serve {
        server: usize,
    },
    Goal {
        scorer: usize,
        score: [u32; 2],
    },
    PaddleHit {
        paddle: usize,
        speed: f32,
    },
    WallHit {
        speed: f32,
    },
    GameOver {
        winner: usize,
    },
}

/// The evidence one match leaves, for `matches.jsonl`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchReport {
    pub game: String,
    pub seed: u64,
    pub score: [u32; 2],
    pub winner: Option<usize>,
    pub ticks: u64,
    pub sudden_death: bool,
}

pub struct Sim {
    pub tick: u64,
    pub phase: Phase,
    pub paddles: [Paddle; 2],
    pub puck: Puck,
    pub score: [u32; 2],
    pub seed: u64,
    /// Set once the match reaches [`MATCH_TIME_LIMIT_TICKS`] while tied: from then on, any goal ends it.
    sudden_death: bool,
    rng: Rng,
    events: Vec<Event>,
}

/// Where the puck is frozen during a serve: the server's own half, at the table's centre line in X.
fn serve_z_for(server: usize) -> f32 {
    if server == 0 {
        -TABLE_HALF_LENGTH * 0.5
    } else {
        TABLE_HALF_LENGTH * 0.5
    }
}

/// The single source of truth for turning a target into paddle motion: clamps the target into the
/// paddle's own half and the table bounds (inset by [`PADDLE_RADIUS`]), then moves at up to
/// [`PADDLE_MAX_SPEED`] per tick. `Sim::step` (authoritative) and `AirHockeyView`'s own-paddle
/// prediction both call this, so the two can never diverge in how paddle movement is computed. Clamped
/// here, not trusted from the wire: a hostile or buggy `Input` cannot walk a paddle out of its half.
pub fn apply_paddle_input(paddle: &mut Paddle, input: &Input, slot: usize) {
    let x_max = TABLE_HALF_WIDTH - PADDLE_RADIUS;
    let target_x = input.target_x.clamp(-1., 1.) * x_max;
    let target_z_norm = input.target_z.clamp(-1., 1.);
    // `back` is this paddle's own backline (inset from the table edge); `center` is the table's centre
    // line (Z = 0), which both paddles may approach but never cross.
    let (back, center) =
        if slot == 0 { (-(TABLE_HALF_LENGTH - PADDLE_RADIUS), 0.) } else { (TABLE_HALF_LENGTH - PADDLE_RADIUS, 0.) };
    let target_z = back + (center - back) * (target_z_norm + 1.) * 0.5;
    let target = V(target_x, 0., target_z);
    paddle.target = target;
    let delta = target - paddle.pos;
    let distance = delta.length();
    let max_step = PADDLE_MAX_SPEED * TICK;
    let new_pos =
        if distance > max_step && distance > 1e-6 { paddle.pos + delta * (max_step / distance) } else { target };
    paddle.vel = (new_pos - paddle.pos) * (1. / TICK);
    paddle.pos = new_pos;
}

/// Circle-circle mass-weighted push-out + closing-velocity impulse, adapted from `collide_karts`
/// (`~/SpookyKart/src/sim.rs`). Deliberately **not** symmetric like `collide_karts`: only the puck
/// receives the impulse, because a human/bot-driven paddle is a kinematic target-tracker, not a free
/// body receiving reaction forces. Returns the closing speed (usable for a sound's intensity) when a
/// collision actually happened.
#[allow(clippy::neg_multiply)] // kept as `* -1.` to match the plan's formula (and `collide_karts`) verbatim
fn collide_puck_paddle(puck: &mut Puck, paddle: &mut Paddle) -> Option<f32> {
    let delta = puck.pos - paddle.pos;
    let distance = delta.length();
    let min_dist = PUCK_RADIUS + PADDLE_RADIUS;
    if distance >= min_dist {
        return None;
    }
    let normal = if distance > 1e-3 { delta * (1. / distance) } else { V(1., 0., 0.) };
    let overlap = min_dist - distance;
    let (wa, wb) = (PADDLE_MASS / (PUCK_MASS + PADDLE_MASS), PUCK_MASS / (PUCK_MASS + PADDLE_MASS));
    puck.pos = puck.pos + normal * (overlap * wa);
    paddle.pos = paddle.pos - normal * (overlap * wb);
    let closing = (paddle.vel - puck.vel).dot(normal) * -1.;
    if closing <= 0. {
        return None; // separating already: no impulse
    }
    let j = closing * PADDLE_PUCK_RESTITUTION / (1. / PUCK_MASS + 1. / PADDLE_MASS);
    puck.vel = puck.vel + normal * (j / PUCK_MASS);
    Some(closing)
}

/// Reflect only the puck's velocity component along `normal`; the tangential component is untouched, so
/// a glancing hit keeps most of its speed while a square hit loses the most.
fn bounce_off_wall(puck: &mut Puck, normal: V) {
    let v_n = puck.vel.dot(normal);
    if v_n >= 0. {
        return; // already moving away from the wall
    }
    let v_normal = normal * v_n;
    let v_tangent = puck.vel - v_normal;
    puck.vel = v_tangent - v_normal * WALL_RESTITUTION;
}

impl Sim {
    /// A fresh match: the first serve is a seeded coin flip.
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let first_server = rng.below(2);
        let paddles = [
            Paddle { pos: V(0., 0., -TABLE_HALF_LENGTH * 0.5), vel: V(0., 0., 0.), target: V(0., 0., 0.), ai: false },
            Paddle { pos: V(0., 0., TABLE_HALF_LENGTH * 0.5), vel: V(0., 0., 0.), target: V(0., 0., 0.), ai: false },
        ];
        let puck = Puck { pos: V(0., 0., serve_z_for(first_server)), vel: V(0., 0., 0.) };
        Self {
            tick: 0,
            phase: Phase::Serve { ticks_left: SERVE_TICKS, server: first_server },
            paddles,
            puck,
            score: [0, 0],
            seed,
            sudden_death: false,
            rng,
            events: vec![Event::Serve { server: first_server }],
        }
    }

    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::GameOver { .. })
    }

    /// Whether the match has passed the soft time cap while tied: from here on, any goal ends it.
    pub fn sudden_death(&self) -> bool {
        self.sudden_death
    }

    /// Advance exactly one 60 Hz tick. A finished match ignores all input and steps nothing further
    /// (the same "finished round ignores further input" pattern every BlueEngine game uses).
    pub fn step(&mut self, inputs: &Inputs) {
        if matches!(self.phase, Phase::GameOver { .. }) {
            return;
        }
        self.tick += 1;

        // The soft time cap: checked only while actually playing, so a match paused mid-serve at the
        // exact tick still gets its chance to end once play resumes.
        if matches!(self.phase, Phase::Playing) && !self.sudden_death && self.tick >= MATCH_TIME_LIMIT_TICKS {
            if self.score[0] != self.score[1] {
                let winner = usize::from(self.score[1] > self.score[0]);
                self.phase = Phase::GameOver { winner };
                self.events.push(Event::GameOver { winner });
                return;
            }
            self.sudden_death = true;
        }

        // Every paddle moves every tick, even during a serve, so players can posture before the puck
        // goes live: a released (bot) paddle reads only the puck's position, exactly like a human would.
        for slot in 0..2 {
            let input = if self.paddles[slot].ai { bot::paddle_ai(self.puck.pos, slot) } else { inputs.0[slot] };
            apply_paddle_input(&mut self.paddles[slot], &input, slot);
        }

        match self.phase {
            Phase::Serve { ticks_left, server } => {
                self.puck.pos = V(0., 0., serve_z_for(server));
                self.puck.vel = V(0., 0., 0.);
                let remaining = ticks_left.saturating_sub(1);
                self.phase =
                    if remaining == 0 { Phase::Playing } else { Phase::Serve { ticks_left: remaining, server } };
            }
            Phase::Playing => self.step_playing(),
            Phase::GameOver { .. } => unreachable!("handled above"),
        }
    }

    fn step_playing(&mut self) {
        self.puck.pos = self.puck.pos + self.puck.vel * TICK;

        // The long edges (X): always solid.
        let x_min = -TABLE_HALF_WIDTH + PUCK_RADIUS;
        let x_max = TABLE_HALF_WIDTH - PUCK_RADIUS;
        if self.puck.pos.0 < x_min {
            self.puck.pos.0 = x_min;
            let speed = -self.puck.vel.0;
            bounce_off_wall(&mut self.puck, V(1., 0., 0.));
            if speed > 0. {
                self.events.push(Event::WallHit { speed });
            }
        } else if self.puck.pos.0 > x_max {
            self.puck.pos.0 = x_max;
            let speed = self.puck.vel.0;
            bounce_off_wall(&mut self.puck, V(-1., 0., 0.));
            if speed > 0. {
                self.events.push(Event::WallHit { speed });
            }
        }

        // The end walls (Z): solid everywhere except the goal mouth, where the puck can fly clean
        // through until its centre has fully left the table (`±(TABLE_HALF_LENGTH + PUCK_RADIUS)`).
        let in_goal_mouth = self.puck.pos.0.abs() <= GOAL_HALF_WIDTH;
        let goal_line = TABLE_HALF_LENGTH + PUCK_RADIUS;
        let z_min = -TABLE_HALF_LENGTH + PUCK_RADIUS;
        let z_max = TABLE_HALF_LENGTH - PUCK_RADIUS;
        if self.puck.pos.2 <= -goal_line && in_goal_mouth {
            self.puck.pos.2 = -goal_line;
            self.score_goal(1);
            return;
        } else if self.puck.pos.2 >= goal_line && in_goal_mouth {
            self.puck.pos.2 = goal_line;
            self.score_goal(0);
            return;
        } else if self.puck.pos.2 < z_min && !in_goal_mouth {
            self.puck.pos.2 = z_min;
            let speed = -self.puck.vel.2;
            bounce_off_wall(&mut self.puck, V(0., 0., 1.));
            if speed > 0. {
                self.events.push(Event::WallHit { speed });
            }
        } else if self.puck.pos.2 > z_max && !in_goal_mouth {
            self.puck.pos.2 = z_max;
            let speed = self.puck.vel.2;
            bounce_off_wall(&mut self.puck, V(0., 0., -1.));
            if speed > 0. {
                self.events.push(Event::WallHit { speed });
            }
        }

        for i in 0..2 {
            let mut paddle = self.paddles[i];
            if let Some(speed) = collide_puck_paddle(&mut self.puck, &mut paddle) {
                self.paddles[i] = paddle;
                self.events.push(Event::PaddleHit { paddle: i, speed });
            }
        }

        let speed = self.puck.vel.length();
        if speed > PUCK_MAX_SPEED {
            self.puck.vel = self.puck.vel * (PUCK_MAX_SPEED / speed);
        }
    }

    /// `scorer` just put the puck in the opponent's goal: score it, and either end the match (reached
    /// [`WIN_GOALS`], or any goal at all once sudden death has begun) or serve again (the conceding
    /// player serves next, the classic air-hockey convention).
    fn score_goal(&mut self, scorer: usize) {
        self.score[scorer] += 1;
        self.events.push(Event::Goal { scorer, score: self.score });
        if self.score[scorer] >= WIN_GOALS || self.sudden_death {
            self.phase = Phase::GameOver { winner: scorer };
            self.events.push(Event::GameOver { winner: scorer });
        } else {
            let server = 1 - scorer;
            self.phase = Phase::Serve { ticks_left: SERVE_TICKS, server };
            self.events.push(Event::Serve { server });
        }
    }

    /// Events since the last call, oldest first.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn report(&self) -> MatchReport {
        let winner = match self.phase {
            Phase::GameOver { winner } => Some(winner),
            _ => None,
        };
        MatchReport {
            game: "slapstick".into(),
            seed: self.seed,
            score: self.score,
            winner,
            ticks: self.tick,
            sudden_death: self.sudden_death,
        }
    }
}

impl Simulation for Sim {
    type Input = Inputs;
    fn step(&mut self, input: &Inputs) {
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
                "match",
                one(&|h| {
                    h.u64(self.tick).u32(self.score[0]).u32(self.score[1]).bool(self.sudden_death);
                    match self.phase {
                        Phase::Serve { ticks_left, server } => {
                            h.u32(0).u32(ticks_left).u64(server as u64);
                        }
                        Phase::Playing => {
                            h.u32(1);
                        }
                        Phase::GameOver { winner } => {
                            h.u32(2).u64(winner as u64);
                        }
                    };
                }),
            ),
            (
                "paddles",
                one(&|h| {
                    for p in &self.paddles {
                        h.f32(p.pos.0).f32(p.pos.2).f32(p.vel.0).f32(p.vel.2).f32(p.target.0).f32(p.target.2);
                        h.bool(p.ai);
                    }
                }),
            ),
            (
                "puck",
                one(&|h| {
                    h.f32(self.puck.pos.0).f32(self.puck.pos.2).f32(self.puck.vel.0).f32(self.puck.vel.2);
                }),
            ),
            (
                "rng",
                one(&|h| {
                    h.u64(self.rng.state());
                }),
            ),
        ]
    }
}

/// Everything that decides the match's future, as plain data for a save.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub phase: Phase,
    pub paddles: [Paddle; 2],
    pub puck: Puck,
    pub score: [u32; 2],
    pub seed: u64,
    pub sudden_death: bool,
    pub rng: Rng,
}

impl Snapshot for Sim {
    const KIND: &'static str = "slapstick";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            phase: self.phase,
            paddles: self.paddles,
            puck: self.puck,
            score: self.score,
            seed: self.seed,
            sudden_death: self.sudden_death,
            rng: self.rng.clone(),
        }
    }
    /// Refuse a state this game could not have produced; the caller then keeps the running match.
    fn restore(&mut self, state: SimState) -> Result<(), String> {
        let slack = 0.5;
        let sane = |v: V| v.finite() && v.0.abs() < TABLE_HALF_WIDTH + slack && v.2.abs() < TABLE_HALF_LENGTH + slack;
        if !state.paddles.iter().all(|p| sane(p.pos) && p.vel.finite() && p.target.finite()) {
            return Err("the save puts a paddle outside the table".into());
        }
        if !sane(state.puck.pos) || !state.puck.vel.finite() || state.puck.vel.length() > PUCK_MAX_SPEED + 50. {
            return Err("the save puts the puck somewhere this match could not have".into());
        }
        match state.phase {
            Phase::Serve { server, .. } if server >= 2 => {
                return Err("the save names a server that does not exist".into())
            }
            Phase::GameOver { winner } if winner >= 2 => {
                return Err("the save names a winner that does not exist".into())
            }
            _ => {}
        }
        if state.score[0] > WIN_GOALS + 100 || state.score[1] > WIN_GOALS + 100 {
            return Err("the save has an impossible score".into());
        }
        self.tick = state.tick;
        self.phase = state.phase;
        self.paddles = state.paddles;
        self.puck = state.puck;
        self.score = state.score;
        self.seed = state.seed;
        self.sudden_death = state.sudden_death;
        self.rng = state.rng;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step_n(sim: &mut Sim, n: u32, inputs: &Inputs) {
        for _ in 0..n {
            sim.step(inputs);
        }
    }

    #[test]
    fn a_fresh_match_serves_from_a_seeded_coin_flip_and_freezes_the_puck() {
        let sim = Sim::new(1);
        match sim.phase {
            Phase::Serve { ticks_left, server } => {
                assert_eq!(ticks_left, SERVE_TICKS);
                assert!(server < 2);
                assert_eq!(sim.puck.pos.2, serve_z_for(server));
            }
            _ => panic!("a fresh match must start in Serve"),
        }
        assert_eq!(sim.puck.vel, V(0., 0., 0.));
    }

    #[test]
    fn different_seeds_pick_both_servers_over_enough_tries() {
        let servers: std::collections::HashSet<usize> = (0..40u64)
            .map(|seed| match Sim::new(seed).phase {
                Phase::Serve { server, .. } => server,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(servers.len(), 2, "{servers:?}");
    }

    #[test]
    fn the_puck_resumes_physics_after_serve_ticks_elapse() {
        let mut sim = Sim::new(2);
        let inputs = Inputs::default();
        step_n(&mut sim, SERVE_TICKS, &inputs);
        assert!(matches!(sim.phase, Phase::Playing), "{:?}", sim.phase);
    }

    #[test]
    fn paddles_can_posture_during_a_serve() {
        let mut sim = Sim::new(3);
        let inputs = Inputs([Input { target_x: 1., target_z: 1. }, Input { target_x: -1., target_z: -1. }]);
        let before = sim.paddles;
        sim.step(&inputs);
        assert_ne!(sim.paddles[0].pos, before[0].pos);
        assert_ne!(sim.paddles[1].pos, before[1].pos);
    }

    #[test]
    fn a_paddle_never_crosses_the_center_line_into_the_opponents_half() {
        let mut sim = Sim::new(4);
        let inputs = Inputs([Input { target_x: 1., target_z: 1. }, Input { target_x: 1., target_z: 1. }]);
        for _ in 0..600 {
            sim.step(&inputs);
            assert!(sim.paddles[0].pos.2 <= 1e-6, "{}", sim.paddles[0].pos.2);
            assert!(sim.paddles[1].pos.2 >= -1e-6, "{}", sim.paddles[1].pos.2);
        }
    }

    #[test]
    fn a_paddle_never_leaves_the_table_bounds() {
        let mut sim = Sim::new(5);
        let inputs = Inputs([Input { target_x: 2., target_z: -2. }, Input { target_x: -2., target_z: 2. }]);
        for _ in 0..300 {
            sim.step(&inputs);
            for p in &sim.paddles {
                assert!(p.pos.0.abs() <= TABLE_HALF_WIDTH + 1e-4);
                assert!(p.pos.2.abs() <= TABLE_HALF_LENGTH + 1e-4);
            }
        }
    }

    #[test]
    fn the_puck_never_leaves_the_table_bounds() {
        for seed in 0..12u64 {
            let mut sim = Sim::new(seed);
            let mut rng = Rng::new(seed ^ 0xABCD);
            for _ in 0..4000 {
                let inputs = Inputs([
                    Input { target_x: rng.range(-1., 1.), target_z: rng.range(-1., 1.) },
                    Input { target_x: rng.range(-1., 1.), target_z: rng.range(-1., 1.) },
                ]);
                sim.step(&inputs);
                let slack = PUCK_RADIUS + 1e-3;
                assert!(sim.puck.pos.0.abs() <= TABLE_HALF_WIDTH + slack, "seed {seed}: {:?}", sim.puck.pos);
                assert!(sim.puck.pos.2.abs() <= TABLE_HALF_LENGTH + slack, "seed {seed}: {:?}", sim.puck.pos);
            }
        }
    }

    #[test]
    fn the_puck_never_exceeds_its_speed_cap() {
        for seed in 0..12u64 {
            let mut sim = Sim::new(seed);
            let mut rng = Rng::new(seed ^ 0x1234);
            for _ in 0..4000 {
                let inputs = Inputs([
                    Input { target_x: rng.range(-1., 1.), target_z: rng.range(-1., 1.) },
                    Input { target_x: rng.range(-1., 1.), target_z: rng.range(-1., 1.) },
                ]);
                sim.step(&inputs);
                assert!(sim.puck.vel.length() <= PUCK_MAX_SPEED + 1e-3, "seed {seed}: {}", sim.puck.vel.length());
            }
        }
    }

    #[test]
    fn a_goal_is_detected_the_exact_tick_the_puck_crosses_the_goal_line() {
        let mut sim = Sim::new(6);
        // Force Playing with the puck lined up on a clean shot into paddle 1's goal (+Z).
        sim.phase = Phase::Playing;
        sim.puck = Puck { pos: V(0., 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
        let inputs = Inputs::default();
        let mut scored_tick = None;
        for t in 1..=10u32 {
            sim.step(&inputs);
            if sim.score[0] == 1 {
                scored_tick = Some(t);
                break;
            }
        }
        assert!(scored_tick.is_some(), "the goal was never detected");
        assert!(matches!(sim.phase, Phase::Serve { server: 1, .. }), "{:?}", sim.phase);
    }

    #[test]
    fn scoring_outside_the_goal_mouth_bounces_off_the_end_wall_instead() {
        let mut sim = Sim::new(7);
        sim.phase = Phase::Playing;
        let wide_x = GOAL_HALF_WIDTH + PADDLE_RADIUS; // outside the goal mouth
        sim.puck = Puck { pos: V(wide_x, 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
        let inputs = Inputs::default();
        for _ in 0..3 {
            sim.step(&inputs);
        }
        assert_eq!(sim.score, [0, 0], "a shot wide of the goal mouth must not score");
        assert!(sim.puck.pos.2 <= TABLE_HALF_LENGTH - PUCK_RADIUS + 1e-4);
        assert!(sim.puck.vel.2 < 0., "it bounced back");
    }

    #[test]
    fn the_conceding_player_serves_next() {
        let mut sim = Sim::new(8);
        sim.phase = Phase::Playing;
        sim.puck = Puck { pos: V(0., 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
        sim.step(&Inputs::default());
        for _ in 0..10 {
            sim.step(&Inputs::default());
            if sim.score[0] == 1 {
                break;
            }
        }
        assert_eq!(sim.score[0], 1);
        assert!(matches!(sim.phase, Phase::Serve { server: 1, .. }), "the player scored on (1) serves next");
    }

    #[test]
    fn the_match_ends_the_instant_a_player_reaches_win_goals() {
        let mut sim = Sim::new(9);
        sim.score = [WIN_GOALS - 1, 2];
        sim.phase = Phase::Playing;
        sim.puck = Puck { pos: V(0., 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
        let mut events = Vec::new();
        for _ in 0..10 {
            sim.step(&Inputs::default());
            events.extend(sim.drain_events());
            if sim.is_over() {
                break;
            }
        }
        assert!(sim.is_over());
        assert!(matches!(sim.phase, Phase::GameOver { winner: 0 }));
        assert!(events.contains(&Event::GameOver { winner: 0 }));
    }

    #[test]
    fn a_leading_player_wins_immediately_at_the_time_limit() {
        let mut sim = Sim::new(10);
        sim.phase = Phase::Playing;
        sim.score = [3, 1];
        sim.tick = MATCH_TIME_LIMIT_TICKS - 1;
        sim.step(&Inputs::default());
        assert!(sim.is_over());
        assert!(matches!(sim.phase, Phase::GameOver { winner: 0 }));
    }

    #[test]
    fn a_tied_match_continues_past_the_time_limit_in_sudden_death() {
        let mut sim = Sim::new(11);
        sim.phase = Phase::Playing;
        sim.score = [2, 2];
        sim.tick = MATCH_TIME_LIMIT_TICKS - 1;
        sim.step(&Inputs::default());
        assert!(!sim.is_over(), "a tie does not end the match at the time limit");
        assert!(sim.sudden_death);
        // Now any goal at all ends it, even though neither player has reached WIN_GOALS.
        sim.puck = Puck { pos: V(0., 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
        for _ in 0..10 {
            sim.step(&Inputs::default());
            if sim.is_over() {
                break;
            }
        }
        assert!(sim.is_over());
        assert!(matches!(sim.phase, Phase::GameOver { winner: 0 }));
        assert_eq!(sim.score, [3, 2]);
    }

    #[test]
    fn a_finished_match_ignores_further_input() {
        let mut sim = Sim::new(12);
        sim.phase = Phase::GameOver { winner: 0 };
        let before = (sim.paddles, sim.puck, sim.tick);
        sim.step(&Inputs([Input { target_x: 1., target_z: 1. }, Input { target_x: -1., target_z: -1. }]));
        assert_eq!((sim.paddles, sim.puck, sim.tick), before, "a finished match does not step");
    }

    #[test]
    fn releasing_a_participant_hands_their_paddle_to_the_bot_ai() {
        let mut sim = Sim::new(13);
        sim.paddles[0].ai = true;
        // Drive the puck to the AI's own half so it has to move to intercept.
        sim.phase = Phase::Playing;
        sim.puck = Puck { pos: V(0.3, 0., -0.8), vel: V(0., 0., 0.) };
        let before = sim.paddles[0].pos;
        for _ in 0..30 {
            sim.step(&Inputs::default());
        }
        assert_ne!(sim.paddles[0].pos, before, "the AI paddle must move on its own, with no human input");
    }

    #[test]
    fn the_same_seed_and_inputs_replay_identically() {
        let script: Vec<Inputs> = (0..500)
            .map(|t| {
                let f = t as f32;
                Inputs([
                    Input { target_x: (f * 0.03).sin(), target_z: (f * 0.02).cos() },
                    Input { target_x: (f * 0.025).cos(), target_z: (f * 0.018).sin() },
                ])
            })
            .collect();
        vesper3d::viewer::devkit::assert_deterministic(|| Sim::new(42), &script);
    }

    #[test]
    fn a_save_from_any_tick_resumes_as_the_game_promises_in_a_brand_new_match() {
        let script: Vec<Inputs> = (0..400)
            .map(|t| {
                let f = t as f32;
                Inputs([
                    Input { target_x: (f * 0.03).sin(), target_z: (f * 0.02).cos() },
                    Input { target_x: (f * 0.025).cos(), target_z: (f * 0.018).sin() },
                ])
            })
            .collect();
        vesper3d::viewer::devkit::snapshot::assert_resumes_as_promised(|| Sim::new(44), &script, 30);
    }

    #[test]
    fn a_bad_save_is_refused_and_the_running_match_is_left_alone() {
        use vesper3d::viewer::devkit::snapshot;
        let mut sim = Sim::new(45);
        let script: Vec<Inputs> = (0..100).map(|_| Inputs::default()).collect();
        for input in &script {
            sim.step(input);
        }
        let bytes = snapshot::save(&sim, "good").unwrap();
        for input in &script[..50] {
            sim.step(input);
        }
        let before = sim.state_hash();
        let mut damaged = bytes.clone();
        let mid = damaged.len() / 2;
        damaged[mid] ^= 0x20;
        assert!(snapshot::restore(&mut sim, &damaged).is_err());
        assert!(snapshot::restore(&mut sim, &bytes[..bytes.len() / 2]).is_err());
        assert_eq!(sim.state_hash(), before, "a refused load changes nothing");
        snapshot::restore(&mut sim, &bytes).unwrap();
        assert_eq!(sim.tick, 100);
    }

    #[test]
    fn quick_save_and_quick_load_go_through_a_slot_with_a_backup() {
        use vesper3d::viewer::devkit::{snapshot, SaveError, SaveSlots, QUICK_SLOT};
        let dir = std::env::temp_dir().join(format!("slapstick-slots-{}", std::process::id()));
        let slots = SaveSlots::new(&dir);
        let mut sim = Sim::new(46);
        assert!(matches!(snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT), Err(SaveError::NotFound(_))));
        for _ in 0..60 {
            sim.step(&Inputs::default());
        }
        snapshot::save_to_slot(&sim, &slots, QUICK_SLOT, "first").unwrap();
        let saved = sim.state_hash();
        for _ in 0..60 {
            sim.step(&Inputs::default());
        }
        let (header, _) = snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT).unwrap();
        assert_eq!((header.label.as_str(), sim.state_hash()), ("first", saved));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
