//! Deadfall as a [`NetGame`]: the layouts of what crosses the wire, how seats become players, and what a client
//! keeps between snapshots (a replica of the world, its own predicted body and hands, and the recent past for the
//! killcam). The network kit owns the lobby, sessions, input streaming and event delivery.
use crate::hands::{Busy, Ctx, Gun, Hands, Inventory, Out, Sel, Seen};
use crate::input::Input;
use crate::sim::{self, Event, Match, Phase, RosterEntry, Settings, MAX_PLAYERS};
use crate::weapons;
use std::collections::VecDeque;
use vesper3d::math::V;
use vesper3d::viewer::controller::{Controller, ControllerState};
use vesper3d::viewer::net::codec::{Reader, WireError, WireResult, Writer};
use vesper3d::viewer::netplay::{ClientView, NetGame, PredictionStats, Seat};

/// Bump when any layout or rule both sides must agree on changes (the fingerprint folds it in).
pub const PROTOCOL: u32 = 1;
/// Remote players are drawn this far in the past so there is always a snapshot to interpolate to (seconds).
pub const INTERP_DELAY: f32 = 0.1;
/// How much history a client keeps, for the killcam (seconds).
pub const HISTORY_SECONDS: f32 = 14.;

pub struct DeadfallGame;

// ---- what the server says -----------------------------------------------------------------------------------------

pub mod flag {
    pub const ALIVE: u8 = 1;
    pub const CROUCH: u8 = 2;
    pub const AIR: u8 = 4;
    pub const ADS: u8 = 8;
    pub const FIRING: u8 = 16;
    pub const RELOAD: u8 = 32;
    pub const BOT: u8 = 64;
    pub const PROTECT: u8 = 128;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerView {
    pub slot: u8,
    pub flags: u8,
    /// The eye.
    pub eye: V,
    pub yaw: f32,
    pub pitch: f32,
    pub health: u8,
    pub weapon: u8,
    pub kills: u8,
    pub deaths: u8,
    /// Floor height under the player (for the body model).
    pub feet: f32,
}

impl PlayerView {
    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }
}

