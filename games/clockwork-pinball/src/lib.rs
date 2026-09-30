//! Clockwork Pinball's rules: a pure, deterministic, fixed-step simulation with no window, no sound device and no
//! wall clock. The physics is real rigid bodies (the engine's `rapier`, see `physics.rs`): a fast steel ball with
//! continuous collision detection, kinematic flippers and a spinning gear that push it, walls with bounce, and an
//! inclined playfield modelled by a gravity vector that points partly down the table.
//!
//! Layout (metres; the table lies in the XZ plane, up-table is -Z, the flippers are at +Z):
//! a launch lane on the right, the playfield to its left, three bumpers that kick, two posts, three rollover
//! lanes across the top, a spinning gear in the middle, and a drain between the flippers.
pub mod physics;

use physics::{Body, BodyState, Material, Physics};
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::devkit::{Rng, SavePolicy, Simulation, Snapshot, StateHasher, TICK};

/// Ball radius.
pub const BALL_R: f32 = 0.15;
/// Playfield edges (x from LEFT to the lane divider, z from TOP to the drain).
pub const LEFT: f32 = -3.0;
pub const RIGHT: f32 = 2.4;
pub const TOP: f32 = -5.0;
pub const DRAIN_Z: f32 = 5.6;
/// The launch lane's centre line and the ball's starting spot on the plunger.
pub const LANE_X: f32 = 2.65;
pub const PLUNGER_Z: f32 = 4.55;
/// Gravity: straight down plus a pull down-table (+Z) that stands in for the table's slope.
pub const GRAVITY: V = V(0., -9.81, 5.0);
pub const FLIPPER_LEN: f32 = 1.1;
pub const FLIPPER_HALF_THICK: f32 = 0.13;
/// Flipper pivots (left, right).
pub const PIVOTS: [V; 2] = [V(-1.45, 0., 4.1), V(0.85, 0., 4.1)];
/// Flipper angles from the horizontal, toward the drain at rest and up-table when flipped.
pub const REST_ANGLE: f32 = 0.61;
pub const FLIP_ANGLE: f32 = -0.44;
/// How fast a flipper swings (rad/s): 0.61 + 0.44 rad in about a tenth of a second.
pub const FLIPPER_SPEED: f32 = 11.;
pub const BUMPERS: [(f32, f32); 3] = [(-1.45, -2.3), (0.35, -2.75), (-0.55, -0.85)];
pub const BUMPER_R: f32 = 0.38;
pub const POSTS: [(f32, f32); 2] = [(-1.75, 2.35), (1.0, 2.35)];
pub const POST_R: f32 = 0.17;
/// Rollover lane centres across the top.
pub const LANES: [(f32, f32); 3] = [(-2.15, -4.3), (-1.15, -4.3), (-0.15, -4.3)];
pub const LANE_R: f32 = 0.3;
pub const GEAR_AT: V = V(-0.6, 0., -3.6);
pub const GEAR_HALF: f32 = 0.6;
const GEAR_SPEED: f32 = 2.2;
const BALLS: u32 = 3;
/// Least speed a bumper kicks the ball away at.
const KICK: f32 = 7.5;
const BALL_HEIGHT: f32 = BALL_R;

/// One tick of player intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    /// Left and right flipper buttons (held).
    pub left: bool,
    pub right: bool,
    /// Plunger (held to pull back, released to launch).
    pub plunger: bool,
}

