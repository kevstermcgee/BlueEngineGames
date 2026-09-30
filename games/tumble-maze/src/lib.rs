//! Tumble Maze's rules: a pure, deterministic, fixed-step simulation with no window, no sound device and no wall
//! clock. A steel marble rolls on a maze board that the player tilts. The board itself stays flat in the physics
//! world and the tilt is a *gravity vector* (a real ball on a real incline rolls identically), so the maze is fixed
//! boxes, the marble is a rolling sphere with continuous collision detection, and sweeping bars are kinematic
//! bodies that shove it around. The window draws the same board rotated by the tilt.
//!
//! Levels are ASCII maps, so the solvability of every level is a test (breadth-first search) and the bot that
//! plays in the window (`autoplay`) uses the same search to find its way.

pub mod physics;

use physics::{Body, BodyState, Material, Physics};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use vesper3d::math::V;
use vesper3d::viewer::devkit::{SavePolicy, Simulation, Snapshot, StateHasher, TICK};

/// Marble radius in metres (one maze cell is one metre).
pub const BALL_R: f32 = 0.22;
/// The furthest the board tilts, in radians, and how fast it tilts toward the target (radians per second).
pub const MAX_TILT: f32 = 0.30;
pub const TILT_RATE: f32 = 1.3;
/// A marble whose centre is this close to a hole's centre falls in.
pub const HOLE_R: f32 = 0.34;
pub const STAR_R: f32 = 0.42;
pub const GOAL_R: f32 = 0.36;
/// Seconds added to the clock by a fall into a hole.
pub const FALL_PENALTY: f32 = 5.;
/// Ticks the "level cleared" pause lasts before the next board appears.
pub const CLEAR_PAUSE: u32 = 150;
pub const WALL_HEIGHT: f32 = 0.5;
pub const BAR_HALF_THICK: f32 = 0.15;

const GRAVITY: f32 = 9.81;

/// A bar that slides back and forth across the board and shoves the marble.
#[derive(Clone, Copy, Debug)]
pub struct Mover {
    /// Centre of travel, in cell units (a cell centre is `col + 0.5`).
    pub at: (f32, f32),
    /// Half extents along x and z.
    pub half: (f32, f32),
    /// 0 slides along x, 1 along z.
    pub axis: u8,
    pub amp: f32,
    pub period: u32,
}

pub struct Level {
    pub name: &'static str,
    pub map: &'static str,
    pub par_secs: f32,
    pub movers: &'static [Mover],
}

pub const LEVELS: [Level; 3] = [
    Level {
        name: "First Tilt",
        par_secs: 40.,
        movers: &[],
        map: "
###############
#S....#.......#
#.###.#.#####.#
#.#...#.#...#.#
#.#.###.#.#.#.#
#.#.#*..#.#.#.#
#.#.#.###.#...#
#.#...#...#####
#.#####.#.....#
#*..*..#.###G.#
###############
",
    },
    Level {
        name: "Sinkholes",
        par_secs: 60.,
        movers: &[],
        map: "
#################
#S..............#
#.....o.....o...#
#...#.....#.....#
#.o.....*....o..#
#.....#...#.....#
#...o.....*..o..#
#.....#...#.....#
#*..o.....o....G#
#################
",
    },
    Level {
        name: "Sweepers",
        par_secs: 80.,
        movers: &[
            Mover { at: (3.5, 4.5), half: (BAR_HALF_THICK, 0.9), axis: 0, amp: 1.7, period: 300 },
            Mover { at: (8.5, 4.5), half: (1.1, BAR_HALF_THICK), axis: 1, amp: 2.6, period: 240 },
        ],
        map: "
###################
#S....#.......#...#
#.....#.......#.*.#
#.....#...#...#...#
#.....#...#.......#
#..*......#...#..##
#.....#...#...#...#
#.....#...#...#.*.#
#.....#.......#..G#
###################
",
    },
];

/// A parsed level map.
pub struct Grid {
    pub w: i32,
    pub h: i32,
    pub walls: Vec<bool>,
    pub holes: Vec<(i32, i32)>,
    pub stars: Vec<(i32, i32)>,
    pub start: (i32, i32),
    pub goal: (i32, i32),
}

