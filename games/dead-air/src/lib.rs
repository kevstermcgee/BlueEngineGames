//! Dead Air: the rules. A pure, deterministic, fixed-step simulation with no window, no sound device
//! and no wall clock; `main.rs` presents it.
//!
//! You are the new night operator at the Hollow Pine relay station. Keep the generator fuelled, send
//! the five scheduled all-clear broadcasts before dawn, and stay away from the thing that answers the
//! radio when nobody else does. It cannot see. It listens. A flashlight startles it for a few seconds;
//! so, more reliably, does a bullet — neither one stops it for good.
mod layout;
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller, ControllerState, Movement};
use vesper3d::viewer::devkit::{Rng, SavePolicy, Simulation, Snapshot, StateHasher, TICK};

pub use layout::{
    colliders, AMMO, ARCHIVE_ROOM, BATTERIES, CONTROL_ROOM, CORRIDOR_X, CORRIDOR_Z, DORMITORY, FUEL_CANS, GENERATOR,
    GENERATOR_ROOM, NOTES, PLAYER_SPAWN, RADIO_DESK,
};

pub const NOTE_COUNT: usize = NOTES.len();
pub const BROADCASTS_NEEDED: u32 = 5;
pub const BROADCAST_DUE: [f32; BROADCASTS_NEEDED as usize] = [100., 200., 300., 400., 500.];
pub const SHIFT_LENGTH: f32 = 600.;

pub const GENERATOR_DRAIN_PER_SEC: f32 = 100. / 240.;
pub const FUEL_CAN_AMOUNT: f32 = 90.;
pub const BATTERY_DRAIN_PER_SEC: f32 = 100. / 150.;
pub const BATTERY_PACK_AMOUNT: f32 = 60.;
pub const START_AMMO: u32 = 4;
pub const AMMO_PICKUP_AMOUNT: u32 = 2;

pub const HUNT_SENSE_RADIUS: f32 = 4.0;
pub const CATCH_RADIUS: f32 = 1.1;
pub const CATCH_HOLD_TICKS: u32 = 18;
pub const PATROL_SPEED: f32 = 1.4;
pub const INVESTIGATE_SPEED: f32 = 2.2;
pub const HUNT_SPEED: f32 = 3.6;
pub const POWER_OUT_HUNT_BONUS: f32 = 0.3;
pub const SUSPICION_THRESHOLD: f32 = 1.0;
pub const POWER_OUT_SUSPICION_THRESHOLD: f32 = 0.6;
pub const SUSPICION_DECAY_PER_SEC: f32 = 0.15;
pub const INVESTIGATE_TIMEOUT: f32 = 6.0;
pub const LIGHT_STAGGER_RANGE: f32 = 10.0;
pub const LIGHT_STAGGER_COS: f32 = 0.9659; // cos(15 degrees): half-angle of the flashlight's effective cone
pub const LIGHT_STAGGER_DURATION: f32 = 3.5;
pub const LIGHT_STAGGER_COOLDOWN: f32 = 6.0;
pub const PISTOL_STAGGER_RANGE: f32 = 15.0;
pub const PISTOL_STAGGER_RADIUS: f32 = 0.6;
pub const PISTOL_STAGGER_DURATION: f32 = 5.0;
pub const PISTOL_SHOT_INTERVAL: f32 = 0.4;
pub const WALK_NOISE_RADIUS: f32 = 6.0;
pub const SNEAK_NOISE_RADIUS: f32 = 3.0;
pub const NOISE_STEP_DISTANCE: f32 = 0.6;
pub const GUNSHOT_NOISE_RADIUS: f32 = 20.0;
pub const GUNSHOT_NOISE_SECONDS: f32 = 1.0;
pub const GENERATOR_NOISE_RADIUS: f32 = 10.0;
pub const GENERATOR_NOISE_SECONDS: f32 = 1.0;

/// A cyclic tour of the station's waypoints the Caller walks while undisturbed.
const PATROL_ROUTE: [usize; 20] = [1, 2, 3, 2, 1, 4, 5, 4, 1, 6, 7, 8, 7, 6, 9, 10, 9, 6, 11, 6];

/// One tick of player intent. The window builds it from devices, tests and bots build it directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub sneak: bool,
    /// Radian look deltas since the previous tick: `[yaw, pitch]`.
    pub look: [f32; 2],
    pub flashlight_toggle: bool,
    pub fire: bool,
    pub interact: bool,
}

