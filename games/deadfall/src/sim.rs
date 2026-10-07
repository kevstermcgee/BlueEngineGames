//! The authoritative match: players, weapons, projectiles, pickups, scoring. Pure, seeded and stepped at 60 Hz
//! by the network server (or by a local server thread for solo play). Nothing here touches a window, a clock or
//! the network; it reaches the outside world only through [`Event`]s and [`Snapshot`](crate::netgame::Snapshot)s.
use crate::bots::{self, BotState};
use crate::hands::{self, Ctx, Gun, Hands, Inventory, Out, Sel, DT};
use crate::input::Input;
use crate::level::{Level, Material};
use crate::team::Team;
use crate::weapons::{self, Class, Effect, Slot, WeaponDef, WeaponId};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller};
use vesper3d::viewer::devkit::Rng;
use vesper3d::viewer::profile::ControllerProfile;

pub const TICK_HZ: u64 = 60;
pub const MAX_PLAYERS: usize = 12;
pub const TEAM_SIZE: usize = 6;
/// Seconds between dying and returning: the killcam runs for all of it.
pub const KILLCAM_SECONDS: f32 = 4.;
pub const RESPAWN_TICKS: u32 = (KILLCAM_SECONDS * 60.) as u32;
pub const PROTECT_TICKS: u32 = 150;
/// How far back (ticks) lag compensation may rewind other players.
pub const MAX_REWIND: u32 = 20;
/// After the match is decided the server keeps running this long so the last kill is seen.
pub const OVER_LINGER: u32 = 240;
/// Below this height a player has left the map.
pub const KILL_PLANE: f32 = -20.;
pub const START_ARMOR: f32 = 100.;
pub const GRAVITY: f32 = 9.8;
pub const PICKUP_RANGE: f32 = 1.5;
pub const DROP_LIFETIME: u32 = 60 * 45;

/// `DEADFALL_AUTOPILOT=1` makes a bot drive every human slot (for screenshots and soak runs nobody plays).
pub fn autopilot() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("DEADFALL_AUTOPILOT").is_some())
}

// ---- match settings -------------------------------------------------------------------------------------------

/// What ends the match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndRule {
    /// Whoever leads when this many minutes are up.
    Time { minutes: u16 },
    /// The first team to this many kills.
    Kills { target: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub end: EndRule,
    /// Fill the teams with bots up to six a side. Off by default: only the people who joined play.
    pub bots: bool,
    /// 0 easy, 1 normal, 2 hard.
    pub bot_skill: u8,
    pub mode: crate::modes::GameMode,
    pub map: crate::maps::MapId,
    pub duel: bool,
    pub objective_target: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            end: EndRule::Kills { target: 40 },
            bots: false,
            bot_skill: 1,
            mode: crate::modes::GameMode::TeamDeathmatch,
            map: crate::maps::MapId::Slagworks,
            duel: false,
            objective_target: 3,
        }
    }
}

static SETTINGS: Mutex<Settings> = Mutex::new(Settings {
    end: EndRule::Kills { target: 40 },
    bots: false,
    bot_skill: 1,
    mode: crate::modes::GameMode::TeamDeathmatch,
    map: crate::maps::MapId::Slagworks,
    duel: false,
    objective_target: 3,
});

/// The settings the next match uses. The network kit starts matches through a static function, so a server
/// process says how it wants them here before its first match.
pub fn set_settings(s: Settings) {
    // Build collision/navigation before accepting players, rather than hitching the first match tick.
    let _ = world_on(s.map);
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = s;
}

pub fn settings() -> Settings {
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner())
}

// ---- the world every match shares -----------------------------------------------------------------------------

/// The map and everything derived from it, built once per process.
pub struct World {
    pub level: Level,
    pub colliders: Vec<Collider>,
    pub nav: crate::nav::Nav,
}

/// Build a derived world for a test arena.
pub fn world_for(level: Level) -> Arc<World> {
    let colliders = level.colliders();
    let nav = crate::nav::Nav::build(&level);
    Arc::new(World { level, colliders, nav })
}
pub fn world() -> Arc<World> {
    world_on(crate::maps::MapId::Slagworks)
}
pub fn world_on(map: crate::maps::MapId) -> Arc<World> {
    static WORLDS: [OnceLock<Arc<World>>; 3] = [const { OnceLock::new() }; 3];
    WORLDS[map as usize].get_or_init(|| world_for(map.level().clone())).clone()
}

// ---- bodies ---------------------------------------------------------------------------------------------------

pub fn profile() -> ControllerProfile {
    ControllerProfile {
        height: 1.8,
        crouched_height: 1.1,
        radius: 0.23,
        eye_height: 1.68,
        walk_speed: 7.2,
        sprint_speed: 7.2,
        crouch_speed: 2.8,
        jump_height: 0.85,
    }
}

/// A fresh controller standing at `feet`.
pub fn new_body(feet: V, yaw: f32) -> Controller {
    let mut c = Controller::for_profile(profile(), feet, yaw).expect("the player profile is valid");
    c.set_gravity(GRAVITY);
    c.set_floor(None);
    c
}

/// One tick of movement: the shared function the server and a predicting client both run.
pub fn step_body(ctrl: &mut Controller, input: &Input, speed_scale: f32, colliders: &[Collider]) {
    ctrl.yaw = input.yaw;
    ctrl.pitch = input.pitch;
    ctrl.update(input.movement(speed_scale), DT, colliders);
}

/// Horizontal speed as a fraction of the run speed.
pub fn speed_fraction(ctrl: &Controller) -> f32 {
    let v = ctrl.velocity();
    ((v.0 * v.0 + v.2 * v.2).sqrt() / profile().walk_speed).clamp(0., 1.)
}

/// Head sphere centre and radius for a body whose eye is at `eye`.
pub fn head_of(eye: V) -> (V, f32) {
    (V(eye.0, eye.1 - 0.05, eye.2), 0.135)
}

// ---- players --------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Player {
    pub slot: usize,
    pub team: Team,
    /// Cosmetic avatar (bits 1..2) and skin (bits 3..4), with team in bit 0.
    pub appearance: u8,
    pub name: String,
    pub ctrl: Controller,
    pub alive: bool,
    pub health: f32,
    pub armor: f32,
    pub inv: Inventory,
    pub hands: Hands,
    pub last_input: Input,
    pub bot: Option<BotState>,
    /// A human drives this slot (false once they leave and a bot takes over, or for a bot from the start).
    pub human: bool,
    pub kills: u16,
    pub deaths: u16,
    pub headshots: u16,
    pub damage: u32,
    pub died_at: u32,
    pub killed_by: Option<u8>,
    pub killed_with: WeaponId,
    pub protect_until: u32,
    pub flash_until: u32,
    pub flash_total: u32,
    pub last_shot_tick: u32,
    pub foot_phase: f32,
}