impl Grid {
    pub fn parse(map: &str) -> Grid {
        let rows: Vec<&[u8]> = map.trim().lines().map(|l| l.as_bytes()).collect();
        let h = rows.len() as i32;
        let w = rows[0].len() as i32;
        let (mut holes, mut stars) = (Vec::new(), Vec::new());
        let (mut start, mut goal) = ((1, 1), (1, 1));
        let mut walls = vec![false; (w * h) as usize];
        for (y, row) in rows.iter().enumerate() {
            assert_eq!(row.len() as i32, w, "row {y} of the map is not {w} wide");
            for (x, c) in row.iter().enumerate() {
                let cell = (x as i32, y as i32);
                match c {
                    b'#' => walls[y * w as usize + x] = true,
                    b'o' => holes.push(cell),
                    b'*' => stars.push(cell),
                    b'S' => start = cell,
                    b'G' => goal = cell,
                    _ => {}
                }
            }
        }
        Grid { w, h, walls, holes, stars, start, goal }
    }

    pub fn wall(&self, c: (i32, i32)) -> bool {
        c.0 < 0 || c.1 < 0 || c.0 >= self.w || c.1 >= self.h || self.walls[(c.1 * self.w + c.0) as usize]
    }

    /// World position of a cell's centre (the board is centred on the origin, y is the floor).
    pub fn centre(&self, c: (i32, i32)) -> V {
        V(c.0 as f32 + 0.5 - self.w as f32 / 2., 0., c.1 as f32 + 0.5 - self.h as f32 / 2.)
    }

    pub fn cell_of(&self, p: V) -> (i32, i32) {
        ((p.0 + self.w as f32 / 2.).floor() as i32, (p.2 + self.h as f32 / 2.).floor() as i32)
    }

    /// Shortest route over open cells from `from` to the goal, avoiding the cells next to holes when a route
    /// exists that does. Empty if the goal cannot be reached.
    pub fn route(&self, from: (i32, i32)) -> Vec<(i32, i32)> {
        let risky = |c: (i32, i32)| self.holes.iter().any(|h| (h.0 - c.0).abs() <= 1 && (h.1 - c.1).abs() <= 1);
        for careful in [true, false] {
            let blocked = |c: (i32, i32)| {
                self.wall(c) || self.holes.contains(&c) || (careful && c != self.goal && c != from && risky(c))
            };
            let mut prev = vec![None; (self.w * self.h) as usize];
            let idx = |c: (i32, i32)| (c.1 * self.w + c.0) as usize;
            let mut queue = VecDeque::from([from]);
            let mut seen = vec![false; prev.len()];
            seen[idx(from)] = true;
            while let Some(c) = queue.pop_front() {
                if c == self.goal {
                    let mut path = vec![c];
                    let mut at = c;
                    while let Some(p) = prev[idx(at)] {
                        path.push(p);
                        at = p;
                    }
                    path.reverse();
                    return path;
                }
                for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let n = (c.0 + d.0, c.1 + d.1);
                    if !blocked(n) && !seen[idx(n)] {
                        seen[idx(n)] = true;
                        prev[idx(n)] = Some(c);
                        queue.push_back(n);
                    }
                }
            }
        }
        Vec::new()
    }
}

/// One tick of player intent: how far to tilt the board toward +x and +z, each in -1..=1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub x: f32,
    pub z: f32,
}

/// What happened this tick; the window reacts with sound and effects.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Bump { speed: f32 },
    Star { at: V, total: u32 },
    Fell { at: V },
    Cleared { level: usize, time: f32, points: u32 },
    AllDone { score: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Playing,
    /// The goal was reached; ticks until the next board.
    Cleared(u32),
    Done,
}

pub struct Sim {
    pub tick: u64,
    pub level: usize,
    pub grid: Grid,
    pub phys: Physics,
    pub ball: Body,
    pub movers: Vec<Body>,
    pub phase: Phase,
    /// Board tilt in radians: positive x lowers the +x edge, positive z the +z edge.
    pub tilt: (f32, f32),
    /// Ticks on this level's clock (penalties included).
    pub clock: u32,
    pub falls: u32,
    pub stars_got: Vec<bool>,
    pub score: u64,
    bump_cool: u32,
    events: Vec<Event>,
}