/// Everything a client needs to predict its own body and hands, and to show its own state.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnView {
    pub ctrl: ControllerState,
    pub hands: Hands,
    pub inv: Inventory,
    pub health: f32,
    pub armor: f32,
    /// Seconds of blindness left and its full length (for the white-out).
    pub flash_left: f32,
    pub flash_total: f32,
    /// Dead: ticks until respawn, who killed this player and with what, and the tick they died (the killcam).
    pub respawn_ticks: u16,
    pub killer: u8,
    pub killer_weapon: u8,
    pub died_tick: u32,
    pub alive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjView {
    pub id: u16,
    pub weapon: u8,
    pub pos: V,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneView {
    pub kind: u8,
    pub pos: V,
    pub radius: f32,
    pub seconds_left: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DroppedView {
    pub id: u16,
    pub weapon: u8,
    pub pos: V,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub tick: u32,
    /// 0 live, 1 over.
    pub over: bool,
    /// 0 Ironclad, 1 Nightwatch, 2 draw (meaningful when over).
    pub winner: u8,
    pub scores: [u16; 2],
    /// Seconds left in a timed match, or `None` for a kill target.
    pub time_left: Option<f32>,
    pub kill_target: u16,
    pub players: Vec<PlayerView>,
    pub me: Option<OwnView>,
    pub projectiles: Vec<ProjView>,
    pub zones: Vec<ZoneView>,
    pub loot: u64,
    pub dropped: Vec<DroppedView>,
}

// ---- quantisation -------------------------------------------------------------------------------------------------

const POS_SCALE: f32 = 32.;

fn w_pos(w: &mut Writer, p: V) {
    for v in [p.0, p.1, p.2] {
        w.u16(((v * POS_SCALE).round().clamp(-32000., 32000.) as i16) as u16);
    }
}

fn r_pos(r: &mut Reader) -> WireResult<V> {
    let mut v = [0f32; 3];
    for x in &mut v {
        *x = (r.u16()? as i16) as f32 / POS_SCALE;
    }
    Ok(V(v[0], v[1], v[2]))
}

fn w_angle(w: &mut Writer, a: f32) {
    w.u16((a.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 65536.).min(65535.) as u16);
}

fn r_angle(r: &mut Reader) -> WireResult<f32> {
    Ok(r.u16()? as f32 / 65536. * std::f32::consts::TAU)
}

fn w_pitch(w: &mut Writer, p: f32) {
    w.u8(((p.clamp(-1.5, 1.5) / 1.5 * 127.).round() as i8) as u8);
}

fn r_pitch(r: &mut Reader) -> WireResult<f32> {
    Ok((r.u8()? as i8) as f32 / 127. * 1.5)
}

fn w_v(w: &mut Writer, v: V) {
    w.f32(v.0);
    w.f32(v.1);
    w.f32(v.2);
}

fn r_v(r: &mut Reader, limit: f32) -> WireResult<V> {
    Ok(V(r.f32_within(limit)?, r.f32_within(limit)?, r.f32_within(limit)?))
}

fn w_gun(w: &mut Writer, g: Option<Gun>) {
    match g {
        Some(g) => {
            w.u8(g.id);
            w.u16(g.mag);
            w.u16(g.reserve);
        }
        None => w.u8(0),
    }
}

fn r_gun(r: &mut Reader) -> WireResult<Option<Gun>> {
    let id = r.u8()?;
    if id == 0 {
        return Ok(None);
    }
    if weapons::get(id).is_none() {
        return Err(WireError("unknown weapon"));
    }
    Ok(Some(Gun { id, mag: r.u16()?, reserve: r.u16()? }))
}

fn w_hands(w: &mut Writer, h: &Hands) {
    w.u8(h.sel as u8);
    w.u8(h.busy as u8);
    w.u16(h.left);
    w.u16(h.total);
    w.u16(((h.cooldown + 1.) * 64.).clamp(0., 65535.) as u16);
    w.u8(h.burst_left);
    w.u8(h.burst_gap.min(255) as u8);
    w.u16((h.recoil * 256.).clamp(0., 65535.) as u16);
    w.u8((h.ads * 255.).round() as u8);
    w.u8(h.pin as u8 | (h.prev_fire as u8) << 1 | (h.prev_alt as u8) << 2 | (h.heavy as u8) << 3);
    w.u8(h.return_to.map_or(255, |s| s as u8));
    w.u16(h.impact_at);
    for s in [h.seen.reload, h.seen.use_, h.seen.melee, h.seen.drop, h.seen.switch] {
        w.u8(s);
    }
}

fn r_hands(r: &mut Reader) -> WireResult<Hands> {
    let sel = Sel::from_index(r.u8()?);
    let busy = Busy::from_index(r.u8()?);
    let (left, total) = (r.u16()?, r.u16()?);
    let cooldown = r.u16()? as f32 / 64. - 1.;
    let (burst_left, burst_gap) = (r.u8()?, r.u8()? as u16);
    let recoil = r.u16()? as f32 / 256.;
    let ads = r.u8()? as f32 / 255.;
    let bits = r.u8()?;
    let rt = r.u8()?;
    let impact_at = r.u16()?;
    let seen = Seen { reload: r.u8()?, use_: r.u8()?, melee: r.u8()?, drop: r.u8()?, switch: r.u8()? };
    Ok(Hands {
        sel,
        busy,
        left,
        total,
        cooldown,
        burst_left,
        burst_gap,
        recoil,
        ads,
        pin: bits & 1 != 0,
        prev_fire: bits & 2 != 0,
        prev_alt: bits & 4 != 0,
        heavy: bits & 8 != 0,
        return_to: (rt < 4).then(|| Sel::from_index(rt)),
        impact_at,
        seen,
    })
}

fn w_ctrl(w: &mut Writer, c: &ControllerState) {
    w_v(w, c.position);
    w_v(w, c.velocity);
    w.f32(c.feet);
    w.f32(c.body_height);
    w.f32(c.vertical_velocity);
    w.u8(c.grounded as u8 | ((c.push != V::ZERO) as u8) << 1);
    if c.push != V::ZERO {
        w_v(w, c.push);
    }
}

fn r_ctrl(r: &mut Reader) -> WireResult<ControllerState> {
    let position = r_v(r, 5000.)?;
    let velocity = r_v(r, 200.)?;
    let feet = r.f32_within(5000.)?;
    let body_height = r.f32_within(5.)?;
    let vertical_velocity = r.f32_within(200.)?;
    let bits = r.u8()?;
    let push = if bits & 2 != 0 { r_v(r, 200.)? } else { V::ZERO };
    Ok(ControllerState { position, yaw: 0., pitch: 0., velocity, feet, body_height, vertical_velocity, grounded: bits & 1 != 0, push })
}

impl Snapshot {
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.tick);
        w.u8(self.over as u8 | self.winner << 1);
        w.u16(self.scores[0]);
        w.u16(self.scores[1]);
        w.u16(self.time_left.map_or(u16::MAX, |t| (t * 4.).min(65000.) as u16));
        w.u16(self.kill_target);
        w.u8(self.players.len() as u8);
        for p in &self.players {
            w.u8(p.slot);
            w.u8(p.flags);
            w_pos(w, p.eye);
            w_pos(w, V(p.feet, 0., 0.));
            w_angle(w, p.yaw);
            w_pitch(w, p.pitch);
            w.u8(p.health);
            w.u8(p.weapon);
            w.u8(p.kills);
            w.u8(p.deaths);
        }
        match &self.me {
            None => w.u8(0),
            Some(o) => {
                w.u8(1);
                w_ctrl(w, &o.ctrl);
                w_hands(w, &o.hands);
                w_gun(w, o.inv.primary);
                w_gun(w, o.inv.secondary);
                w.u8(o.inv.melee);
                w.u8(o.inv.grenades[0]);
                w.u8(o.inv.grenades[1]);
                w.u8(o.health.round().clamp(0., 255.) as u8);
                w.u8(o.armor.round().clamp(0., 255.) as u8);
                w.u8((o.flash_left * 20.).clamp(0., 255.) as u8);
                w.u8((o.flash_total * 20.).clamp(0., 255.) as u8);
                w.u16(o.respawn_ticks);
                w.u8(o.killer);
                w.u8(o.killer_weapon);
                w.u32(o.died_tick);
                w.u8(o.alive as u8);
            }
        }
        w.u8(self.projectiles.len().min(24) as u8);
        for p in self.projectiles.iter().take(24) {
            w.u16(p.id);
            w.u8(p.weapon);
            w_pos(w, p.pos);
        }
        w.u8(self.zones.len().min(12) as u8);
        for z in self.zones.iter().take(12) {
            w.u8(z.kind);
            w_pos(w, z.pos);
            w.u8((z.radius * 4.).min(255.) as u8);
            w.u8(z.seconds_left.clamp(0., 255.) as u8);
        }
        w.u64(self.loot);
        w.u8(self.dropped.len().min(24) as u8);
        for d in self.dropped.iter().take(24) {
            w.u16(d.id);
            w.u8(d.weapon);
            w_pos(w, d.pos);
        }
    }

    pub fn read(r: &mut Reader) -> WireResult<Snapshot> {
        let tick = r.u32()?;
        let bits = r.u8()?;
        let scores = [r.u16()?, r.u16()?];
        let tl = r.u16()?;
        let kill_target = r.u16()?;
        let n = r.u8()? as usize;
        if n > MAX_PLAYERS {
            return Err(WireError("too many players"));
        }
        let mut players = Vec::with_capacity(n);
        for _ in 0..n {
            let slot = r.u8()?;
            let flags = r.u8()?;
            let eye = r_pos(r)?;
            let feet = r_pos(r)?.0;
            let yaw = r_angle(r)?;
            let pitch = r_pitch(r)?;
            players.push(PlayerView { slot, flags, eye, feet, yaw, pitch, health: r.u8()?, weapon: r.u8()?, kills: r.u8()?, deaths: r.u8()? });
        }
        let me = if r.u8()? == 1 {
            let ctrl = r_ctrl(r)?;
            let hands = r_hands(r)?;
            let inv = Inventory { primary: r_gun(r)?, secondary: r_gun(r)?, melee: r.u8()?, grenades: [r.u8()?, r.u8()?] };
            Some(OwnView {
                ctrl,
                hands,
                inv,
                health: r.u8()? as f32,
                armor: r.u8()? as f32,
                flash_left: r.u8()? as f32 / 20.,
                flash_total: r.u8()? as f32 / 20.,
                respawn_ticks: r.u16()?,
                killer: r.u8()?,
                killer_weapon: r.u8()?,
                died_tick: r.u32()?,
                alive: r.u8()? != 0,
            })
        } else {
            None
        };
        let np = r.u8()? as usize;
        let mut projectiles = Vec::with_capacity(np.min(24));
        for _ in 0..np.min(24) {
            projectiles.push(ProjView { id: r.u16()?, weapon: r.u8()?, pos: r_pos(r)? });
        }
        let nz = r.u8()? as usize;
        let mut zones = Vec::with_capacity(nz.min(12));
        for _ in 0..nz.min(12) {
            zones.push(ZoneView { kind: r.u8()?, pos: r_pos(r)?, radius: r.u8()? as f32 / 4., seconds_left: r.u8()? as f32 });
        }
        let loot = r.u64()?;
        let nd = r.u8()? as usize;
        let mut dropped = Vec::with_capacity(nd.min(24));
        for _ in 0..nd.min(24) {
            dropped.push(DroppedView { id: r.u16()?, weapon: r.u8()?, pos: r_pos(r)? });
        }
        Ok(Snapshot {
            tick,
            over: bits & 1 != 0,
            winner: (bits >> 1).min(2),
            scores,
            time_left: (tl != u16::MAX).then(|| tl as f32 / 4.),
            kill_target,
            players,
            me,
            projectiles,
            zones,
            loot,
            dropped,
        })
    }
}

