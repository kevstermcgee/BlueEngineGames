//! The authoritative server: a lobby, then a race, then results, forever. Graphics-free, and generic over the
//! engine's `DatagramTransport`, so the same code runs on UDP, on QUIC/TLS and in the in-memory test network.
//!
//! - The server owns the race (`Sim` at 60 Hz). Clients send inputs; it never trusts a client's state.
//! - Snapshots go out at 30 Hz with the events each client has not yet acknowledged.
//! - A player who leaves or times out mid-race is replaced by a bot at once.
//! - Every finished race appends one JSON line (race report plus network statistics) to `races.jsonl`:
//!   the evidence later sessions use to tune the karts and the netcode.
use crate::character::{Character, ALL, MAX_RACERS};
use crate::kart::{Driver, KartInput};
use crate::sim::{Event, Inputs, RaceReport, Sim};
use crate::wire::{ClientMsg, KartWire, LobbyEntry, LobbyState, ServerMsg, Snapshot, Token, MAX_DATAGRAM};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use vesper3d::viewer::devkit::Rng;
use vesper3d::viewer::net::{constant_time_eq, random_token, DatagramTransport, HandshakeLimiter, SessionRegistry};

pub const TICK_HZ: u64 = 60;
/// Snapshots per second while racing (every second tick).
pub const SNAPSHOT_EVERY: u64 = 2;
/// How many ticks of events the server remembers for clients that have not acknowledged them.
const EVENT_MEMORY: u64 = 180;
/// Inputs a client may run ahead of the server before the oldest are dropped.
const MAX_BUFFERED_INPUTS: usize = 12;
/// Buffered inputs after which a missing sequence number is declared lost and skipped.
const GAP_TOLERANCE: usize = 3;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Required in every Hello when set. Sent inside the encrypted channel on the production transport.
    pub join_key: Option<String>,
    /// Karts on the grid (humans plus bots), 1..=8.
    pub racers: usize,
    /// Start the countdown this long after the first player joins even if not everyone is ready; 0 = never.
    pub auto_start_seconds: u32,
    pub countdown_seconds: u32,
    pub results_seconds: u32,
    /// Where `races.jsonl` goes; `None` keeps the server from writing files.
    pub report_dir: Option<PathBuf>,
    pub session_timeout: Duration,
    /// Fixed seed for reproducible races (tests); `None` draws a fresh one from the OS.
    pub seed: Option<u64>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            join_key: None,
            racers: MAX_RACERS,
            auto_start_seconds: 45,
            countdown_seconds: 5,
            results_seconds: 12,
            report_dir: None,
            session_timeout: Duration::from_secs(6),
            seed: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Lobby,
    Race,
    Results,
}

/// What the network did for one player during one race.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PeerStats {
    pub inputs_received: u64,
    /// Inputs that arrived after the server had already moved past them.
    pub inputs_late: u64,
    /// Ticks the server had no fresh input and repeated the last one.
    pub ticks_repeated: u64,
    /// Sequence numbers declared lost and skipped.
    pub inputs_skipped: u64,
    pub packets_in: u64,
    pub packets_out: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub rtt_samples: u64,
    pub rtt_sum_ms: u64,
    pub rtt_max_ms: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeerReport {
    pub name: String,
    pub kart_slot: usize,
    pub character: Character,
    pub rtt_ms_mean: f32,
    pub rtt_ms_max: u16,
    pub left_early: bool,
    #[serde(flatten)]
    pub stats: PeerStats,
}

/// The server's own load during a race.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerLoad {
    pub ticks: u64,
    pub tick_us_mean: f32,
    pub tick_us_max: u32,
    pub snapshots_sent: u64,
    pub snapshot_bytes_mean: f32,
    pub snapshot_bytes_max: usize,
    pub bad_datagrams: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetReport {
    pub peers: Vec<PeerReport>,
    pub server: ServerLoad,
}

/// One line of `races.jsonl`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaceLog {
    pub unix_seconds: u64,
    pub race_index: u32,
    pub race: RaceReport,
    pub net: NetReport,
}