/// What happened this tick; the window reacts with sound, light and the HUD.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    PickedUpFuel,
    PickedUpBattery,
    PickedUpAmmo,
    ReadNote(usize),
    Refuelled,
    FlashlightOn,
    FlashlightOff,
    FlashlightEmpty,
    GeneratorDied,
    GeneratorRestarted,
    Fired { hit_caller: bool },
    DryFire,
    CallerStaggered,
    BroadcastSent { count: u32 },
    BroadcastRefused,
    Won,
    Caught,
    RanOutOfTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CallerState {
    Patrol,
    Investigate,
    Hunt,
    Staggered,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Caller {
    pub pos: V,
    pub state: CallerState,
    patrol_index: usize,
    target_waypoint: usize,
    route: Vec<usize>,
    route_index: usize,
    investigate_point: Option<V>,
    suspicion: f32,
    time_since_heard: f32,
    stagger_timer: f32,
    light_stagger_cooldown: f32,
    catch_hold: u32,
}

impl Caller {
    fn new() -> Self {
        Self {
            pos: layout::graph().position(11),
            state: CallerState::Patrol,
            patrol_index: 0,
            target_waypoint: 11,
            route: vec![11],
            route_index: 0,
            investigate_point: None,
            suspicion: 0.,
            // A large but finite sentinel (not f32::INFINITY, which cannot round-trip through a JSON
            // save file): comfortably past INVESTIGATE_TIMEOUT from the first tick.
            time_since_heard: 1e6,
            stagger_timer: 0.,
            light_stagger_cooldown: 0.,
            catch_hold: 0,
        }
    }

    fn retarget(&mut self, waypoint: usize) {
        if waypoint == self.target_waypoint && !self.route.is_empty() {
            return;
        }
        self.target_waypoint = waypoint;
        let from = layout::graph().nearest(self.pos);
        self.route = layout::graph().path(from, waypoint);
        self.route_index = if self.route.first() == Some(&from) { 1.min(self.route.len() - 1) } else { 0 };
    }

    /// Walk the current route; true once the final waypoint is reached.
    fn advance_along_route(&mut self, speed: f32, dt: f32) -> bool {
        if self.route.is_empty() {
            return true;
        }
        let waypoint = self.route[self.route_index];
        let to = layout::graph().position(waypoint) - self.pos;
        let flat = V(to.0, 0., to.2);
        let distance = flat.length();
        if distance < 0.25 {
            if self.route_index + 1 < self.route.len() {
                self.route_index += 1;
                return false;
            }
            return true;
        }
        self.pos = self.pos + flat.norm() * (speed * dt).min(distance);
        false
    }

    fn arrived(&self) -> bool {
        self.route_index + 1 >= self.route.len()
    }
}

/// A noise loud enough to echo for a moment after it happens (a gunshot, a generator clang), so the
/// Caller can notice it even a tick or two later.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Echo {
    pos: V,
    radius: f32,
    seconds_left: f32,
}

/// The whole game state.
pub struct Sim {
    pub tick: u64,
    pub player: Controller,
    pub flashlight_on: bool,
    pub flashlight_battery: f32,
    pub ammo: u32,
    pub generator_fuel: f32,
    pub carrying_fuel: bool,
    pub notes_found: [bool; NOTE_COUNT],
    pub broadcasts_done: u32,
    pub shift_seconds: f32,
    pub caller: Caller,
    pub outcome: Option<Outcome>,
    fuel_taken: [bool; 2],
    battery_taken: [bool; 2],
    ammo_taken: bool,
    shot_cooldown: f32,
    muzzle_flash: f32,
    distance_since_step: f32,
    echo: Option<Echo>,
    colliders: Vec<Collider>,
    rng: Rng,
    events: Vec<Event>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Won,
    Caught,
    RanOutOfTime,
}