fn write_event(e: &Event, w: &mut Writer) {
    match e {
        Event::Roster(entries) => {
            w.u8(0);
            w.u8(entries.len().min(MAX_PLAYERS) as u8);
            for r in entries.iter().take(MAX_PLAYERS) {
                w.u8(r.slot);
                w.u8(r.team | (r.bot as u8) << 1);
                w.text(&r.name, 20);
            }
        }
        Event::Shot { shooter, weapon, from, to, hit, material } => {
            w.u8(1);
            w.u8(*shooter);
            w.u8(*weapon);
            w_pos(w, *from);
            w_pos(w, *to);
            w.u8(*hit | material << 4);
        }
        Event::Hurt { victim, attacker, damage, head, weapon, from } => {
            w.u8(2);
            w.u8(*victim);
            w.u8(*attacker);
            w.u8(*damage);
            w.u8(*head as u8);
            w.u8(*weapon);
            w_pos(w, *from);
        }
        Event::Kill { killer, victim, weapon, head } => {
            w.u8(3);
            w.u8(*killer);
            w.u8(*victim);
            w.u8(*weapon);
            w.u8(*head as u8);
        }
        Event::Blast { pos, kind, radius } => {
            w.u8(4);
            w_pos(w, *pos);
            w.u8(*kind);
            w.u8((radius * 4.).min(255.) as u8);
        }
        Event::Strike { attacker, weapon, heavy, hit } => {
            w.u8(5);
            w.u8(*attacker);
            w.u8(*weapon);
            w.u8(*heavy as u8 | (*hit as u8) << 1);
        }
        Event::Launch { shooter, weapon, from } => {
            w.u8(6);
            w.u8(*shooter);
            w.u8(*weapon);
            w_pos(w, *from);
        }
        Event::Throw { thrower, weapon } => {
            w.u8(7);
            w.u8(*thrower);
            w.u8(*weapon);
        }
        Event::Pickup { player, weapon } => {
            w.u8(8);
            w.u8(*player);
            w.u8(*weapon);
        }
        Event::Dropped { player, weapon } => {
            w.u8(9);
            w.u8(*player);
            w.u8(*weapon);
        }
        Event::Spawned { player } => {
            w.u8(10);
            w.u8(*player);
        }
        Event::Over { winner, scores } => {
            w.u8(11);
            w.u8(*winner);
            w.u16(scores[0]);
            w.u16(scores[1]);
        }
    }
}

