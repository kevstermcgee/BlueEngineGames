//! Magnet Mine rules: steer three metal drones into sockets with a polarity-switching field.
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller, ControllerState, Movement};
use vesper3d::viewer::devkit::{Rng, Simulation, Snapshot, StateHasher, TICK};

pub const PLATFORM_HALF: f32 = 7.;
pub const VOID_Y: f32 = -8.;
pub const ORBS: usize = 3;
pub const BUMPER_SPEED: f32 = 5.;
pub const KNOCKBACK: f32 = 2.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input { pub forward: f32, pub right: f32, pub look: [f32; 2], pub jump: bool }

#[derive(Clone, Debug, PartialEq)]
pub enum Event { Collected { at: V, score: u32 }, Bumped { at: V }, Jumped, Landed, Fell, Won }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bumper { pub pos: V, pub vel: V }

pub struct Sim {
    pub tick: u64, pub player: Controller, pub orbs: Vec<V>, pub bumpers: Vec<Bumper>,
    pub score: u32, pub over: bool, ground: Collider, rng: Rng, events: Vec<Event>,
    was_grounded: bool, bump_cooldown: u32, polarity: f32, jump_was_down: bool,
}

impl Sim {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let player = Controller::for_profile(Default::default(), V(0., 0., 5.), 0.).expect("valid profile");
        let mut drones = Vec::new();
        for _ in 0..ORBS {
            drones.push(Bumper { pos: V(rng.range(-4., 4.), 0.45, rng.range(-2., 3.)), vel: V(0., 0., 0.) });
        }
        Self { tick: 0, player, orbs: vec![V(-4.5, 0.45, -4.5), V(0., 0.45, -5.), V(4.5, 0.45, -4.5)],
            bumpers: drones, score: 0, over: false,
            ground: Collider { min: V(-PLATFORM_HALF, -1., -PLATFORM_HALF), max: V(PLATFORM_HALF, 0., PLATFORM_HALF) },
            rng, events: Vec::new(), was_grounded: true, bump_cooldown: 0, polarity: 1., jump_was_down: false }
    }

    pub fn polarity(&self) -> f32 { self.polarity }

    pub fn step(&mut self, input: &Input) {
        if self.over { return; }
        self.tick += 1;
        self.player.look(input.look[0], input.look[1], 1., false);
        self.player.update(Movement { forward: input.forward, right: input.right, jump: false, ..Default::default() }, TICK, std::slice::from_ref(&self.ground));
        if input.jump && !self.jump_was_down {
            self.polarity = -self.polarity;
            self.events.push(Event::Jumped);
        }
        self.jump_was_down = input.jump;
        let p = V(self.player.position.0, 0.45, self.player.position.2);
        for drone in &mut self.bumpers {
            let delta = p - drone.pos;
            let dist = delta.length().max(0.4);
            drone.vel = drone.vel + delta.norm() * (self.polarity * 18. / dist) * TICK;
            drone.vel = drone.vel * 0.992;
            if drone.vel.length() > BUMPER_SPEED { drone.vel = drone.vel.norm() * BUMPER_SPEED; }
            drone.pos = drone.pos + drone.vel * TICK;
            for axis in [0, 2] {
                let v = if axis == 0 { drone.pos.0 } else { drone.pos.2 };
                if v.abs() > PLATFORM_HALF - 0.4 {
                    if axis == 0 { drone.pos.0 = v.clamp(-6.6, 6.6); drone.vel.0 *= -0.8; }
                    else { drone.pos.2 = v.clamp(-6.6, 6.6); drone.vel.2 *= -0.8; }
                }
            }
        }
        let mut matched = None;
        'outer: for (di, drone) in self.bumpers.iter().enumerate() {
            for (si, socket) in self.orbs.iter().enumerate() {
                if (drone.pos - *socket).length() < 0.75 { matched = Some((di, si, *socket)); break 'outer; }
            }
        }
        if let Some((di, si, at)) = matched {
            self.bumpers.remove(di); self.orbs.remove(si); self.score += 1;
            self.events.push(Event::Collected { at, score: self.score });
            if self.orbs.is_empty() { self.over = true; self.events.push(Event::Won); }
        }
        self.bump_cooldown = self.bump_cooldown.saturating_sub(1);
        if self.bump_cooldown == 0 {
            if let Some(d) = self.bumpers.iter().find(|d| (d.pos - p).length() < 0.8) {
                let away = (p - d.pos).norm();
                self.player.apply_impulse(away * KNOCKBACK + V(0., 1., 0.));
                self.bump_cooldown = 24; self.events.push(Event::Bumped { at: d.pos });
            }
        }
    }

    pub fn drain_events(&mut self) -> Vec<Event> { std::mem::take(&mut self.events) }
}

impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, input: &Input) { Sim::step(self, input); }
    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new(); h.u64(self.tick).u32(self.score).bool(self.over).f32(self.polarity);
        let p = self.player.position; h.f32(p.0).f32(p.1).f32(p.2);
        for d in &self.bumpers { h.f32(d.pos.0).f32(d.pos.2).f32(d.vel.0).f32(d.vel.2); }
        for s in &self.orbs { h.f32(s.0).f32(s.2); } h.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState { pub tick: u64, pub player: ControllerState, pub orbs: Vec<V>, pub bumpers: Vec<Bumper>, pub score: u32,
    pub over: bool, pub rng: Rng, pub was_grounded: bool, pub bump_cooldown: u32, pub polarity: f32, pub jump_was_down: bool }

impl Snapshot for Sim {
    const KIND: &'static str = "magnet-mine"; type State = SimState;
    fn capture(&self) -> SimState { SimState { tick:self.tick, player:self.player.network_state(), orbs:self.orbs.clone(), bumpers:self.bumpers.clone(),
        score:self.score, over:self.over, rng:self.rng.clone(), was_grounded:self.was_grounded, bump_cooldown:self.bump_cooldown,
        polarity:self.polarity, jump_was_down:self.jump_was_down } }
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        if s.orbs.len() > ORBS || s.bumpers.len() > ORBS { return Err("too many magnetic pieces".into()); }
        self.tick=s.tick; self.player.restore_network_state(&s.player); self.orbs=s.orbs; self.bumpers=s.bumpers; self.score=s.score;
        self.over=s.over; self.rng=s.rng; self.was_grounded=s.was_grounded; self.bump_cooldown=s.bump_cooldown;
        self.polarity=s.polarity; self.jump_was_down=s.jump_was_down; self.events.clear(); Ok(())
    }
    fn save_tick(&self) -> u64 { self.tick }
}
