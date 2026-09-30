//! The race: a pure, deterministic, fixed-step simulation of up to eight karts on Haunted Hollow.
//! Humans supply one `KartInput` a tick; bots are driven from inside (`bot::drive`), so they replay
//! identically and are part of a save.
use crate::bot;
use crate::character::{Character, Perk, ALL, MAX_RACERS};
use crate::kart::{Driver, Kart, KartInput};
use crate::track::{forward, Track};
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::devkit::{Rng, SavePolicy, Simulation, Snapshot, StateHasher, TICK};

/// Ticks of countdown before the start (the lights show 3, 2, 1).
pub const COUNTDOWN_TICKS: u32 = 240;
/// Laps to win.
pub const LAPS: u32 = 2;
/// A kart's body, for bumping.
pub const KART_RADIUS: f32 = 1.2;
/// After the first kart finishes, the rest have this long to finish.
pub const FINISH_GRACE_TICKS: u32 = 1800;

/// One tick of input for the whole grid; slots of bot karts (or empty slots) are ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Inputs(pub [KartInput; MAX_RACERS]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Ticks left before the start.
    Countdown(u32),
    Racing,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HazardKind {
    /// Slows whoever drives over it.
    Bandage,
    /// Spins out whoever hits it, once.
    Bone,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hazard {
    pub pos: V,
    pub radius: f32,
    pub ttl: u32,
    pub owner: usize,
    pub kind: HazardKind,
}

/// What happened this tick; the window reacts with sound and effects, and the server logs them.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Count(u32),
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

/// One racer's line in the race report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RacerReport {
    pub slot: usize,
    pub character: Character,
    pub human: bool,
    pub place: u32,
    pub finished: bool,
    pub finish_seconds: Option<f32>,
    pub best_lap_seconds: Option<f32>,
    pub drift_seconds: f32,
    pub offroad_seconds: f32,
    pub wall_hits: u32,
    pub collisions: u32,
    pub perk_uses: u32,
    pub hazard_hits: u32,
    pub top_speed: f32,
    pub mean_speed: f32,
}

/// The evidence one race leaves: which characters win and why. The server appends one line per race.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaceReport {
    pub game: String,
    pub track: String,
    pub seed: u64,
    pub laps: u32,
    pub race_seconds: f32,
    pub racers: Vec<RacerReport>,
}

pub struct Sim {
    pub tick: u64,
    pub phase: Phase,
    pub karts: Vec<Kart>,
    pub hazards: Vec<Hazard>,
    /// Ticks since the start signal.
    pub race_tick: u32,
    pub first_finish: Option<u32>,
    pub finished_count: u32,
    pub seed: u64,
    track: Track,
    rng: Rng,
    events: Vec<Event>,
}

impl Sim {
    /// A full grid of all eight characters; slot 0 (the Vampire) is the human, the rest are bots.
    pub fn new(seed: u64) -> Self {
        let grid: Vec<(Character, Driver)> =
            ALL.iter().enumerate().map(|(i, c)| (*c, if i == 0 { Driver::Human } else { Driver::Bot })).collect();
        Self::with_grid(seed, &grid)
    }

    /// A race with exactly these drivers, in grid order (at most eight).
    pub fn with_grid(seed: u64, grid: &[(Character, Driver)]) -> Self {
        let track = Track::haunted_hollow();
        let mut rng = Rng::new(seed);
        let karts = grid
            .iter()
            .take(MAX_RACERS)
            .enumerate()
            .map(|(slot, (character, driver))| {
                let (pos, yaw) = track.grid_slot(slot);
                Kart::new(*character, *driver, pos, yaw, &track, rng.range(0.85, 1.0))
            })
            .collect();
        Self {
            tick: 0,
            phase: Phase::Countdown(COUNTDOWN_TICKS),
            karts,
            hazards: Vec::new(),
            race_tick: 0,
            first_finish: None,
            finished_count: 0,
            seed,
            track,
            rng,
            events: Vec::new(),
        }
    }