fn read_event(r: &mut Reader) -> WireResult<Event> {
    Ok(match r.u8()? {
        0 => {
            let n = r.u8()? as usize;
            if n > MAX_PLAYERS {
                return Err(WireError("roster too long"));
            }
            let mut entries = Vec::new();
            for _ in 0..n {
                let slot = r.u8()?;
                let b = r.u8()?;
                entries.push(RosterEntry { slot, team: b & 1, bot: b & 2 != 0, name: r.text(20)? });
            }
            Event::Roster(entries)
        }
        1 => {
            let shooter = r.u8()?;
            let weapon = r.u8()?;
            let from = r_pos(r)?;
            let to = r_pos(r)?;
            let b = r.u8()?;
            Event::Shot { shooter, weapon, from, to, hit: b & 15, material: b >> 4 }
        }
        2 => Event::Hurt { victim: r.u8()?, attacker: r.u8()?, damage: r.u8()?, head: r.u8()? != 0, weapon: r.u8()?, from: r_pos(r)? },
        3 => Event::Kill { killer: r.u8()?, victim: r.u8()?, weapon: r.u8()?, head: r.u8()? != 0 },
        4 => Event::Blast { pos: r_pos(r)?, kind: r.u8()?, radius: r.u8()? as f32 / 4. },
        5 => {
            let attacker = r.u8()?;
            let weapon = r.u8()?;
            let b = r.u8()?;
            Event::Strike { attacker, weapon, heavy: b & 1 != 0, hit: b & 2 != 0 }
        }
        6 => Event::Launch { shooter: r.u8()?, weapon: r.u8()?, from: r_pos(r)? },
        7 => Event::Throw { thrower: r.u8()?, weapon: r.u8()? },
        8 => Event::Pickup { player: r.u8()?, weapon: r.u8()? },
        9 => Event::Dropped { player: r.u8()?, weapon: r.u8()? },
        10 => Event::Spawned { player: r.u8()? },
        11 => Event::Over { winner: r.u8()?, scores: [r.u16()?, r.u16()?] },
        _ => return Err(WireError("unknown event")),
    })
}

// ---- the game -----------------------------------------------------------------------------------------------------