impl Sim {
    /// A fresh shift; the same seed always plays out the same way (the Caller's patrol start is fixed,
    /// but a future variation could use `seed` to vary pickup placement).
    pub fn new(seed: u64) -> Self {
        let mut player = Controller::for_profile(Default::default(), PLAYER_SPAWN, layout::PLAYER_SPAWN_YAW)
            .expect("the default profile is valid");
        player.set_floor(Some(0.));
        Self {
            tick: 0,
            player,
            flashlight_on: false,
            flashlight_battery: 100.,
            ammo: START_AMMO,
            generator_fuel: 100.,
            carrying_fuel: false,
            notes_found: [false; NOTE_COUNT],
            broadcasts_done: 0,
            shift_seconds: 0.,
            caller: Caller::new(),
            outcome: None,
            fuel_taken: [false; 2],
            battery_taken: [false; 2],
            ammo_taken: false,
            shot_cooldown: 0.,
            muzzle_flash: 0.,
            distance_since_step: 0.,
            echo: None,
            colliders: layout::colliders(),
            rng: Rng::new(seed),
            events: Vec::new(),
        }
    }

    fn feet(&self) -> V {
        V(self.player.position.0, self.player.feet_height(), self.player.position.2)
    }

    /// Whether fuel-can spot `i` has already been collected (so the window can stop drawing its marker).
    pub fn fuel_taken(&self, i: usize) -> bool {
        self.fuel_taken[i]
    }
    /// Whether battery spot `i` has already been collected.
    pub fn battery_taken(&self, i: usize) -> bool {
        self.battery_taken[i]
    }
    /// Whether the ammo spot has already been collected.
    pub fn ammo_taken(&self) -> bool {
        self.ammo_taken
    }

    fn power_out(&self) -> bool {
        self.generator_fuel <= 0.
    }

    /// Advance exactly one 60 Hz tick. A finished shift ignores further input.
    pub fn step(&mut self, input: &Input) {
        if self.outcome.is_some() {
            return;
        }
        self.tick += 1;
        self.shift_seconds += TICK;
        let was_powered = !self.power_out();
        self.generator_fuel = (self.generator_fuel - GENERATOR_DRAIN_PER_SEC * TICK).max(0.);
        if was_powered && self.power_out() {
            self.events.push(Event::GeneratorDied);
        }

        self.player.look(input.look[0], input.look[1], 1., false);
        let speed_scale = if input.sneak { 0.4 } else { 1. };
        let movement = Movement {
            forward: input.forward * speed_scale,
            right: input.right * speed_scale,
            crouch: input.sneak,
            ..Default::default()
        };
        let before = self.feet();
        self.player.update(movement, TICK, &self.colliders);
        let moved = (self.feet() - before).length();

        self.handle_flashlight(input);
        self.handle_pickups_and_interactions(input);
        self.handle_pistol(input);

        self.distance_since_step += moved;
        if self.distance_since_step > NOISE_STEP_DISTANCE {
            self.distance_since_step = 0.;
            let radius = if input.sneak { SNEAK_NOISE_RADIUS } else { WALK_NOISE_RADIUS };
            self.hear(self.feet(), radius);
        }
        if let Some(echo) = &mut self.echo {
            echo.seconds_left -= TICK;
            let (pos, radius) = (echo.pos, echo.radius);
            if echo.seconds_left <= 0. {
                self.echo = None;
            } else {
                self.hear(pos, radius);
            }
        }

        self.step_caller();
        self.check_outcome();
    }

    fn hear(&mut self, at: V, radius: f32) {
        let distance = (self.caller.pos - at).length();
        if distance >= radius || self.caller.state == CallerState::Hunt || self.caller.state == CallerState::Staggered {
            return;
        }
        self.caller.suspicion += (1. - distance / radius) * TICK * 4.;
        self.caller.time_since_heard = 0.;
        self.caller.investigate_point = Some(at);
    }

    fn handle_flashlight(&mut self, input: &Input) {
        if input.flashlight_toggle {
            if self.flashlight_on {
                self.flashlight_on = false;
                self.events.push(Event::FlashlightOff);
            } else if self.flashlight_battery > 0. {
                self.flashlight_on = true;
                self.events.push(Event::FlashlightOn);
            }
        }
        if self.flashlight_on {
            self.flashlight_battery = (self.flashlight_battery - BATTERY_DRAIN_PER_SEC * TICK).max(0.);
            if self.flashlight_battery <= 0. {
                self.flashlight_on = false;
                self.events.push(Event::FlashlightEmpty);
            }
        }
        // A flashlight shone close on the Caller, within its effective cone, startles it.
        if self.flashlight_on && self.caller.light_stagger_cooldown <= 0. && self.caller.state != CallerState::Staggered
        {
            let to_caller = self.caller.pos - self.player.position;
            let distance = to_caller.length();
            if distance > 0.01 && distance < LIGHT_STAGGER_RANGE {
                let facing = self.player.direction();
                if facing.dot(to_caller.norm()) > LIGHT_STAGGER_COS {
                    self.stagger_caller(LIGHT_STAGGER_DURATION);
                    self.caller.light_stagger_cooldown = LIGHT_STAGGER_COOLDOWN;
                }
            }
        }
        self.caller.light_stagger_cooldown = (self.caller.light_stagger_cooldown - TICK).max(0.);
    }

