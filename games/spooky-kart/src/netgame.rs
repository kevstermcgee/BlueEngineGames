//! Spooky Kart as a [`NetGame`]: everything BlueEngine's netplay kit needs to host and join races. The kit
//! owns the lobby, sessions, input streaming, snapshots and statistics; this file is what is particular to
//! karts: the layouts of an input, a snapshot and an event, how seats become karts, and how a client predicts
//! its own kart and smooths everyone else's.
use crate::character::{Character, Perk, ALL, MAX_RACERS};
use crate::kart::{Driver, Kart, KartInput, KartStats};
use crate::sim::{Event, Hazard, HazardKind, Inputs, Phase, Sim, LAPS};
use crate::track::{wrap_angle, Track};
use vesper3d::math::V;
use vesper3d::viewer::devkit::Rng;
use vesper3d::viewer::net::codec::{Reader, WireError, WireResult, Writer};
use vesper3d::viewer::netplay::{ClientView, NetGame, PredictionStats, Seat};

/// A prediction error larger than this snaps instead of easing (metres).
const SNAP_DISTANCE: f32 = 6.;
/// The time between snapshots (the server sends 30 a second).
const SNAPSHOT_INTERVAL: f64 = 1. / 30.;
const MAX_HAZARDS: usize = 24;

pub struct KartGame;

/// The race as one server tick shows it: everything a client draws and predicts from.
#[derive(Clone, Debug, PartialEq)]
pub struct KartSnapshot {
    pub phase: Phase,
    pub race_tick: u32,
    pub first_finish: Option<u32>,
    pub finished_count: u32,
    pub karts: Vec<Kart>,
    pub hazards: Vec<Hazard>,
}

impl KartGame {
    fn write_kart(w: &mut Writer, k: &Kart) {
        w.u8(k.character.index() as u8);
        w.bool(k.driver == Driver::Human);
        for v in [k.pos.0, k.pos.2, k.vel.0, k.vel.2, k.yaw] {
            w.f32(v);
        }
        w.u8(k.drifting as u8 | (k.offroad as u8) << 1);
        w.u8(k.drift_dir as i8 as u8);
        w.f32(k.drift_charge);
        w.u8(k.boost_ticks.min(255) as u8);
        w.f32(k.boost_power);
        w.u8(k.slow_ticks.min(255) as u8);
        w.f32(k.slow_factor);
        w.u8(k.spin_ticks.min(255) as u8);
        w.u16(k.cooldown.min(65535) as u16);
        w.u8(k.phase_ticks.min(255) as u8);
        w.u16(k.hint.min(65535) as u16);
        for v in [k.s, k.lateral, k.progress, k.best_progress] {
            w.f32(v);
        }
        w.u8(k.lap.min(255) as u8);
        w.u32(k.finished_tick.unwrap_or(u32::MAX));
        w.u8(k.place.min(255) as u8);
    }

    fn read_kart(r: &mut Reader) -> WireResult<Kart> {
        let character = Character::from_wire(r.u8()?).ok_or(WireError("bad character"))?;
        let human = r.bool()?;
        let (px, pz) = (r.f32_within(5000.)?, r.f32_within(5000.)?);
        let (vx, vz) = (r.f32_within(500.)?, r.f32_within(500.)?);
        let yaw = r.f32()?;
        let flags = r.u8()?;
        if flags > 3 {
            return Err(WireError("bad kart flags"));
        }
        let drift_dir = r.u8()? as i8 as f32;
        let drift_charge = r.f32()?;
        let boost_ticks = r.u8()? as u32;
        let boost_power = r.f32()?;
        let slow_ticks = r.u8()? as u32;
        let slow_factor = r.f32()?;
        let spin_ticks = r.u8()? as u32;
        let cooldown = r.u16()? as u32;
        let phase_ticks = r.u8()? as u32;
        let hint = r.u16()? as usize;
        let (s, lateral, progress, best_progress) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
        let lap = r.u8()? as u32;
        let finished = r.u32()?;
        let place = r.u8()? as u32;
        Ok(Kart {
            character,
            driver: if human { Driver::Human } else { Driver::Bot },
            pos: V(px, 0., pz),
            vel: V(vx, 0., vz),
            yaw,
            drifting: flags & 1 != 0,
            drift_dir,
            drift_charge,
            boost_ticks,
            boost_power,
            slow_ticks,
            slow_factor,
            spin_ticks,
            cooldown,
            phase_ticks,
            hint,
            s,
            lateral,
            progress,
            best_progress,
            lap,
            finished_tick: (finished != u32::MAX).then_some(finished),
            place,
            offroad: flags & 2 != 0,
            skill: 1.,
            stats: KartStats::default(),
        })
    }
}