struct Player {
    id: u8,
    name: String,
    character: u8,
    ready: bool,
    /// Kart slot in the current race.
    kart: Option<usize>,
    buffer: BTreeMap<u32, KartInput>,
    next_seq: u32,
    started: bool,
    last_input: KartInput,
    applied_seq: u32,
    acked_tick: u32,
    left_early: bool,
    stats: PeerStats,
}

/// Counters that reset every race.
#[derive(Default)]
struct LoadCounters {
    ticks: u64,
    tick_us_sum: u64,
    tick_us_max: u32,
    snapshots: u64,
    snapshot_bytes: u64,
    snapshot_bytes_max: usize,
    bad_datagrams: u64,
}

pub struct KartServer<T: DatagramTransport> {
    transport: T,
    sessions: SessionRegistry<Player>,
    limiter: HandshakeLimiter,
    cfg: ServerConfig,
    stage: Stage,
    sim: Option<Sim>,
    tick: u64,
    stage_since: u64,
    /// Ticks left in a start countdown, when one is running.
    countdown: Option<u64>,
    first_join_tick: Option<u64>,
    lobby_dirty: bool,
    events: VecDeque<(u32, Event)>,
    race_index: u32,
    load: LoadCounters,
    departed: Vec<PeerReport>,
    rng: Rng,
    log: Vec<RaceLog>,
    fingerprint: u32,
}

impl<T: DatagramTransport> KartServer<T> {
    pub fn new(transport: T, cfg: ServerConfig) -> vesper3d::Result<Self> {
        let seed = match cfg.seed {
            Some(s) => s,
            None => random_token()?[0],
        };
        Ok(Self {
            transport,
            sessions: SessionRegistry::new(MAX_RACERS, cfg.session_timeout),
            limiter: HandshakeLimiter::new(32),
            cfg,
            stage: Stage::Lobby,
            sim: None,
            tick: 0,
            stage_since: 0,
            countdown: None,
            first_join_tick: None,
            lobby_dirty: false,
            events: VecDeque::new(),
            race_index: 0,
            load: LoadCounters::default(),
            departed: Vec::new(),
            rng: Rng::new(seed),
            log: Vec::new(),
            fingerprint: crate::content_fingerprint(),
        })
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn players(&self) -> usize {
        self.sessions.count()
    }

    pub fn sim(&self) -> Option<&Sim> {
        self.sim.as_ref()
    }

    /// Every race finished since the server started (also appended to `races.jsonl` when configured).
    pub fn race_log(&self) -> &[RaceLog] {
        &self.log
    }

    pub fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        self.transport.local_addr()
    }

    fn send(&mut self, peer: SocketAddr, msg: &ServerMsg) {
        let bytes = msg.encode();
        debug_assert!(bytes.len() <= MAX_DATAGRAM);
        if let Some(entry) = self.sessions.get_by_peer_mut(&peer) {
            entry.data.stats.packets_out += 1;
            entry.data.stats.bytes_out += bytes.len() as u64;
        }
        // Backpressure just drops this datagram: the next snapshot supersedes it.
        let _ = self.transport.try_send(peer, &bytes);
    }

    fn reject(&mut self, peer: SocketAddr, reason: &str) {
        self.send(peer, &ServerMsg::Rejected { reason: reason.into() });
    }

    fn lobby_state(&self) -> LobbyState {
        let mut entries: Vec<LobbyEntry> = self
            .sessions
            .iter()
            .map(|s| LobbyEntry {
                slot: s.data.id,
                character: s.data.character,
                ready: s.data.ready,
                name: s.data.name.clone(),
            })
            .collect();
        entries.sort_by_key(|e| e.slot);
        let stage = match self.stage {
            Stage::Lobby => 0,
            Stage::Race => 1,
            Stage::Results => 2,
        };
        LobbyState {
            stage,
            seconds_left: self.countdown.map_or(0, |t| t.div_ceil(TICK_HZ) as u8),
            racers: self.cfg.racers.clamp(1, MAX_RACERS) as u8,
            entries,
        }
    }