    fn handle_pickups_and_interactions(&mut self, input: &Input) {
        let feet = self.feet();
        for (i, spot) in FUEL_CANS.iter().enumerate() {
            if !self.fuel_taken[i] && !self.carrying_fuel && spot.contains(feet) {
                self.fuel_taken[i] = true;
                self.carrying_fuel = true;
                self.events.push(Event::PickedUpFuel);
            }
        }
        for (i, spot) in BATTERIES.iter().enumerate() {
            if !self.battery_taken[i] && spot.contains(feet) {
                self.battery_taken[i] = true;
                self.flashlight_battery = (self.flashlight_battery + BATTERY_PACK_AMOUNT).min(100.);
                self.events.push(Event::PickedUpBattery);
            }
        }
        if !self.ammo_taken && AMMO.contains(feet) {
            self.ammo_taken = true;
            self.ammo += AMMO_PICKUP_AMOUNT;
            self.events.push(Event::PickedUpAmmo);
        }
        for (i, spot) in NOTES.iter().enumerate() {
            if !self.notes_found[i] && spot.contains(feet) {
                self.notes_found[i] = true;
                self.events.push(Event::ReadNote(i));
            }
        }
        if !input.interact {
            return;
        }
        if self.carrying_fuel && GENERATOR.contains(feet) {
            let was_out = self.power_out();
            self.carrying_fuel = false;
            self.generator_fuel = (self.generator_fuel + FUEL_CAN_AMOUNT).min(100.);
            self.events.push(Event::Refuelled);
            self.echo = Some(Echo {
                pos: GENERATOR.pos,
                radius: GENERATOR_NOISE_RADIUS,
                seconds_left: GENERATOR_NOISE_SECONDS,
            });
            if was_out {
                self.events.push(Event::GeneratorRestarted);
            }
        }
        if RADIO_DESK.contains(feet) {
            let due = self.broadcasts_done < BROADCASTS_NEEDED
                && self.shift_seconds >= BROADCAST_DUE[self.broadcasts_done as usize];
            if due {
                self.broadcasts_done += 1;
                self.events.push(Event::BroadcastSent { count: self.broadcasts_done });
            } else if self.broadcasts_done < BROADCASTS_NEEDED {
                self.events.push(Event::BroadcastRefused);
            }
        }
    }

    fn handle_pistol(&mut self, input: &Input) {
        self.shot_cooldown = (self.shot_cooldown - TICK).max(0.);
        self.muzzle_flash = (self.muzzle_flash - TICK).max(0.);
        if !input.fire || self.shot_cooldown > 0. {
            return;
        }
        self.shot_cooldown = PISTOL_SHOT_INTERVAL;
        if self.ammo == 0 {
            self.events.push(Event::DryFire);
            return;
        }
        self.ammo -= 1;
        self.muzzle_flash = 0.06;
        self.echo =
            Some(Echo { pos: self.player.position, radius: GUNSHOT_NOISE_RADIUS, seconds_left: GUNSHOT_NOISE_SECONDS });
        let ray = self.player.ray();
        let to_caller = self.caller.pos - ray.o;
        let along = to_caller.dot(ray.d);
        let hit = along > 0. && along < PISTOL_STAGGER_RANGE && self.caller.state != CallerState::Staggered && {
            let closest = ray.o + ray.d * along;
            (closest - self.caller.pos).length() < PISTOL_STAGGER_RADIUS
        };
        if hit {
            self.stagger_caller(PISTOL_STAGGER_DURATION);
        }
        self.events.push(Event::Fired { hit_caller: hit });
    }

    fn stagger_caller(&mut self, seconds: f32) {
        self.caller.state = CallerState::Staggered;
        self.caller.stagger_timer = seconds;
        self.caller.catch_hold = 0;
        self.events.push(Event::CallerStaggered);
    }