    /// A race with nobody in it, for a client to fill from snapshots (see `apply_view`).
    pub fn replica() -> Self {
        Self::with_grid(0, &[])
    }

    /// Overwrite the visible race state with what a server snapshot says. Presentation only: a replica
    /// is never stepped.
    pub fn apply_view(
        &mut self,
        phase: Phase,
        race_tick: u32,
        first_finish: Option<u32>,
        finished_count: u32,
        karts: Vec<Kart>,
        hazards: Vec<Hazard>,
    ) {
        self.phase = phase;
        self.race_tick = race_tick;
        self.first_finish = first_finish;
        self.finished_count = finished_count;
        self.karts = karts;
        self.hazards = hazards;
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    pub fn racing(&self) -> bool {
        self.phase == Phase::Racing
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::Finished
    }

    /// Kart indices from first place to last: finishers by place, then the rest by distance driven.
    pub fn standings(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.karts.len()).collect();
        order.sort_by(|&a, &b| {
            let (ka, kb) = (&self.karts[a], &self.karts[b]);
            match (ka.place, kb.place) {
                (0, 0) => kb.best_progress.partial_cmp(&ka.best_progress).unwrap_or(std::cmp::Ordering::Equal),
                (0, _) => std::cmp::Ordering::Greater,
                (_, 0) => std::cmp::Ordering::Less,
                (pa, pb) => pa.cmp(&pb),
            }
            .then(a.cmp(&b))
        });
        order
    }

    /// Advance exactly one 60 Hz tick.
    pub fn step(&mut self, inputs: &Inputs) {
        if self.phase == Phase::Finished {
            return;
        }
        self.tick += 1;
        let racing = match self.phase {
            Phase::Countdown(left) => {
                if left <= 180 && left % 60 == 0 {
                    self.events.push(Event::Count(left / 60));
                }
                if left <= 1 {
                    self.phase = Phase::Racing;
                    self.events.push(Event::Go);
                } else {
                    self.phase = Phase::Countdown(left - 1);
                }
                false
            }
            _ => true,
        };
        if racing {
            self.race_tick += 1;
        }

        // Who is asking for what: humans from `inputs`, bots (and finishers on autopilot) from their own eyes.
        let resolved: Vec<KartInput> = (0..self.karts.len())
            .map(|i| {
                let kart = &self.karts[i];
                match kart.driver {
                    Driver::Human if kart.finished_tick.is_none() => inputs.0[i],
                    _ => {
                        let mut input = bot::drive(self, i);
                        if kart.finished_tick.is_some() {
                            input.perk = false;
                        }
                        input
                    }
                }
            })
            .collect();

        if racing {
            self.use_perks(&resolved);
        }
        self.update_hazards();
        let (track, race_tick) = (&self.track, self.race_tick);
        for (i, input) in resolved.iter().enumerate() {
            let notes = self.karts[i].update(track, input, racing, race_tick, LAPS);
            if let Some(tier) = notes.boost_tier {
                self.events.push(Event::DriftBoost { kart: i, tier });
            }
            if let Some(speed) = notes.wall_impact {
                self.events.push(Event::WallHit { kart: i, speed });
            }
            let done_before = self.karts[i].lap - notes.laps_done;
            for n in 1..=notes.laps_done {
                let lap = done_before + n;
                let ticks = self.karts[i].stats.laps[(lap - 1) as usize];
                self.events.push(Event::LapDone { kart: i, lap, ticks });
            }
        }
        self.collide_karts();
        self.apply_hazards();
        if racing {
            self.settle_finishers();
        }
    }

