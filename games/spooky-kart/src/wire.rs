//! The network protocol: compact little-endian binary messages, checked on every read.
//!
//! A datagram is `b"SK"`, the protocol version, a message kind and the payload. Nothing here trusts the
//! sender: a truncated, oversized or nonsensical datagram is an `Err`, never a panic, and the server drops it.
//! The engine ships no binary codec, so this is also the first draft of one (see DESIGN.md).
use crate::character::{Character, Perk};
use crate::kart::{Driver, Kart, KartInput, KartStats};
use crate::sim::{Event, Hazard, HazardKind, Phase};
use vesper3d::math::V;

pub const MAGIC: [u8; 2] = *b"SK";
/// Bump when the layout of any message changes; peers with a different version are refused.
pub const PROTOCOL: u8 = 1;
/// A datagram budget safely below the engine's 1400-byte limit and a typical QUIC datagram.
pub const MAX_DATAGRAM: usize = 1200;
/// Longest text field (join key, name) a message may carry.
pub const MAX_TEXT: usize = 32;
/// Inputs bundled into one datagram, newest last. Redundancy covers lost packets without retransmission.
pub const INPUT_BUNDLE: usize = 4;
/// Events one snapshot may carry.
pub const MAX_EVENTS: usize = 24;
/// Hazards one snapshot may carry.
pub const MAX_HAZARDS: usize = 24;
/// Session token as the engine's session registry hands it out.
pub type Token = [u64; 2];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireError(pub &'static str);

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad datagram: {}", self.0)
    }
}
impl std::error::Error for WireError {}

type Res<T> = Result<T, WireError>;

#[derive(Default)]
pub struct W(pub Vec<u8>);

impl W {
    fn new(kind: u8) -> Self {
        let mut w = W(Vec::with_capacity(512));
        w.0.extend_from_slice(&MAGIC);
        w.0.push(PROTOCOL);
        w.0.push(kind);
        w
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bool(&mut self, v: bool) {
        self.u8(v as u8);
    }
    fn token(&mut self, t: &Token) {
        self.u64(t[0]);
        self.u64(t[1]);
    }
    fn text(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let n = bytes.len().min(MAX_TEXT);
        // Never split a character: back up to a boundary.
        let mut n = n;
        while n > 0 && !s.is_char_boundary(n) {
            n -= 1;
        }
        self.u8(n as u8);
        self.0.extend_from_slice(&bytes[..n]);
    }
}

pub struct R<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(WireError("length overflow"))?;
        let slice = self.buf.get(self.pos..end).ok_or(WireError("truncated"))?;
        self.pos = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Res<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Res<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Res<f32> {
        let v = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if v.is_finite() {
            Ok(v)
        } else {
            Err(WireError("non-finite number"))
        }
    }
    fn bool(&mut self) -> Res<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError("bad flag")),
        }
    }
    fn token(&mut self) -> Res<Token> {
        Ok([self.u64()?, self.u64()?])
    }
    fn text(&mut self) -> Res<String> {
        let n = self.u8()? as usize;
        if n > MAX_TEXT {
            return Err(WireError("text too long"));
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| WireError("text is not utf-8"))
    }
    fn done(&self) -> Res<()> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(WireError("trailing bytes"))
        }
    }
}

fn open(bytes: &[u8]) -> Res<(u8, R<'_>)> {
    if bytes.len() < 4 || bytes.len() > MAX_DATAGRAM {
        return Err(WireError("wrong size"));
    }
    if bytes[..2] != MAGIC {
        return Err(WireError("not a Spooky Kart datagram"));
    }
    if bytes[2] != PROTOCOL {
        return Err(WireError("protocol version mismatch"));
    }
    Ok((bytes[3], R { buf: bytes, pos: 4 }))
}

fn input_bits(i: &KartInput) -> u8 {
    (i.drift as u8) | ((i.perk as u8) << 1)
}

fn write_input(w: &mut W, seq: u32, i: &KartInput) {
    w.u32(seq);
    // Throttle and steer as signed bytes: 1/127 resolution is finer than a thumbstick.
    w.u8((i.throttle.clamp(-1., 1.) * 127.).round() as i8 as u8);
    w.u8((i.steer.clamp(-1., 1.) * 127.).round() as i8 as u8);
    w.u8(input_bits(i));
}