/// A number that changes whenever the rules, the armoury or the map change.
fn fingerprint() -> u32 {
    use std::sync::OnceLock;
    static F: OnceLock<u32> = OnceLock::new();
    *F.get_or_init(|| {
        let mut h: u32 = 0x811C_9DC5 ^ PROTOCOL;
        let mut mix = |v: u32| {
            h ^= v;
            h = h.wrapping_mul(0x0100_0193);
        };
        for w in weapons::WEAPONS {
            for b in w.key.bytes() {
                mix(b as u32);
            }
            for f in [w.damage, w.rpm, w.range_m, w.reload_s, w.spread_deg, w.move_speed, w.blast_damage] {
                mix(f.to_bits());
            }
            mix(w.mag as u32);
            mix(w.reserve as u32);
        }
        let world = sim::world();
        mix(world.level.blocks.len() as u32);
        for b in &world.level.blocks {
            for f in [b.min.0, b.min.1, b.min.2, b.max.0, b.max.1, b.max.2] {
                mix(f.to_bits());
            }
        }
        for t in 0..2 {
            for s in &world.level.spawns[t] {
                mix(s.pos.0.to_bits());
                mix(s.pos.2.to_bits());
            }
        }
        for l in &world.level.loot {
            mix(l.weapon as u32);
        }
        h
    })
}

impl NetGame for DeadfallGame {
    type Input = Input;
    type Match = Match;
    type View = DeadfallView;
    type Snapshot = Snapshot;
    type Event = Event;

    const NAME: &'static str = "deadfall";
    const MAX_SEATS: usize = MAX_PLAYERS;
    /// The lobby choice is the team: 0 Ironclad, 1 Nightwatch.
    const CHOICES: u8 = 2;
    const UNIQUE_CHOICES: bool = false;

    fn fingerprint() -> u32 {
        fingerprint()
    }
    fn write_input(input: &Input, w: &mut Writer) {
        input.write(w);
    }
    fn read_input(r: &mut Reader) -> WireResult<Input> {
        Input::read(r)
    }
    fn write_snapshot(s: &Snapshot, w: &mut Writer) {
        s.write(w);
    }
    fn read_snapshot(r: &mut Reader) -> WireResult<Snapshot> {
        Snapshot::read(r)
    }
    fn write_event(e: &Event, w: &mut Writer) {
        write_event(e, w);
    }
    fn read_event(r: &mut Reader) -> WireResult<Event> {
        read_event(r)
    }

    fn start(seed: u64, seats: &[Seat], _participants: usize) -> (Match, Vec<usize>) {
        let humans: Vec<(u8, String)> = seats.iter().map(|s| (s.choice, s.name.clone())).collect();
        Match::new(seed, &humans, sim::settings())
    }
    fn participants(m: &Match) -> usize {
        m.players.len()
    }
    fn step(m: &mut Match, inputs: &[Option<Input>]) -> Vec<Event> {
        m.step(inputs);
        std::mem::take(&mut m.events)
    }
    fn release(m: &mut Match, participant: usize) {
        m.release(participant);
    }
    fn snapshot(m: &Match, participant: Option<usize>) -> Snapshot {
        snapshot_of(m, participant)
    }
    fn is_over(m: &Match) -> bool {
        m.is_over()
    }
    fn report(m: &Match) -> serde_json::Value {
        serde_json::json!({
            "game": "deadfall",
            "ticks": m.tick,
            "scores": m.scores,
            "winner": match m.phase { Phase::Over { winner: Some(t), .. } => t.name(), _ => "draw" },
            "players": m.players.iter().map(|p| serde_json::json!({
                "name": p.name, "team": p.team.name(), "human": p.human,
                "kills": p.kills, "deaths": p.deaths, "headshots": p.headshots, "damage": p.damage,
            })).collect::<Vec<_>>(),
        })
    }
}