impl Player {
    pub fn eye(&self) -> V {
        self.ctrl.position
    }
    pub fn feet(&self) -> V {
        V(self.ctrl.position.0, self.ctrl.feet_height(), self.ctrl.position.2)
    }
    pub fn crouched(&self) -> bool {
        self.ctrl.is_crouched()
    }
    pub fn weapon(&self) -> WeaponId {
        self.hands.weapon(&self.inv)
    }
}

/// Where a body was at one tick, for lag compensation.
#[derive(Clone, Copy, Debug, Default)]
pub struct PoseRecord {
    pub eye: V,
    pub feet: f32,
    pub alive: bool,
}

// ---- things in the world --------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Projectile {
    pub id: u16,
    pub weapon: WeaponId,
    pub owner: u8,
    pub pos: V,
    pub vel: V,
    pub age: u32,
    pub fuse: u32,
    pub bounces: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneKind {
    Smoke = 0,
    Fire = 1,
}

#[derive(Clone, Copy, Debug)]
pub struct Zone {
    pub kind: ZoneKind,
    pub pos: V,
    pub radius: f32,
    pub until: u32,
    pub owner: u8,
    pub dps: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LootState {
    pub weapon: WeaponId,
    pub available: bool,
    pub back_at: u32,
}

/// A weapon lying where someone dropped it.
#[derive(Clone, Copy, Debug)]
pub struct Dropped {
    pub id: u16,
    pub gun: Gun,
    pub grenade: bool,
    pub pos: V,
    pub expires: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RosterEntry {
    pub slot: u8,
    pub team: u8,
    pub bot: bool,
    pub name: String,
}

/// What happened this tick, for sound and effects. Every client receives every event once.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Roster(Vec<RosterEntry>),
    Shot { shooter: u8, weapon: u8, from: V, to: V, hit: u8, material: u8 },
    Hurt { victim: u8, attacker: u8, damage: u8, head: bool, weapon: u8, from: V },
    Kill { killer: u8, victim: u8, weapon: u8, head: bool },
    Blast { pos: V, kind: u8, radius: f32 },
    Strike { attacker: u8, weapon: u8, heavy: bool, hit: bool },
    Launch { shooter: u8, weapon: u8, from: V },
    Throw { thrower: u8, weapon: u8 },
    Pickup { player: u8, weapon: u8 },
    Dropped { player: u8, weapon: u8 },
    Spawned { player: u8 },
    Over { winner: u8, scores: [u16; 2] },
}

/// Hit classification carried by [`Event::Shot`].
pub mod hit {
    pub const NONE: u8 = 0;
    pub const WORLD: u8 = 1;
    pub const BODY: u8 = 2;
    pub const HEAD: u8 = 3;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Live,
    Over { winner: Option<Team>, at: u32 },
}

// ---- the match ------------------------------------------------------------------------------------------------

pub struct Match {
    pub world: Arc<World>,
    pub settings: Settings,
    pub tick: u32,
    pub phase: Phase,
    pub players: Vec<Player>,
    pub projectiles: Vec<Projectile>,
    pub zones: Vec<Zone>,
    pub loot: Vec<LootState>,
    pub dropped: Vec<Dropped>,
    pub scores: [u16; 2],
    pub objective: crate::modes::Objectives,
    pub winner_slot: u8,
    pub rng: Rng,
    pub events: Vec<Event>,
    pub history: VecDeque<Vec<PoseRecord>>,
    next_id: u16,
}

impl Match {
    /// Start a match. `humans` are `(team choice, name)` in seat order; returns the match and each human's slot.
    pub fn new(seed: u64, humans: &[(u8, String)], settings: Settings) -> (Match, Vec<usize>) {
        Self::new_in(world_on(settings.map), seed, humans, settings)
    }