    fn broadcast_lobby(&mut self) {
        let msg = ServerMsg::Lobby(self.lobby_state());
        let peers: Vec<SocketAddr> = self.sessions.iter().map(|s| s.peer).collect();
        for peer in peers {
            self.send(peer, &msg);
        }
        self.lobby_dirty = false;
    }

    /// Read whatever has arrived and act on it.
    pub fn poll(&mut self, now: Instant) {
        let Ok(datagrams) = self.transport.receive() else { return };
        for d in datagrams {
            match ClientMsg::decode(&d.data) {
                Ok(msg) => {
                    if let Some(entry) = self.sessions.get_by_peer_mut(&d.peer) {
                        entry.data.stats.packets_in += 1;
                        entry.data.stats.bytes_in += d.data.len() as u64;
                    }
                    self.on_message(d.peer, msg, now);
                }
                Err(_) => self.load.bad_datagrams += 1,
            }
        }
    }

    fn free_character(&self, wanted: u8) -> Option<u8> {
        let taken: Vec<u8> = self.sessions.iter().map(|s| s.data.character).collect();
        if Character::from_wire(wanted).is_some() && !taken.contains(&wanted) {
            return Some(wanted);
        }
        (0..MAX_RACERS as u8).find(|c| !taken.contains(c))
    }

    /// The session behind `token`, only if it really belongs to `peer` (a token alone is not enough).
    fn owned(&mut self, peer: SocketAddr, token: &Token) -> Option<&mut Player> {
        let entry = self.sessions.get_mut(token)?;
        (entry.peer == peer).then_some(&mut entry.data)
    }