impl NetGame for KartGame {
    type Input = KartInput;
    type Match = Sim;
    type View = KartView;
    type Snapshot = KartSnapshot;
    type Event = Event;

    const NAME: &'static str = "spooky-kart";
    const MAX_SEATS: usize = MAX_RACERS;
    const CHOICES: u8 = MAX_RACERS as u8;

    fn fingerprint() -> u32 {
        crate::content_fingerprint()
    }

    fn write_input(i: &KartInput, w: &mut Writer) {
        // Throttle and steer as signed bytes: 1/127 resolution is finer than a thumbstick.
        w.u8((i.throttle.clamp(-1., 1.) * 127.).round() as i8 as u8);
        w.u8((i.steer.clamp(-1., 1.) * 127.).round() as i8 as u8);
        w.u8((i.drift as u8) | ((i.perk as u8) << 1));
    }

    fn read_input(r: &mut Reader) -> WireResult<KartInput> {
        let throttle = r.u8()? as i8 as f32 / 127.;
        let steer = r.u8()? as i8 as f32 / 127.;
        let bits = r.u8()?;
        if bits > 3 {
            return Err(WireError("bad input flags"));
        }
        Ok(KartInput { throttle, steer, drift: bits & 1 != 0, perk: bits & 2 != 0 })
    }

    fn write_snapshot(s: &KartSnapshot, w: &mut Writer) {
        match s.phase {
            Phase::Countdown(n) => {
                w.u8(0);
                w.u32(n);
            }
            Phase::Racing => {
                w.u8(1);
                w.u32(0);
            }
            Phase::Finished => {
                w.u8(2);
                w.u32(0);
            }
        }
        w.u32(s.race_tick);
        w.u32(s.first_finish.unwrap_or(u32::MAX));
        w.u32(s.finished_count);
        w.u8(s.karts.len().min(MAX_RACERS) as u8);
        for k in s.karts.iter().take(MAX_RACERS) {
            Self::write_kart(w, k);
        }
        let hazards = &s.hazards[s.hazards.len().saturating_sub(MAX_HAZARDS)..];
        w.u8(hazards.len() as u8);
        for h in hazards {
            w.f32(h.pos.0);
            w.f32(h.pos.2);
            w.u16(h.ttl.min(65535) as u16);
            w.u8(h.owner as u8);
            w.u8((h.kind == HazardKind::Bone) as u8);
        }
    }

    fn read_snapshot(r: &mut Reader) -> WireResult<KartSnapshot> {
        let phase = match (r.u8()?, r.u32()?) {
            (0, n) => Phase::Countdown(n),
            (1, _) => Phase::Racing,
            (2, _) => Phase::Finished,
            _ => return Err(WireError("bad phase")),
        };
        let race_tick = r.u32()?;
        let first = r.u32()?;
        let finished_count = r.u32()?;
        let n = r.u8()? as usize;
        if n > MAX_RACERS {
            return Err(WireError("too many karts"));
        }
        let karts = (0..n).map(|_| Self::read_kart(r)).collect::<WireResult<Vec<_>>>()?;
        let nh = r.u8()? as usize;
        if nh > MAX_HAZARDS {
            return Err(WireError("too many hazards"));
        }
        let mut hazards = Vec::with_capacity(nh);
        for _ in 0..nh {
            let (x, z) = (r.f32_within(5000.)?, r.f32_within(5000.)?);
            let (ttl, owner, bone) = (r.u16()? as u32, r.u8()? as usize, r.u8()?);
            if bone > 1 || owner >= MAX_RACERS {
                return Err(WireError("bad hazard"));
            }
            hazards.push(Hazard {
                pos: V(x, 0., z),
                radius: if bone == 1 { 1.6 } else { 2.4 },
                ttl,
                owner,
                kind: if bone == 1 { HazardKind::Bone } else { HazardKind::Bandage },
            });
        }
        Ok(KartSnapshot {
            phase,
            race_tick,
            first_finish: (first != u32::MAX).then_some(first),
            finished_count,
            karts,
            hazards,
        })
    }