    /// [`Match::new`] on a chosen world.
    pub fn new_in(
        world: Arc<World>,
        seed: u64,
        humans: &[(u8, String)],
        mut settings: Settings,
    ) -> (Match, Vec<usize>) {
        if settings.duel {
            settings.bots = false;
        }
        let humans = &humans[..humans.len().min(if settings.duel { 2 } else { MAX_PLAYERS })];
        let mut rng = Rng::new(seed);
        // Team placement: honour the choice, then move the overflow to the other side (at most six a side).
        let mut teams: Vec<usize> = humans.iter().map(|(c, _)| (*c as usize) & 1).collect();
        let mut counts = [0usize; 2];
        for t in &teams {
            counts[*t] += 1;
        }
        // Balance human teams before bots: two friends choosing the same team still get a fair duel.
        for i in (0..teams.len()).rev() {
            let t = teams[i];
            if counts[t] > counts[1 - t] + 1 {
                counts[t] -= 1;
                counts[1 - t] += 1;
                teams[i] = 1 - t;
            }
        }
        let mut players: Vec<Player> = Vec::new();
        let mut slots = Vec::new();
        for (i, (_, name)) in humans.iter().enumerate().take(MAX_PLAYERS) {
            slots.push(players.len());
            let mut p = blank_player(players.len(), Team::from_index(teams[i]), name.clone(), !autopilot());
            p.appearance = (humans[i].0 & 30) | teams[i] as u8;
            if autopilot() {
                p.bot = Some(BotState::new(2, &mut rng));
            }
            players.push(p);
        }
        if settings.bots {
            let mut have = [0usize; 2];
            for p in &players {
                have[p.team.index()] += 1;
            }
            let mut n = 0;
            for (t, count) in have.iter_mut().enumerate() {
                while *count < TEAM_SIZE && players.len() < MAX_PLAYERS {
                    let slot = players.len();
                    let mut p = blank_player(slot, Team::from_index(t), bots::name(n), false);
                    p.appearance = ((slot as u8 % 4) << 1) | (((slot as u8 / 3) % 4) << 3) | t as u8;
                    p.bot = Some(BotState::new(settings.bot_skill, &mut rng));
                    players.push(p);
                    *count += 1;
                    n += 1;
                }
            }
        }
        let loot =
            world.level.loot.iter().map(|l| LootState { weapon: l.weapon, available: true, back_at: 0 }).collect();
        let mut m = Match {
            world,
            settings,
            tick: 0,
            phase: Phase::Live,
            players,
            projectiles: Vec::new(),
            zones: Vec::new(),
            loot,
            dropped: Vec::new(),
            scores: [0, 0],
            objective: crate::modes::Objectives::new(settings.map.bases()),
            winner_slot: 255,
            rng,
            events: Vec::new(),
            history: VecDeque::new(),
            next_id: 1,
        };
        for slot in 0..m.players.len() {
            m.spawn(slot);
        }
        let roster = m
            .players
            .iter()
            .map(|p| RosterEntry {
                slot: p.slot as u8,
                team: p.team.index() as u8,
                bot: !p.human,
                name: p.name.clone(),
            })
            .collect();
        m.events.push(Event::Roster(roster));
        (m, slots)
    }

    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::Over { at, .. } if self.tick >= at + OVER_LINGER)
    }

    /// Seconds left in a timed match (`None` for a kill target).
    pub fn seconds_left(&self) -> Option<f32> {
        match self.settings.end {
            EndRule::Time { minutes } => Some((minutes as f32 * 60. - self.tick as f32 / 60.).max(0.)),
            EndRule::Kills { .. } => None,
        }
    }

    fn fresh_id(&mut self) -> u16 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    // ---- spawning -------------------------------------------------------------------------------------------

    /// Put a player at the team spawn that is farthest from every living enemy (and not crowded by a friend).
    pub fn spawn(&mut self, slot: usize) {
        let team = self.players[slot].team;
        let all_spawns;
        let spawns = if self.settings.mode == crate::modes::GameMode::FreeForAll {
            all_spawns = self.world.level.spawns.iter().flatten().copied().collect::<Vec<_>>();
            &all_spawns
        } else {
            &self.world.level.spawns[team.index()]
        };
        let enemies: Vec<V> =
            self.players.iter().filter(|p| p.alive && self.enemies(slot, p.slot)).map(|p| p.feet()).collect();
        let friends: Vec<V> = self.players.iter().filter(|p| p.alive && p.slot != slot).map(|p| p.feet()).collect();
        let mut best = (f32::MIN, 0usize);
        for (i, s) in spawns.iter().enumerate() {
            let crowded = friends.iter().any(|f| (*f - s.pos).length() < 1.2);
            let clear = enemies.iter().map(|e| (*e - s.pos).length()).fold(f32::MAX, f32::min);
            let jitter = self.rng.range(0., 3.);
            let score = if crowded { -1000. } else { clear.min(80.) + jitter };
            if score > best.0 {
                best = (score, i);
            }
        }
        let s = spawns.get(best.1).copied().unwrap_or(crate::level::Spawn { pos: V(0., 0., 0.), yaw: 0. });
        let p = &mut self.players[slot];
        p.ctrl = new_body(s.pos, s.yaw);
        p.alive = true;
        p.health = weapons::MAX_HEALTH;
        p.armor = START_ARMOR;
        p.inv = Inventory::starting();
        p.hands = Hands::new(&p.inv);
        p.hands.seen = seen_from(&p.last_input);
        p.protect_until = self.tick + PROTECT_TICKS;
        p.flash_until = 0;
        p.killed_by = None;
        // Testing aid: `DEADFALL_GIVE=awm,frag,axe` starts every human with these weapons.
        if p.human || autopilot() {
            if let Some(list) = std::env::var_os("DEADFALL_GIVE") {
                let keys: Vec<String> = list.to_string_lossy().split(',').map(|k| k.trim().to_string()).collect();
                for key in keys {
                    if let Some(id) = weapons::id_of(&key) {
                        let _ = self.give(slot, id, None);
                    }
                }
                let p = &mut self.players[slot];
                p.hands = Hands::new(&p.inv);
            }
        }
        self.events.push(Event::Spawned { player: slot as u8 });
    }

    // ---- the tick ---------------------------------------------------------------------------------------------

    /// Advance one tick. `inputs[i]` is slot `i`'s input if a human drives it.
    pub fn step(&mut self, inputs: &[Option<Input>]) {
        self.tick += 1;
        let world = self.world.clone();
        for slot in 0..self.players.len() {
            let input = match (inputs.get(slot).copied().flatten(), self.players[slot].human) {
                (Some(i), true) => i,
                _ => {
                    let mut bot = self.players[slot].bot.take();
                    let i = match bot.as_mut() {
                        Some(b) => bots::think(self, slot, b),
                        None => Input::default(),
                    };
                    self.players[slot].bot = bot;
                    i
                }
            };
            if self.players[slot].alive {
                self.step_player(slot, input, &world);
            } else {
                self.players[slot].last_input = input;
                if self.phase == Phase::Live
                    && self.settings.mode != crate::modes::GameMode::SearchDestroy
                    && self.tick >= self.players[slot].died_at + RESPAWN_TICKS
                {
                    self.spawn(slot);
                }
            }
        }
        self.step_projectiles(&world);
        self.step_zones();
        self.step_loot();
        self.record_history();
        self.step_objectives();
        self.check_end();
    }

    fn record_history(&mut self) {
        let rec = self
            .players
            .iter()
            .map(|p| PoseRecord { eye: p.eye(), feet: p.ctrl.feet_height(), alive: p.alive })
            .collect();
        self.history.push_back(rec);
        while self.history.len() > MAX_REWIND as usize + 2 {
            self.history.pop_front();
        }
    }

    fn check_end(&mut self) {
        if self.phase != Phase::Live {
            return;
        }
        if self.settings.mode == crate::modes::GameMode::SearchDestroy {
            return;
        }
        if self.settings.mode == crate::modes::GameMode::FreeForAll {
            let target = match self.settings.end {
                EndRule::Kills { target } => target,
                _ => u16::MAX,
            };
            let timed = self.seconds_left().is_some_and(|t| t <= 0.);
            if timed || self.players.iter().any(|p| p.kills >= target) {
                let top = self.players.iter().map(|p| p.kills).max().unwrap_or(0);
                let mut leaders = self.players.iter().filter(|p| p.kills == top);
                self.winner_slot = leaders.next().map_or(255, |p| p.slot as u8);
                if leaders.next().is_some() {
                    self.winner_slot = 255;
                }
                self.finish(None, 2);
            }
            return;
        }
        if self.settings.mode == crate::modes::GameMode::CaptureFlag
            && self.scores.iter().any(|s| *s >= self.settings.objective_target)
        {
            let t = usize::from(self.scores[1] > self.scores[0]);
            self.finish(Some(Team::from_index(t)), t as u8);
            return;
        }
        let winner = match self.settings.end {
            EndRule::Kills { target } => {
                if self.settings.mode == crate::modes::GameMode::TeamDeathmatch
                    && (self.scores[0] >= target || self.scores[1] >= target)
                {
                    Some(if self.scores[0] >= self.scores[1] { Some(Team::Ironclad) } else { Some(Team::Nightwatch) })
                } else {
                    None
                }
            }
            EndRule::Time { minutes } => {
                if self.tick >= minutes as u32 * 3600 {
                    Some(match self.scores[0].cmp(&self.scores[1]) {
                        std::cmp::Ordering::Greater => Some(Team::Ironclad),
                        std::cmp::Ordering::Less => Some(Team::Nightwatch),
                        std::cmp::Ordering::Equal => None,
                    })
                } else {
                    None
                }
            }
        };
        if let Some(w) = winner {
            self.finish(w, w.map_or(2, |t| t.index() as u8));
        }
    }

    pub(crate) fn finish(&mut self, winner: Option<Team>, code: u8) {
        self.phase = Phase::Over { winner, at: self.tick };
        self.events.push(Event::Over { winner: code, scores: self.scores });
    }

    /// A human left: a bot takes the slot over.
    pub fn release(&mut self, slot: usize) {
        if self.settings.duel {
            if let Some(p) = self.players.get_mut(slot) {
                p.human = false;
                p.alive = false;
            }
            if self.phase == Phase::Live {
                let winner = self.players.iter().find(|p| p.human).map(|p| (p.team, p.slot as u8));
                if let Some((team, winner_slot)) = winner {
                    self.winner_slot = winner_slot;
                    self.finish(Some(team), team.index() as u8);
                }
            }
            return;
        }
        if let Some(p) = self.players.get_mut(slot) {
            p.human = false;
            if p.bot.is_none() {
                let mut rng = Rng::new(self.rng.next_u64());
                p.bot = Some(BotState::new(self.settings.bot_skill, &mut rng));
            }
            p.name = format!("{} (bot)", p.name);
        }
    }

    // ---- one player -------------------------------------------------------------------------------------------

    fn step_player(&mut self, slot: usize, mut input: Input, world: &Arc<World>) {
        if self.phase != Phase::Live
            || self.settings.mode == crate::modes::GameMode::SearchDestroy
                && (self.objective.phase == 0 || self.objective.phase == 3)
        {
            input.buttons = 0;
            input.right = 0;
            input.forward = 0;
        }
        let weapon_speed = {
            let p = &self.players[slot];
            weapons::get(p.weapon()).map_or(1., |d| d.move_speed)
        };
        {
            let p = &mut self.players[slot];
            step_body(&mut p.ctrl, &input, weapon_speed, &world.colliders);
            // The controller has no foot phase; a simple odometer drives footsteps on the client.
            let v = p.ctrl.velocity();
            p.foot_phase += (v.0 * v.0 + v.2 * v.2).sqrt() * DT;
            if p.ctrl.position.1 < KILL_PLANE {
                p.health = 0.;
            }
        }
        if self.players[slot].health <= 0. {
            self.kill(slot, None, 0, false);
            return;
        }
        let ctx = {
            let p = &self.players[slot];
            Ctx { speed_frac: speed_fraction(&p.ctrl), crouched: p.crouched(), airborne: !p.ctrl.is_grounded() }
        };
        let outs = {
            let p = &mut self.players[slot];
            let mut inv = p.inv;
            let outs = p.hands.tick(&mut inv, &input, &ctx);
            p.inv = inv;
            outs
        };
        // Pickups and drops.
        let use_pressed = input.use_seq != self.players[slot].hands.seen.use_;
        let drop_pressed = input.drop_seq != self.players[slot].hands.seen.drop;
        {
            let seen = &mut self.players[slot].hands.seen;
            seen.use_ = input.use_seq;
            seen.drop = input.drop_seq;
        }
        if drop_pressed {
            self.drop_selected(slot);
        }
        self.pickups(slot, use_pressed);
        self.players[slot].last_input = input;
        for out in outs {
            self.handle_out(slot, out, &input);
        }
    }

    fn handle_out(&mut self, slot: usize, out: Out, input: &Input) {
        match out {
            Out::Shot { weapon, pellets, punch, spread } => {
                if let Some(def) = weapons::get(weapon) {
                    self.fire_hitscan(slot, def, weapon, pellets, punch, spread, input);
                }
            }
            Out::Launch { weapon, punch } => {
                if let Some(def) = weapons::get(weapon) {
                    self.launch(slot, def, weapon, punch, input);
                }
            }
            Out::Throw { weapon, lob } => {
                if let Some(def) = weapons::get(weapon) {
                    self.throw(slot, def, weapon, lob, input);
                }
            }
            Out::Strike { weapon, heavy } => {
                if let Some(def) = weapons::get(weapon) {
                    self.strike(slot, def, weapon, heavy);
                }
            }
            Out::Dry { .. } | Out::ReloadStarted { .. } | Out::Switched { .. } => {}
        }
        self.players[slot].last_shot_tick = self.tick;
    }

    // ---- shooting ---------------------------------------------------------------------------------------------

    fn aim_dir(yaw: f32, pitch: f32) -> V {
        V(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos())
    }

    /// The pose of `slot` as the shooter saw it `back` ticks ago.
    fn pose_back(&self, slot: usize, back: u32) -> PoseRecord {
        let cur = PoseRecord {
            eye: self.players[slot].eye(),
            feet: self.players[slot].ctrl.feet_height(),
            alive: self.players[slot].alive,
        };
        if back == 0 || self.history.is_empty() {
            return cur;
        }
        let idx = self.history.len() as i64 - back as i64;
        if idx < 0 {
            return cur;
        }
        self.history.get(idx as usize).and_then(|r| r.get(slot)).copied().unwrap_or(cur)
    }

    fn rewind_for(&self, input: &Input) -> u32 {
        let behind = (self.tick as u16).wrapping_sub(input.seen_tick) as u32;
        behind.min(MAX_REWIND)
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_hitscan(
        &mut self,
        slot: usize,
        def: &WeaponDef,
        weapon: WeaponId,
        pellets: u8,
        punch: (f32, f32),
        spread: f32,
        input: &Input,
    ) {
        let origin = self.players[slot].eye();
        let back = self.rewind_for(input);
        let mut events_to: Option<(V, u8, u8)> = None;
        let mut per_victim: Vec<(usize, f32, bool)> = Vec::new();
        for _ in 0..pellets.max(1) {
            // The aim is the view plus the spray's kick, then a random offset inside the spread cone.
            let (mut yaw, mut pitch) = (input.yaw + punch.1, input.pitch + punch.0);
            if spread > 0. {
                let r = spread.to_radians() * self.rng.f32().sqrt();
                let a = self.rng.range(0., std::f32::consts::TAU);
                yaw += r * a.cos() / pitch.cos().abs().max(0.2);
                pitch += r * a.sin();
            }
            let dir = Self::aim_dir(yaw, pitch.clamp(-1.55, 1.55));
            let shot = self.trace(slot, origin, dir, def.range_m * 2.5, back);
            let dist = shot.distance;
            if let Some((victim, part)) = shot.victim {
                let head = part == Part::Head;
                let mult = match part {
                    Part::Head => def.head_mult,
                    Part::Torso => 1.,
                    Part::Legs => 0.75,
                };
                let dmg = def.damage_at(dist) * mult;
                match per_victim.iter_mut().find(|v| v.0 == victim) {
                    Some(v) => {
                        v.1 += dmg;
                        v.2 |= head;
                    }
                    None => per_victim.push((victim, dmg, head)),
                }
                if events_to.is_none() {
                    events_to = Some((origin + dir * dist, if head { hit::HEAD } else { hit::BODY }, 0));
                }
            } else if events_to.is_none() {
                let (kind, mat) = match shot.world {
                    Some(b) => (hit::WORLD, material_code(self.world.level.blocks[b].material)),
                    None => (hit::NONE, 0),
                };
                events_to = Some((origin + dir * dist, kind, mat));
            }
        }
        let (to, kind, mat) = events_to.unwrap_or((origin, hit::NONE, 0));
        let from = origin + Self::aim_dir(input.yaw, input.pitch) * 0.6;
        self.events.push(Event::Shot { shooter: slot as u8, weapon, from, to, hit: kind, material: mat });
        for (victim, dmg, head) in per_victim {
            self.damage(victim, Some(slot), weapon, dmg, head, def.armor_pen, origin);
        }
    }

    /// Nearest thing a bullet from `origin` along `dir` hits within `max`: a player part or a block.
    pub fn trace(&self, shooter: usize, origin: V, dir: V, max: f32, back: u32) -> TraceHit {
        let world_hit = self.world.level.raycast(origin, dir, max);
        let limit = world_hit.map_or(max, |(d, _)| d);
        let mut best: Option<(f32, usize, Part)> = None;
        for p in &self.players {
            if p.slot == shooter || !p.alive || !self.enemies(shooter, p.slot) {
                continue;
            }
            let pose = self.pose_back(p.slot, back);
            if !pose.alive {
                continue;
            }
            if let Some((t, part)) = hit_body(origin, dir, pose.eye, pose.feet, best.map_or(limit, |b| b.0)) {
                best = Some((t, p.slot, part));
            }
        }
        match best {
            Some((t, slot, part)) => TraceHit { distance: t, victim: Some((slot, part)), world: None },
            None => TraceHit { distance: limit, victim: None, world: world_hit.map(|(_, b)| b) },
        }
    }

    // ---- damage and death ---------------------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn damage(
        &mut self,
        victim: usize,
        attacker: Option<usize>,
        weapon: WeaponId,
        amount: f32,
        head: bool,
        armor_pen: f32,
        from: V,
    ) {
        if !self.players[victim].alive || amount <= 0. || self.phase != Phase::Live {
            return;
        }
        if let Some(a) = attacker {
            if a != victim && !self.enemies(a, victim) {
                return; // no friendly fire
            }
        }
        if self.tick < self.players[victim].protect_until {
            return;
        }
        let self_hit = attacker == Some(victim);
        let mut dmg = if self_hit { amount * 0.5 } else { amount };
        let p = &mut self.players[victim];
        if !head && p.armor > 0. {
            let through = dmg * armor_pen;
            let soaked = dmg - through;
            let hurt_armor = (soaked * 0.5).min(p.armor);
            p.armor -= hurt_armor;
            dmg = through + soaked - hurt_armor;
            dmg = dmg.max(through);
        }
        p.health -= dmg;
        let dealt = dmg.round().clamp(0., 255.) as u8;
        if let Some(a) = attacker {
            if a != victim {
                self.players[a].damage += dealt as u32;
            }
        }
        self.events.push(Event::Hurt {
            victim: victim as u8,
            attacker: attacker.map_or(255, |a| a as u8),
            damage: dealt,
            head,
            weapon,
            from,
        });
        if self.players[victim].health <= 0. {
            self.kill(victim, attacker, weapon, head);
        }
    }

    fn kill(&mut self, victim: usize, killer: Option<usize>, weapon: WeaponId, head: bool) {
        if !self.players[victim].alive {
            return;
        }
        let tick = self.tick;
        {
            let p = &mut self.players[victim];
            p.alive = false;
            p.health = 0.;
            p.deaths += 1;
            p.died_at = tick;
            p.killed_by = killer.map(|k| k as u8);
            p.killed_with = weapon;
        }
        // Weapons fall where the player did.
        let feet = self.players[victim].feet();
        let inv = self.players[victim].inv;
        let starting = weapons::id_of(weapons::START_SIDEARM).unwrap_or(0);
        for gun in [inv.primary, inv.secondary].into_iter().flatten() {
            if gun.id != starting {
                self.drop_gun(gun, feet);
            }
        }
        for g in inv.grenades {
            if g != 0 {
                let gun = Gun { id: g, mag: 1, reserve: 0 };
                self.drop_gun(gun, feet);
            }
        }
        if let Some(k) = killer {
            if k != victim {
                let team = self.players[k].team.index();
                self.players[k].kills += 1;
                if head {
                    self.players[k].headshots += 1;
                }
                if self.settings.mode == crate::modes::GameMode::TeamDeathmatch {
                    self.scores[team] += 1;
                }
            }
        }
        self.events.push(Event::Kill { killer: killer.map_or(255, |k| k as u8), victim: victim as u8, weapon, head });
    }

    fn drop_gun(&mut self, gun: Gun, at: V) {
        let id = self.fresh_id();
        let jitter = V(self.rng.range(-0.4, 0.4), 0., self.rng.range(-0.4, 0.4));
        let grenade = weapons::get(gun.id).is_some_and(|d| d.class == Class::Grenade);
        self.dropped.push(Dropped { id, gun, grenade, pos: at + jitter, expires: self.tick + DROP_LIFETIME });
        while self.dropped.len() > 24 {
            self.dropped.remove(0);
        }
    }

    fn drop_selected(&mut self, slot: usize) {
        let sel = self.players[slot].hands.sel;
        let feet = self.players[slot].feet();
        let id = {
            let p = &mut self.players[slot];
            match sel {
                Sel::Primary => p.inv.primary.take(),
                Sel::Secondary if p.inv.secondary.map(|g| g.id) != weapons::id_of(weapons::START_SIDEARM) => {
                    p.inv.secondary.take()
                }
                _ => None,
            }
        };
        if let Some(gun) = id {
            self.drop_gun(gun, feet + V(0., 0., 0.));
            self.events.push(Event::Dropped { player: slot as u8, weapon: gun.id });
            let p = &mut self.players[slot];
            p.hands.busy = hands::Busy::Idle;
            p.hands.left = 0;
            p.hands.sel = if p.inv.primary.is_some() { Sel::Primary } else { Sel::Secondary };
        }
    }

    // ---- pickups ------------------------------------------------------------------------------------------------

    fn step_loot(&mut self) {
        let tick = self.tick;
        for (i, l) in self.loot.iter_mut().enumerate() {
            if !l.available && l.back_at != 0 && tick >= l.back_at {
                l.available = true;
                let _ = i;
            }
        }
        self.dropped.retain(|d| d.expires > tick);
    }

    /// Give `slot` a gun; returns whether it was taken (and the gun it displaced, if any).
    fn give(&mut self, slot: usize, id: WeaponId, gun: Option<Gun>) -> Option<Option<Gun>> {
        let def = weapons::get(id)?;
        let p = &mut self.players[slot];
        match def.slot {
            Slot::Primary | Slot::Secondary => {
                let gun = gun.or_else(|| Gun::fresh(id))?;
                let cell = if def.slot == Slot::Primary { &mut p.inv.primary } else { &mut p.inv.secondary };
                match cell {
                    Some(have) if have.id == id => {
                        // The same weapon: take its ammunition, never a second copy.
                        let cap = def.mag * 3 + def.reserve;
                        let add = if gun.mag + gun.reserve > 0 { gun.mag + gun.reserve } else { def.mag };
                        have.reserve = (have.reserve + add).min(cap);
                        Some(None)
                    }
                    Some(_) => {
                        let old = cell.replace(gun);
                        Some(old)
                    }
                    None => {
                        *cell = Some(gun);
                        Some(None)
                    }
                }
            }
            Slot::Melee => {
                if p.inv.melee == id {
                    return None;
                }
                p.inv.melee = id;
                Some(None)
            }
            Slot::Grenade => {
                if p.inv.grenades[0] == 0 {
                    p.inv.grenades[0] = id;
                } else if p.inv.grenades[1] == 0 {
                    p.inv.grenades[1] = id;
                } else {
                    return None;
                }
                Some(None)
            }
        }
    }

    fn pickups(&mut self, slot: usize, use_pressed: bool) {
        let feet = self.players[slot].feet();
        // Things on the map.
        let near = |p: V| {
            let d = p - feet;
            d.0 * d.0 + d.2 * d.2 < PICKUP_RANGE * PICKUP_RANGE && d.1.abs() < 1.4
        };
        for i in 0..self.loot.len() {
            let l = self.loot[i];
            if !l.available || !near(self.world.level.loot[i].pos) {
                continue;
            }
            let Some(def) = weapons::get(l.weapon) else { continue };
            let p = &self.players[slot];
            let empty = match def.slot {
                Slot::Primary => p.inv.primary.is_none(),
                Slot::Secondary => p.inv.secondary.is_none(),
                Slot::Grenade => p.inv.grenade_count() < 2,
                Slot::Melee => false,
            };
            // Free slots fill by walking over; otherwise the player has to ask.
            if !empty && !use_pressed {
                continue;
            }
            if let Some(old) = self.give(slot, l.weapon, None) {
                self.events.push(Event::Pickup { player: slot as u8, weapon: l.weapon });
                let respawn = self.world.level.loot[i].respawn_s;
                self.loot[i].available = false;
                self.loot[i].back_at = if respawn > 0. { self.tick + (respawn * 60.) as u32 } else { 0 };
                if let Some(old) = old {
                    self.drop_gun(old, feet);
                }
                self.after_pickup(slot, def);
                return;
            }
        }
        for i in 0..self.dropped.len() {
            let d = self.dropped[i];
            if !near(d.pos) {
                continue;
            }
            let Some(def) = weapons::get(d.gun.id) else { continue };
            let p = &self.players[slot];
            let empty = match def.slot {
                Slot::Primary => p.inv.primary.is_none(),
                Slot::Secondary => p.inv.secondary.is_none(),
                Slot::Grenade => p.inv.grenade_count() < 2,
                Slot::Melee => false,
            };
            if !empty && !use_pressed {
                continue;
            }
            self.dropped.remove(i);
            match self.give(slot, d.gun.id, Some(d.gun)) {
                Some(old) => {
                    self.events.push(Event::Pickup { player: slot as u8, weapon: d.gun.id });
                    if let Some(old) = old {
                        self.drop_gun(old, feet);
                    }
                    self.after_pickup(slot, def);
                }
                None => self.dropped.insert(i.min(self.dropped.len()), d),
            }
            return;
        }
    }

    fn after_pickup(&mut self, slot: usize, def: &WeaponDef) {
        // A gun picked up into an empty hand is taken out; otherwise the player keeps what they hold.
        let p = &mut self.players[slot];
        let want = match def.slot {
            Slot::Primary => Some(Sel::Primary),
            Slot::Secondary if p.hands.sel == Sel::Melee => Some(Sel::Secondary),
            _ => None,
        };
        if let Some(sel) = want {
            if p.hands.sel != sel
                && matches!(p.hands.busy, hands::Busy::Idle)
                && p.inv.id_in(sel) != 0
                && sel == Sel::Primary
            {
                p.hands.sel = sel;
                p.hands.busy = hands::Busy::Draw;
                p.hands.total = hands::ticks(def.draw_s);
                p.hands.left = p.hands.total;
            }
        }
    }

    // ---- projectiles --------------------------------------------------------------------------------------------

    fn launch(&mut self, slot: usize, def: &WeaponDef, weapon: WeaponId, punch: (f32, f32), input: &Input) {
        let Some(pr) = def.projectile else { return };
        let origin = self.players[slot].eye();
        let dir = Self::aim_dir(input.yaw + punch.1, (input.pitch + punch.0).clamp(-1.55, 1.55));
        let right = V(input.yaw.cos(), 0., input.yaw.sin());
        let pos = origin + dir * 0.7 + right * 0.15 + V(0., -0.12, 0.);
        let v = self.players[slot].ctrl.velocity();
        let id = self.fresh_id();
        self.projectiles.push(Projectile {
            id,
            weapon,
            owner: slot as u8,
            pos,
            vel: dir * pr.speed + V(v.0, 0., v.2) * 0.3,
            age: 0,
            fuse: if pr.fuse_s > 0. { (pr.fuse_s * 60.) as u32 } else { 0 },
            bounces: 0,
        });
        self.events.push(Event::Launch { shooter: slot as u8, weapon, from: pos });
    }

    fn throw(&mut self, slot: usize, def: &WeaponDef, weapon: WeaponId, lob: bool, input: &Input) {
        let Some(pr) = def.projectile else { return };
        let origin = self.players[slot].eye();
        let dir = Self::aim_dir(input.yaw, (input.pitch + 0.12).clamp(-1.4, 1.4));
        let speed = if lob { pr.speed * 0.4 } else { pr.speed };
        let v = self.players[slot].ctrl.velocity();
        let id = self.fresh_id();
        self.projectiles.push(Projectile {
            id,
            weapon,
            owner: slot as u8,
            pos: origin + dir * 0.45 + V(0., -0.15, 0.),
            vel: dir * speed + V(v.0, v.1, v.2) * 0.5,
            age: 0,
            fuse: (pr.fuse_s * 60.) as u32,
            bounces: 0,
        });
        self.events.push(Event::Throw { thrower: slot as u8, weapon });
    }

    fn step_projectiles(&mut self, world: &Arc<World>) {
        let mut i = 0;
        while i < self.projectiles.len() {
            let mut p = self.projectiles[i];
            let Some(def) = weapons::get(p.weapon) else {
                self.projectiles.swap_remove(i);
                continue;
            };
            let Some(pr) = def.projectile else {
                self.projectiles.swap_remove(i);
                continue;
            };
            p.age += 1;
            p.vel.1 -= GRAVITY * pr.gravity * DT;
            let mut boom = pr.fuse_s > 0. && p.age >= p.fuse;
            // Move in short sub-steps so nothing tunnels through a thin wall.
            let steps = 3;
            for _ in 0..steps {
                let step = p.vel * (DT / steps as f32);
                let len = step.length();
                if len < 1e-6 {
                    break;
                }
                let dir = step * (1. / len);
                match world.level.raycast_normal(p.pos, dir, len + 0.04) {
                    Some((t, _, n)) => {
                        if pr.bounces {
                            p.pos = p.pos + dir * (t - 0.04).max(0.);
                            p.vel = p.vel.reflect(n) * 0.45;
                            p.bounces = p.bounces.saturating_add(1);
                        } else {
                            p.pos = p.pos + dir * t.max(0.);
                            boom = true;
                        }
                        break;
                    }
                    None => p.pos = p.pos + step,
                }
            }
            // Rockets also burst on a player they fly into.
            if !pr.bounces && !boom {
                for q in &self.players {
                    if !q.alive || q.slot as u8 == p.owner && p.age < 12 {
                        continue;
                    }
                    if hit_body(
                        p.pos - p.vel * DT,
                        p.vel.norm(),
                        q.eye(),
                        q.ctrl.feet_height(),
                        p.vel.length() * DT + 0.2,
                    )
                    .is_some()
                    {
                        boom = true;
                        break;
                    }
                }
            }
            if p.pos.1 < KILL_PLANE {
                self.projectiles.swap_remove(i);
                continue;
            }
            if boom {
                self.projectiles.swap_remove(i);
                self.detonate(def, p);
                continue;
            }
            self.projectiles[i] = p;
            i += 1;
        }
    }

    fn detonate(&mut self, def: &WeaponDef, p: Projectile) {
        let Some(pr) = def.projectile else { return };
        let owner = p.owner as usize;
        match pr.effect {
            Effect::Explosion { radius } => {
                self.events.push(Event::Blast { pos: p.pos, kind: 0, radius });
                let centre = p.pos + V(0., 0.2, 0.);
                for v in 0..self.players.len() {
                    if !self.players[v].alive {
                        continue;
                    }
                    let chest = self.players[v].eye() - V(0., 0.45, 0.);
                    let d = (chest - centre).length();
                    if d > radius || !self.world.level.line_of_sight(centre, chest) && d > 0.6 {
                        continue;
                    }
                    let falloff = (1. - d / radius).clamp(0., 1.);
                    let dmg = def.blast_damage * (0.25 + 0.75 * falloff);
                    let attacker = if owner < self.players.len() { Some(owner) } else { None };
                    self.damage(v, attacker, p.weapon, dmg, false, 0.6, centre);
                }
            }
            Effect::Flash { radius, blind_s } => {
                self.events.push(Event::Blast { pos: p.pos, kind: 1, radius });
                for v in 0..self.players.len() {
                    if !self.players[v].alive {
                        continue;
                    }
                    let eye = self.players[v].eye();
                    let to = p.pos - eye;
                    let d = to.length();
                    if d > radius || !self.world.level.line_of_sight(p.pos, eye) {
                        continue;
                    }
                    let look = Self::aim_dir(self.players[v].ctrl.yaw, self.players[v].ctrl.pitch);
                    let facing = look.dot(to.norm()).clamp(-1., 1.);
                    let amount = blind_s * (1. - d / radius * 0.5) * (0.3 + 0.7 * (facing * 0.5 + 0.5));
                    let until = self.tick + (amount * 60.) as u32;
                    if until > self.players[v].flash_until {
                        self.players[v].flash_until = until;
                        self.players[v].flash_total = (amount * 60.) as u32;
                    }
                }
            }
            Effect::Smoke { radius, seconds } => {
                self.events.push(Event::Blast { pos: p.pos, kind: 2, radius });
                self.zones.push(Zone {
                    kind: ZoneKind::Smoke,
                    pos: p.pos + V(0., 0.6, 0.),
                    radius,
                    until: self.tick + (seconds * 60.) as u32,
                    owner: p.owner,
                    dps: 0.,
                });
            }
            Effect::Fire { radius, seconds } => {
                self.events.push(Event::Blast { pos: p.pos, kind: 3, radius });
                self.zones.push(Zone {
                    kind: ZoneKind::Fire,
                    pos: p.pos,
                    radius,
                    until: self.tick + (seconds * 60.) as u32,
                    owner: p.owner,
                    dps: def.blast_damage,
                });
            }
        }
        while self.zones.len() > 12 {
            self.zones.remove(0);
        }
    }

    fn step_zones(&mut self) {
        let tick = self.tick;
        self.zones.retain(|z| z.until > tick);
        let zones = self.zones.clone();
        for z in zones {
            if z.kind != ZoneKind::Fire {
                continue;
            }
            for v in 0..self.players.len() {
                if !self.players[v].alive {
                    continue;
                }
                let f = self.players[v].feet();
                let d = f - z.pos;
                if (d.0 * d.0 + d.2 * d.2).sqrt() < z.radius && d.1.abs() < 1.5 {
                    let attacker = if (z.owner as usize) < self.players.len() { Some(z.owner as usize) } else { None };
                    self.damage(v, attacker, 0, z.dps * DT, false, 1., z.pos);
                }
            }
        }
    }

    // ---- melee --------------------------------------------------------------------------------------------------

    fn strike(&mut self, slot: usize, def: &WeaponDef, weapon: WeaponId, heavy: bool) {
        let Some(m) = def.melee else { return };
        let origin = self.players[slot].eye();
        let (yaw, pitch) = (self.players[slot].ctrl.yaw, self.players[slot].ctrl.pitch);
        let dir = Self::aim_dir(yaw, pitch);
        let mut best: Option<(f32, usize, Part)> = None;
        let wall = self.world.level.raycast(origin, dir, m.reach).map_or(m.reach, |(d, _)| d);
        for p in &self.players {
            if p.slot == slot || !p.alive || !self.enemies(slot, p.slot) {
                continue;
            }
            // Melee is forgiving: a fat ray (three rays fanned a little) so a swing at the body connects.
            for (dy, dp) in [(0., 0.), (0.25, 0.), (-0.25, 0.), (0., 0.3)] {
                let d = Self::aim_dir(yaw + dy, pitch + dp);
                if let Some((t, part)) =
                    hit_body(origin, d, p.eye(), p.ctrl.feet_height(), wall.min(best.map_or(m.reach, |b| b.0)))
                {
                    best = Some((t, p.slot, part));
                }
            }
        }
        match best {
            Some((_, victim, part)) => {
                let back = {
                    let v = &self.players[victim].ctrl;
                    let facing = Self::aim_dir(v.yaw, 0.);
                    let toward = V(dir.0, 0., dir.2).norm();
                    facing.dot(toward) > 0.5
                };
                let mut dmg = if heavy { m.heavy } else { m.light };
                if back {
                    dmg *= m.back_mult;
                }
                let head = part == Part::Head && heavy;
                self.events.push(Event::Strike { attacker: slot as u8, weapon, heavy, hit: true });
                self.damage(victim, Some(slot), weapon, dmg, head, 1., origin);
            }
            None => self.events.push(Event::Strike { attacker: slot as u8, weapon, heavy, hit: false }),
        }
    }

    /// Put a player somewhere (scenarios, tests and tools).
    pub fn teleport(&mut self, slot: usize, feet: V, yaw: f32, pitch: f32) {
        let p = &mut self.players[slot];
        p.ctrl = new_body(feet, yaw);
        p.ctrl.pitch = pitch;
        p.last_input.yaw = yaw;
        p.last_input.pitch = pitch;
    }

    /// Whether `from` could see `to` right now (walls and smoke block it).
    pub fn can_see(&self, from: V, to: V) -> bool {
        if !self.world.level.line_of_sight(from, to) {
            return false;
        }
        let d = to - from;
        let len = d.length();
        if len < 1e-3 {
            return true;
        }
        let dir = d * (1. / len);
        for z in self.zones.iter().filter(|z| z.kind == ZoneKind::Smoke) {
            // Distance from the zone centre to the sight line.
            let t = (z.pos - from).dot(dir).clamp(0., len);
            let closest = from + dir * t;
            if (closest - z.pos).length() < z.radius * 0.8 {
                return false;
            }
        }
        true
    }
}

