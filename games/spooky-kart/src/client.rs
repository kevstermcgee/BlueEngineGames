//! The network client: connect, choose a driver, race. Graphics-free and generic over the engine's
//! `DatagramTransport`, so the window, the tests and the bot load generator all share it.
//!
//! Your own kart is predicted: each tick's input drives it at once and is sent (bundled with the last few, so
//! a lost datagram costs nothing). When a snapshot arrives the kart is reset to the server's version of
//! itself and the inputs the server has not yet applied are replayed on top, so a disagreement shows as a small
//! correction, never as lag. Everyone else is drawn by interpolating between the last two snapshots.
use crate::kart::{Kart, KartInput};
use crate::sim::{Event, Phase, Sim, LAPS};
use crate::track::{wrap_angle, Track};
use crate::wire::{ClientMsg, LobbyState, ServerMsg, Snapshot, Token, INPUT_BUNDLE};
use std::collections::VecDeque;
use std::net::SocketAddr;
use vesper3d::math::V;
use vesper3d::viewer::net::{random_nonce, DatagramTransport};

/// Seconds between Hellos while connecting, and before giving up.
const HELLO_EVERY: f64 = 0.5;
const CONNECT_TIMEOUT: f64 = 8.;
/// Silence from the server this long means the connection is gone.
const SILENCE_LIMIT: f64 = 5.;
const PING_EVERY: f64 = 1.;
/// The time between snapshots (the server sends 30 a second).
const SNAPSHOT_INTERVAL: f64 = 1. / 30.;
/// A prediction error larger than this snaps instead of easing.
const SNAP_DISTANCE: f32 = 6.;
/// Inputs kept for replay (two seconds).
const MAX_PENDING: usize = 120;

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub name: String,
    pub key: String,
    /// Preferred driver (0..8); the server gives the first free one if it is taken.
    pub character: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientState {
    Connecting,
    Lobby,
    Racing,
    Rejected(String),
    Disconnected(String),
}

/// What the network looked like from this side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetStats {
    pub rtt_ms: f32,
    pub packets_in: u64,
    pub bytes_in: u64,
    pub snapshots: u64,
    /// Snapshots that disagreed with the prediction enough to move the kart.
    pub corrections: u64,
    /// Corrections too big to ease (a hard snap).
    pub snaps: u64,
    pub last_error_m: f32,
    pub max_error_m: f32,
}

pub struct KartClient<T: DatagramTransport> {
    transport: T,
    server: SocketAddr,
    cfg: ClientConfig,
    state: ClientState,
    token: Option<Token>,
    id: u8,
    nonce: [u64; 2],
    lobby: Option<LobbyState>,
    /// What this player wants in the lobby; re-sent until the server's lobby agrees (datagrams get lost).
    want_character: u8,
    want_ready: bool,
    sync_attempts: u32,
    last_sync: f64,
    sim: Sim,
    track: Track,
    my_kart: Option<usize>,
    predicted: Option<Kart>,
    /// The visible gap between where prediction put the kart and where the server says it is; it eases out.
    offset: V,
    pending: VecDeque<(u32, KartInput)>,
    next_seq: u32,
    prev: Option<(Vec<Kart>, f64)>,
    cur: Option<(Vec<Kart>, f64)>,
    phase: Phase,
    race_tick: u32,
    last_server_tick: u32,
    last_event_tick: u32,
    events: Vec<Event>,
    started_at: Option<f64>,
    last_hello: f64,
    last_ping: f64,
    last_packet: f64,
    now: f64,
    stats: NetStats,
}

impl<T: DatagramTransport> KartClient<T> {
    pub fn new(transport: T, server: SocketAddr, cfg: ClientConfig) -> vesper3d::Result<Self> {
        let cfg_character = cfg.character;
        Ok(Self {
            transport,
            server,
            cfg,
            state: ClientState::Connecting,
            token: None,
            id: 0,
            nonce: random_nonce()?,
            lobby: None,
            want_character: cfg_character,
            want_ready: false,
            sync_attempts: 0,
            last_sync: f64::NEG_INFINITY,
            sim: Sim::replica(),
            track: Track::haunted_hollow(),
            my_kart: None,
            predicted: None,
            offset: V(0., 0., 0.),
            pending: VecDeque::new(),
            next_seq: 1,
            prev: None,
            cur: None,
            phase: Phase::Countdown(0),
            race_tick: 0,
            last_server_tick: 0,
            last_event_tick: 0,
            events: Vec::new(),
            started_at: None,
            last_hello: f64::NEG_INFINITY,
            last_ping: f64::NEG_INFINITY,
            last_packet: 0.,
            now: 0.,
            stats: NetStats::default(),
        })
    }