pub fn snapshot_of(m: &Match, participant: Option<usize>) -> Snapshot {
    let players = m
        .players
        .iter()
        .map(|p| {
            let mut flags = 0u8;
            let set = |flags: &mut u8, on: bool, f: u8| {
                if on {
                    *flags |= f;
                }
            };
            set(&mut flags, p.alive, flag::ALIVE);
            set(&mut flags, p.crouched(), flag::CROUCH);
            set(&mut flags, !p.ctrl.is_grounded(), flag::AIR);
            set(&mut flags, p.hands.ads > 0.5, flag::ADS);
            set(&mut flags, m.tick.saturating_sub(p.last_shot_tick) < 6 && p.alive && p.last_shot_tick > 0 && p.hands.recoil > 0.5, flag::FIRING);
            set(&mut flags, matches!(p.hands.busy, Busy::Reload | Busy::ShellLoad), flag::RELOAD);
            set(&mut flags, !p.human, flag::BOT);
            set(&mut flags, m.tick < p.protect_until, flag::PROTECT);
            PlayerView {
                slot: p.slot as u8,
                flags,
                eye: p.eye(),
                yaw: p.ctrl.yaw,
                pitch: p.ctrl.pitch,
                health: p.health.round().clamp(0., 255.) as u8,
                weapon: p.weapon(),
                kills: p.kills.min(255) as u8,
                deaths: p.deaths.min(255) as u8,
                feet: p.ctrl.feet_height(),
            }
        })
        .collect();
    let me = participant.and_then(|i| m.players.get(i)).map(|p| {
        let mut ctrl = p.ctrl.network_state();
        ctrl.yaw = p.ctrl.yaw;
        ctrl.pitch = p.ctrl.pitch;
        OwnView {
            ctrl,
            hands: p.hands,
            inv: p.inv,
            health: p.health,
            armor: p.armor,
            flash_left: if p.flash_until > m.tick { (p.flash_until - m.tick) as f32 / 60. } else { 0. },
            flash_total: p.flash_total as f32 / 60.,
            respawn_ticks: if p.alive { 0 } else { (p.died_at + sim::RESPAWN_TICKS).saturating_sub(m.tick).min(65535) as u16 },
            killer: p.killed_by.unwrap_or(255),
            killer_weapon: p.killed_with,
            died_tick: p.died_at,
            alive: p.alive,
        }
    });
    let mut loot = 0u64;
    for (i, l) in m.loot.iter().enumerate().take(64) {
        if l.available {
            loot |= 1 << i;
        }
    }
    Snapshot {
        tick: m.tick,
        over: matches!(m.phase, Phase::Over { .. }),
        winner: match m.phase {
            Phase::Over { winner: Some(t), .. } => t.index() as u8,
            _ => 2,
        },
        scores: m.scores,
        time_left: m.seconds_left(),
        kill_target: match m.settings.end {
            sim::EndRule::Kills { target } => target,
            _ => 0,
        },
        players,
        me,
        projectiles: m.projectiles.iter().map(|p| ProjView { id: p.id, weapon: p.weapon, pos: p.pos }).collect(),
        zones: m
            .zones
            .iter()
            .map(|z| ZoneView { kind: z.kind as u8, pos: z.pos, radius: z.radius, seconds_left: z.until.saturating_sub(m.tick) as f32 / 60. })
            .collect(),
        loot,
        dropped: m.dropped.iter().map(|d| DroppedView { id: d.id, weapon: d.gun.id, pos: d.pos }).collect(),
    }
}

// ---- what a client keeps ------------------------------------------------------------------------------------------

/// A cosmetic effect of the local player's own input, produced immediately (before the server answers).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Local {
    Shot { weapon: u8, punch: (f32, f32) },
    Launch { weapon: u8 },
    Throw { weapon: u8 },
    Strike { weapon: u8, heavy: bool },
    Dry { weapon: u8 },
    ReloadStarted { weapon: u8 },
    Switched { weapon: u8 },
}

#[derive(Clone, Debug)]
pub struct Stamped {
    /// When it arrived (seconds on the client clock).
    pub at: f64,
    pub snap: Snapshot,
}

/// The client's replica: the recent past, and the local player's body and hands, predicted.
pub struct DeadfallView {
    pub history: VecDeque<Stamped>,
    pub roster: Vec<RosterEntry>,
    pub body: Option<Controller>,
    pub hands: Hands,
    pub inv: Inventory,
    pub own: Option<OwnView>,
    /// Visual-only offset that eases a correction out instead of snapping the camera.
    pub error: V,
    pub last_input: Input,
    pub local: Vec<Local>,
    me: Option<usize>,
    last_seq: u32,
    corrections: u32,
    snaps: u32,
    max_error: f32,
    now: f64,
}

impl DeadfallView {
    pub fn latest(&self) -> Option<&Stamped> {
        self.history.back()
    }

    /// The server tick the screen is showing for other players right now (fractional).
    pub fn render_tick(&self) -> f32 {
        match self.latest() {
            None => 0.,
            Some(l) => {
                let since = (self.now - l.at).clamp(0., 0.25) as f32;
                l.snap.tick as f32 + since * 60. - INTERP_DELAY * 60.
            }
        }
    }

    /// Everyone as they were at server tick `tick`, interpolated between the snapshots around it.
    pub fn players_at(&self, tick: f32) -> Vec<PlayerView> {
        let mut before: Option<&Snapshot> = None;
        let mut after: Option<&Snapshot> = None;
        for s in self.history.iter().map(|s| &s.snap) {
            if s.tick as f32 <= tick {
                before = Some(s);
            } else {
                after = Some(s);
                break;
            }
        }
        match (before, after) {
            (Some(a), Some(b)) => {
                let t = ((tick - a.tick as f32) / (b.tick - a.tick).max(1) as f32).clamp(0., 1.);
                a.players
                    .iter()
                    .map(|p| match b.players.iter().find(|q| q.slot == p.slot) {
                        Some(q) => lerp_player(p, q, t),
                        None => *p,
                    })
                    .collect()
            }
            (Some(a), None) => a.players.clone(),
            (None, Some(b)) => b.players.clone(),
            (None, None) => Vec::new(),
        }
    }

    pub fn snapshot_at(&self, tick: f32) -> Option<&Snapshot> {
        self.history.iter().map(|s| &s.snap).filter(|s| s.tick as f32 <= tick).last().or_else(|| self.history.front().map(|s| &s.snap))
    }