fn material_code(m: Material) -> u8 {
    match m {
        Material::Concrete | Material::ConcreteDark | Material::Asphalt | Material::Plaster | Material::Brick => 0,
        Material::Metal
        | Material::RustMetal
        | Material::Hazard
        | Material::ContainerRed
        | Material::ContainerBlue
        | Material::ContainerGreen
        | Material::ContainerYellow => 1,
        Material::Wood => 2,
        Material::Glass => 3,
        Material::Gravel | Material::Dirt | Material::Grass => 4,
        Material::Water => 5,
    }
}

/// Seen counters that match an input, so a freshly spawned player does not act on old presses.
pub fn seen_from(i: &Input) -> hands::Seen {
    hands::Seen { reload: i.reload_seq, use_: i.use_seq, melee: i.melee_seq, drop: i.drop_seq, switch: i.switch_seq }
}

fn blank_player(slot: usize, team: Team, name: String, human: bool) -> Player {
    Player {
        slot,
        team,
        appearance: team.index() as u8,
        name,
        ctrl: new_body(V(0., 0., 0.), 0.),
        alive: false,
        health: 0.,
        armor: 0.,
        inv: Inventory::starting(),
        hands: Hands::default(),
        last_input: Input::default(),
        bot: None,
        human,
        kills: 0,
        deaths: 0,
        headshots: 0,
        damage: 0,
        died_at: 0,
        killed_by: None,
        killed_with: 0,
        protect_until: 0,
        flash_until: 0,
        flash_total: 0,
        last_shot_tick: 0,
        foot_phase: 0.,
    }
}