fn mover_pose(grid: &Grid, m: &Mover, tick: u64) -> V {
    let phase = (tick % m.period as u64) as f32 / m.period as f32 * std::f32::consts::TAU;
    let s = m.amp * phase.sin();
    let (dx, dz) = if m.axis == 0 { (s, 0.) } else { (0., s) };
    V(m.at.0 - grid.w as f32 / 2. + dx, WALL_HEIGHT * 0.5, m.at.1 - grid.h as f32 / 2. + dz)
}

impl Sim {
    /// A level's board as rigid bodies: the same bodies in the same slots every time, which is what lets a save
    /// put poses back by slot number. The marble is always the last slot.
    fn build(level: usize) -> (Grid, Physics, Body, Vec<Body>) {
        let grid = Grid::parse(LEVELS[level].map);
        let mut phys = Physics::new(V(0., -GRAVITY, 0.));
        let floor = Material { friction: 0.35, restitution: 0., ..Default::default() };
        let wall = Material { friction: 0.15, restitution: 0.35, ..Default::default() };
        let (hw, hh) = (grid.w as f32 / 2., grid.h as f32 / 2.);
        phys.fixed_box(V(0., -0.5, 0.), V(hw + 1., 0.5, hh + 1.), 0., floor);
        // Horizontal runs of wall cells become one box each, so the marble never snags on a seam between two.
        for y in 0..grid.h {
            let mut x = 0;
            while x < grid.w {
                if !grid.wall((x, y)) {
                    x += 1;
                    continue;
                }
                let start = x;
                while grid.wall((x, y)) && x < grid.w {
                    x += 1;
                }
                let len = (x - start) as f32;
                let cx = start as f32 + len / 2. - hw;
                phys.fixed_box(
                    V(cx, WALL_HEIGHT / 2., y as f32 + 0.5 - hh),
                    V(len / 2., WALL_HEIGHT / 2., 0.5),
                    0.,
                    wall,
                );
            }
        }
        let bar = Material { friction: 0.2, restitution: 0.3, ..Default::default() };
        let movers = LEVELS[level]
            .movers
            .iter()
            .map(|m| phys.kinematic_box(mover_pose(&grid, m, 0), V(m.half.0, WALL_HEIGHT * 0.5, m.half.1), 0., bar))
            .collect();
        let marble = Material {
            friction: 0.5,
            restitution: 0.25,
            density: 7.8,
            linear_damping: 0.5,
            angular_damping: 0.3,
            ccd: true,
        };
        let c = grid.centre(grid.start);
        let ball = phys.dynamic_ball(V(c.0, BALL_R + 0.01, c.2), BALL_R, marble);
        (grid, phys, ball, movers)
    }

    pub fn new(_seed: u64) -> Self {
        Self::at(0)
    }

    /// A fresh game starting on `level` (for tests and the `--level` flag).
    pub fn at(level: usize) -> Self {
        Self::at_level(level.min(LEVELS.len() - 1), 0, 0)
    }

    fn at_level(level: usize, score: u64, tick: u64) -> Self {
        let (grid, phys, ball, movers) = Self::build(level);
        let stars_got = vec![false; grid.stars.len()];
        Self {
            tick,
            level,
            grid,
            phys,
            ball,
            movers,
            phase: Phase::Playing,
            tilt: (0., 0.),
            clock: 0,
            falls: 0,
            stars_got,
            score,
            bump_cool: 0,
            events: Vec::new(),
        }
    }

    pub fn ball_pos(&self) -> V {
        self.phys.position(self.ball)
    }

    /// Seconds on the current level's clock.
    pub fn seconds(&self) -> f32 {
        self.clock as f32 * TICK
    }

    pub fn stars(&self) -> u32 {
        self.stars_got.iter().filter(|s| **s).count() as u32
    }