    fn use_perks(&mut self, inputs: &[KartInput]) {
        for i in 0..self.karts.len() {
            let kart = &self.karts[i];
            let perk = kart.character.perk();
            if !inputs[i].perk
                || !perk.is_active()
                || kart.cooldown > 0
                || kart.spin_ticks > 0
                || kart.finished_tick.is_some()
            {
                continue;
            }
            let (pos, yaw, s) = (kart.pos, kart.yaw, kart.s);
            let back = forward(yaw) * -1.;
            match perk {
                Perk::BandageTrail => {
                    for d in [3., 6., 9., 12.] {
                        self.hazards.push(Hazard {
                            pos: pos + back * d,
                            radius: 2.4,
                            ttl: 420,
                            owner: i,
                            kind: HazardKind::Bandage,
                        });
                    }
                }
                Perk::Rattle => {
                    for d in [4., 9.] {
                        self.hazards.push(Hazard {
                            pos: pos + back * d,
                            radius: 1.6,
                            ttl: 600,
                            owner: i,
                            kind: HazardKind::Bone,
                        });
                    }
                }
                Perk::Phase => self.karts[i].phase_ticks = Character::PHASE_TICKS,
                Perk::CrowSwarm => {
                    // The nearest kart ahead, within forty metres of road.
                    let target = (0..self.karts.len())
                        .filter(|&j| j != i && !self.karts[j].phased())
                        .map(|j| (j, self.track.arc_delta(s, self.karts[j].s)))
                        .filter(|&(_, d)| d > 0. && d < 40.)
                        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                    if let Some((j, _)) = target {
                        self.karts[j].apply_slow(0.45, 90);
                    }
                }
                Perk::Honk => {
                    for j in 0..self.karts.len() {
                        if j == i || self.karts[j].phased() {
                            continue;
                        }
                        let away = self.karts[j].pos - pos;
                        let distance = away.length();
                        if distance < 9. {
                            let dir = if distance > 1e-3 { away * (1. / distance) } else { V(1., 0., 0.) };
                            let mass = self.karts[j].character.stats().mass;
                            self.karts[j].vel = self.karts[j].vel + dir * (9. / mass);
                        }
                    }
                }
                Perk::BatBoost | Perk::Ram | Perk::Undead => continue,
            }
            let kart = &mut self.karts[i];
            kart.cooldown = kart.character.cooldown_ticks();
            kart.stats.perk_uses += 1;
            self.events.push(Event::PerkUsed { kart: i, perk });
        }
    }

    fn update_hazards(&mut self) {
        for h in &mut self.hazards {
            h.ttl = h.ttl.saturating_sub(1);
        }
        self.hazards.retain(|h| h.ttl > 0);
    }

    fn apply_hazards(&mut self) {
        for hi in 0..self.hazards.len() {
            let h = self.hazards[hi];
            for i in 0..self.karts.len() {
                let kart = &mut self.karts[i];
                let immune = i == h.owner || kart.phased() || kart.character.perk() == Perk::Rattle;
                if immune || (kart.pos - h.pos).length() > h.radius + KART_RADIUS * 0.5 {
                    continue;
                }
                match h.kind {
                    HazardKind::Bandage => {
                        if kart.slow_ticks == 0 {
                            kart.stats.hazard_hits += 1;
                            self.events.push(Event::HazardHit { kart: i, kind: h.kind });
                        }
                        kart.apply_slow(0.55, 60);
                    }
                    HazardKind::Bone => {
                        if self.hazards[hi].ttl > 0 {
                            kart.spin_ticks = 50;
                            kart.stats.hazard_hits += 1;
                            self.hazards[hi].ttl = 0;
                            self.events.push(Event::HazardHit { kart: i, kind: h.kind });
                        }
                    }
                }
            }
        }
        self.hazards.retain(|h| h.ttl > 0);
    }