    fn on_message(&mut self, peer: SocketAddr, msg: ClientMsg, now: Instant) {
        match msg {
            ClientMsg::Hello { key, name, character, nonce, fingerprint } => {
                if !self.limiter.allow(now) {
                    return;
                }
                if fingerprint != self.fingerprint {
                    return self.reject(peer, "Game version differs from the server; update Spooky Kart");
                }
                if let Some(expected) = &self.cfg.join_key {
                    if !constant_time_eq(expected.as_bytes(), key.as_bytes()) {
                        return self.reject(peer, "Wrong join key");
                    }
                }
                // A repeated Hello from a connected peer (a lost Welcome) gets the same answer again.
                let existing =
                    self.sessions.get_by_peer(&peer).filter(|e| e.nonce == nonce).map(|e| (e.token, e.data.id));
                if let Some((token, id)) = existing {
                    self.send(peer, &ServerMsg::Welcome { token, slot: id, fingerprint: self.fingerprint });
                    self.lobby_dirty = true;
                    return;
                }
                if self.stage == Stage::Race {
                    return self.reject(peer, "A race is in progress; try again in a moment");
                }
                if self.sessions.get_by_peer(&peer).is_none() && self.sessions.is_full() {
                    return self.reject(peer, "Server is full (8 players)");
                }
                let Some(character) = self.free_character(character) else {
                    return self.reject(peer, "Server is full (8 players)");
                };
                let used: Vec<u8> = self.sessions.iter().filter(|s| s.peer != peer).map(|s| s.data.id).collect();
                let id = (0..MAX_RACERS as u8).find(|i| !used.contains(i)).unwrap_or(0);
                let player = Player {
                    id,
                    name: sanitize(&name, id),
                    character,
                    ready: false,
                    kart: None,
                    buffer: BTreeMap::new(),
                    next_seq: 0,
                    started: false,
                    last_input: KartInput::default(),
                    applied_seq: 0,
                    acked_tick: 0,
                    left_early: false,
                    stats: PeerStats::default(),
                };
                match self.sessions.register(peer, nonce, now, player) {
                    Ok(token) => {
                        self.first_join_tick.get_or_insert(self.tick);
                        self.send(peer, &ServerMsg::Welcome { token, slot: id, fingerprint: self.fingerprint });
                        self.broadcast_lobby();
                    }
                    Err(e) => self.reject(peer, &e.to_string()),
                }
            }
            ClientMsg::Select { token, character } => {
                if self.stage != Stage::Lobby {
                    return;
                }
                let taken = self.sessions.iter().any(|s| s.data.character == character && s.token != token);
                if Character::from_wire(character).is_none() || taken {
                    self.lobby_dirty = true; // tell the client what it really has
                    return;
                }
                let changed = self
                    .owned(peer, &token)
                    .map(|p| {
                        p.character = character;
                        p.ready = false;
                    })
                    .is_some();
                self.lobby_dirty |= changed;
            }
            ClientMsg::Ready { token, ready } => {
                if self.stage != Stage::Lobby {
                    return;
                }
                let changed = self.owned(peer, &token).map(|p| p.ready = ready).is_some();
                self.lobby_dirty |= changed;
            }
            ClientMsg::Input { token, frames, ack_tick } => {
                if self.owned(peer, &token).is_none() {
                    return;
                }
                self.sessions.touch(&token, now);
                let Some(entry) = self.sessions.get_mut(&token) else { return };
                let p = &mut entry.data;
                p.acked_tick = p.acked_tick.max(ack_tick);
                for (seq, input) in frames {
                    if !p.started {
                        p.next_seq = seq;
                        p.started = true;
                    }
                    if seq < p.next_seq || p.buffer.contains_key(&seq) {
                        if seq < p.next_seq {
                            p.stats.inputs_late += 1;
                        }
                        continue;
                    }
                    p.stats.inputs_received += 1;
                    p.buffer.insert(seq, input);
                }
                while p.buffer.len() > MAX_BUFFERED_INPUTS {
                    if let Some((&oldest, _)) = p.buffer.iter().next() {
                        p.buffer.remove(&oldest);
                        p.next_seq = p.next_seq.max(oldest + 1);
                        p.stats.inputs_skipped += 1;
                    }
                }
            }
            ClientMsg::Ping { token, stamp, rtt_ms } => {
                if self.owned(peer, &token).is_none() {
                    return;
                }
                self.sessions.touch(&token, now);
                if let Some(entry) = self.sessions.get_mut(&token) {
                    let s = &mut entry.data.stats;
                    if rtt_ms > 0 {
                        s.rtt_samples += 1;
                        s.rtt_sum_ms += rtt_ms as u64;
                        s.rtt_max_ms = s.rtt_max_ms.max(rtt_ms);
                    }
                }
                self.send(peer, &ServerMsg::Pong { stamp });
            }
            ClientMsg::Leave { token } => {
                if self.owned(peer, &token).is_some() {
                    if let Some(entry) = self.sessions.remove(&token) {
                        self.depart(entry.data, true);
                    }
                }
            }
        }
    }

    /// A player is gone. Mid-race their kart carries on under bot control.
    fn depart(&mut self, mut player: Player, _voluntary: bool) {
        if self.stage == Stage::Race {
            if let (Some(sim), Some(slot)) = (self.sim.as_mut(), player.kart) {
                if let Some(kart) = sim.karts.get_mut(slot) {
                    kart.driver = Driver::Bot;
                }
                player.left_early = true;
                let report = peer_report(&player, sim);
                self.departed.push(report);
            }
        }
        self.lobby_dirty = true;
        if self.sessions.count() == 0 {
            self.first_join_tick = None;
            self.countdown = None;
        }
    }

    /// Advance the server exactly one 60 Hz tick.
    pub fn step(&mut self, now: Instant) {
        let started = Instant::now();
        self.tick += 1;
        for entry in self.sessions.evict_timeouts(now) {
            self.depart(entry.data, false);
        }
        match self.stage {
            Stage::Lobby => self.step_lobby(),
            Stage::Race => self.step_race(),
            Stage::Results => self.step_results(),
        }
        if self.lobby_dirty || (self.stage == Stage::Lobby && self.tick % TICK_HZ == 0) {
            self.broadcast_lobby();
        }
        if self.stage == Stage::Race {
            let elapsed = started.elapsed().as_micros() as u64;
            self.load.ticks += 1;
            self.load.tick_us_sum += elapsed;
            self.load.tick_us_max = self.load.tick_us_max.max(elapsed as u32);
        }
    }