/// What happened this tick; the window reacts with sound and effects.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Launch { power: f32 },
    Flip { right: bool },
    Bumper { at: V, points: u32 },
    Post { at: V, points: u32 },
    Gear { at: V, points: u32 },
    Lane { index: usize, completed: bool },
    Multiplier(u32),
    BallLost { balls_left: u32 },
    GameOver { score: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// The ball waits on the plunger.
    Waiting,
    Playing,
    /// A ball just drained: ticks until the next one is served (or the game ends).
    Lost(u32),
    GameOver,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flipper {
    pub angle: f32,
    pub held: bool,
}

pub struct Sim {
    pub tick: u64,
    pub phys: Physics,
    pub ball: Body,
    pub flippers: [(Body, Flipper); 2],
    pub gear: Body,
    pub gear_angle: f32,
    pub phase: Phase,
    pub score: u64,
    pub multiplier: u32,
    pub balls_left: u32,
    pub lanes: [bool; 3],
    /// Plunger pull, 0..1.
    pub plunger: f32,
    bumper_cool: [u32; 3],
    post_cool: [u32; 2],
    gear_cool: u32,
    still: u32,
    /// Ticks the ball has sat back on the plunger after a weak launch.
    returned: u32,
    rng: Rng,
    events: Vec<Event>,
}

fn flipper_pose(pivot: V, angle: f32, right: bool) -> (V, f32) {
    // The left flipper points toward +X, the right toward -X; `angle` turns the tip toward the drain (+Z).
    let dir = if right { V(-angle.cos(), 0., angle.sin()) } else { V(angle.cos(), 0., angle.sin()) };
    let centre = V(pivot.0, 0.2, pivot.2) + dir * (FLIPPER_LEN * 0.5);
    (centre, -dir.2.atan2(dir.0))
}

impl Sim {
    /// The table as rigid bodies: the same bodies in the same slots every time, which is what lets a save put
    /// poses back by slot number.
    fn build_table() -> (Physics, Body, [(Body, Flipper); 2], Body) {
        let mut phys = Physics::new(GRAVITY);
        let wall = Material { friction: 0.2, restitution: 0.45, ..Default::default() };
        let floor = Material { friction: 0.25, restitution: 0.0, ..Default::default() };
        // Floor, then the outer walls (height 0.6), the lane divider, and the deflectors that guide the ball.
        phys.fixed_box(V(0., -0.25, 0.), V(12., 0.25, 12.), 0., floor);
        // The glass: a fast ball that bounces upward off a wall comes back down instead of leaving the table.
        phys.fixed_box(
            V(0., 1.4, 0.),
            V(6., 0.5, 7.),
            0.,
            Material { friction: 0.1, restitution: 0.3, ..Default::default() },
        );
        let h = 0.45;
        phys.fixed_box(V(LEFT - 0.1, h, 0.), V(0.1, h, 6.), 0., wall);
        phys.fixed_box(V(3.05, h, 0.), V(0.1, h, 6.), 0., wall);
        phys.fixed_box(V(0.0, h, TOP - 0.1), V(3.2, h, 0.1), 0., wall);
        phys.fixed_box(V(LANE_X, h, PLUNGER_Z + 0.25), V(0.3, h, 0.1), 0., wall); // the plunger stop
                                                                                  // Lane divider from just below the top deflector to the bottom of the lane.
        phys.fixed_box(V(RIGHT + 0.05, h, 0.35), V(0.05, h, 4.0), 0., wall);
        // Top-right deflector sends the launched ball left; the top-left corner is cut off.
        phys.fixed_box(V(2.38, h, -4.45), V(0.78, h, 0.08), -0.85, wall);
        phys.fixed_box(V(-2.45, h, -4.55), V(0.75, h, 0.08), 0.78, wall);
        // Inlane guides: slopes from the side walls down to the flipper pivots.
        phys.fixed_box(V(-2.25, h, 3.3), V(0.85, h, 0.08), -0.62, wall);
        phys.fixed_box(V(1.7, h, 3.3), V(0.85, h, 0.08), 0.62, wall);
        // Bumpers and posts (kicks are applied by the rules; the bodies give them a real surface).
        let rubber = Material { friction: 0.1, restitution: 0.8, ..Default::default() };
        for (x, z) in BUMPERS {
            phys.fixed_cylinder(V(x, h, z), BUMPER_R, h, rubber);
        }
        for (x, z) in POSTS {
            phys.fixed_cylinder(V(x, h, z), POST_R, h, rubber);
        }
        // The steel ball, resting on the plunger.
        let steel = Material {
            friction: 0.3,
            restitution: 0.5,
            density: 1000.,
            linear_damping: 0.05,
            angular_damping: 0.3,
            ccd: true,
        };
        let ball = phys.dynamic_ball(V(LANE_X, BALL_HEIGHT, PLUNGER_Z), BALL_R, steel);
        // Flippers and the gear are driven by hand each tick.
        let metal = Material { friction: 0.25, restitution: 0.35, ..Default::default() };
        let mk = |phys: &mut Physics, right: bool| {
            let (centre, yaw) = flipper_pose(PIVOTS[right as usize], REST_ANGLE, right);
            let half = V(FLIPPER_LEN * 0.5, 0.2, FLIPPER_HALF_THICK);
            (phys.kinematic_box(centre, half, yaw, metal), Flipper { angle: REST_ANGLE, held: false })
        };
        let flippers = [mk(&mut phys, false), mk(&mut phys, true)];
        let gear = phys.kinematic_box(V(GEAR_AT.0, 0.2, GEAR_AT.2), V(GEAR_HALF, 0.2, 0.08), 0., metal);
        (phys, ball, flippers, gear)
    }

    /// A fresh game; the same seed always plays out the same way.
    pub fn new(seed: u64) -> Self {
        let (phys, ball, flippers, gear) = Self::build_table();
        Self {
            tick: 0,
            phys,
            ball,
            flippers,
            gear,
            gear_angle: 0.,
            phase: Phase::Waiting,
            score: 0,
            multiplier: 1,
            balls_left: BALLS,
            lanes: [false; 3],
            plunger: 0.,
            bumper_cool: [0; 3],
            post_cool: [0; 2],
            gear_cool: 0,
            still: 0,
            returned: 0,
            rng: Rng::new(seed),
            events: Vec::new(),
        }
    }

    pub fn ball_pos(&self) -> V {
        self.phys.position(self.ball)
    }

    pub fn ball_speed(&self) -> f32 {
        self.phys.speed(self.ball)
    }

    pub fn in_lane(&self) -> bool {
        let p = self.ball_pos();
        p.0 > RIGHT + 0.12 && p.2 > 0.
    }

    fn serve(&mut self) {
        self.phys.set_position(self.ball, V(LANE_X, BALL_HEIGHT, PLUNGER_Z));
        self.phys.set_linvel(self.ball, V(0., 0., 0.));
        self.phys.set_angvel(self.ball, V(0., 0., 0.));
        self.plunger = 0.;
        self.still = 0;
        self.phase = Phase::Waiting;
    }

    fn add_score(&mut self, points: u32) -> u32 {
        let earned = points * self.multiplier;
        self.score += earned as u64;
        earned
    }

    /// Advance exactly one 60 Hz tick. A finished game ignores further input.
    pub fn step(&mut self, input: &Input) {
        if self.phase == Phase::GameOver {
            return;
        }
        self.tick += 1;
        // Flippers swing toward their target at a fixed speed; the kinematic body pushes the ball as it goes.
        for (i, (body, f)) in self.flippers.iter_mut().enumerate() {
            let right = i == 1;
            let held = if right { input.right } else { input.left };
            if held && !f.held {
                self.events.push(Event::Flip { right });
            }
            f.held = held;
            let target = if held { FLIP_ANGLE } else { REST_ANGLE };
            let step = FLIPPER_SPEED * TICK;
            f.angle += (target - f.angle).clamp(-step, step);
            let (centre, yaw) = flipper_pose(PIVOTS[i], f.angle, right);
            self.phys.move_kinematic(*body, centre, yaw);
        }
        self.gear_angle += GEAR_SPEED * TICK;
        self.phys.move_kinematic(self.gear, V(GEAR_AT.0, 0.2, GEAR_AT.2), self.gear_angle);

        // The plunger: hold to pull back, release to launch (only with the ball on it).
        match self.phase {
            Phase::Waiting => {
                if input.plunger {
                    self.plunger = (self.plunger + TICK * 1.1).min(1.);
                } else if self.plunger > 0.05 {
                    let power = self.plunger;
                    self.phys.set_linvel(self.ball, V(0., 0., -(10. + 13. * power)));
                    self.events.push(Event::Launch { power });
                    self.plunger = 0.;
                    self.phase = Phase::Playing;
                } else {
                    self.plunger = 0.;
                }
            }
            Phase::Lost(left) => {
                if left <= 1 {
                    self.serve();
                } else {
                    self.phase = Phase::Lost(left - 1);
                }
            }
            _ => {}
        }

        self.phys.step();
        if self.phase == Phase::Playing {
            self.kicks_and_targets();
            let p = self.ball_pos();
            // Drained, or flung off the table.
            if p.2 > DRAIN_Z || p.1 < -1. || p.0.abs() > 6. || p.2 < TOP - 2. {
                self.balls_left -= 1;
                self.events.push(Event::BallLost { balls_left: self.balls_left });
                self.multiplier = 1;
                self.lanes = [false; 3];
                if self.balls_left == 0 {
                    self.phase = Phase::GameOver;
                    self.events.push(Event::GameOver { score: self.score });
                } else {
                    self.phys.set_linvel(self.ball, V(0., 0., 0.));
                    self.phys.set_position(self.ball, V(LANE_X, BALL_HEIGHT, PLUNGER_Z));
                    self.phase = Phase::Lost(75);
                }
            }
            // A launch too weak to clear the lane rolls back to the plunger: serve it again.
            if self.in_lane() && p.2 > PLUNGER_Z - 0.2 && self.ball_speed() < 0.3 {
                self.returned += 1;
                if self.returned > 20 {
                    self.returned = 0;
                    self.phase = Phase::Waiting;
                }
            } else {
                self.returned = 0;
            }
            // A ball that stops on a flat spot is nudged on.
            if self.ball_speed() < 0.12 && !self.in_lane() {
                self.still += 1;
                if self.still > 150 {
                    let kick = V(self.rng.range(-1., 1.), 0., self.rng.range(1., 2.)) * 1.5;
                    self.phys.set_linvel(self.ball, kick);
                    self.still = 0;
                }
            } else {
                self.still = 0;
            }
        }
        for c in &mut self.bumper_cool {
            *c = c.saturating_sub(1);
        }
        for c in &mut self.post_cool {
            *c = c.saturating_sub(1);
        }
        self.gear_cool = self.gear_cool.saturating_sub(1);
    }

    /// The rules the physics does not decide: bumpers and posts kick, lanes light, the gear scores.
    fn kicks_and_targets(&mut self) {
        let p = self.ball_pos();
        let flat = |x: f32, z: f32| ((p.0 - x).powi(2) + (p.2 - z).powi(2)).sqrt();
        for (i, (x, z)) in BUMPERS.iter().enumerate() {
            if flat(*x, *z) < BUMPER_R + BALL_R + 0.03 && self.bumper_cool[i] == 0 {
                self.bumper_cool[i] = 8;
                let away = V(p.0 - x, 0., p.2 - z).norm();
                let speed = (self.ball_speed() * 0.9).max(KICK);
                self.phys.set_linvel(self.ball, away * speed);
                let at = V(*x, 0.3, *z);
                let points = self.add_score(100);
                self.events.push(Event::Bumper { at, points });
            }
        }
        for (i, (x, z)) in POSTS.iter().enumerate() {
            if flat(*x, *z) < POST_R + BALL_R + 0.03 && self.post_cool[i] == 0 {
                self.post_cool[i] = 8;
                let away = V(p.0 - x, 0., p.2 - z).norm();
                self.phys.set_linvel(self.ball, away * (self.ball_speed() * 0.8).max(5.));
                let at = V(*x, 0.3, *z);
                let points = self.add_score(50);
                self.events.push(Event::Post { at, points });
            }
        }
        // The gear scores whenever it touches the ball.
        if self.gear_cool == 0 && self.phys.touching(self.ball).contains(&self.gear) {
            self.gear_cool = 20;
            let points = self.add_score(25);
            self.events.push(Event::Gear { at: V(p.0, 0.3, p.2), points });
        }
        for (i, (x, z)) in LANES.iter().enumerate() {
            if !self.lanes[i] && flat(*x, *z) < LANE_R {
                self.lanes[i] = true;
                let done = self.lanes.iter().all(|l| *l);
                self.add_score(500);
                self.events.push(Event::Lane { index: i, completed: done });
                if done {
                    self.add_score(2000);
                    self.multiplier = (self.multiplier + 1).min(5);
                    self.lanes = [false; 3];
                    self.events.push(Event::Multiplier(self.multiplier));
                }
            }
        }
    }

    /// Events since the last call, oldest first.
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
                    h.u64(self.tick).u64(self.score).u32(self.multiplier).u32(self.balls_left).f32(self.plunger);
                    h.f32(self.gear_angle).u32(self.still).u32(self.returned).u32(self.gear_cool);
                    for l in self.lanes {
                        h.bool(l);
                    }
                    for c in self.bumper_cool.iter().chain(&self.post_cool) {
                        h.u32(*c);
                    }
                    match self.phase {
                        Phase::Waiting => h.u32(0),
                        Phase::Playing => h.u32(1),
                        Phase::Lost(n) => h.u32(2 + n),
                        Phase::GameOver => h.u32(u32::MAX),
                    };
                }),
            ),
            (
                "flippers",
                one(&|h| {
                    for (_, f) in &self.flippers {
                        h.f32(f.angle).bool(f.held);
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
/// library's contact cache is not saved), so a load is a fair continuation, not a bit-exact one: see
/// `SavePolicy::PhysicsContinuation`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub bodies: Vec<BodyState>,
    pub flippers: [Flipper; 2],
    pub gear_angle: f32,
    pub phase: Phase,
    pub score: u64,
    pub multiplier: u32,
    pub balls_left: u32,
    pub lanes: [bool; 3],
    pub plunger: f32,
    pub bumper_cool: [u32; 3],
    pub post_cool: [u32; 2],
    pub gear_cool: u32,
    pub still: u32,
    pub returned: u32,
    pub rng: Rng,
}

impl Snapshot for Sim {
    const KIND: &'static str = "clockwork-pinball";
    const POLICY: SavePolicy = SavePolicy::PhysicsContinuation;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            bodies: self.phys.states(),
            flippers: [self.flippers[0].1, self.flippers[1].1],
            gear_angle: self.gear_angle,
            phase: self.phase,
            score: self.score,
            multiplier: self.multiplier,
            balls_left: self.balls_left,
            lanes: self.lanes,
            plunger: self.plunger,
            bumper_cool: self.bumper_cool,
            post_cool: self.post_cool,
            gear_cool: self.gear_cool,
            still: self.still,
            returned: self.returned,
            rng: self.rng.clone(),
        }
    }
    /// Refuse a state this game could not have produced; the caller then keeps the running game.
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        if s.balls_left > BALLS
            || s.multiplier == 0
            || s.multiplier > 5
            || !s.plunger.is_finite()
            || !(0. ..=1.).contains(&s.plunger)
        {
            return Err("the save has scores or counters this game cannot reach".into());
        }
        if s.flippers.iter().any(|f| !f.angle.is_finite() || f.angle < FLIP_ANGLE - 0.01 || f.angle > REST_ANGLE + 0.01)
        {
            return Err("the save puts a flipper outside its swing".into());
        }
        // Rebuild the physics world from scratch, then put the saved poses back: the library's own state (contact
        // cache, islands) is not in the save, so nothing of the old world may survive a load.
        let (mut phys, ball, mut flippers, gear) = Self::build_table();
        if !phys.restore(&s.bodies) {
            return Err("the save's bodies do not match this table".into());
        }
        flippers[0].1 = s.flippers[0];
        flippers[1].1 = s.flippers[1];
        for (i, (body, f)) in flippers.iter().enumerate() {
            let (centre, yaw) = flipper_pose(PIVOTS[i], f.angle, i == 1);
            phys.move_kinematic(*body, centre, yaw);
        }
        self.phys = phys;
        self.ball = ball;
        self.flippers = flippers;
        self.gear = gear;
        self.tick = s.tick;
        self.gear_angle = s.gear_angle;
        self.phase = s.phase;
        self.score = s.score;
        self.multiplier = s.multiplier;
        self.balls_left = s.balls_left;
        self.lanes = s.lanes;
        self.plunger = s.plunger;
        self.bumper_cool = s.bumper_cool;
        self.post_cool = s.post_cool;
        self.gear_cool = s.gear_cool;
        self.still = s.still;
        self.returned = s.returned;
        self.rng = s.rng;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}

/// A simple player: pulls the plunger back almost all the way, then flips whichever flipper the ball is about to
/// reach. Used for `--autoplay` captures, and in the tests as proof that the table can be played.
pub fn autoplay(sim: &Sim) -> Input {
    match sim.phase {
        Phase::Waiting => Input { plunger: sim.plunger < 0.9, ..Input::default() },
        Phase::Playing => {
            let p = sim.ball_pos();
            let low = p.2 > 3.3 && p.2 < DRAIN_Z;
            // A flick, not a hold: a ball left cradled on a raised flipper would never come back into play.
            let flick = sim.tick % 26 < 10;
            Input {
                left: low && flick && p.0 < -0.15 && p.0 > -1.8,
                right: low && flick && p.0 > -0.75 && p.0 < 1.3,
                plunger: false,
            }
        }
        _ => Input::default(),
    }
}
