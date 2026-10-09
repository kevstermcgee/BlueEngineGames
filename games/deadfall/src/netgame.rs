//! Deadfall as a [`NetGame`]: the layouts of what crosses the wire, how seats become players, and what a client
//! keeps between snapshots (a replica of the world, its own predicted body and hands, and the recent past for the
//! killcam). The network kit owns the lobby, sessions, input streaming and event delivery.
use crate::hands::{Busy, Ctx, Gun, Hands, Inventory, Out, Seen, Sel};
use crate::input::Input;
use crate::sim::{self, Event, Match, Phase, RosterEntry, Settings, MAX_PLAYERS};
use crate::weapons;
use std::collections::VecDeque;
use vesper3d::math::V;
use vesper3d::viewer::controller::{Controller, ControllerState};
use vesper3d::viewer::net::codec::{Reader, WireError, WireResult, Writer};
use vesper3d::viewer::netplay::{ClientView, NetGame, PredictionStats, Seat, SettingKind, SettingSpec};

/// Bump when any layout or rule both sides must agree on changes (the fingerprint folds it in).
pub const PROTOCOL: u32 = 3;
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
    pub appearance: u8,
    /// The eye.
    pub eye: V,
    pub yaw: f32,
    pub pitch: f32,
    pub health: u8,
    pub weapon: u8,
    pub kills: u16,
    pub deaths: u16,
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
    pub map: crate::maps::MapId,
    pub mode: crate::modes::GameMode,
    pub objective: crate::modes::Objectives,
    pub objective_target: u16,
    pub winner_slot: u8,
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
    Ok(ControllerState {
        position,
        yaw: 0.,
        pitch: 0.,
        velocity,
        feet,
        body_height,
        vertical_velocity,
        grounded: bits & 1 != 0,
        push,
    })
}