    fn write_event(e: &Event, w: &mut Writer) {
        match e {
            Event::Count(n) => {
                w.u8(0);
                w.u8(*n as u8);
            }
            Event::Go => w.u8(1),
            Event::LapDone { kart, lap, ticks } => {
                w.u8(2);
                w.u8(*kart as u8);
                w.u8(*lap as u8);
                w.u32(*ticks);
            }
            Event::Finished { kart, place, ticks } => {
                w.u8(3);
                w.u8(*kart as u8);
                w.u8(*place as u8);
                w.u32(*ticks);
            }
            Event::DriftBoost { kart, tier } => {
                w.u8(4);
                w.u8(*kart as u8);
                w.u8(*tier as u8);
            }
            Event::Bump { a, b, speed } => {
                w.u8(5);
                w.u8(*a as u8);
                w.u8(*b as u8);
                w.f32(*speed);
            }
            Event::WallHit { kart, speed } => {
                w.u8(6);
                w.u8(*kart as u8);
                w.f32(*speed);
            }
            Event::PerkUsed { kart, perk } => {
                w.u8(7);
                w.u8(*kart as u8);
                w.u8(perk.index());
            }
            Event::HazardHit { kart, kind } => {
                w.u8(8);
                w.u8(*kart as u8);
                w.u8((*kind == HazardKind::Bone) as u8);
            }
            Event::RaceOver => w.u8(9),
        }
    }

    fn read_event(r: &mut Reader) -> WireResult<Event> {
        Ok(match r.u8()? {
            0 => Event::Count(r.u8()? as u32),
            1 => Event::Go,
            2 => Event::LapDone { kart: r.u8()? as usize, lap: r.u8()? as u32, ticks: r.u32()? },
            3 => Event::Finished { kart: r.u8()? as usize, place: r.u8()? as u32, ticks: r.u32()? },
            4 => Event::DriftBoost { kart: r.u8()? as usize, tier: r.u8()? as u32 },
            5 => Event::Bump { a: r.u8()? as usize, b: r.u8()? as usize, speed: r.f32()? },
            6 => Event::WallHit { kart: r.u8()? as usize, speed: r.f32()? },
            7 => Event::PerkUsed {
                kart: r.u8()? as usize,
                perk: Perk::from_index(r.u8()?).ok_or(WireError("bad perk"))?,
            },
            8 => Event::HazardHit {
                kart: r.u8()? as usize,
                kind: if r.u8()? == 1 { HazardKind::Bone } else { HazardKind::Bandage },
            },
            9 => Event::RaceOver,
            _ => return Err(WireError("unknown event")),
        })
    }

    /// Humans keep the characters they picked; bots take the rest in table order; the grid is shuffled.
    fn start(seed: u64, seats: &[Seat], participants: usize) -> (Sim, Vec<usize>) {
        let picked: Vec<Character> =
            seats.iter().map(|s| Character::from_wire(s.choice).unwrap_or(Character::Vampire)).collect();
        let total = participants.clamp(1, MAX_RACERS).max(seats.len());
        let mut grid: Vec<(Option<usize>, Character, Driver)> =
            picked.iter().enumerate().map(|(i, c)| (Some(i), *c, Driver::Human)).collect();
        for c in ALL.iter().filter(|c| !picked.contains(c)) {
            if grid.len() >= total {
                break;
            }
            grid.push((None, *c, Driver::Bot));
        }
        let mut order: Vec<usize> = (0..grid.len()).collect();
        Rng::new(seed).shuffle(&mut order);
        let shuffled: Vec<_> = order.iter().map(|&i| grid[i]).collect();
        let sim = Sim::with_grid(seed, &shuffled.iter().map(|g| (g.1, g.2)).collect::<Vec<_>>());
        let assigned =
            (0..seats.len()).map(|seat| shuffled.iter().position(|g| g.0 == Some(seat)).unwrap_or(0)).collect();
        (sim, assigned)
    }

    fn participants(m: &Sim) -> usize {
        m.karts.len()
    }

    fn step(m: &mut Sim, inputs: &[Option<KartInput>]) -> Vec<Event> {
        let mut all = Inputs::default();
        for (i, input) in inputs.iter().enumerate().take(MAX_RACERS) {
            if let Some(input) = input {
                all.0[i] = *input;
            }
        }
        m.step(&all);
        m.drain_events()
    }

    fn release(m: &mut Sim, participant: usize) {
        if let Some(kart) = m.karts.get_mut(participant) {
            kart.driver = Driver::Bot;
        }
    }

    fn snapshot(m: &Sim, _participant: Option<usize>) -> KartSnapshot {
        KartSnapshot {
            phase: m.phase,
            race_tick: m.race_tick,
            first_finish: m.first_finish,
            finished_count: m.finished_count,
            karts: m.karts.clone(),
            hazards: m.hazards.clone(),
        }
    }

    fn is_over(m: &Sim) -> bool {
        m.is_over()
    }

    fn report(m: &Sim) -> serde_json::Value {
        serde_json::to_value(m.report()).unwrap_or(serde_json::Value::Null)
    }
}