    pub fn gravity_for(tilt: (f32, f32)) -> V {
        V(GRAVITY * tilt.0.sin(), -GRAVITY * tilt.0.cos() * tilt.1.cos(), GRAVITY * tilt.1.sin())
    }

    pub fn mover_pose(&self, i: usize) -> V {
        mover_pose(&self.grid, &LEVELS[self.level].movers[i], self.tick)
    }

    fn respawn(&mut self) {
        let c = self.grid.centre(self.grid.start);
        self.phys.set_position(self.ball, V(c.0, BALL_R + 0.01, c.2));
        self.phys.set_linvel(self.ball, V(0., 0., 0.));
        self.phys.set_angvel(self.ball, V(0., 0., 0.));
    }

    pub fn dist_xz(a: V, b: V) -> f32 {
        ((a.0 - b.0).powi(2) + (a.2 - b.2).powi(2)).sqrt()
    }

    pub fn step(&mut self, input: &Input) {
        match self.phase {
            Phase::Done => return,
            Phase::Cleared(n) => {
                self.tick += 1;
                if n > 1 {
                    self.phase = Phase::Cleared(n - 1);
                } else {
                    *self = Self::at_level(self.level + 1, self.score, self.tick);
                }
                return;
            }
            Phase::Playing => {}
        }
        self.tick += 1;
        self.clock += 1;
        let clamp = |v: f32| if v.is_finite() { v.clamp(-1., 1.) } else { 0. };
        let step = TILT_RATE * TICK;
        let toward = |now: f32, target: f32| now + (target - now).clamp(-step, step);
        self.tilt = (toward(self.tilt.0, clamp(input.x) * MAX_TILT), toward(self.tilt.1, clamp(input.z) * MAX_TILT));
        self.phys.set_gravity(Self::gravity_for(self.tilt));
        for i in 0..self.movers.len() {
            let pose = self.mover_pose(i);
            self.phys.move_kinematic(self.movers[i], pose, 0.);
        }
        let before = self.phys.linvel(self.ball);
        self.phys.step();
        let after = self.phys.linvel(self.ball);
        self.bump_cool = self.bump_cool.saturating_sub(1);
        let jolt = ((after.0 - before.0).powi(2) + (after.2 - before.2).powi(2)).sqrt();
        let horizontal = (before.0 * before.0 + before.2 * before.2).sqrt();
        if self.bump_cool == 0 && jolt > 0.8 && horizontal > 0.8 {
            self.bump_cool = 8;
            self.events.push(Event::Bump { speed: horizontal });
        }

        let p = self.ball_pos();
        let off_board = p.1 < -1. || p.0.abs() > self.grid.w as f32 || p.2.abs() > self.grid.h as f32;
        let in_hole = self.grid.holes.iter().any(|h| Self::dist_xz(p, self.grid.centre(*h)) < HOLE_R);
        if in_hole || off_board {
            self.falls += 1;
            self.clock += (FALL_PENALTY / TICK) as u32;
            self.events.push(Event::Fell { at: V(p.0, 0., p.2) });
            self.respawn();
            return;
        }
        for i in 0..self.grid.stars.len() {
            if !self.stars_got[i] && Self::dist_xz(p, self.grid.centre(self.grid.stars[i])) < STAR_R {
                self.stars_got[i] = true;
                let at = self.grid.centre(self.grid.stars[i]);
                self.events.push(Event::Star { at, total: self.stars() });
            }
        }
        if Self::dist_xz(p, self.grid.centre(self.grid.goal)) < GOAL_R {
            let time = self.seconds();
            let par = LEVELS[self.level].par_secs;
            let points = 1000 + 250 * self.stars() + ((par - time).max(0.) * 10.) as u32;
            self.score += points as u64;
            self.events.push(Event::Cleared { level: self.level, time, points });
            if self.level + 1 < LEVELS.len() {
                self.phase = Phase::Cleared(CLEAR_PAUSE);
            } else {
                self.phase = Phase::Done;
                self.events.push(Event::AllDone { score: self.score });
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
                    h.u64(self.tick).u64(self.score).u32(self.level as u32).u32(self.clock).u32(self.falls);
                    h.f32(self.tilt.0).f32(self.tilt.1).u32(self.bump_cool);
                    for s in &self.stars_got {
                        h.bool(*s);
                    }
                    match self.phase {
                        Phase::Playing => h.u32(0),
                        Phase::Cleared(n) => h.u32(1 + n),
                        Phase::Done => h.u32(u32::MAX),
                    };
                }),
            ),
            ("bodies", one(&|h| self.phys.hash_into(h))),
        ]
    }
}