    pub fn state(&self) -> &ClientState {
        &self.state
    }
    pub fn lobby(&self) -> Option<&LobbyState> {
        self.lobby.as_ref()
    }
    /// The race as this client sees it (predicted own kart, interpolated rivals). Only meaningful while racing.
    pub fn sim(&self) -> &Sim {
        &self.sim
    }
    pub fn my_kart(&self) -> Option<usize> {
        self.my_kart
    }
    /// The lobby seat number the server gave this client.
    pub fn seat(&self) -> u8 {
        self.id
    }
    /// The session token (a secret; exposed so tests can play the attacker).
    pub fn token(&self) -> Option<Token> {
        self.token
    }
    pub fn stats(&self) -> &NetStats {
        &self.stats
    }
    /// Events for effects and sound, oldest first, each delivered once.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn send(&self, msg: &ClientMsg) {
        let _ = self.transport.try_send(self.server, &msg.encode());
    }

    fn send_hello(&mut self) {
        self.send(&ClientMsg::Hello {
            key: self.cfg.key.clone(),
            name: self.cfg.name.clone(),
            character: self.cfg.character,
            nonce: self.nonce,
            fingerprint: crate::content_fingerprint(),
        });
    }

    /// Choose a driver in the lobby. Un-readies you, as on the server.
    pub fn select(&mut self, character: u8) {
        self.want_character = character;
        self.want_ready = false;
        self.sync_attempts = 0;
        self.last_sync = f64::NEG_INFINITY;
        if let (Some(token), ClientState::Lobby) = (self.token, &self.state) {
            self.send(&ClientMsg::Select { token, character });
        }
    }

    pub fn ready(&mut self, ready: bool) {
        self.want_ready = ready;
        self.sync_attempts = 0;
        self.last_sync = f64::NEG_INFINITY;
        if let (Some(token), ClientState::Lobby) = (self.token, &self.state) {
            self.send(&ClientMsg::Ready { token, ready });
        }
    }

    /// Re-send the lobby choice the server has not yet confirmed (a few times, then accept its answer).
    fn sync_lobby(&mut self, now: f64) {
        let (Some(token), Some(lobby)) = (self.token, &self.lobby) else { return };
        if lobby.stage != 0 || now - self.last_sync < 0.25 {
            return;
        }
        let Some(me) = lobby.entries.iter().find(|e| e.slot == self.id).cloned() else { return };
        if self.sync_attempts == 0
            && self.want_character == self.cfg.character
            && me.character != self.want_character
            && !self.want_ready
        {
            // The server gave us another driver at join time (ours was taken): take its choice.
            self.want_character = me.character;
        }
        if self.sync_attempts >= 8 {
            self.want_character = me.character;
            self.want_ready = me.ready;
            return;
        }
        let msg = if me.character != self.want_character {
            Some(ClientMsg::Select { token, character: self.want_character })
        } else if me.ready != self.want_ready {
            Some(ClientMsg::Ready { token, ready: self.want_ready })
        } else {
            self.sync_attempts = 0;
            None
        };
        if let Some(msg) = msg {
            self.sync_attempts += 1;
            self.last_sync = now;
            self.send(&msg);
        }
    }

    pub fn leave(&mut self) {
        if let Some(token) = self.token {
            self.send(&ClientMsg::Leave { token });
        }
        self.state = ClientState::Disconnected("You left the game".into());
    }

    /// Read the network and keep the connection alive. Call every frame with a steady clock in seconds.
    pub fn poll(&mut self, now: f64) {
        self.now = now;
        let started = *self.started_at.get_or_insert(now);
        if let Ok(datagrams) = self.transport.receive() {
            for d in datagrams {
                if d.peer != self.server {
                    continue;
                }
                if let Ok(msg) = ServerMsg::decode(&d.data) {
                    self.stats.packets_in += 1;
                    self.stats.bytes_in += d.data.len() as u64;
                    self.last_packet = now;
                    self.on_message(msg);
                }
            }
        }
        match &self.state {
            ClientState::Connecting => {
                if now - started > CONNECT_TIMEOUT {
                    self.state = ClientState::Rejected("Could not reach the server".into());
                } else if now - self.last_hello >= HELLO_EVERY {
                    self.last_hello = now;
                    self.send_hello();
                }
            }
            ClientState::Lobby | ClientState::Racing => {
                if self.state == ClientState::Lobby {
                    self.sync_lobby(now);
                }
                if now - self.last_packet > SILENCE_LIMIT {
                    self.state = ClientState::Disconnected("Lost connection to the server".into());
                } else if now - self.last_ping >= PING_EVERY {
                    self.last_ping = now;
                    if let Some(token) = self.token {
                        self.send(&ClientMsg::Ping {
                            token,
                            stamp: (now * 1000.) as u32,
                            rtt_ms: self.stats.rtt_ms.round().clamp(0., 65535.) as u16,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn on_message(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Welcome { token, slot, .. } => {
                self.token = Some(token);
                self.id = slot;
                if self.state == ClientState::Connecting {
                    self.state = ClientState::Lobby;
                }
            }
            ServerMsg::Rejected { reason } => self.state = ClientState::Rejected(reason),
            ServerMsg::Lobby(l) => {
                let stage = l.stage;
                self.lobby = Some(l);
                match (&self.state, stage) {
                    (ClientState::Racing, 0) => self.back_to_lobby(),
                    (ClientState::Lobby, 1) => self.state = ClientState::Racing,
                    _ => {}
                }
            }
            ServerMsg::Snapshot(s) => self.on_snapshot(s),
            ServerMsg::Pong { stamp } => {
                let rtt = (self.now * 1000.) - stamp as f64;
                if (0. ..60_000.).contains(&rtt) {
                    let rtt = rtt as f32;
                    self.stats.rtt_ms = if self.stats.rtt_ms == 0. { rtt } else { self.stats.rtt_ms * 0.8 + rtt * 0.2 };
                }
            }
        }
    }

    fn back_to_lobby(&mut self) {
        self.state = ClientState::Lobby;
        self.predicted = None;
        self.my_kart = None;
        self.pending.clear();
        self.prev = None;
        self.cur = None;
        self.offset = V(0., 0., 0.);
        self.sim = Sim::replica();
    }

    fn on_snapshot(&mut self, s: Snapshot) {
        if s.server_tick <= self.last_server_tick {
            return; // late or duplicated
        }
        self.last_server_tick = s.server_tick;
        self.stats.snapshots += 1;
        if self.state == ClientState::Lobby {
            self.state = ClientState::Racing;
        }
        for (tick, event) in &s.events {
            if *tick > self.last_event_tick {
                self.events.push(event.clone());
            }
        }
        if let Some(newest) = s.events.iter().map(|e| e.0).max() {
            self.last_event_tick = self.last_event_tick.max(newest);
        }
        self.phase = s.phase;
        self.race_tick = s.race_tick;
        let karts: Vec<Kart> = s.karts.iter().map(|k| k.kart.clone()).collect();
        self.my_kart = (s.your_kart != 255 && (s.your_kart as usize) < karts.len()).then_some(s.your_kart as usize);
        self.reconcile(&karts, s.applied_seq);
        self.prev = self.cur.take().or_else(|| Some((karts.clone(), self.now)));
        self.cur = Some((karts, self.now));
        self.sim.apply_view(s.phase, s.race_tick, s.first_finish, s.finished_count, Vec::new(), s.hazards);
        self.refresh(self.now);
    }

    /// Reset the predicted kart to the server's and replay the inputs the server has not applied yet.
    fn reconcile(&mut self, karts: &[Kart], applied: u32) {
        let Some(slot) = self.my_kart else { return };
        let mut mine = karts[slot].clone();
        while self.pending.front().is_some_and(|(seq, _)| *seq <= applied) {
            self.pending.pop_front();
        }
        let before = self.predicted.as_ref().map(|p| p.pos);
        if mine.finished_tick.is_none() {
            let racing = self.phase == Phase::Racing;
            let mut tick = self.race_tick;
            for (_, input) in &self.pending {
                tick += 1;
                mine.update(&self.track, input, racing, tick, LAPS);
            }
        }
        if let Some(before) = before {
            let error = before - mine.pos;
            let distance = error.length();
            self.stats.last_error_m = distance;
            self.stats.max_error_m = self.stats.max_error_m.max(distance);
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

    /// One fixed 60 Hz tick of local input: predict it and send it. Call only while `state()` is `Racing`.
    pub fn tick(&mut self, input: KartInput) {
        if self.state != ClientState::Racing {
            return;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.pending.push_back((seq, input));
        while self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
        if let Some(kart) = self.predicted.as_mut() {
            if kart.finished_tick.is_none() {
                self.race_tick += 1;
                kart.update(&self.track, &input, self.phase == Phase::Racing, self.race_tick, LAPS);
            }
        }
        if let Some(token) = self.token {
            let frames: Vec<(u32, KartInput)> = self.pending.iter().rev().take(INPUT_BUNDLE).rev().cloned().collect();
            self.send(&ClientMsg::Input { token, frames, ack_tick: self.last_server_tick });
        }
    }

    /// Rebuild what is drawn for time `now`: interpolate rivals, ease the correction out of your own kart.
    pub fn frame(&mut self, now: f64, dt: f32) {
        self.now = now;
        self.offset = self.offset * (-8. * dt.min(0.1)).exp();
        self.refresh(now);
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
        if let (Some(slot), Some(mine)) = (self.my_kart, &self.predicted) {
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
        let (phase, race_tick, first, count) =
            (self.phase, self.race_tick, self.sim.first_finish, self.sim.finished_count);
        let hazards = std::mem::take(&mut self.sim.hazards);
        self.sim.apply_view(phase, race_tick, first, count, karts, hazards);
    }
}