    fn step_lobby(&mut self) {
        let humans = self.sessions.count();
        let all_ready = humans > 0 && self.sessions.iter().all(|s| s.data.ready);
        let waited_too_long = self.cfg.auto_start_seconds > 0
            && humans > 0
            && self.first_join_tick.is_some_and(|t| self.tick - t >= self.cfg.auto_start_seconds as u64 * TICK_HZ);
        match (self.countdown, all_ready || waited_too_long) {
            (None, true) => {
                self.countdown = Some(self.cfg.countdown_seconds as u64 * TICK_HZ);
                self.lobby_dirty = true;
            }
            (Some(_), false) => {
                // Somebody un-readied or left: stop the clock.
                self.countdown = None;
                self.lobby_dirty = true;
            }
            (Some(left), true) => {
                let left = left.saturating_sub(1);
                if left == 0 {
                    self.countdown = None;
                    self.start_race();
                } else {
                    self.countdown = Some(left);
                    self.lobby_dirty |= left % TICK_HZ == 0;
                }
            }
            (None, false) => {}
        }
    }

    fn start_race(&mut self) {
        self.race_index += 1;
        let seed = self.rng.next_u64();
        // Humans keep the characters they picked; bots take the rest, in table order.
        let mut humans: Vec<(u8, Character)> = self
            .sessions
            .iter()
            .filter_map(|s| Character::from_wire(s.data.character).map(|c| (s.data.id, c)))
            .collect();
        humans.sort_by_key(|h| h.0);
        let taken: Vec<Character> = humans.iter().map(|h| h.1).collect();
        let total = self.cfg.racers.clamp(1, MAX_RACERS).max(humans.len());
        let mut grid: Vec<(Option<u8>, Character, Driver)> =
            humans.iter().map(|(id, c)| (Some(*id), *c, Driver::Human)).collect();
        for c in ALL.iter().filter(|c| !taken.contains(c)) {
            if grid.len() >= total {
                break;
            }
            grid.push((None, *c, Driver::Bot));
        }
        let mut order: Vec<usize> = (0..grid.len()).collect();
        Rng::new(seed).shuffle(&mut order);
        let shuffled: Vec<_> = order.iter().map(|&i| grid[i]).collect();
        let sim = Sim::with_grid(seed, &shuffled.iter().map(|g| (g.1, g.2)).collect::<Vec<_>>());
        for entry in self.sessions.iter().map(|s| s.token).collect::<Vec<_>>() {
            if let Some(e) = self.sessions.get_mut(&entry) {
                let id = e.data.id;
                e.data.kart = shuffled.iter().position(|g| g.0 == Some(id));
                e.data.buffer.clear();
                e.data.started = false;
                e.data.applied_seq = 0;
                e.data.last_input = KartInput::default();
                e.data.acked_tick = 0;
                e.data.left_early = false;
                e.data.stats = PeerStats::default();
            }
        }
        self.sim = Some(sim);
        self.events.clear();
        self.load = LoadCounters::default();
        self.departed.clear();
        self.stage = Stage::Race;
        self.stage_since = self.tick;
        self.countdown = None;
        self.lobby_dirty = true;
    }