// ---- hit volumes ----------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Head,
    Torso,
    Legs,
}

pub struct TraceHit {
    pub distance: f32,
    pub victim: Option<(usize, Part)>,
    pub world: Option<usize>,
}

/// Ray against a standing or crouched body: a head sphere, a torso cylinder, a legs cylinder. `eye` is the eye
/// position, `feet` the floor height under it. Returns the distance and the part of the nearest hit.
pub fn hit_body(o: V, d: V, eye: V, feet: f32, max: f32) -> Option<(f32, Part)> {
    let (head_c, head_r) = head_of(eye);
    let mut best: Option<(f32, Part)> = None;
    if let Some(t) = ray_sphere(o, d, head_c, head_r, max) {
        best = Some((t, Part::Head));
    }
    let head_bottom = head_c.1 - head_r;
    let hip = feet + (head_bottom - feet) * 0.45;
    if let Some(t) = ray_cylinder(o, d, eye.0, eye.2, 0.26, hip, head_bottom, best.map_or(max, |b| b.0)) {
        best = Some((t, Part::Torso));
    }
    if let Some(t) = ray_cylinder(o, d, eye.0, eye.2, 0.2, feet, hip, best.map_or(max, |b| b.0)) {
        best = Some((t, Part::Legs));
    }
    best
}