    pub fn name_of(&self, slot: u8) -> String {
        self.roster.iter().find(|r| r.slot == slot).map_or_else(|| format!("Player {}", slot + 1), |r| r.name.clone())
    }
    pub fn team_of(&self, slot: u8) -> Option<crate::Team> {
        self.roster.iter().find(|r| r.slot == slot).map(|r| crate::Team::from_index(r.team as usize))
    }
    pub fn my_slot(&self) -> Option<usize> {
        self.me
    }

    /// The camera eye for the local player, with corrections eased out.
    pub fn eye(&self) -> Option<V> {
        self.body.as_ref().map(|b| b.position + self.error)
    }

    fn apply_input(&mut self, input: &Input, emit: bool) {
        let world = sim::world();
        let Some(body) = self.body.as_mut() else { return };
        let speed = weapons::get(self.inv.id_in(self.hands.sel)).map_or(1., |d| d.move_speed);
        sim::step_body(body, input, speed, &world.colliders);
        let ctx = Ctx { speed_frac: sim::speed_fraction(body), crouched: body.is_crouched(), airborne: !body.is_grounded() };
        let outs = self.hands.tick(&mut self.inv, input, &ctx);
        // Presses the server handles (use, drop) are acknowledged here too so a replay never doubles them.
        self.hands.seen.use_ = input.use_seq;
        self.hands.seen.drop = input.drop_seq;
        if emit {
            for o in outs {
                self.local.push(match o {
                    Out::Shot { weapon, punch, .. } => Local::Shot { weapon, punch },
                    Out::Launch { weapon, .. } => Local::Launch { weapon },
                    Out::Throw { weapon, .. } => Local::Throw { weapon },
                    Out::Strike { weapon, heavy } => Local::Strike { weapon, heavy },
                    Out::Dry { weapon } => Local::Dry { weapon },
                    Out::ReloadStarted { weapon } => Local::ReloadStarted { weapon },
                    Out::Switched { weapon } => Local::Switched { weapon },
                });
            }
        }
    }
}

fn lerp_player(a: &PlayerView, b: &PlayerView, t: f32) -> PlayerView {
    let d = |x: f32, y: f32| {
        let mut d = (y - x).rem_euclid(std::f32::consts::TAU);
        if d > std::f32::consts::PI {
            d -= std::f32::consts::TAU;
        }
        x + d * t
    };
    let snap = (b.eye - a.eye).length() > 6.; // a respawn: do not slide across the map
    PlayerView {
        eye: if snap { b.eye } else { a.eye.lerp(b.eye, t) },
        feet: if snap { b.feet } else { a.feet + (b.feet - a.feet) * t },
        yaw: d(a.yaw, b.yaw),
        pitch: a.pitch + (b.pitch - a.pitch) * t,
        ..if t < 0.5 { *a } else { *b }
    }
}

impl ClientView<DeadfallGame> for DeadfallView {
    fn new() -> Self {
        DeadfallView {
            history: VecDeque::new(),
            roster: Vec::new(),
            body: None,
            hands: Hands::default(),
            inv: Inventory::default(),
            own: None,
            error: V::ZERO,
            last_input: Input::default(),
            local: Vec::new(),
            me: None,
            last_seq: 0,
            corrections: 0,
            snaps: 0,
            max_error: 0.,
            now: 0.,
        }
    }

    fn on_snapshot(&mut self, snapshot: &Snapshot, participant: Option<usize>, pending: &[(u32, Input)], now: f64) {
        self.now = now;
        self.me = participant;
        self.history.push_back(Stamped { at: now, snap: snapshot.clone() });
        let cutoff = snapshot.tick.saturating_sub((HISTORY_SECONDS * 60.) as u32);
        while self.history.front().is_some_and(|s| s.snap.tick < cutoff) {
            self.history.pop_front();
        }
        let Some(own) = snapshot.me.clone() else {
            self.own = None;
            return;
        };
        let before = self.body.as_ref().map(|b| b.position);
        let body = self.body.get_or_insert_with(|| sim::new_body(V::ZERO, 0.));
        body.restore_network_state(&own.ctrl);
        body.yaw = self.last_input.yaw;
        body.pitch = self.last_input.pitch;
        self.hands = own.hands;
        self.inv = own.inv;
        self.own = Some(own.clone());
        if own.alive {
            for (seq, input) in pending {
                self.apply_input(input, false);
                self.last_seq = self.last_seq.max(*seq);
            }
            if let (Some(old), Some(new)) = (before, self.body.as_ref().map(|b| b.position)) {
                let e = old + self.error - new;
                self.max_error = self.max_error.max(e.length());
                if e.length() > 3. {
                    self.snaps += 1;
                    self.error = V::ZERO;
                } else {
                    if e.length() > 0.02 {
                        self.corrections += 1;
                    }
                    self.error = e;
                }
            }
        } else {
            self.error = V::ZERO;
        }
    }