    fn step_race(&mut self) {
        let tick32 = self.tick as u32;
        // This tick's input for each human: the next one in order, or the last one held.
        let mut inputs = Inputs::default();
        for entry in self.sessions.iter().map(|s| s.token).collect::<Vec<_>>() {
            let Some(e) = self.sessions.get_mut(&entry) else { continue };
            let p = &mut e.data;
            let Some(slot) = p.kart else { continue };
            let input = next_input(p);
            if slot < MAX_RACERS {
                inputs.0[slot] = input;
            }
        }
        let Some(sim) = self.sim.as_mut() else { return };
        sim.step(&inputs);
        for e in sim.drain_events() {
            self.events.push_back((tick32, e));
        }
        while self.events.front().is_some_and(|(t, _)| (tick32 - t) as u64 > EVENT_MEMORY) {
            self.events.pop_front();
        }
        if self.tick % SNAPSHOT_EVERY == 0 || self.sim.as_ref().is_some_and(|s| s.is_over()) {
            self.send_snapshots();
        }
        if self.sim.as_ref().is_some_and(|s| s.is_over()) {
            self.finish_race();
        }
    }

    fn snapshot_for(&self, applied_seq: u32, acked_tick: u32, your_kart: Option<usize>) -> Snapshot {
        let sim = self.sim.as_ref().expect("snapshots are only built during a race");
        Snapshot {
            server_tick: self.tick as u32,
            applied_seq,
            your_kart: your_kart.map_or(255, |k| k as u8),
            phase: sim.phase,
            race_tick: sim.race_tick,
            first_finish: sim.first_finish,
            finished_count: sim.finished_count,
            karts: sim
                .karts
                .iter()
                .map(|k| KartWire {
                    character: k.character.index() as u8,
                    human: k.driver == Driver::Human,
                    kart: k.clone(),
                })
                .collect(),
            hazards: sim.hazards.clone(),
            events: self.events.iter().filter(|(t, _)| *t > acked_tick).cloned().collect(),
        }
    }

    fn send_snapshots(&mut self) {
        let targets: Vec<(SocketAddr, u32, u32, Option<usize>)> =
            self.sessions.iter().map(|s| (s.peer, s.data.applied_seq, s.data.acked_tick, s.data.kart)).collect();
        for (peer, applied, acked, kart) in targets {
            let msg = ServerMsg::Snapshot(self.snapshot_for(applied, acked, kart));
            let len = msg.encode().len();
            self.load.snapshots += 1;
            self.load.snapshot_bytes += len as u64;
            self.load.snapshot_bytes_max = self.load.snapshot_bytes_max.max(len);
            self.send(peer, &msg);
        }
    }

    fn finish_race(&mut self) {
        let Some(sim) = self.sim.as_ref() else { return };
        let race = sim.report();
        let mut peers: Vec<PeerReport> = self.departed.clone();
        peers.extend(self.sessions.iter().filter(|s| s.data.kart.is_some()).map(|s| peer_report(&s.data, sim)));
        peers.sort_by_key(|p| p.kart_slot);
        let ticks = self.load.ticks.max(1);
        let net = NetReport {
            peers,
            server: ServerLoad {
                ticks: self.load.ticks,
                tick_us_mean: self.load.tick_us_sum as f32 / ticks as f32,
                tick_us_max: self.load.tick_us_max,
                snapshots_sent: self.load.snapshots,
                snapshot_bytes_mean: self.load.snapshot_bytes as f32 / self.load.snapshots.max(1) as f32,
                snapshot_bytes_max: self.load.snapshot_bytes_max,
                bad_datagrams: self.load.bad_datagrams,
            },
        };
        let entry = RaceLog {
            unix_seconds: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            race_index: self.race_index,
            race,
            net,
        };
        if let Some(dir) = &self.cfg.report_dir {
            if let Err(e) = append_log(dir, &entry) {
                eprintln!("[Server] Could not write the race report: {e}");
            }
        }
        println!(
            "[Server] Race {} finished: {} racers, {:.1}s, server tick mean {:.0} us max {} us",
            entry.race_index,
            entry.race.racers.len(),
            entry.race.race_seconds,
            entry.net.server.tick_us_mean,
            entry.net.server.tick_us_max
        );
        self.log.push(entry);
        self.stage = Stage::Results;
        self.stage_since = self.tick;
        self.lobby_dirty = true;
    }