    fn step_caller(&mut self) {
        if self.caller.state == CallerState::Staggered {
            self.caller.stagger_timer -= TICK;
            if self.caller.stagger_timer <= 0. {
                self.caller.state = CallerState::Investigate;
                self.caller.investigate_point = Some(self.feet());
                self.caller.time_since_heard = 0.;
            }
            return;
        }

        let feet = self.feet();
        let dist_to_player = (self.caller.pos - feet).length();
        if dist_to_player < HUNT_SENSE_RADIUS {
            self.caller.state = CallerState::Hunt;
            self.caller.investigate_point = Some(feet);
        }

        self.caller.time_since_heard += TICK;
        self.caller.suspicion = (self.caller.suspicion - SUSPICION_DECAY_PER_SEC * TICK).max(0.);
        let threshold = if self.power_out() { POWER_OUT_SUSPICION_THRESHOLD } else { SUSPICION_THRESHOLD };
        if self.caller.state == CallerState::Patrol && self.caller.suspicion > threshold {
            self.caller.state = CallerState::Investigate;
            self.caller.suspicion = 0.;
        }
        if self.caller.state == CallerState::Investigate && self.caller.time_since_heard > INVESTIGATE_TIMEOUT {
            self.caller.state = CallerState::Patrol;
            self.caller.investigate_point = None;
        }

        let hunt_bonus = if self.power_out() { POWER_OUT_HUNT_BONUS } else { 0. };
        match self.caller.state {
            CallerState::Patrol => {
                let target = PATROL_ROUTE[self.caller.patrol_index];
                self.caller.retarget(target);
                if self.caller.advance_along_route(PATROL_SPEED, TICK) {
                    self.caller.patrol_index = (self.caller.patrol_index + 1) % PATROL_ROUTE.len();
                }
            }
            CallerState::Investigate => {
                if let Some(point) = self.caller.investigate_point {
                    let target = layout::graph().nearest(point);
                    self.caller.retarget(target);
                    let arrived_node = self.caller.advance_along_route(INVESTIGATE_SPEED, TICK);
                    if arrived_node && self.caller.arrived() {
                        let to = V(point.0, 0., point.2) - self.caller.pos;
                        if to.length() > 0.2 {
                            self.caller.pos = self.caller.pos + to.norm() * (INVESTIGATE_SPEED * TICK).min(to.length());
                        }
                    }
                }
            }
            CallerState::Hunt => {
                let target = layout::graph().nearest(feet);
                self.caller.retarget(target);
                let arrived_node = self.caller.advance_along_route(HUNT_SPEED + hunt_bonus, TICK);
                if arrived_node {
                    let to = V(feet.0, 0., feet.2) - self.caller.pos;
                    if to.length() > 0.05 {
                        self.caller.pos =
                            self.caller.pos + to.norm() * ((HUNT_SPEED + hunt_bonus) * TICK).min(to.length());
                    }
                }
                if dist_to_player < CATCH_RADIUS {
                    self.caller.catch_hold += 1;
                } else {
                    self.caller.catch_hold = 0;
                }
                if dist_to_player > HUNT_SENSE_RADIUS * 2.5 && self.caller.time_since_heard > INVESTIGATE_TIMEOUT {
                    self.caller.state = CallerState::Investigate;
                }
            }
            CallerState::Staggered => unreachable!(),
        }
    }

    fn check_outcome(&mut self) {
        if self.caller.state == CallerState::Hunt && self.caller.catch_hold >= CATCH_HOLD_TICKS {
            self.outcome = Some(Outcome::Caught);
            self.events.push(Event::Caught);
        } else if self.broadcasts_done >= BROADCASTS_NEEDED {
            self.outcome = Some(Outcome::Won);
            self.events.push(Event::Won);
        } else if self.shift_seconds >= SHIFT_LENGTH {
            self.outcome = Some(Outcome::RanOutOfTime);
            self.events.push(Event::RanOutOfTime);
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
        let mut h = StateHasher::new();
        let p = self.player.position;
        h.u64(self.tick)
            .f32(p.0)
            .f32(p.1)
            .f32(p.2)
            .f32(self.player.yaw)
            .f32(self.player.pitch)
            .bool(self.flashlight_on)
            .f32(self.flashlight_battery)
            .u64(u64::from(self.ammo))
            .f32(self.generator_fuel)
            .bool(self.carrying_fuel)
            .u64(u64::from(self.broadcasts_done))
            .f32(self.shift_seconds)
            .f32(self.caller.pos.0)
            .f32(self.caller.pos.2);
        for found in self.notes_found {
            h.bool(found);
        }
        h.finish()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        vec![("state", self.state_hash())]
    }
}

/// Everything that decides where the shift goes next, as plain data for a save file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub player: ControllerState,
    pub flashlight_on: bool,
    pub flashlight_battery: f32,
    pub ammo: u32,
    pub generator_fuel: f32,
    pub carrying_fuel: bool,
    pub notes_found: [bool; NOTE_COUNT],
    pub broadcasts_done: u32,
    pub shift_seconds: f32,
    pub caller: Caller,
    pub outcome: Option<Outcome>,
    pub fuel_taken: [bool; 2],
    pub battery_taken: [bool; 2],
    pub ammo_taken: bool,
    pub shot_cooldown: f32,
    pub distance_since_step: f32,
    pub echo: Option<Echo>,
    pub rng: Rng,
}