fn ray_sphere(o: V, d: V, c: V, r: f32, max: f32) -> Option<f32> {
    let oc = o - c;
    let b = oc.dot(d);
    let cc = oc.dot(oc) - r * r;
    let disc = b * b - cc;
    if disc < 0. {
        return None;
    }
    let s = disc.sqrt();
    let t = if -b - s >= 0. { -b - s } else { -b + s };
    (0. ..=max).contains(&t).then_some(t)
}

#[allow(clippy::too_many_arguments)]
fn ray_cylinder(o: V, d: V, cx: f32, cz: f32, r: f32, y0: f32, y1: f32, max: f32) -> Option<f32> {
    let (ox, oz) = (o.0 - cx, o.2 - cz);
    let a = d.0 * d.0 + d.2 * d.2;
    let mut best: Option<f32> = None;
    if a > 1e-9 {
        let b = ox * d.0 + oz * d.2;
        let c = ox * ox + oz * oz - r * r;
        let disc = b * b - a * c;
        if disc >= 0. {
            let s = disc.sqrt();
            for t in [(-b - s) / a, (-b + s) / a] {
                if (0. ..=max).contains(&t) {
                    let y = o.1 + d.1 * t;
                    if y >= y0 && y <= y1 && best.is_none_or(|b| t < b) {
                        best = Some(t);
                    }
                }
            }
        }
    }
    // The end caps, for shots from above or below.
    if d.1.abs() > 1e-6 {
        for y in [y0, y1] {
            let t = (y - o.1) / d.1;
            if (0. ..=max).contains(&t) {
                let (x, z) = (ox + d.0 * t, oz + d.2 * t);
                if x * x + z * z <= r * r && best.is_none_or(|b| t < b) {
                    best = Some(t);
                }
            }
        }
    }
    best
}