    fn step_results(&mut self) {
        if self.tick % (TICK_HZ / 10) == 0 {
            let targets: Vec<(SocketAddr, u32, u32, Option<usize>)> =
                self.sessions.iter().map(|s| (s.peer, s.data.applied_seq, s.data.acked_tick, s.data.kart)).collect();
            for (peer, applied, acked, kart) in targets {
                let msg = ServerMsg::Snapshot(self.snapshot_for(applied, acked, kart));
                self.send(peer, &msg);
            }
        }
        if self.tick - self.stage_since >= self.cfg.results_seconds as u64 * TICK_HZ {
            self.sim = None;
            self.stage = Stage::Lobby;
            self.first_join_tick = (self.sessions.count() > 0).then_some(self.tick);
            for entry in self.sessions.iter().map(|s| s.token).collect::<Vec<_>>() {
                if let Some(e) = self.sessions.get_mut(&entry) {
                    e.data.ready = false;
                    e.data.kart = None;
                }
            }
            self.lobby_dirty = true;
        }
    }

    /// Run in real time until `stop` is set or `max_ticks` have passed. Prints a status line every 5 s.
    pub fn run_realtime(&mut self, stop: Arc<AtomicBool>, max_ticks: Option<u64>) -> vesper3d::Result<()> {
        let frame = Duration::from_micros(1_000_000 / TICK_HZ);
        let mut next = Instant::now();
        let mut last_status = Instant::now();
        println!("[Server] Spooky Kart listening on {} (fingerprint {:08x})", self.local_addr()?, self.fingerprint);
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            self.poll(now);
            self.step(now);
            if max_ticks.is_some_and(|m| self.tick >= m) {
                break;
            }
            if last_status.elapsed() >= Duration::from_secs(5) {
                last_status = Instant::now();
                println!(
                    "[Server] tick {} | stage {:?} | players {} | races {}",
                    self.tick,
                    self.stage,
                    self.sessions.count(),
                    self.race_index
                );
            }
            next += frame;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else if now - next > frame * 30 {
                next = now; // fell far behind (a suspended machine): do not try to catch up
            }
        }
        Ok(())
    }
}

/// The input for this tick: the next in sequence, the next after a lost gap, or the last held.
fn next_input(p: &mut Player) -> KartInput {
    if let Some(input) = p.buffer.remove(&p.next_seq) {
        p.applied_seq = p.next_seq;
        p.next_seq += 1;
        p.last_input = input;
        return input;
    }
    if let Some((&first, _)) = p.buffer.iter().next() {
        if p.buffer.len() >= GAP_TOLERANCE {
            // The missing input is not coming: skip to what we have.
            p.stats.inputs_skipped += (first - p.next_seq) as u64;
            let input = p.buffer.remove(&first).unwrap_or_default();
            p.applied_seq = first;
            p.next_seq = first + 1;
            p.last_input = input;
            return input;
        }
    }
    // Nothing fresh: keep doing what they were doing, but a held perk press must not repeat.
    if p.started {
        p.stats.ticks_repeated += 1;
    }
    KartInput { perk: false, ..p.last_input }
}

fn peer_report(p: &Player, sim: &Sim) -> PeerReport {
    let slot = p.kart.unwrap_or(0);
    let character = sim.karts.get(slot).map_or(Character::Vampire, |k| k.character);
    PeerReport {
        name: p.name.clone(),
        kart_slot: slot,
        character,
        rtt_ms_mean: if p.stats.rtt_samples > 0 { p.stats.rtt_sum_ms as f32 / p.stats.rtt_samples as f32 } else { 0. },
        rtt_ms_max: p.stats.rtt_max_ms,
        left_early: p.left_early,
        stats: p.stats.clone(),
    }
}

fn sanitize(name: &str, id: u8) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).take(16).collect();
    let clean = clean.trim().to_string();
    if clean.is_empty() {
        format!("Racer {}", id + 1)
    } else {
        clean
    }
}

fn append_log(dir: &std::path::Path, entry: &RaceLog) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("races.jsonl"))?;
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    writeln!(file, "{line}")
}