    fn on_input(&mut self, input: &Input) {
        self.last_input = *input;
        if self.own.as_ref().is_some_and(|o| o.alive) {
            self.apply_input(input, true);
        }
    }

    fn frame(&mut self, now: f64, dt: f32) {
        self.now = now;
        let k = (-dt * 12.).exp();
        self.error = self.error * k;
        if self.error.length() < 0.001 {
            self.error = V::ZERO;
        }
    }

    fn reset(&mut self) {
        *self = <Self as ClientView<DeadfallGame>>::new();
    }

    fn prediction(&self) -> PredictionStats {
        PredictionStats { corrections: self.corrections as u64, snaps: self.snaps as u64, last_error: self.error.length(), max_error: self.max_error }
    }
}

/// `Settings` as the lobby shows them.
pub fn describe(s: &Settings) -> String {
    let end = match s.end {
        sim::EndRule::Time { minutes } => format!("{minutes} minutes"),
        sim::EndRule::Kills { target } => format!("first to {target} kills"),
    };
    format!("{end}{}", if s.bots { ", bots fill the teams" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_round_trips_and_fits_a_datagram_in_the_worst_case() {
        let (mut m, _) = Match::new(1, &[(0, "A".into()), (1, "B".into())], Settings { bots: true, ..Default::default() });
        for _ in 0..30 {
            m.step(&[None, None]);
        }
        let s = snapshot_of(&m, Some(0));
        let mut w = Writer::new();
        s.write(&mut w);
        let bytes = w.finish();
        let back = Snapshot::read(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(back.tick, s.tick);
        assert_eq!(back.players.len(), 12);
        assert_eq!(back.me.as_ref().unwrap().inv, s.me.as_ref().unwrap().inv);
        assert!((back.players[3].eye - s.players[3].eye).length() < 0.03);
        // The worst case: twelve players, full inventory, 24 projectiles, 12 zones, 24 dropped weapons.
        let mut big = s.clone();
        big.projectiles = vec![ProjView { id: 1, weapon: 26, pos: V(1., 1., 1.) }; 24];
        big.zones = vec![ZoneView { kind: 0, pos: V(1., 1., 1.), radius: 5., seconds_left: 10. }; 12];
        big.dropped = vec![DroppedView { id: 1, weapon: 9, pos: V(1., 0., 1.) }; 24];
        let mut w = Writer::new();
        big.write(&mut w);
        assert!(w.len() < 900, "the largest snapshot is {} bytes; a datagram holds 1200 with events", w.len());
    }

    #[test]
    fn every_event_round_trips() {
        let events = vec![
            Event::Roster(vec![RosterEntry { slot: 1, team: 1, bot: true, name: "Bot 1".into() }]),
            Event::Shot { shooter: 2, weapon: 9, from: V(1., 2., 3.), to: V(4., 5., 6.), hit: 3, material: 2 },
            Event::Hurt { victim: 1, attacker: 2, damage: 33, head: true, weapon: 9, from: V(0., 1., 0.) },
            Event::Kill { killer: 2, victim: 1, weapon: 9, head: true },
            Event::Blast { pos: V(1., 0., 1.), kind: 2, radius: 5. },
            Event::Strike { attacker: 1, weapon: 30, heavy: true, hit: true },
            Event::Launch { shooter: 1, weapon: 24, from: V(0., 1., 0.) },
            Event::Throw { thrower: 1, weapon: 26 },
            Event::Pickup { player: 1, weapon: 3 },
            Event::Dropped { player: 1, weapon: 3 },
            Event::Spawned { player: 4 },
            Event::Over { winner: 1, scores: [10, 12] },
        ];
        for e in events {
            let mut w = Writer::new();
            write_event(&e, &mut w);
            let bytes = w.finish();
            let back = read_event(&mut Reader::new(&bytes)).unwrap();
            match (&e, &back) {
                (Event::Shot { from, to, .. }, Event::Shot { from: f2, to: t2, .. }) => assert!((*from - *f2).length() < 0.05 && (*to - *t2).length() < 0.05),
                (Event::Hurt { from, .. }, Event::Hurt { from: f2, .. }) => assert!((*from - *f2).length() < 0.05),
                _ => assert_eq!(e, back),
            }
        }
    }

    #[test]
    fn garbage_never_panics() {
        let mut seed = 12345u64;
        for len in 0..200 {
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    (seed >> 33) as u8
                })
                .collect();
            let _ = Snapshot::read(&mut Reader::new(&bytes));
            let _ = read_event(&mut Reader::new(&bytes));
            let _ = Input::read(&mut Reader::new(&bytes));
        }
    }

    #[test]
    fn the_fingerprint_is_stable_within_a_run() {
        assert_eq!(DeadfallGame::fingerprint(), DeadfallGame::fingerprint());
    }
}