/// Everything that decides where the game goes next. The bodies carry poses and velocities only (the physics
/// library's contact cache is not saved), so a load is a fair continuation, not a bit-exact one: see
/// `SavePolicy::PhysicsContinuation`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub level: usize,
    pub bodies: Vec<BodyState>,
    pub phase: Phase,
    pub tilt: (f32, f32),
    pub clock: u32,
    pub falls: u32,
    pub stars_got: Vec<bool>,
    pub score: u64,
    pub bump_cool: u32,
}

impl Snapshot for Sim {
    const KIND: &'static str = "tumble-maze";
    const POLICY: SavePolicy = SavePolicy::PhysicsContinuation;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            level: self.level,
            bodies: self.phys.states(),
            phase: self.phase,
            tilt: self.tilt,
            clock: self.clock,
            falls: self.falls,
            stars_got: self.stars_got.clone(),
            score: self.score,
            bump_cool: self.bump_cool,
        }
    }
    /// Refuse a state this game could not have produced; the caller then keeps the running game.
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        if s.level >= LEVELS.len() {
            return Err("the save is for a level this game does not have".into());
        }
        let limit = MAX_TILT + 0.01;
        if !s.tilt.0.is_finite() || !s.tilt.1.is_finite() || s.tilt.0.abs() > limit || s.tilt.1.abs() > limit {
            return Err("the save tilts the board further than it can go".into());
        }
        if s.stars_got.len() != Grid::parse(LEVELS[s.level].map).stars.len() {
            return Err("the save's stars do not match this level".into());
        }
        // Rebuild the physics world from scratch, then put the saved poses back: the library's own state (contact
        // cache, islands) is not in the save, so nothing of the old world may survive a load.
        let mut fresh = Self::at_level(s.level, s.score, s.tick);
        if !fresh.phys.restore(&s.bodies) {
            return Err("the save's bodies do not match this level".into());
        }
        fresh.phase = s.phase;
        fresh.tilt = s.tilt;
        fresh.clock = s.clock;
        fresh.falls = s.falls;
        fresh.stars_got = s.stars_got;
        fresh.bump_cool = s.bump_cool;
        *self = fresh;
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}

/// A simple player: follows the shortest route cell to cell, steering the tilt like a thermostat toward a wanted
/// speed. It does not collect stars, so it is slow but it finishes: the tests use it to prove each level can be
/// beaten by rolling, not only by path search.
pub fn autoplay(sim: &Sim) -> Input {
    if sim.phase != Phase::Playing {
        return Input::default();
    }
    let p = sim.ball_pos();
    let route = sim.grid.route(sim.grid.cell_of(p));
    if route.len() < 2 {
        return Input::default();
    }
    // Aim at the furthest cell in a straight line ahead (up to three), so corners are cut only when it is safe.
    let mut aim = route[1];
    for next in route.iter().skip(2).take(2) {
        let straight =
            (next.0 == route[0].0) == (aim.0 == route[0].0) && (next.1 == route[0].1) == (aim.1 == route[0].1);
        if straight {
            aim = *next;
        } else {
            break;
        }
    }
    let target = sim.grid.centre(aim);
    let (dx, dz) = (target.0 - p.0, target.2 - p.2);
    let dist = (dx * dx + dz * dz).sqrt().max(1e-3);
    let want = (dist * 1.6).min(1.8);
    let v = sim.phys.linvel(sim.ball);
    let cmd = |wanted: f32, have: f32| ((wanted - have) * 1.4).clamp(-1., 1.);
    Input { x: cmd(dx / dist * want, v.0), z: cmd(dz / dist * want, v.2) }
}