/// What a client keeps: a replica of the race to draw, its own predicted kart, and the last two snapshots to
/// interpolate the others between.
pub struct KartView {
    sim: Sim,
    track: Track,
    participant: Option<usize>,
    predicted: Option<Kart>,
    /// The visible gap between where prediction put the kart and where the server says it is; it eases out.
    offset: V,
    prev: Option<(Vec<Kart>, f64)>,
    cur: Option<(Vec<Kart>, f64)>,
    phase: Phase,
    race_tick: u32,
    now: f64,
    stats: PredictionStats,
}

impl KartView {
    /// The race as this client sees it (predicted own kart, interpolated rivals). Draw from this.
    pub fn sim(&self) -> &Sim {
        &self.sim
    }

    fn refresh(&mut self, now: f64) {
        let Some((cur, arrived)) = &self.cur else { return };
        let t = (((now - arrived) / SNAPSHOT_INTERVAL) as f32).clamp(0., 1.6);
        let mut karts = cur.clone();
        if let Some((prev, _)) = &self.prev {
            if prev.len() == karts.len() {
                for (k, p) in karts.iter_mut().zip(prev) {
                    k.pos = p.pos.lerp(k.pos, t);
                    k.yaw = wrap_angle(p.yaw + wrap_angle(k.yaw - p.yaw) * t);
                }
            }
        }
        if let (Some(slot), Some(mine)) = (self.participant, &self.predicted) {
            if slot < karts.len() {
                let server = &karts[slot];
                let (place, finished, lap) = (server.place, server.finished_tick, server.lap);
                let mut shown = mine.clone();
                shown.pos = mine.pos + self.offset;
                if finished.is_some() {
                    shown = server.clone();
                }
                shown.place = place;
                shown.finished_tick = finished;
                shown.lap = shown.lap.max(lap);
                karts[slot] = shown;
            }
        }
        let (phase, race_tick, first, count) =
            (self.phase, self.race_tick, self.sim.first_finish, self.sim.finished_count);
        let hazards = std::mem::take(&mut self.sim.hazards);
        self.sim.apply_view(phase, race_tick, first, count, karts, hazards);
    }
}

impl ClientView<KartGame> for KartView {
    fn new() -> Self {
        Self {
            sim: Sim::replica(),
            track: Track::haunted_hollow(),
            participant: None,
            predicted: None,
            offset: V(0., 0., 0.),
            prev: None,
            cur: None,
            phase: Phase::Countdown(0),
            race_tick: 0,
            now: 0.,
            stats: PredictionStats::default(),
        }
    }

    /// Reset the predicted kart to the server's and replay the inputs the server has not applied yet.
    fn on_snapshot(&mut self, s: &KartSnapshot, participant: Option<usize>, pending: &[(u32, KartInput)], now: f64) {
        self.now = now;
        self.phase = s.phase;
        self.race_tick = s.race_tick;
        self.participant = participant.filter(|p| *p < s.karts.len());
        if let Some(slot) = self.participant {
            let mut mine = s.karts[slot].clone();
            let before = self.predicted.as_ref().map(|p| p.pos);
            if mine.finished_tick.is_none() {
                let racing = s.phase == Phase::Racing;
                let mut tick = s.race_tick;
                for (_, input) in pending {
                    tick += 1;
                    mine.update(&self.track, input, racing, tick, LAPS);
                }
            }
            if let Some(before) = before {
                let error = before - mine.pos;
                let distance = error.length();
                self.stats.last_error = distance;
                self.stats.max_error = self.stats.max_error.max(distance);
                if distance > 0.02 {
                    self.stats.corrections += 1;
                    if distance > SNAP_DISTANCE {
                        self.stats.snaps += 1;
                        self.offset = V(0., 0., 0.);
                    } else {
                        // Keep showing the old position and ease toward the new one.
                        self.offset = self.offset + error;
                    }
                }
            }
            self.predicted = Some(mine);
        }
        self.prev = self.cur.take().or_else(|| Some((s.karts.clone(), now)));
        self.cur = Some((s.karts.clone(), now));
        self.sim.apply_view(s.phase, s.race_tick, s.first_finish, s.finished_count, Vec::new(), s.hazards.clone());
        self.refresh(now);
    }

    fn on_input(&mut self, input: &KartInput) {
        if let Some(kart) = self.predicted.as_mut() {
            if kart.finished_tick.is_none() {
                self.race_tick += 1;
                kart.update(&self.track, input, self.phase == Phase::Racing, self.race_tick, LAPS);
            }
        }
    }

    fn frame(&mut self, now: f64, dt: f32) {
        self.now = now;
        self.offset = self.offset * (-8. * dt.min(0.1)).exp();
        self.refresh(now);
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn prediction(&self) -> PredictionStats {
        self.stats.clone()
    }
}