impl Snapshot for Sim {
    const KIND: &'static str = "dead_air";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            player: self.player.network_state(),
            flashlight_on: self.flashlight_on,
            flashlight_battery: self.flashlight_battery,
            ammo: self.ammo,
            generator_fuel: self.generator_fuel,
            carrying_fuel: self.carrying_fuel,
            notes_found: self.notes_found,
            broadcasts_done: self.broadcasts_done,
            shift_seconds: self.shift_seconds,
            caller: self.caller.clone(),
            outcome: self.outcome,
            fuel_taken: self.fuel_taken,
            battery_taken: self.battery_taken,
            ammo_taken: self.ammo_taken,
            shot_cooldown: self.shot_cooldown,
            distance_since_step: self.distance_since_step,
            echo: self.echo,
            rng: self.rng.clone(),
        }
    }
    fn restore(&mut self, state: SimState) -> Result<(), String> {
        if state.broadcasts_done > BROADCASTS_NEEDED {
            return Err("the save has more broadcasts than a shift needs".into());
        }
        if !(0. ..=100.).contains(&state.flashlight_battery) || !(0. ..=100.).contains(&state.generator_fuel) {
            return Err("the save's battery or fuel is out of range".into());
        }
        self.tick = state.tick;
        self.player.restore_network_state(&state.player);
        self.flashlight_on = state.flashlight_on;
        self.flashlight_battery = state.flashlight_battery;
        self.ammo = state.ammo;
        self.generator_fuel = state.generator_fuel;
        self.carrying_fuel = state.carrying_fuel;
        self.notes_found = state.notes_found;
        self.broadcasts_done = state.broadcasts_done;
        self.shift_seconds = state.shift_seconds;
        self.caller = state.caller;
        self.outcome = state.outcome;
        self.fuel_taken = state.fuel_taken;
        self.battery_taken = state.battery_taken;
        self.ammo_taken = state.ammo_taken;
        self.shot_cooldown = state.shot_cooldown;
        self.distance_since_step = state.distance_since_step;
        self.echo = state.echo;
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

    fn still() -> Input {
        Input::default()
    }

    #[test]
    fn a_fresh_shift_starts_with_full_resources_and_no_outcome() {
        let sim = Sim::new(1);
        assert_eq!(sim.ammo, START_AMMO);
        assert_eq!(sim.generator_fuel, 100.);
        assert_eq!(sim.flashlight_battery, 100.);
        assert_eq!(sim.broadcasts_done, 0);
        assert!(sim.outcome.is_none());
        assert!(!sim.flashlight_on);
    }

    #[test]
    fn the_same_seed_and_inputs_replay_identically() {
        vesper3d::viewer::devkit::assert_deterministic(|| Sim::new(7), &[still(); 120]);
    }

    #[test]
    fn flashlight_drains_only_while_on_and_cannot_turn_on_empty() {
        let mut sim = Sim::new(1);
        sim.step(&Input { flashlight_toggle: true, ..still() });
        assert!(sim.flashlight_on);
        for _ in 0..60 {
            sim.step(&still());
        }
        assert!(sim.flashlight_battery < 100.);
        sim.flashlight_battery = 0.;
        sim.flashlight_on = false;
        sim.step(&Input { flashlight_toggle: true, ..still() });
        assert!(!sim.flashlight_on, "no battery, no light");
    }

    #[test]
    fn generator_drains_and_a_carried_can_refuels_it_at_the_generator() {
        let mut sim = Sim::new(1);
        for _ in 0..(241 * 60) {
            sim.step(&still());
        }
        assert_eq!(sim.generator_fuel, 0.);
        sim.player.position = GENERATOR.pos;
        sim.carrying_fuel = true;
        sim.step(&Input { interact: true, ..still() });
        assert!(sim.generator_fuel > 0.);
        assert!(!sim.carrying_fuel);
    }

    #[test]
    fn picking_up_a_fuel_can_requires_visiting_its_spot_and_is_one_shot() {
        let mut sim = Sim::new(1);
        sim.player.position = FUEL_CANS[0].pos;
        sim.step(&still());
        assert!(sim.carrying_fuel);
        assert!(sim.fuel_taken[0]);
        sim.carrying_fuel = false;
        sim.step(&still());
        assert!(!sim.carrying_fuel, "an already-taken can gives nothing twice");
    }

    #[test]
    fn broadcasts_only_count_once_due_and_winning_requires_all_five() {
        let mut sim = Sim::new(1);
        sim.player.position = RADIO_DESK.pos;
        sim.step(&Input { interact: true, ..still() });
        assert_eq!(sim.broadcasts_done, 0, "too early");
        sim.shift_seconds = BROADCAST_DUE[0];
        sim.step(&Input { interact: true, ..still() });
        assert_eq!(sim.broadcasts_done, 1);
        sim.broadcasts_done = BROADCASTS_NEEDED - 1;
        sim.shift_seconds = BROADCAST_DUE[BROADCASTS_NEEDED as usize - 1];
        sim.step(&Input { interact: true, ..still() });
        assert_eq!(sim.outcome, Some(Outcome::Won));
    }

    #[test]
    fn running_out_the_clock_without_finishing_loses() {
        let mut sim = Sim::new(1);
        sim.shift_seconds = SHIFT_LENGTH - TICK;
        sim.step(&still());
        assert_eq!(sim.outcome, Some(Outcome::RanOutOfTime));
    }

    #[test]
    fn noise_draws_the_caller_toward_it_and_proximity_forces_a_hunt() {
        let mut sim = Sim::new(1);
        sim.caller.pos = layout::graph().position(3);
        sim.caller.target_waypoint = 3;
        sim.caller.route = vec![3];
        sim.caller.route_index = 0;
        sim.player.position = layout::graph().position(2);
        for _ in 0..300 {
            sim.hear(sim.feet(), WALK_NOISE_RADIUS);
            sim.step_caller();
        }
        assert_ne!(sim.caller.state, CallerState::Patrol, "a nearby, repeated noise should draw attention");
    }

    #[test]
    fn a_pistol_hit_staggers_the_caller_and_spends_ammo() {
        let mut sim = Sim::new(1);
        sim.player.position = V(0., vesper3d::viewer::controller::EYE_HEIGHT, 0.);
        sim.player.yaw = 0.;
        sim.player.pitch = 0.;
        sim.caller.pos = sim.player.position + sim.player.direction() * 5.;
        sim.step(&Input { fire: true, ..still() });
        assert_eq!(sim.ammo, START_AMMO - 1);
        assert_eq!(sim.caller.state, CallerState::Staggered);
    }

    #[test]
    fn dry_fire_with_no_ammo_does_not_panic_or_stagger() {
        let mut sim = Sim::new(1);
        sim.ammo = 0;
        sim.caller.pos = sim.player.position + sim.player.direction() * 5.;
        sim.step(&Input { fire: true, ..still() });
        assert_eq!(sim.ammo, 0);
        assert_ne!(sim.caller.state, CallerState::Staggered);
    }

    #[test]
    fn sustained_proximity_in_hunt_mode_catches_the_player() {
        let mut sim = Sim::new(1);
        sim.caller.state = CallerState::Hunt;
        sim.caller.pos = sim.feet();
        for _ in 0..(CATCH_HOLD_TICKS + 1) {
            sim.step(&still());
        }
        assert_eq!(sim.outcome, Some(Outcome::Caught));
    }

    #[test]
    fn a_staggered_caller_cannot_catch_the_player_even_at_point_blank() {
        let mut sim = Sim::new(1);
        sim.caller.pos = sim.feet();
        sim.stagger_caller(LIGHT_STAGGER_DURATION);
        for _ in 0..(CATCH_HOLD_TICKS + 5) {
            sim.step(&still());
        }
        assert!(sim.outcome.is_none());
    }
}
