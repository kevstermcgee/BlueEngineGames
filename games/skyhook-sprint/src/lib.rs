//! Skyhook Sprint rules: chain launch pads through three airborne gates without falling.
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller, ControllerState, Movement};
use vesper3d::viewer::devkit::{Rng, Simulation, Snapshot, StateHasher, TICK};

pub const PLATFORM_HALF: f32 = 8.;
pub const VOID_Y: f32 = -8.;
pub const ORBS: usize = 3;
pub const BUMPER_SPEED: f32 = 8.5;
pub const KNOCKBACK: f32 = 0.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input { pub forward: f32, pub right: f32, pub look: [f32; 2], pub jump: bool }

#[derive(Clone, Debug, PartialEq)]
pub enum Event { Collected { at: V, score: u32 }, Bumped { at: V }, Jumped, Landed, Fell, Won }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bumper { pub pos: V, pub vel: V }

pub struct Sim {
    pub tick: u64,
    pub player: Controller,
    pub orbs: Vec<V>,
    pub bumpers: Vec<Bumper>,
    pub score: u32,
    pub over: bool,
    platforms: Vec<Collider>,
    rng: Rng,
    events: Vec<Event>,
    was_grounded: bool,
    bump_cooldown: u32,
}

impl Sim {
    pub fn new(seed: u64) -> Self {
        let mut player = Controller::for_profile(Default::default(), V(0., 0., 7.), 0.)
            .expect("the default profile is valid");
        player.set_floor(None);
        let platforms = vec![
            Collider { min: V(-3., -1., 5.), max: V(3., 0., 8.) },
            Collider { min: V(-3., -0.5, 0.5), max: V(3., 0.5, 3.5) },
            Collider { min: V(-3., 0., -4.), max: V(3., 1., -1.) },
            Collider { min: V(-3., 0.5, -8.), max: V(3., 1.5, -5.5) },
        ];
        Self {
            tick: 0,
            player,
            orbs: vec![V(0., 1.8, 3.8), V(0., 2.3, -0.6), V(0., 2.8, -5.0)],
            bumpers: vec![
                Bumper { pos: V(0., 0.08, 5.7), vel: V(0., 0., -BUMPER_SPEED) },
                Bumper { pos: V(0., 0.58, 1.1), vel: V(0., 0., -BUMPER_SPEED) },
                Bumper { pos: V(0., 1.08, -3.4), vel: V(0., 0., -BUMPER_SPEED) },
            ],
            score: 0,
            over: false,
            platforms,
            rng: Rng::new(seed),
            events: Vec::new(),
            was_grounded: true,
            bump_cooldown: 0,
        }
    }

    pub fn step(&mut self, input: &Input) {
        if self.over { return; }
        self.tick += 1;
        self.player.look(input.look[0], input.look[1], 1., false);
        self.player.update(Movement { forward: input.forward, right: input.right, jump: input.jump, ..Default::default() }, TICK, &self.platforms);
        let grounded = self.player.is_grounded();
        if input.jump && self.was_grounded && !grounded { self.events.push(Event::Jumped); }
        if grounded && !self.was_grounded { self.events.push(Event::Landed); }
        self.was_grounded = grounded;
        self.bump_cooldown = self.bump_cooldown.saturating_sub(1);
        let feet = V(self.player.position.0, self.player.feet_height(), self.player.position.2);
        for pad in &self.bumpers {
            let flat = V(feet.0 - pad.pos.0, 0., feet.2 - pad.pos.2);
            if self.bump_cooldown == 0 && flat.length() < 0.85 && (feet.1 - pad.pos.1).abs() < 0.8 {
                self.player.apply_impulse(pad.vel + V(0., 7.2, 0.));
                self.bump_cooldown = 36;
                self.events.push(Event::Bumped { at: pad.pos });
                break;
            }
        }
        let center = V(feet.0, feet.1 + 0.8, feet.2);
        if let Some(gate) = self.orbs.first().copied() {
            if (gate - center).length() < 1.15 {
                self.orbs.remove(0);
                self.score += 1;
                self.events.push(Event::Collected { at: gate, score: self.score });
                if self.orbs.is_empty() { self.over = true; self.events.push(Event::Won); }
            }
        }
        if self.player.position.1 < VOID_Y { self.over = true; self.events.push(Event::Fell); }
    }

    pub fn drain_events(&mut self) -> Vec<Event> { std::mem::take(&mut self.events) }
}

impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, input: &Input) { Sim::step(self, input); }
    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.u64(self.tick).u32(self.score).bool(self.over);
        let p = self.player.position;
        h.f32(p.0).f32(p.1).f32(p.2).f32(self.player.yaw).f32(self.player.pitch);
        for gate in &self.orbs { h.f32(gate.0).f32(gate.1).f32(gate.2); }
        h.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64, pub player: ControllerState, pub orbs: Vec<V>, pub score: u32, pub over: bool,
    pub rng: Rng, pub was_grounded: bool, pub bump_cooldown: u32,
}

impl Snapshot for Sim {
    const KIND: &'static str = "skyhook-sprint";
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState { tick: self.tick, player: self.player.network_state(), orbs: self.orbs.clone(), score: self.score,
            over: self.over, rng: self.rng.clone(), was_grounded: self.was_grounded, bump_cooldown: self.bump_cooldown }
    }
    fn restore(&mut self, state: SimState) -> Result<(), String> {
        if state.orbs.len() > ORBS || state.orbs.iter().any(|p| p.0.abs() > 20. || p.1.abs() > 20. || p.2.abs() > 20.) {
            return Err("the save contains invalid gates".into());
        }
        self.tick = state.tick;
        self.player.restore_network_state(&state.player);
        self.orbs = state.orbs;
        self.score = state.score;
        self.over = state.over;
        self.rng = state.rng;
        self.was_grounded = state.was_grounded;
        self.bump_cooldown = state.bump_cooldown;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 { self.tick }
}