impl Snapshot {
    fn write_required(&self, w: &mut Writer) {
        w.u32(self.tick);
        w.u8(self.map as u8);
        w.u8(self.mode as u8);
        w.u16(self.objective_target);
        w.u8(self.winner_slot);
        let o = self.objective;
        for f in o.flags {
            w_pos(w, f.pos);
            w.u8(f.carrier);
            w.u32(f.dropped_at);
        }
        w.u16(o.round);
        w.u8(o.phase);
        w.u32(o.deadline);
        w_pos(w, o.bomb);
        w.u8(o.carrier);
        w.u8(o.site);
        w.u8(o.actor);
        w.u16(o.progress);
        w.u8(o.winner);
        w.u8(self.over as u8 | self.winner << 1);
        w.u16(self.scores[0]);
        w.u16(self.scores[1]);
        w.u16(self.time_left.map_or(u16::MAX, |t| (t * 4.).min(65000.) as u16));
        w.u16(self.kill_target);
        w.u8(self.players.len() as u8);
        for p in &self.players {
            w.u8(p.slot);
            w.u8(p.flags);
            w.u8(p.appearance);
            w_pos(w, p.eye);
            w_pos(w, V(p.feet, 0., 0.));
            w_angle(w, p.yaw);
            w_pitch(w, p.pitch);
            w.u8(p.health);
            w.u8(p.weapon);
            w.u16(p.kills);
            w.u16(p.deaths);
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
    }
    pub fn write(&self, w: &mut Writer) {
        self.write_required(w);
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

    pub fn fit_budget(&mut self, max_bytes: usize) {
        let mut required = Writer::new();
        self.write_required(&mut required);
        // Each optional world object uses nine bytes, with eleven bytes of counts and loot state.
        let mut room = max_bytes.saturating_sub(required.len() + 11) / 9;
        self.projectiles.truncate(room.min(24));
        room = room.saturating_sub(self.projectiles.len());
        self.zones.truncate(room.min(12));
        room = room.saturating_sub(self.zones.len());
        self.dropped.truncate(room.min(24));
    }
    pub fn read(r: &mut Reader) -> WireResult<Snapshot> {
        let tick = r.u32()?;
        let map_id = r.u8()?;
        let mode_id = r.u8()?;
        if map_id > 2 || mode_id > 3 {
            return Err(WireError("invalid map or mode"));
        }
        let map = crate::maps::MapId::from_id(map_id);
        let mode = crate::modes::GameMode::from_id(mode_id);
        let objective_target = r.u16()?;
        let winner_slot = r.u8()?;
        let mut objective = crate::modes::Objectives::new(map.bases());
        for f in &mut objective.flags {
            f.pos = r_pos(r)?;
            f.carrier = r.u8()?;
            f.dropped_at = r.u32()?;
            if f.carrier != 255 && f.carrier as usize >= MAX_PLAYERS {
                return Err(WireError("flag carrier"));
            }
        }
        objective.round = r.u16()?;
        objective.phase = r.u8()?;
        objective.deadline = r.u32()?;
        objective.bomb = r_pos(r)?;
        objective.carrier = r.u8()?;
        objective.site = r.u8()?;
        objective.actor = r.u8()?;
        objective.progress = r.u16()?;
        objective.winner = r.u8()?;
        if objective.phase > 3
            || objective.carrier != 255 && objective.carrier as usize >= MAX_PLAYERS
            || objective.actor != 255 && objective.actor as usize >= MAX_PLAYERS
            || objective.site != 255 && objective.site > 1
        {
            return Err(WireError("invalid objective"));
        }
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
            let appearance = r.u8()? & 31;
            let eye = r_pos(r)?;
            let feet = r_pos(r)?.0;
            let yaw = r_angle(r)?;
            let pitch = r_pitch(r)?;
            players.push(PlayerView {
                slot,
                flags,
                appearance,
                eye,
                feet,
                yaw,
                pitch,
                health: r.u8()?,
                weapon: r.u8()?,
                kills: r.u16()?,
                deaths: r.u16()?,
            });
        }
        let me = if r.u8()? == 1 {
            let ctrl = r_ctrl(r)?;
            let hands = r_hands(r)?;
            let inv =
                Inventory { primary: r_gun(r)?, secondary: r_gun(r)?, melee: r.u8()?, grenades: [r.u8()?, r.u8()?] };
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
            zones.push(ZoneView {
                kind: r.u8()?,
                pos: r_pos(r)?,
                radius: r.u8()? as f32 / 4.,
                seconds_left: r.u8()? as f32,
            });
        }
        let loot = r.u64()?;
        let nd = r.u8()? as usize;
        let mut dropped = Vec::with_capacity(nd.min(24));
        for _ in 0..nd.min(24) {
            dropped.push(DroppedView { id: r.u16()?, weapon: r.u8()?, pos: r_pos(r)? });
        }
        Ok(Snapshot {
            tick,
            map,
            mode,
            objective,
            objective_target,
            winner_slot,
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
        2 => Event::Hurt {
            victim: r.u8()?,
            attacker: r.u8()?,
            damage: r.u8()?,
            head: r.u8()? != 0,
            weapon: r.u8()?,
            from: r_pos(r)?,
        },
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
/// Gameplay numbers at 0.1 mm / 0.0001 numeric units. Raw atan2 bits can differ across OS math libraries.
fn fingerprint_number(value: f32) -> u32 {
    (f64::from(value) * 10_000.).round() as i32 as u32
}

pub fn fingerprint() -> u32 {
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
            for f in [
                w.damage,
                w.head_mult,
                w.armor_pen,
                w.far_fraction,
                w.rpm,
                w.range_m,
                w.reload_s,
                w.spread_deg,
                w.move_spread_deg,
                w.ads_spread_mult,
                w.recoil_up_deg,
                w.recoil_side_deg,
                w.recoil_recover,
                w.move_speed,
                w.draw_s,
                w.ads_fov,
                w.ads_s,
                w.blast_damage,
            ] {
                mix(fingerprint_number(f));
            }
            mix(w.class as u32);
            mix(w.slot as u32);
            mix(w.shell_reload as u32);
            mix(w.pellets as u32);
            match w.fire {
                weapons::Fire::Semi => mix(0),
                weapons::Fire::Auto => mix(1),
                weapons::Fire::Burst { rounds, gap_s } => {
                    mix(2);
                    mix(rounds as u32);
                    mix(fingerprint_number(gap_s));
                }
                weapons::Fire::Cycle { cycle_s } => {
                    mix(3);
                    mix(fingerprint_number(cycle_s));
                }
                weapons::Fire::Throw => mix(4),
                weapons::Fire::Swing => mix(5),
            }
            match w.sight {
                weapons::Sight::Iron => mix(0),
                weapons::Sight::Dot => mix(1),
                weapons::Sight::Scope { zoom } => {
                    mix(2);
                    mix(fingerprint_number(zoom));
                }
                weapons::Sight::None => mix(3),
            }
            if let Some(p) = w.projectile {
                mix(1);
                for f in [p.speed, p.gravity, p.fuse_s] {
                    mix(fingerprint_number(f));
                }
                mix(p.bounces as u32);
                match p.effect {
                    weapons::Effect::Explosion { radius } => {
                        mix(0);
                        mix(fingerprint_number(radius));
                    }
                    weapons::Effect::Flash { radius, blind_s } => {
                        mix(1);
                        mix(fingerprint_number(radius));
                        mix(fingerprint_number(blind_s));
                    }
                    weapons::Effect::Smoke { radius, seconds } => {
                        mix(2);
                        mix(fingerprint_number(radius));
                        mix(fingerprint_number(seconds));
                    }
                    weapons::Effect::Fire { radius, seconds } => {
                        mix(3);
                        mix(fingerprint_number(radius));
                        mix(fingerprint_number(seconds));
                    }
                }
            } else {
                mix(0);
            }
            if let Some(m) = w.melee {
                mix(1);
                for f in [m.reach, m.light, m.heavy, m.light_s, m.heavy_s, m.back_mult] {
                    mix(fingerprint_number(f));
                }
            } else {
                mix(0);
            }
            mix(w.mag as u32);
            mix(w.reserve as u32);
        }
        for map in crate::maps::MapId::ALL {
            let world = map.level();
            mix(world.blocks.len() as u32);
            for b in &world.blocks {
                mix(b.material as u32);
                for f in [b.min.0, b.min.1, b.min.2, b.max.0, b.max.1, b.max.2] {
                    mix(fingerprint_number(f));
                }
            }
            for t in 0..2 {
                for s in &world.spawns[t] {
                    mix(fingerprint_number(s.pos.0));
                    mix(fingerprint_number(s.pos.1));
                    mix(fingerprint_number(s.pos.2));
                    mix(fingerprint_number(s.yaw));
                }
            }
            for l in &world.loot {
                mix(l.weapon as u32);
                for f in [l.pos.0, l.pos.1, l.pos.2, l.respawn_s] {
                    mix(fingerprint_number(f));
                }
            }
            for p in map.bases().into_iter().chain(map.sites()) {
                for f in [p.0, p.1, p.2] {
                    mix(fingerprint_number(f));
                }
            }
        }
        let profile = sim::profile();
        for f in [
            profile.walk_speed,
            profile.sprint_speed,
            profile.jump_height,
            profile.crouch_speed,
            sim::KILLCAM_SECONDS,
            sim::GRAVITY,
        ] {
            mix(fingerprint_number(f));
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
    const CHOICES: u8 = 32;
    const UNIQUE_CHOICES: bool = false;
    const RELIABLE_EVENTS: bool = true;
    fn lobby_capacity() -> usize {
        if sim::settings().duel {
            2
        } else {
            MAX_PLAYERS
        }
    }
    fn minimum_players() -> usize {
        if sim::settings().bots && !sim::settings().duel {
            1
        } else {
            2
        }
    }

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

    fn settings() -> &'static [SettingSpec] {
        SETTINGS
    }
    fn configure(values: &[(u8, u32)]) -> Result<(), String> {
        sim::set_settings(settings_from(values)?);
        Ok(())
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
    fn snapshot_with_budget(m: &Match, participant: Option<usize>, max_bytes: usize) -> Snapshot {
        let mut s = snapshot_of(m, participant);
        s.fit_budget(max_bytes);
        s
    }
    fn is_over(m: &Match) -> bool {
        m.is_over()
    }
    fn report(m: &Match) -> serde_json::Value {
        serde_json::json!({
            "game": "deadfall",
            "ticks": m.tick,
            "map":m.settings.map.name(),"mode":m.settings.mode.name(),"duel":m.settings.duel,"rounds":m.objective.round,"winner_slot":m.winner_slot,
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
            set(
                &mut flags,
                m.tick.saturating_sub(p.last_shot_tick) < 6 && p.alive && p.last_shot_tick > 0 && p.hands.recoil > 0.5,
                flag::FIRING,
            );
            set(&mut flags, matches!(p.hands.busy, Busy::Reload | Busy::ShellLoad), flag::RELOAD);
            set(&mut flags, !p.human, flag::BOT);
            set(&mut flags, m.tick < p.protect_until, flag::PROTECT);
            PlayerView {
                slot: p.slot as u8,
                flags,
                appearance: p.appearance,
                eye: p.eye(),
                yaw: p.ctrl.yaw,
                pitch: p.ctrl.pitch,
                health: p.health.round().clamp(0., 255.) as u8,
                weapon: p.weapon(),
                kills: p.kills,
                deaths: p.deaths,
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
            respawn_ticks: if p.alive {
                0
            } else {
                (p.died_at + sim::RESPAWN_TICKS).saturating_sub(m.tick).min(65535) as u16
            },
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
    let eye = participant.and_then(|p| m.players.get(p)).map_or(V::ZERO, |p| p.eye());
    let mut result = Snapshot {
        tick: m.tick,
        map: m.settings.map,
        mode: m.settings.mode,
        objective: m.objective,
        objective_target: m.settings.objective_target,
        winner_slot: m.winner_slot,
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
            .map(|z| ZoneView {
                kind: z.kind as u8,
                pos: z.pos,
                radius: z.radius,
                seconds_left: z.until.saturating_sub(m.tick) as f32 / 60.,
            })
            .collect(),
        loot,
        dropped: m.dropped.iter().map(|d| DroppedView { id: d.id, weapon: d.gun.id, pos: d.pos }).collect(),
    };
    result.projectiles.sort_by(|a, b| (a.pos - eye).length().total_cmp(&(b.pos - eye).length()));
    result.zones.sort_by(|a, b| (a.pos - eye).length().total_cmp(&(b.pos - eye).length()));
    result.dropped.sort_by(|a, b| (a.pos - eye).length().total_cmp(&(b.pos - eye).length()));
    result
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
    previous_eye: Option<V>,
    /// Effects received with each snapshot, retained alongside the killcam world history.
    pub replay_events: VecDeque<(u32, Event)>,
    server_tick_offset: Option<u32>,
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
        let i = self.history.partition_point(|s| s.snap.tick as f32 <= tick);
        let before = i.checked_sub(1).map(|i| &self.history[i].snap);
        let after = self.history.get(i).map(|s| &s.snap);
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
        let i = self.history.partition_point(|s| s.snap.tick as f32 <= tick);
        self.history.get(i.saturating_sub(1)).map(|s| &s.snap)
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

    /// Interpolate fixed movement ticks for presentation only; prediction stays authoritative.
    pub fn eye_at(&self, fraction: f32) -> Option<V> {
        self.body
            .as_ref()
            .map(|b| self.previous_eye.unwrap_or(b.position).lerp(b.position, fraction.clamp(0., 1.)) + self.error)
    }

    pub fn remember_timed_events(&mut self, server_tick: u32, events: &[(u32, Event)]) {
        let game_tick = self.latest().map_or(0, |s| s.snap.tick);
        let measured = server_tick.saturating_sub(game_tick);
        let offset = *self.server_tick_offset.get_or_insert(measured);
        let offset = offset.min(measured);
        self.server_tick_offset = Some(offset);
        for (tick, e) in events {
            self.replay_events.push_back((tick.saturating_sub(offset), e.clone()));
        }
        let cutoff = game_tick.saturating_sub((HISTORY_SECONDS * 60.) as u32);
        while self.replay_events.front().is_some_and(|(t, _)| *t < cutoff) {
            self.replay_events.pop_front();
        }
    }
    pub fn remember_events(&mut self, events: &[Event]) {
        let tick = self.latest().map_or(0, |s| s.snap.tick);
        self.replay_events.extend(events.iter().cloned().map(|e| (tick, e)));
        let cutoff = tick.saturating_sub((HISTORY_SECONDS * 60.) as u32);
        while self.replay_events.front().is_some_and(|e| e.0 < cutoff) {
            self.replay_events.pop_front();
        }
    }

    fn apply_input(&mut self, input: &Input, emit: bool) {
        let world = sim::world_on(self.latest().map_or(crate::maps::MapId::Slagworks, |s| s.snap.map));
        let Some(body) = self.body.as_mut() else { return };
        self.previous_eye = Some(body.position);
        let speed = weapons::get(self.inv.id_in(self.hands.sel)).map_or(1., |d| d.move_speed);
        sim::step_body(body, input, speed, &world.colliders);
        let ctx =
            Ctx { speed_frac: sim::speed_fraction(body), crouched: body.is_crouched(), airborne: !body.is_grounded() };
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
    // A death or respawn happens at its snapshot, never halfway through the preceding interval.
    if a.has(flag::ALIVE) != b.has(flag::ALIVE) || (b.eye - a.eye).length() > 6. {
        return if t < 1. { *a } else { *b };
    }
    PlayerView {
        eye: a.eye.lerp(b.eye, t),
        feet: a.feet + (b.feet - a.feet) * t,
        yaw: d(a.yaw, b.yaw),
        pitch: a.pitch + (b.pitch - a.pitch) * t,
        ..if t < 1. { *a } else { *b }
    }
}

impl ClientView<DeadfallGame> for DeadfallView {
    fn new() -> Self {
        DeadfallView {
            history: VecDeque::new(),
            roster: Vec::new(),
            body: None,
            previous_eye: None,
            replay_events: VecDeque::new(),
            server_tick_offset: None,
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
        let respawned = own.alive && self.own.as_ref().is_none_or(|old| !old.alive || old.died_tick != own.died_tick);
        let before = if respawned { None } else { self.body.as_ref().map(|b| b.position) };
        if respawned {
            self.error = V::ZERO;
            self.previous_eye = Some(own.ctrl.position);
        }
        let previous_eye = self.previous_eye;
        let body = self.body.get_or_insert_with(|| sim::new_body(V::ZERO, 0.));
        body.restore_network_state(&own.ctrl);
        body.yaw = self.last_input.yaw;
        body.pitch = self.last_input.pitch;
        self.hands = own.hands;
        self.inv = own.inv;
        self.own = Some(own.clone());
        if own.alive && !sim::autopilot() {
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
                self.previous_eye = Some(if e.length() > 3. { new } else { previous_eye.unwrap_or(old) + new - old });
            }
        } else {
            self.error = V::ZERO;
            self.previous_eye = self.body.as_ref().map(|b| b.position);
            if own.alive {
                // Autopilot: the server drives, the camera simply follows.
                self.last_input.yaw = own.ctrl.yaw;
                self.last_input.pitch = own.ctrl.pitch;
            }
        }
    }

    fn on_input(&mut self, input: &Input) {
        self.last_input = *input;
        if self.own.as_ref().is_some_and(|o| o.alive) && !sim::autopilot() {
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
        PredictionStats {
            corrections: self.corrections as u64,
            snaps: self.snaps as u64,
            last_error: self.error.length(),
            max_error: self.max_error,
        }
    }
}

// ---- room settings (hub and server flags) ------------------------------------------------------------------------

/// The setting ids are on the hub's wire and in its registry: never renumber or reuse one. The names `bots` and `kills`
/// are what the engine hub's legacy adapter maps the shipped clients' `DFHB` Create request to.
pub const SETTING_BOTS: u8 = 1;
pub const SETTING_KILLS: u8 = 2;
pub const SETTING_SKILL: u8 = 3;
pub const SETTING_MINUTES: u8 = 4;
pub const SETTING_MODE: u8 = 5;
pub const SETTING_MAP: u8 = 6;
pub const SETTING_DUEL: u8 = 7;
pub const SETTING_OBJECTIVE: u8 = 8;

/// What a room may be asked for: `--bots`, `--kills N`, `--skill N`, `--minutes N` on `deadfall-server`, or
/// `--set ID=VALUE` from a hub.
pub static SETTINGS: &[SettingSpec] = &[
    SettingSpec { id: SETTING_BOTS, name: "bots", flag: "bots", kind: SettingKind::Bool, min: 0, max: 1, default: 0 },
    SettingSpec {
        id: SETTING_KILLS,
        name: "kills",
        flag: "kills",
        kind: SettingKind::Int,
        min: 1,
        max: 500,
        default: 40,
    },
    SettingSpec {
        id: SETTING_SKILL,
        name: "skill",
        flag: "skill",
        kind: SettingKind::Choice,
        min: 0,
        max: 2,
        default: 1,
    },
    SettingSpec {
        id: SETTING_MINUTES,
        name: "minutes",
        flag: "minutes",
        kind: SettingKind::Int,
        min: 0,
        max: 60,
        default: 0,
    },
    SettingSpec { id: SETTING_MODE, name: "mode", flag: "mode", kind: SettingKind::Choice, min: 0, max: 3, default: 0 },
    SettingSpec { id: SETTING_MAP, name: "map", flag: "map", kind: SettingKind::Choice, min: 0, max: 2, default: 0 },
    SettingSpec { id: SETTING_DUEL, name: "duel", flag: "duel", kind: SettingKind::Bool, min: 0, max: 1, default: 0 },
    SettingSpec {
        id: SETTING_OBJECTIVE,
        name: "objective",
        flag: "objective",
        kind: SettingKind::Int,
        min: 1,
        max: 20,
        default: 3,
    },
];

/// The match [`Settings`] for the `(id, value)` pairs a server was started with (a missing id takes its default).
/// `minutes` above zero ends the match on the clock and wins over `kills`; zero means the kill target.
pub fn settings_from(values: &[(u8, u32)]) -> Result<Settings, String> {
    let get = |id: u8| -> Result<u32, String> {
        let spec = SETTINGS.iter().find(|s| s.id == id).expect("a known setting id");
        let v = values.iter().find(|(i, _)| *i == id).map_or(spec.default, |(_, v)| *v);
        if (spec.min..=spec.max).contains(&v) {
            Ok(v)
        } else {
            Err(format!("{} must be {}..={}, got {v}", spec.name, spec.min, spec.max))
        }
    };
    if let Some((id, _)) = values.iter().find(|(id, _)| !SETTINGS.iter().any(|s| s.id == *id)) {
        return Err(format!("unknown setting id {id}"));
    }
    let minutes = get(SETTING_MINUTES)?;
    Ok(Settings {
        end: if minutes > 0 {
            sim::EndRule::Time { minutes: minutes as u16 }
        } else {
            sim::EndRule::Kills { target: get(SETTING_KILLS)? as u16 }
        },
        bots: get(SETTING_BOTS)? != 0 && get(SETTING_DUEL)? == 0,
        bot_skill: get(SETTING_SKILL)? as u8,
        mode: crate::modes::GameMode::from_id(get(SETTING_MODE)? as u8),
        map: crate::maps::MapId::from_id(get(SETTING_MAP)? as u8),
        duel: get(SETTING_DUEL)? != 0,
        objective_target: get(SETTING_OBJECTIVE)? as u16,
    })
}

/// `Settings` as the lobby shows them.
pub fn describe(s: &Settings) -> String {
    let end = match s.end {
        sim::EndRule::Time { minutes } => format!("{minutes} minutes"),
        sim::EndRule::Kills { target } => format!("first to {target} kills"),
    };
    format!(
        "{} / {} / {}{}",
        s.map.name(),
        s.mode.name(),
        if s.mode == crate::modes::GameMode::CaptureFlag || s.mode == crate::modes::GameMode::SearchDestroy {
            format!("first to {}", s.objective_target)
        } else {
            end
        },
        if s.duel {
            " / 1v1"
        } else if s.bots {
            " / bots"
        } else {
            ""
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_ignores_platform_roundoff_but_detects_gameplay_changes() {
        let angle = (-14f32).atan2(50.);
        let adjacent = f32::from_bits(angle.to_bits() + 1);
        assert_eq!(fingerprint_number(angle), fingerprint_number(adjacent));
        assert_eq!(fingerprint_number(-0.), fingerprint_number(0.));
        assert_ne!(fingerprint_number(7.2), fingerprint_number(7.21));
    }

    #[test]
    fn room_settings_declare_stable_ids_and_build_the_match_rules() {
        let table: Vec<_> = SETTINGS.iter().map(|s| (s.id, s.name, s.flag, s.kind, s.min, s.max, s.default)).collect();
        assert_eq!(
            table,
            vec![
                (1, "bots", "bots", SettingKind::Bool, 0, 1, 0),
                (2, "kills", "kills", SettingKind::Int, 1, 500, 40),
                (3, "skill", "skill", SettingKind::Choice, 0, 2, 1),
                (4, "minutes", "minutes", SettingKind::Int, 0, 60, 0),
                (5, "mode", "mode", SettingKind::Choice, 0, 3, 0),
                (6, "map", "map", SettingKind::Choice, 0, 2, 0),
                (7, "duel", "duel", SettingKind::Bool, 0, 1, 0),
                (8, "objective", "objective", SettingKind::Int, 1, 20, 3),
            ]
        );
        assert_eq!(settings_from(&[]), Ok(Settings::default()), "no values means the old server's defaults");
        let s = settings_from(&[(1, 1), (2, 25), (3, 2)]).unwrap();
        assert_eq!((s.end, s.bots, s.bot_skill), (sim::EndRule::Kills { target: 25 }, true, 2));
        let s = settings_from(&[(2, 25), (4, 10)]).unwrap();
        assert_eq!(s.end, sim::EndRule::Time { minutes: 10 }, "minutes win over kills");
        assert_eq!(settings_from(&[(4, 0), (2, 7)]).unwrap().end, sim::EndRule::Kills { target: 7 });
        assert!(settings_from(&[(2, 0)]).is_err() && settings_from(&[(2, 501)]).is_err());
        assert!(
            settings_from(&[(3, 3)]).is_err()
                && settings_from(&[(4, 61)]).is_err()
                && settings_from(&[(9, 1)]).is_err()
        );
    }

    /// The join fingerprint of the shipped clients and the deployed server. A change to a weapon number, the map or
    /// any wire layout changes it and locks every shipped client out: if this fails, either revert the change or
    /// accept that the next release is a breaking one and update the pin on purpose.
    #[test]
    fn the_join_fingerprint_is_pinned() {
        assert_eq!(fingerprint(), 0x6D4F8D2B, "netgame::fingerprint() changed");
        assert_eq!(<DeadfallGame as NetGame>::fingerprint(), fingerprint());
    }

    #[test]
    fn replay_interpolates_the_recorded_victim_and_keeps_death_at_its_tick() {
        let (m, _) = Match::new(1, &[(0, "A".into()), (1, "B".into())], Settings { bots: false, ..Default::default() });
        let mut a = snapshot_of(&m, Some(0));
        a.tick = 100;
        a.players[0].eye = V(0., 1.68, 0.);
        let mut b = a.clone();
        b.tick = 110;
        b.players[0].eye = V(1., 1.68, 0.);
        let mut dead = b.clone();
        dead.tick = 120;
        dead.players[0].flags &= !flag::ALIVE;
        let mut live = dead.clone();
        live.tick = 600;
        live.players[0].flags |= flag::ALIVE;
        live.players[0].eye = V(40., 1.68, 40.);
        let mut v = <DeadfallView as ClientView<DeadfallGame>>::new();
        for snap in [a, b, dead, live] {
            v.history.push_back(Stamped { at: 0., snap });
        }
        assert!((v.players_at(105.)[0].eye.0 - 0.5).abs() < 0.001);
        assert!(v.players_at(119.9)[0].has(flag::ALIVE), "do not hide the victim before the fatal shot");
        assert!(!v.players_at(120.)[0].has(flag::ALIVE));
        assert!(!v.players_at(599.9)[0].has(flag::ALIVE), "do not pull a later respawn into the replay");
        assert_eq!(v.snapshot_at(105.).unwrap().tick, 100);
        assert_eq!(v.snapshot_at(120.).unwrap().tick, 120);
    }

    #[test]
    fn the_local_camera_moves_smoothly_between_prediction_ticks_without_mutating_them() {
        let (m, _) = Match::new(1, &[(0, "A".into())], Settings { bots: false, ..Default::default() });
        let s = snapshot_of(&m, Some(0));
        let mut v = <DeadfallView as ClientView<DeadfallGame>>::new();
        v.on_snapshot(&s, Some(0), &[], 0.);
        let start = v.eye().unwrap();
        v.on_input(&Input { forward: 127, ..Default::default() });
        let end = v.eye().unwrap();
        assert!((end - start).length() > 0.001);
        assert!((v.eye_at(0.).unwrap() - start).length() < 0.001);
        assert!((v.eye_at(0.5).unwrap() - start.lerp(end, 0.5)).length() < 0.001);
        assert_eq!(v.eye().unwrap(), end);
        assert_eq!(v.eye_at(1.).unwrap(), end);
    }

    #[test]
    fn replay_effects_expire_with_world_history() {
        let (m, _) = Match::new(1, &[(0, "A".into())], Settings { bots: false, ..Default::default() });
        let mut s = snapshot_of(&m, Some(0));
        let mut v = <DeadfallView as ClientView<DeadfallGame>>::new();
        v.on_snapshot(&s, Some(0), &[], 0.);
        v.remember_events(&[Event::Kill { killer: 1, victim: 0, weapon: 1, head: false }]);
        assert_eq!(v.replay_events.len(), 1);
        s.tick = (HISTORY_SECONDS * 60.) as u32 + 1;
        v.on_snapshot(&s, Some(0), &[], 15.);
        v.remember_events(&[]);
        assert!(v.replay_events.is_empty());
    }

    #[test]
    fn a_snapshot_round_trips_and_fits_a_datagram_in_the_worst_case() {
        let (mut m, _) =
            Match::new(1, &[(0, "A".into()), (1, "B".into())], Settings { bots: true, ..Default::default() });
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
        for p in &mut big.players {
            p.kills = 500;
            p.deaths = 500;
        }
        big.projectiles = vec![ProjView { id: 1, weapon: 26, pos: V(1., 1., 1.) }; 24];
        big.zones = vec![ZoneView { kind: 0, pos: V(1., 1., 1.), radius: 5., seconds_left: 10. }; 12];
        big.dropped = vec![DroppedView { id: 1, weapon: 9, pos: V(1., 0., 1.) }; 24];
        let mut w = Writer::new();
        big.write(&mut w);
        assert!(
            w.len() + 29 + 18 <= vesper3d::viewer::net::MAX_PACKET_BYTES,
            "snapshot plus authenticated state framing is {} bytes",
            w.len() + 47
        );
        for budget in [600, 750, 900, 1055] {
            let mut bounded = big.clone();
            bounded.fit_budget(budget);
            let mut w = Writer::new();
            bounded.write(&mut w);
            assert!(w.len() <= budget, "{} > {budget}", w.len());
            let back = Snapshot::read(&mut Reader::new(w.as_slice())).unwrap();
            assert_eq!(back.players.len(), big.players.len());
            for (a, b) in back.players.iter().zip(&big.players) {
                assert_eq!(
                    (a.slot, a.flags, a.appearance, a.kills, a.deaths),
                    (b.slot, b.flags, b.appearance, b.kills, b.deaths)
                );
                assert!((a.eye - b.eye).length() < 0.03);
            }
            assert_eq!(back.objective, big.objective);
            assert_eq!(back.me.as_ref().unwrap().inv, big.me.as_ref().unwrap().inv);
        }
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
                (Event::Shot { from, to, .. }, Event::Shot { from: f2, to: t2, .. }) => {
                    assert!((*from - *f2).length() < 0.05 && (*to - *t2).length() < 0.05)
                }
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