    fn collide_karts(&mut self) {
        let n = self.karts.len();
        for a in 0..n {
            for b in (a + 1)..n {
                if self.karts[a].phased() || self.karts[b].phased() {
                    continue;
                }
                let delta = self.karts[b].pos - self.karts[a].pos;
                let distance = delta.length();
                if distance >= 2. * KART_RADIUS {
                    continue;
                }
                let normal = if distance > 1e-3 { delta * (1. / distance) } else { V(1., 0., 0.) };
                let (ma, mb) = (self.karts[a].character.stats().mass, self.karts[b].character.stats().mass);
                let overlap = 2. * KART_RADIUS - distance;
                let (wa, wb) = (mb / (ma + mb), ma / (ma + mb));
                self.karts[a].pos = self.karts[a].pos - normal * (overlap * wa);
                self.karts[b].pos = self.karts[b].pos + normal * (overlap * wb);
                let closing = (self.karts[a].vel - self.karts[b].vel).dot(normal);
                if closing <= 0. {
                    continue;
                }
                let j = closing * 1.4 / (1. / ma + 1. / mb);
                // The Monster's Ram: at speed, whoever it hits is launched harder and slowed.
                let ram = |k: &Kart| k.character.perk() == Perk::Ram && k.speed() > 0.7 * k.character.stats().top_speed;
                let (ram_a, ram_b) = (ram(&self.karts[a]), ram(&self.karts[b]));
                let (boost_a, boost_b) = (if ram_b { 1.6 } else { 1. }, if ram_a { 1.6 } else { 1. });
                self.karts[a].vel = self.karts[a].vel - normal * (j / ma * boost_a);
                self.karts[b].vel = self.karts[b].vel + normal * (j / mb * boost_b);
                if ram_a {
                    self.karts[b].apply_slow(0.6, 45);
                }
                if ram_b {
                    self.karts[a].apply_slow(0.6, 45);
                }
                self.karts[a].stats.collisions += 1;
                self.karts[b].stats.collisions += 1;
                self.events.push(Event::Bump { a, b, speed: closing });
            }
        }
    }