fn read_input(r: &mut R) -> Res<(u32, KartInput)> {
    let seq = r.u32()?;
    let throttle = r.u8()? as i8 as f32 / 127.;
    let steer = r.u8()? as i8 as f32 / 127.;
    let bits = r.u8()?;
    if bits > 3 {
        return Err(WireError("bad input flags"));
    }
    Ok((seq, KartInput { throttle, steer, drift: bits & 1 != 0, perk: bits & 2 != 0 }))
}

/// What a client says.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientMsg {
    Hello {
        key: String,
        name: String,
        character: u8,
        nonce: [u64; 2],
        fingerprint: u32,
    },
    Select {
        token: Token,
        character: u8,
    },
    Ready {
        token: Token,
        ready: bool,
    },
    /// Newest inputs (oldest first), plus the newest server tick this client has seen.
    Input {
        token: Token,
        frames: Vec<(u32, KartInput)>,
        ack_tick: u32,
    },
    /// `rtt_ms` is the client's own latest round-trip measurement, so the server can log it.
    Ping {
        token: Token,
        stamp: u32,
        rtt_ms: u16,
    },
    Leave {
        token: Token,
    },
}

impl ClientMsg {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ClientMsg::Hello { key, name, character, nonce, fingerprint } => {
                let mut w = W::new(1);
                w.text(key);
                w.text(name);
                w.u8(*character);
                w.u64(nonce[0]);
                w.u64(nonce[1]);
                w.u32(*fingerprint);
                w.0
            }
            ClientMsg::Select { token, character } => {
                let mut w = W::new(2);
                w.token(token);
                w.u8(*character);
                w.0
            }
            ClientMsg::Ready { token, ready } => {
                let mut w = W::new(3);
                w.token(token);
                w.bool(*ready);
                w.0
            }
            ClientMsg::Input { token, frames, ack_tick } => {
                let mut w = W::new(4);
                w.token(token);
                w.u32(*ack_tick);
                let n = frames.len().min(INPUT_BUNDLE);
                w.u8(n as u8);
                for (seq, input) in &frames[frames.len() - n..] {
                    write_input(&mut w, *seq, input);
                }
                w.0
            }
            ClientMsg::Ping { token, stamp, rtt_ms } => {
                let mut w = W::new(5);
                w.token(token);
                w.u32(*stamp);
                w.u16(*rtt_ms);
                w.0
            }
            ClientMsg::Leave { token } => {
                let mut w = W::new(6);
                w.token(token);
                w.0
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Res<ClientMsg> {
        let (kind, mut r) = open(bytes)?;
        let msg = match kind {
            1 => ClientMsg::Hello {
                key: r.text()?,
                name: r.text()?,
                character: r.u8()?,
                nonce: [r.u64()?, r.u64()?],
                fingerprint: r.u32()?,
            },
            2 => ClientMsg::Select { token: r.token()?, character: r.u8()? },
            3 => ClientMsg::Ready { token: r.token()?, ready: r.bool()? },
            4 => {
                let token = r.token()?;
                let ack_tick = r.u32()?;
                let n = r.u8()? as usize;
                if n > INPUT_BUNDLE {
                    return Err(WireError("too many inputs"));
                }
                let mut frames = Vec::with_capacity(n);
                for _ in 0..n {
                    frames.push(read_input(&mut r)?);
                }
                ClientMsg::Input { token, frames, ack_tick }
            }
            5 => ClientMsg::Ping { token: r.token()?, stamp: r.u32()?, rtt_ms: r.u16()? },
            6 => ClientMsg::Leave { token: r.token()? },
            _ => return Err(WireError("unknown client message")),
        };
        r.done()?;
        Ok(msg)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LobbyEntry {
    pub slot: u8,
    pub character: u8,
    pub ready: bool,
    pub name: String,
}

/// Who is in the lobby and when the race starts.
#[derive(Clone, Debug, PartialEq)]
pub struct LobbyState {
    /// 0 lobby, 1 racing, 2 results.
    pub stage: u8,
    /// Seconds until the race starts, or 0 while waiting.
    pub seconds_left: u8,
    /// Total kart slots that will race (humans plus bots).
    pub racers: u8,
    pub entries: Vec<LobbyEntry>,
}

/// One kart as it crosses the wire: everything a client needs to draw it and to predict it.
#[derive(Clone, Debug, PartialEq)]
pub struct KartWire {
    pub character: u8,
    pub human: bool,
    pub kart: Kart,
}

/// The race as of one server tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub server_tick: u32,
    /// The newest input of yours the server has applied.
    pub applied_seq: u32,
    /// Which kart is yours, or 255 if you are only watching.
    pub your_kart: u8,
    pub phase: Phase,
    pub race_tick: u32,
    pub first_finish: Option<u32>,
    pub finished_count: u32,
    pub karts: Vec<KartWire>,
    pub hazards: Vec<Hazard>,
    /// Events newer than the tick you last acknowledged, oldest first.
    pub events: Vec<(u32, Event)>,
}

/// What the server says.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    Welcome { token: Token, slot: u8, fingerprint: u32 },
    Rejected { reason: String },
    Lobby(LobbyState),
    Snapshot(Snapshot),
    Pong { stamp: u32 },
}

fn write_event(w: &mut W, tick: u32, e: &Event) {
    w.u32(tick);
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

fn read_event(r: &mut R) -> Res<(u32, Event)> {
    let tick = r.u32()?;
    let e = match r.u8()? {
        0 => Event::Count(r.u8()? as u32),
        1 => Event::Go,
        2 => Event::LapDone { kart: r.u8()? as usize, lap: r.u8()? as u32, ticks: r.u32()? },
        3 => Event::Finished { kart: r.u8()? as usize, place: r.u8()? as u32, ticks: r.u32()? },
        4 => Event::DriftBoost { kart: r.u8()? as usize, tier: r.u8()? as u32 },
        5 => Event::Bump { a: r.u8()? as usize, b: r.u8()? as usize, speed: r.f32()? },
        6 => Event::WallHit { kart: r.u8()? as usize, speed: r.f32()? },
        7 => Event::PerkUsed { kart: r.u8()? as usize, perk: Perk::from_index(r.u8()?).ok_or(WireError("bad perk"))? },
        8 => Event::HazardHit {
            kart: r.u8()? as usize,
            kind: if r.u8()? == 1 { HazardKind::Bone } else { HazardKind::Bandage },
        },
        9 => Event::RaceOver,
        _ => return Err(WireError("unknown event")),
    };
    Ok((tick, e))
}

fn write_kart(w: &mut W, k: &KartWire) {
    let kart = &k.kart;
    w.u8(k.character);
    w.u8(k.human as u8);
    for v in [kart.pos.0, kart.pos.2, kart.vel.0, kart.vel.2, kart.yaw] {
        w.f32(v);
    }
    w.u8(kart.drifting as u8 | (kart.offroad as u8) << 1);
    w.u8(kart.drift_dir as i8 as u8);
    w.f32(kart.drift_charge);
    w.u8(kart.boost_ticks.min(255) as u8);
    w.f32(kart.boost_power);
    w.u8(kart.slow_ticks.min(255) as u8);
    w.f32(kart.slow_factor);
    w.u8(kart.spin_ticks.min(255) as u8);
    w.u16(kart.cooldown.min(65535) as u16);
    w.u8(kart.phase_ticks.min(255) as u8);
    w.u16(kart.hint.min(65535) as u16);
    for v in [kart.s, kart.lateral, kart.progress, kart.best_progress] {
        w.f32(v);
    }
    w.u8(kart.lap.min(255) as u8);
    w.u32(kart.finished_tick.unwrap_or(u32::MAX));
    w.u8(kart.place.min(255) as u8);
}

fn read_kart(r: &mut R) -> Res<KartWire> {
    let character_index = r.u8()?;
    let character = Character::from_wire(character_index).ok_or(WireError("bad character"))?;
    let human = r.bool()?;
    let (px, pz, vx, vz, yaw) = (r.f32()?, r.f32()?, r.f32()?, r.f32()?, r.f32()?);
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
    if px.abs() > 5000. || pz.abs() > 5000. || vx.abs() > 500. || vz.abs() > 500. {
        return Err(WireError("kart out of range"));
    }
    let kart = Kart {
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
    };
    Ok(KartWire { character: character_index, human, kart })
}

impl ServerMsg {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ServerMsg::Welcome { token, slot, fingerprint } => {
                let mut w = W::new(101);
                w.token(token);
                w.u8(*slot);
                w.u32(*fingerprint);
                w.0
            }
            ServerMsg::Rejected { reason } => {
                let mut w = W::new(102);
                w.text(reason);
                w.0
            }
            ServerMsg::Lobby(l) => {
                let mut w = W::new(103);
                w.u8(l.stage);
                w.u8(l.seconds_left);
                w.u8(l.racers);
                w.u8(l.entries.len().min(8) as u8);
                for e in l.entries.iter().take(8) {
                    w.u8(e.slot);
                    w.u8(e.character);
                    w.bool(e.ready);
                    w.text(&e.name);
                }
                w.0
            }
            ServerMsg::Snapshot(s) => {
                let mut w = W::new(104);
                w.u32(s.server_tick);
                w.u32(s.applied_seq);
                w.u8(s.your_kart);
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
                w.u8(s.karts.len().min(8) as u8);
                for k in s.karts.iter().take(8) {
                    write_kart(&mut w, k);
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
                let events = &s.events[s.events.len().saturating_sub(MAX_EVENTS)..];
                w.u8(events.len() as u8);
                for (tick, e) in events {
                    write_event(&mut w, *tick, e);
                }
                w.0
            }
            ServerMsg::Pong { stamp } => {
                let mut w = W::new(105);
                w.u32(*stamp);
                w.0
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Res<ServerMsg> {
        let (kind, mut r) = open(bytes)?;
        let msg = match kind {
            101 => ServerMsg::Welcome { token: r.token()?, slot: r.u8()?, fingerprint: r.u32()? },
            102 => ServerMsg::Rejected { reason: r.text()? },
            103 => {
                let (stage, seconds_left, racers) = (r.u8()?, r.u8()?, r.u8()?);
                let n = r.u8()? as usize;
                if n > 8 || stage > 2 {
                    return Err(WireError("bad lobby"));
                }
                let mut entries = Vec::with_capacity(n);
                for _ in 0..n {
                    entries.push(LobbyEntry { slot: r.u8()?, character: r.u8()?, ready: r.bool()?, name: r.text()? });
                }
                ServerMsg::Lobby(LobbyState { stage, seconds_left, racers, entries })
            }
            104 => {
                let server_tick = r.u32()?;
                let applied_seq = r.u32()?;
                let your_kart = r.u8()?;
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
                if n > 8 {
                    return Err(WireError("too many karts"));
                }
                let mut karts = Vec::with_capacity(n);
                for _ in 0..n {
                    karts.push(read_kart(&mut r)?);
                }
                let nh = r.u8()? as usize;
                if nh > MAX_HAZARDS {
                    return Err(WireError("too many hazards"));
                }
                let mut hazards = Vec::with_capacity(nh);
                for _ in 0..nh {
                    let (x, z, ttl, owner, bone) = (r.f32()?, r.f32()?, r.u16()? as u32, r.u8()? as usize, r.u8()?);
                    hazards.push(Hazard {
                        pos: V(x, 0., z),
                        radius: if bone == 1 { 1.6 } else { 2.4 },
                        ttl,
                        owner,
                        kind: if bone == 1 { HazardKind::Bone } else { HazardKind::Bandage },
                    });
                }
                let ne = r.u8()? as usize;
                if ne > MAX_EVENTS {
                    return Err(WireError("too many events"));
                }
                let mut events = Vec::with_capacity(ne);
                for _ in 0..ne {
                    events.push(read_event(&mut r)?);
                }
                ServerMsg::Snapshot(Snapshot {
                    server_tick,
                    applied_seq,
                    your_kart,
                    phase,
                    race_tick,
                    first_finish: (first != u32::MAX).then_some(first),
                    finished_count,
                    karts,
                    hazards,
                    events,
                })
            }
            105 => ServerMsg::Pong { stamp: r.u32()? },
            _ => return Err(WireError("unknown server message")),
        };
        r.done()?;
        Ok(msg)
    }
}