    fn settle_finishers(&mut self) {
        let mut newly: Vec<usize> = (0..self.karts.len())
            .filter(|&i| self.karts[i].finished_tick.is_some() && self.karts[i].place == 0)
            .collect();
        newly.sort_by(|&a, &b| {
            self.karts[b].best_progress.partial_cmp(&self.karts[a].best_progress).unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in newly {
            self.finished_count += 1;
            self.karts[i].place = self.finished_count;
            let ticks = self.karts[i].finished_tick.unwrap_or(self.race_tick);
            self.first_finish.get_or_insert(ticks);
            self.events.push(Event::Finished { kart: i, place: self.finished_count, ticks });
        }
        let all_done = self.karts.iter().all(|k| k.place > 0);
        let humans_done = self.karts.iter().any(|k| k.driver == Driver::Human)
            && self.karts.iter().filter(|k| k.driver == Driver::Human).all(|k| k.place > 0);
        let out_of_time = self.first_finish.is_some_and(|t| self.race_tick.saturating_sub(t) >= FINISH_GRACE_TICKS);
        if all_done || humans_done || out_of_time {
            // Whoever has not finished is placed by how far they got.
            for i in self.standings() {
                if self.karts[i].place == 0 {
                    self.finished_count += 1;
                    self.karts[i].place = self.finished_count;
                }
            }
            self.phase = Phase::Finished;
            self.events.push(Event::RaceOver);
        }
    }

    /// Events since the last call, oldest first.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The evidence this race left, one line per racer.
    pub fn report(&self) -> RaceReport {
        let secs = |ticks: u32| ticks as f32 * TICK;
        let racers = self
            .karts
            .iter()
            .enumerate()
            .map(|(slot, k)| {
                let mut best = None::<u32>;
                let mut previous = 0;
                for &lap_end in &k.stats.laps {
                    let span = lap_end - previous;
                    best = Some(best.map_or(span, |b| b.min(span)));
                    previous = lap_end;
                }
                RacerReport {
                    slot,
                    character: k.character,
                    human: k.driver == Driver::Human,
                    place: k.place,
                    finished: k.finished_tick.is_some(),
                    finish_seconds: k.finished_tick.map(secs),
                    best_lap_seconds: best.map(secs),
                    drift_seconds: secs(k.stats.drift_ticks),
                    offroad_seconds: secs(k.stats.offroad_ticks),
                    wall_hits: k.stats.wall_hits,
                    collisions: k.stats.collisions,
                    perk_uses: k.stats.perk_uses,
                    hazard_hits: k.stats.hazard_hits,
                    top_speed: k.stats.top_speed,
                    mean_speed: if k.stats.racing_ticks > 0 {
                        k.stats.distance / secs(k.stats.racing_ticks)
                    } else {
                        0.
                    },
                }
            })
            .collect();
        RaceReport {
            game: "spooky-kart".into(),
            track: self.track.name().into(),
            seed: self.seed,
            laps: LAPS,
            race_seconds: secs(self.race_tick),
            racers,
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
                "race",
                one(&|h| {
                    h.u64(self.tick)
                        .u32(self.race_tick)
                        .u32(self.finished_count)
                        .u32(self.first_finish.unwrap_or(u32::MAX));
                    match self.phase {
                        Phase::Countdown(n) => h.u32(n),
                        Phase::Racing => h.u32(u32::MAX - 1),
                        Phase::Finished => h.u32(u32::MAX),
                    };
                }),
            ),
            (
                "karts",
                one(&|h| {
                    for k in &self.karts {
                        h.f32(k.pos.0).f32(k.pos.2).f32(k.vel.0).f32(k.vel.2).f32(k.yaw);
                        h.bool(k.drifting).f32(k.drift_dir).f32(k.drift_charge);
                        h.u32(k.boost_ticks).f32(k.boost_power).u32(k.slow_ticks).f32(k.slow_factor).u32(k.spin_ticks);
                        h.u32(k.cooldown).u32(k.phase_ticks).u64(k.hint as u64).f32(k.s).f32(k.lateral);
                        h.f32(k.progress).f32(k.best_progress).u32(k.lap).u32(k.place).f32(k.skill);
                        h.u32(k.finished_tick.unwrap_or(u32::MAX)).bool(k.offroad);
                    }
                }),
            ),
            (
                "hazards",
                one(&|h| {
                    for hz in &self.hazards {
                        h.f32(hz.pos.0)
                            .f32(hz.pos.2)
                            .u32(hz.ttl)
                            .u64(hz.owner as u64)
                            .bool(hz.kind == HazardKind::Bone);
                    }
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

/// Everything that decides where the race goes next, as plain data for a save. The track is a constant of
/// the map and presentation is rebuilt from the state, so neither is in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub phase: Phase,
    pub karts: Vec<Kart>,
    pub hazards: Vec<Hazard>,
    pub race_tick: u32,
    pub first_finish: Option<u32>,
    pub finished_count: u32,
    pub seed: u64,
    pub rng: Rng,
}

impl Snapshot for Sim {
    const KIND: &'static str = "spooky-kart";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            phase: self.phase,
            karts: self.karts.clone(),
            hazards: self.hazards.clone(),
            race_tick: self.race_tick,
            first_finish: self.first_finish,
            finished_count: self.finished_count,
            seed: self.seed,
            rng: self.rng.clone(),
        }
    }
    /// Refuse a state this game could not have produced; the caller then keeps the running race.
    fn restore(&mut self, state: SimState) -> Result<(), String> {
        if state.karts.len() != self.karts.len() {
            return Err(format!("the save has {} karts, this race has {}", state.karts.len(), self.karts.len()));
        }
        let sane = |v: V| v.finite() && v.0.abs() < 2000. && v.2.abs() < 2000.;
        if !state.karts.iter().all(|k| sane(k.pos) && sane(k.vel) && k.hint < self.track.samples().len()) {
            return Err("the save puts a kart outside the world".into());
        }
        if state.hazards.iter().any(|h| !sane(h.pos) || h.owner >= state.karts.len()) {
            return Err("the save has a hazard the race could not have made".into());
        }
        if state.karts.iter().zip(&self.karts).any(|(a, b)| a.character != b.character || a.driver != b.driver) {
            return Err("the save's drivers do not match this race".into());
        }
        self.tick = state.tick;
        self.phase = state.phase;
        self.karts = state.karts;
        self.hazards = state.hazards;
        self.race_tick = state.race_tick;
        self.first_finish = state.first_finish;
        self.finished_count = state.finished_count;
        self.seed = state.seed;
        self.rng = state.rng;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}
