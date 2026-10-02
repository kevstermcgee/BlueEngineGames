//! The Deadfall hub: the always-on front door that lists and creates rooms.
//!
//! The engine's netplay kit runs one match per server process and Deadfall's match settings are process-global, so
//! every room is its own `deadfall-server` child process on its own UDP port. The hub is a small supervisor: it
//! answers a tiny datagram protocol on one well-known port ("which rooms exist?", "make me one"), spawns and reaps the
//! children, and restarts the permanent Public room if it dies. Players click Play Online, pick a room by name, and the
//! client joins that room's port; nobody types a code or an ip:port.
//!
//! This module is pure std and has no window: the wire format, the abuse limits ([`RateLimiter`]), the room table
//! ([`RoomManager`], with process spawning behind the [`Spawner`] trait so tests use a fake) and the datagram handler
//! ([`Hub`]). `bin/hub.rs` is the thin executable around it and `hub_client.rs` is the matching client.
//!
//! # Wire format (UDP, little endian, one datagram per message)
//! ```text
//! request: "DFHB" version(1) kind(1) nonce(u32) payload...   zero-padded up to the kind's minimum length
//! reply:   "DFHB" version(1) kind(1) nonce(u32) build(u32) payload...
//! ```
//! `build` is [`crate::netgame::fingerprint`]: a client whose build differs from the hub's would be refused by the
//! rooms anyway, so it can say "update the game" before trying to join. The nonce is echoed so a client can match a
//! reply to its request. Requests are padded to a minimum length and replies are capped per request kind, so the hub
//! can never be used to amplify traffic by more than a small factor (see [`min_request_len`], [`max_reply_len`]).
use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---- wire format ----------------------------------------------------------------------------------------------------

pub const MAGIC: [u8; 4] = *b"DFHB";
pub const VERSION: u8 = 1;
/// The default hub port; the Public room is the next port up.
pub const DEFAULT_BASE_PORT: u16 = 4100;
/// Every reply fits one safe datagram.
pub const MAX_REPLY: usize = 1000;
/// Requests longer than this are ignored.
pub const MAX_REQUEST: usize = 512;
/// A room name is at most this many characters.
pub const MAX_NAME_CHARS: usize = 24;
/// More rooms than this are never configured (one reply page would not hold them).
pub const MAX_ROOMS_HARD: usize = 24;
/// Players per room (the server's participant count).
pub const ROOM_CAPACITY: u8 = 12;

const REQ_LIST: u8 = 1;
const REQ_CREATE: u8 = 2;
const REQ_PING: u8 = 3;
const REP_ROOMS: u8 = 0x81;
const REP_CREATED: u8 = 0x82;
const REP_ERROR: u8 = 0x83;
const REP_PONG: u8 = 0x84;
const REQUEST_HEADER: usize = 10;
const REPLY_HEADER: usize = 14;
const MAX_ERROR_TEXT: usize = 80;

/// A request, as a client sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Ask for the room list, starting at room number `skip` (a reply holds as many rooms as fit one datagram).
    List { skip: u8 },
    /// Make a room. `kills` is the kill target (0 = the hub's default); `bots` fills empty slots with computer players.
    Create { name: String, bots: bool, kills: u16 },
    /// Is the hub there, and what build is it?
    Ping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomState {
    Lobby,
    Playing,
}

/// One room as listed to clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomInfo {
    pub name: String,
    pub players: u8,
    pub capacity: u8,
    pub state: RoomState,
    /// The UDP port of the room's game server, on the hub's host.
    pub port: u16,
    /// The permanent Public room.
    pub public: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    BadName = 1,
    NameTaken = 2,
    /// The room cap is reached.
    Full = 3,
    RateLimited = 4,
    /// The hub could not start a server process.
    Unavailable = 5,
    BadRequest = 6,
}

impl ErrorCode {
    fn from_u8(v: u8) -> ErrorCode {
        match v {
            1 => ErrorCode::BadName,
            2 => ErrorCode::NameTaken,
            3 => ErrorCode::Full,
            4 => ErrorCode::RateLimited,
            5 => ErrorCode::Unavailable,
            _ => ErrorCode::BadRequest,
        }
    }
}

/// A reply, as the hub sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// `rooms` are numbers `skip..skip + rooms.len()` of `total`.
    Rooms {
        skip: u8,
        total: u8,
        rooms: Vec<RoomInfo>,
    },
    Created {
        room: RoomInfo,
    },
    Error {
        code: ErrorCode,
        text: String,
    },
    Pong,
}

/// A decoded reply with its envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyPacket {
    pub nonce: u32,
    /// The hub's `netgame::fingerprint()`.
    pub build: u32,
    pub reply: Reply,
}

/// The shortest datagram the hub answers for a request kind (clients pad up to it).
pub fn min_request_len(request: &Request) -> usize {
    match request {
        Request::Ping => 32,
        Request::Create { .. } => 96,
        Request::List { .. } => 200,
    }
}

/// The longest reply the hub sends to a request kind: at most about five times the (padded) request.
pub fn max_reply_len(request: &Request) -> usize {
    match request {
        Request::Ping => 32,
        Request::Create { .. } => 128,
        Request::List { .. } => MAX_REPLY,
    }
}

struct Put(Vec<u8>);
impl Put {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str, max: usize) {
        let mut end = s.len().min(max).min(255);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u8(end as u8);
        self.0.extend_from_slice(&s.as_bytes()[..end]);
    }
    fn room(&mut self, r: &RoomInfo) {
        self.str(&r.name, 96);
        self.u8(r.players);
        self.u8(r.capacity);
        self.u8(matches!(r.state, RoomState::Playing) as u8);
        self.u16(r.port);
        self.u8(r.public as u8);
    }
}

struct Cur<'a>(&'a [u8]);
impl Cur<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    fn room(&mut self) -> Option<RoomInfo> {
        Some(RoomInfo {
            name: self.str()?,
            players: self.u8()?,
            capacity: self.u8()?,
            state: if self.u8()? == 0 { RoomState::Lobby } else { RoomState::Playing },
            port: self.u16()?,
            public: self.u8()? != 0,
        })
    }
}

fn header(kind: u8, nonce: u32) -> Put {
    let mut p = Put(Vec::with_capacity(256));
    p.0.extend_from_slice(&MAGIC);
    p.u8(VERSION);
    p.u8(kind);
    p.u32(nonce);
    p
}

fn check_header(data: &[u8]) -> Option<(u8, u32, Cur<'_>)> {
    if data.len() < REQUEST_HEADER || data[..4] != MAGIC || data[4] != VERSION {
        return None;
    }
    let nonce = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    Some((data[5], nonce, Cur(&data[REQUEST_HEADER..])))
}

impl Request {
    /// The datagram for this request, padded to its minimum length.
    pub fn encode(&self, nonce: u32) -> Vec<u8> {
        let mut p = match self {
            Request::List { .. } => header(REQ_LIST, nonce),
            Request::Create { .. } => header(REQ_CREATE, nonce),
            Request::Ping => header(REQ_PING, nonce),
        };
        match self {
            Request::List { skip } => p.u8(*skip),
            Request::Create { name, bots, kills } => {
                p.u8(*bots as u8);
                p.u16(*kills);
                p.str(name, 96);
            }
            Request::Ping => {}
        }
        let min = min_request_len(self);
        if p.0.len() < min {
            p.0.resize(min, 0);
        }
        p.0
    }

    /// `None` for anything that is not a well-formed request of this version (trailing padding is ignored).
    pub fn decode(data: &[u8]) -> Option<(u32, Request)> {
        let (kind, nonce, mut c) = check_header(data)?;
        let request = match kind {
            REQ_LIST => Request::List { skip: c.u8()? },
            REQ_CREATE => Request::Create { bots: c.u8()? != 0, kills: c.u16()?, name: c.str()? },
            REQ_PING => Request::Ping,
            _ => return None,
        };
        Some((nonce, request))
    }
}

impl Reply {
    /// Encoded size of one listed room.
    pub fn room_len(r: &RoomInfo) -> usize {
        1 + r.name.len().min(96) + 6
    }

    pub fn encode(&self, nonce: u32, build: u32) -> Vec<u8> {
        let mut p = match self {
            Reply::Rooms { .. } => header(REP_ROOMS, nonce),
            Reply::Created { .. } => header(REP_CREATED, nonce),
            Reply::Error { .. } => header(REP_ERROR, nonce),
            Reply::Pong => header(REP_PONG, nonce),
        };
        p.u32(build);
        match self {
            Reply::Rooms { skip, total, rooms } => {
                p.u8(*skip);
                p.u8(*total);
                p.u8(rooms.len().min(255) as u8);
                for r in rooms {
                    p.room(r);
                }
            }
            Reply::Created { room } => p.room(room),
            Reply::Error { code, text } => {
                p.u8(*code as u8);
                p.str(text, MAX_ERROR_TEXT);
            }
            Reply::Pong => {}
        }
        p.0
    }

    pub fn decode(data: &[u8]) -> Option<ReplyPacket> {
        if data.len() < REPLY_HEADER {
            return None;
        }
        let (kind, nonce, mut c) = check_header(data)?;
        let build = c.u32()?;
        let reply = match kind {
            REP_ROOMS => {
                let (skip, total, n) = (c.u8()?, c.u8()?, c.u8()? as usize);
                let mut rooms = Vec::with_capacity(n.min(32));
                for _ in 0..n {
                    rooms.push(c.room()?);
                }
                Reply::Rooms { skip, total, rooms }
            }
            REP_CREATED => Reply::Created { room: c.room()? },
            REP_ERROR => Reply::Error { code: ErrorCode::from_u8(c.u8()?), text: c.str()? },
            REP_PONG => Reply::Pong,
            _ => return None,
        };
        Some(ReplyPacket { nonce, build, reply })
    }
}

// ---- room names -------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    /// Only letters, digits, spaces, apostrophes and hyphens are allowed.
    BadCharacter,
}

impl NameError {
    pub fn text(self) -> &'static str {
        match self {
            NameError::Empty => "Give the room a name.",
            NameError::TooLong => "That name is too long (24 characters at most).",
            NameError::BadCharacter => "Use letters, numbers, spaces, apostrophes and hyphens only.",
        }
    }
}

/// Clean a room name: trim, collapse runs of spaces, and accept only Unicode letters, digits, spaces, apostrophes and
/// hyphens (so no control characters, no look-alike tricks, nothing that needs escaping in a log). At most
/// [`MAX_NAME_CHARS`] characters and at least one letter or digit.
pub fn sanitize_name(raw: &str) -> Result<String, NameError> {
    let mut out = String::new();
    let mut last_space = false;
    for ch in raw.trim_matches(' ').chars() {
        if ch == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
            continue;
        }
        last_space = false;
        if !(ch.is_alphanumeric() || ch == '\'' || ch == '-') {
            return Err(NameError::BadCharacter);
        }
        out.push(ch);
    }
    if !out.chars().any(char::is_alphanumeric) {
        return Err(if out.is_empty() && raw.chars().all(char::is_whitespace) {
            NameError::Empty
        } else {
            NameError::BadCharacter
        });
    }
    if out.chars().count() > MAX_NAME_CHARS {
        return Err(NameError::TooLong);
    }
    Ok(out)
}

// ---- abuse limits -----------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: f64,
    last_ms: u64,
}

impl Bucket {
    fn refill(&mut self, now_ms: u64, capacity: f64, per_sec: f64) {
        let dt = now_ms.saturating_sub(self.last_ms) as f64 / 1000.;
        self.tokens = (self.tokens + dt * per_sec).min(capacity);
        self.last_ms = self.last_ms.max(now_ms);
    }
    fn take(&mut self, now_ms: u64, capacity: f64, per_sec: f64, cost: f64) -> bool {
        self.refill(now_ms, capacity, per_sec);
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}

/// A token bucket per source IP address (a source that hops ports still shares one bucket), with a hard cap on how many
/// sources are remembered. Time is passed in, so tests are deterministic.
#[derive(Clone, Debug)]
pub struct RateLimiter {
    /// Burst size, in requests.
    pub capacity: f64,
    /// Sustained rate, requests per second.
    pub per_sec: f64,
    /// At most this many sources are tracked; a new source beyond it is refused until idle ones age out.
    pub max_sources: usize,
    buckets: HashMap<IpAddr, Bucket>,
    last_sweep_ms: u64,
}

impl RateLimiter {
    pub fn new(capacity: f64, per_sec: f64, max_sources: usize) -> Self {
        Self { capacity, per_sec, max_sources, buckets: HashMap::new(), last_sweep_ms: 0 }
    }

    /// Spend one token for `ip` at `now_ms`; false if the source is over its rate (or the table is full of busy sources).
    pub fn allow(&mut self, ip: IpAddr, now_ms: u64) -> bool {
        let (capacity, per_sec) = (self.capacity, self.per_sec);
        if let Some(b) = self.buckets.get_mut(&ip) {
            return b.take(now_ms, capacity, per_sec, 1.);
        }
        if self.buckets.len() >= self.max_sources {
            self.sweep(now_ms);
            if self.buckets.len() >= self.max_sources {
                return false;
            }
        }
        let mut b = Bucket { tokens: capacity, last_ms: now_ms };
        let ok = b.take(now_ms, capacity, per_sec, 1.);
        self.buckets.insert(ip, b);
        ok
    }

    /// Forget sources whose bucket has refilled completely (they look exactly like a source never seen). At most once a second.
    fn sweep(&mut self, now_ms: u64) {
        if now_ms.saturating_sub(self.last_sweep_ms) < 1000 && self.last_sweep_ms != 0 {
            return;
        }
        self.last_sweep_ms = now_ms.max(1);
        let (capacity, per_sec) = (self.capacity, self.per_sec);
        self.buckets.retain(|_, b| {
            b.refill(now_ms, capacity, per_sec);
            b.tokens < capacity
        });
    }

    pub fn tracked(&self) -> usize {
        self.buckets.len()
    }
}

/// The hub's abuse limits.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Per source address: burst and sustained requests per second.
    pub burst: f64,
    pub per_sec: f64,
    /// Creating a room is much costlier than listing: its own, slower bucket per source.
    pub create_burst: f64,
    pub create_per_sec: f64,
    /// Over all sources together (spoofed sources defeat per-source limits, this bounds the hub's total output).
    pub global_burst: f64,
    pub global_per_sec: f64,
    pub max_sources: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            burst: 10.,
            per_sec: 2.,
            create_burst: 3.,
            create_per_sec: 1. / 30.,
            global_burst: 300.,
            global_per_sec: 150.,
            max_sources: 4096,
        }
    }
}

// ---- rooms --------------------------------------------------------------------------------------------------------------

/// What a room's server reports about itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomStatus {
    pub players: u8,
    pub state: RoomState,
}

/// Parse `STATUS players=N stage=lobby|playing|results` (the line `deadfall-server --status-lines` prints each second).
/// The results screen counts as the lobby: the next match is already forming.
pub fn parse_status_line(line: &str) -> Option<RoomStatus> {
    let mut words = line.split_whitespace();
    if words.next()? != "STATUS" {
        return None;
    }
    let (mut players, mut state) = (None, None);
    for w in words {
        match w.split_once('=')? {
            ("players", v) => players = Some(v.parse::<u8>().ok()?),
            ("stage", "lobby") | ("stage", "results") => state = Some(RoomState::Lobby),
            ("stage", "playing") => state = Some(RoomState::Playing),
            _ => {}
        }
    }
    Some(RoomStatus { players: players?, state: state? })
}

/// The settings a room's server is started with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomSettings {
    pub bots: bool,
    /// Kill target; 0 leaves the server's own default (40).
    pub kills: u16,
    pub skill: u8,
    /// Start the countdown this many seconds after the first player joins; 0 = only when everyone is ready.
    pub auto_start: u32,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self { bots: false, kills: 0, skill: 1, auto_start: 30 }
    }
}

/// Everything a [`Spawner`] needs to start one room's server.
#[derive(Clone, Debug)]
pub struct RoomSpec {
    pub name: String,
    pub port: u16,
    /// The address the server binds, e.g. `0.0.0.0:4102`.
    pub listen: String,
    pub settings: RoomSettings,
    pub report_dir: Option<PathBuf>,
    pub public: bool,
}

/// A running room server, as the manager sees it.
pub trait RoomProcess {
    /// The latest status the server reported, if it is recent.
    fn status(&mut self) -> Option<RoomStatus>;
    /// The process has ended.
    fn exited(&mut self) -> bool;
    /// End the process and wait for it.
    fn kill(&mut self);
}

/// Starts room servers. The real one runs `deadfall-server`; tests use a fake.
pub trait Spawner {
    fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>>;
}

#[derive(Clone, Debug)]
pub struct ManagerConfig {
    /// The hub listens here; the Public room is `base_port + 1`, user rooms follow.
    pub base_port: u16,
    /// The IP the room servers bind (the port is added).
    pub listen_ip: String,
    pub public_name: String,
    /// User rooms at most (the permanent Public room is extra).
    pub max_user_rooms: usize,
    /// A user room that has had nobody in it this long is shut down.
    pub empty_timeout_ms: u64,
    /// Wait this long before restarting a dead Public room.
    pub public_restart_ms: u64,
    pub report_dir: Option<PathBuf>,
    pub public_settings: RoomSettings,
    pub user_settings: RoomSettings,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            base_port: DEFAULT_BASE_PORT,
            listen_ip: "0.0.0.0".into(),
            public_name: "Public".into(),
            max_user_rooms: 6,
            empty_timeout_ms: 120_000,
            public_restart_ms: 2_000,
            report_dir: None,
            public_settings: RoomSettings { bots: true, ..RoomSettings::default() },
            user_settings: RoomSettings::default(),
        }
    }
}

impl ManagerConfig {
    pub fn public_port(&self) -> u16 {
        self.base_port + 1
    }
    /// Check the ports fit and the room count is sane.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_user_rooms > MAX_ROOMS_HARD {
            return Err(format!("at most {MAX_ROOMS_HARD} rooms"));
        }
        if self.base_port as usize + 1 + self.max_user_rooms > 65535 {
            return Err("the room ports would run past 65535; use a lower base port".into());
        }
        if sanitize_name(&self.public_name).is_err() {
            return Err("the public room name has characters not allowed in a room name".into());
        }
        Ok(())
    }
}

/// Why a room could not be created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HubError {
    pub code: ErrorCode,
    pub text: String,
}

impl HubError {
    fn new(code: ErrorCode, text: impl Into<String>) -> Self {
        Self { code, text: text.into() }
    }
}

struct Room {
    name: String,
    port: u16,
    public: bool,
    process: Box<dyn RoomProcess>,
    status: Option<RoomStatus>,
    /// Since when the room has been seen empty (a new room counts as empty from its creation).
    empty_since_ms: Option<u64>,
}

impl Room {
    fn info(&self) -> RoomInfo {
        let s = self.status.unwrap_or(RoomStatus { players: 0, state: RoomState::Lobby });
        RoomInfo {
            name: self.name.clone(),
            players: s.players,
            capacity: ROOM_CAPACITY,
            state: s.state,
            port: self.port,
            public: self.public,
        }
    }
}

/// Owns the room table, the port pool and the child servers.
pub struct RoomManager {
    cfg: ManagerConfig,
    spawner: Box<dyn Spawner>,
    public: Option<Room>,
    next_public_try_ms: u64,
    rooms: Vec<Room>,
}

impl RoomManager {
    pub fn new(cfg: ManagerConfig, spawner: Box<dyn Spawner>) -> Self {
        Self { cfg, spawner, public: None, next_public_try_ms: 0, rooms: Vec::new() }
    }

    pub fn config(&self) -> &ManagerConfig {
        &self.cfg
    }

    fn spec(&self, name: &str, port: u16, public: bool, settings: RoomSettings) -> RoomSpec {
        RoomSpec {
            name: name.to_string(),
            port,
            listen: format!("{}:{port}", self.cfg.listen_ip),
            settings,
            // One directory per port so two servers never append to the same matches.jsonl.
            report_dir: self.cfg.report_dir.as_ref().map(|d| d.join(format!("port-{port}"))),
            public,
        }
    }

    /// Housekeeping: read statuses, drop dead and long-empty user rooms, (re)start the Public room. Call a few times a second.
    pub fn tick(&mut self, now_ms: u64) {
        if self.public.as_mut().is_some_and(|r| r.process.exited()) {
            log("The Public room's server ended unexpectedly; restarting it");
            self.public = None;
            self.next_public_try_ms = now_ms + self.cfg.public_restart_ms;
        }
        if self.public.is_none() && now_ms >= self.next_public_try_ms {
            let spec = self.spec(&self.cfg.public_name.clone(), self.cfg.public_port(), true, self.cfg.public_settings);
            match self.spawner.spawn(&spec) {
                Ok(process) => {
                    log(&format!("Public room \"{}\" is up on port {}", spec.name, spec.port));
                    self.public = Some(Room {
                        name: spec.name,
                        port: spec.port,
                        public: true,
                        process,
                        status: None,
                        empty_since_ms: Some(now_ms),
                    });
                }
                Err(e) => {
                    log(&format!("Could not start the Public room: {e}; will retry"));
                    self.next_public_try_ms = now_ms + self.cfg.public_restart_ms.max(5_000);
                }
            }
        }
        let timeout = self.cfg.empty_timeout_ms;
        let refresh = |r: &mut Room| {
            r.status = r.process.status();
            if r.status.is_some_and(|s| s.players > 0) {
                r.empty_since_ms = None;
            } else {
                r.empty_since_ms.get_or_insert(now_ms);
            }
        };
        if let Some(r) = self.public.as_mut() {
            refresh(r);
        }
        let mut i = 0;
        while i < self.rooms.len() {
            let room = &mut self.rooms[i];
            if room.process.exited() {
                log(&format!("Room \"{}\" (port {}) ended unexpectedly; removing it", room.name, room.port));
                self.rooms.remove(i);
                continue;
            }
            refresh(room);
            if room.empty_since_ms.is_some_and(|t| now_ms.saturating_sub(t) >= timeout) {
                log(&format!(
                    "Room \"{}\" (port {}) has been empty for {} s; closing it",
                    room.name,
                    room.port,
                    timeout / 1000
                ));
                room.process.kill();
                self.rooms.remove(i);
                continue;
            }
            i += 1;
        }
    }

    /// Every live room, the Public room first.
    pub fn rooms(&self) -> Vec<RoomInfo> {
        self.public.iter().chain(self.rooms.iter()).map(Room::info).collect()
    }

    fn free_port(&self) -> Option<u16> {
        (0..self.cfg.max_user_rooms as u16)
            .map(|i| self.cfg.base_port + 2 + i)
            .find(|p| !self.rooms.iter().any(|r| r.port == *p))
    }

    /// Make a room (`kills` 0 = default). Fails on a bad or taken name, when the room cap is reached, or if the server will not start.
    pub fn create(&mut self, raw_name: &str, bots: bool, kills: u16, now_ms: u64) -> Result<RoomInfo, HubError> {
        let name = sanitize_name(raw_name).map_err(|e| HubError::new(ErrorCode::BadName, e.text()))?;
        let lower = name.to_lowercase();
        if lower == self.cfg.public_name.to_lowercase() || self.rooms.iter().any(|r| r.name.to_lowercase() == lower) {
            return Err(HubError::new(ErrorCode::NameTaken, "A room with that name already exists."));
        }
        let Some(port) = self.free_port() else {
            return Err(HubError::new(
                ErrorCode::Full,
                "All rooms are in use right now. Join one, or try again in a few minutes.",
            ));
        };
        let mut settings = self.cfg.user_settings;
        settings.bots = bots || settings.bots;
        if kills > 0 {
            settings.kills = kills.clamp(1, 500);
        }
        let spec = self.spec(&name, port, false, settings);
        let process = self.spawner.spawn(&spec).map_err(|e| {
            log(&format!("Could not start a server for room \"{name}\": {e}"));
            HubError::new(ErrorCode::Unavailable, "The server could not start the room. Try again shortly.")
        })?;
        let room = Room { name, port, public: false, process, status: None, empty_since_ms: Some(now_ms) };
        let info = room.info();
        self.rooms.push(room);
        Ok(info)
    }

    /// Kill every child server.
    pub fn shutdown(&mut self) {
        for r in self.public.iter_mut().chain(self.rooms.iter_mut()) {
            r.process.kill();
        }
        self.public = None;
        self.rooms.clear();
    }
}

impl Drop for RoomManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---- real child processes -------------------------------------------------------------------------------------------

/// Runs `deadfall-server` for each room.
pub struct ProcessSpawner {
    pub server_bin: PathBuf,
}

impl ProcessSpawner {
    /// `deadfall-server` next to the running executable (with `.exe` on Windows).
    pub fn default_server_bin() -> PathBuf {
        let name = format!("deadfall-server{}", std::env::consts::EXE_SUFFIX);
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(&name))).unwrap_or_else(|| name.into())
    }

    /// The command-line arguments for a room.
    pub fn args(spec: &RoomSpec) -> Vec<String> {
        let mut a = vec![
            "--listen".to_string(),
            spec.listen.clone(),
            "--status-lines".into(),
            "--exit-on-stdin-eof".into(),
            "--auto-start".into(),
            spec.settings.auto_start.to_string(),
        ];
        if spec.settings.kills > 0 {
            a.extend(["--kills".to_string(), spec.settings.kills.to_string()]);
        }
        if spec.settings.bots {
            a.extend(["--bots".to_string(), "--skill".into(), spec.settings.skill.min(2).to_string()]);
        }
        if let Some(dir) = &spec.report_dir {
            a.extend(["--report-dir".to_string(), dir.display().to_string()]);
        }
        a
    }
}

struct ChildRoom {
    child: Child,
    /// The newest status line and when it arrived.
    status: Arc<Mutex<Option<(RoomStatus, Instant)>>>,
}

/// A status older than this is not trusted (the server prints one a second).
const STATUS_FRESH: Duration = Duration::from_secs(5);

impl RoomProcess for ChildRoom {
    fn status(&mut self) -> Option<RoomStatus> {
        let s = self.status.lock().ok()?;
        s.filter(|(_, at)| at.elapsed() < STATUS_FRESH).map(|(s, _)| s)
    }
    fn exited(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChildRoom {
    fn drop(&mut self) {
        self.kill();
    }
}

impl Spawner for ProcessSpawner {
    fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>> {
        let mut child = Command::new(&self.server_bin)
            .args(Self::args(spec))
            // The hub holds stdin open; the server exits when it closes, so a hub that is killed leaves no orphans.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", self.server_bin.display())))?;
        let status = Arc::new(Mutex::new(None));
        if let Some(out) = child.stdout.take() {
            let shared = status.clone();
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                // Always drain the pipe (a full pipe would stall the server); keep only the status lines.
                for line in BufReader::new(out).lines() {
                    let Ok(line) = line else { break };
                    if let Some(s) = parse_status_line(&line) {
                        if let Ok(mut g) = shared.lock() {
                            *g = Some((s, Instant::now()));
                        }
                    }
                }
            });
        }
        Ok(Box::new(ChildRoom { child, status }))
    }
}

// ---- the front door ---------------------------------------------------------------------------------------------------

/// The datagram handler: abuse limits in front of a [`RoomManager`]. No sockets and no clock of its own.
pub struct Hub {
    manager: RoomManager,
    per_source: RateLimiter,
    create_limit: RateLimiter,
    global: Bucket,
    limits: Limits,
    build: u32,
    /// Recent creates by (source, nonce): a client that repeats a create because the reply was lost gets the same room back.
    recent_creates: std::collections::VecDeque<(IpAddr, u32, String)>,
}

impl Hub {
    pub fn new(cfg: ManagerConfig, limits: Limits, spawner: Box<dyn Spawner>) -> Self {
        let mut hub = Self {
            manager: RoomManager::new(cfg, spawner),
            per_source: RateLimiter::new(limits.burst, limits.per_sec, limits.max_sources),
            create_limit: RateLimiter::new(limits.create_burst, limits.create_per_sec, limits.max_sources),
            global: Bucket { tokens: limits.global_burst, last_ms: 0 },
            limits,
            build: crate::netgame::fingerprint(),
            recent_creates: Default::default(),
        };
        hub.manager.tick(0);
        hub
    }

    pub fn manager(&self) -> &RoomManager {
        &self.manager
    }

    pub fn tick(&mut self, now_ms: u64) {
        self.manager.tick(now_ms);
    }

    pub fn shutdown(&mut self) {
        self.manager.shutdown();
    }

    /// Answer one datagram, or `None` to stay silent (garbage, unpadded requests, and anyone over their rate: silence costs
    /// the hub nothing and gives a flooder nothing).
    pub fn handle(&mut self, src: SocketAddr, data: &[u8], now_ms: u64) -> Option<Vec<u8>> {
        if data.len() > MAX_REQUEST {
            return None;
        }
        let (nonce, request) = Request::decode(data)?;
        if data.len() < min_request_len(&request) {
            return None;
        }
        let g = &self.limits;
        if !self.global.take(now_ms, g.global_burst, g.global_per_sec, 1.) || !self.per_source.allow(src.ip(), now_ms) {
            return None;
        }
        let max_len = max_reply_len(&request);
        let reply = match request {
            Request::Ping => Reply::Pong,
            Request::List { skip } => self.rooms_reply(skip),
            Request::Create { name, bots, kills } => {
                if let Some((_, _, made)) = self.recent_creates.iter().find(|(ip, n, _)| *ip == src.ip() && *n == nonce)
                {
                    // A repeat of a create whose reply was lost: answer with the room it made, if it still exists.
                    if let Some(room) = self.manager.rooms().into_iter().find(|r| &r.name == made) {
                        return Some(Reply::Created { room }.encode(nonce, self.build));
                    }
                }
                if !self.create_limit.allow(src.ip(), now_ms) {
                    return None;
                }
                match self.manager.create(&name, bots, kills, now_ms) {
                    Ok(room) => {
                        log(&format!("{} created room \"{}\" on port {}", src.ip(), room.name, room.port));
                        self.recent_creates.push_back((src.ip(), nonce, room.name.clone()));
                        if self.recent_creates.len() > 64 {
                            self.recent_creates.pop_front();
                        }
                        Reply::Created { room }
                    }
                    Err(e) => Reply::Error { code: e.code, text: e.text },
                }
            }
        };
        let bytes = reply.encode(nonce, self.build);
        debug_assert!(bytes.len() <= max_len, "reply {} bytes", bytes.len());
        Some(bytes)
    }

    /// The rooms from number `skip` on, as many as fit one datagram.
    fn rooms_reply(&self, skip: u8) -> Reply {
        let all = self.manager.rooms();
        let mut size = REPLY_HEADER + 3;
        let mut rooms = Vec::new();
        for r in all.iter().skip(skip as usize) {
            size += Reply::room_len(r);
            if size > MAX_REPLY {
                break;
            }
            rooms.push(r.clone());
        }
        Reply::Rooms { skip, total: all.len().min(255) as u8, rooms }
    }
}

/// The hub's UDP loop: answer datagrams on `socket` and tick the manager until `stop` is set, then kill every child.
pub fn serve(socket: &UdpSocket, hub: &mut Hub, stop: &AtomicBool) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    let start = Instant::now();
    let mut buf = [0u8; 2048];
    let mut last_tick = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let now_ms = start.elapsed().as_millis() as u64;
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                if let Some(reply) = hub.handle(src, &buf[..n], now_ms) {
                    let _ = socket.send_to(&reply, src);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                        | io::ErrorKind::ConnectionReset
                ) => {}
            Err(e) => {
                hub.shutdown();
                return Err(e);
            }
        }
        if now_ms.saturating_sub(last_tick) >= 250 {
            last_tick = now_ms;
            hub.tick(now_ms);
        }
    }
    log("Shutting down: stopping every room");
    hub.shutdown();
    Ok(())
}

// ---- logging ------------------------------------------------------------------------------------------------------------

/// `YYYY-MM-DD HH:MM:SSZ` (UTC) for a unix time.
pub fn timestamp(unix_secs: u64) -> String {
    let (days, rem) = (unix_secs / 86_400, unix_secs % 86_400);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// One timestamped line on stdout.
pub fn log(msg: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    println!("[{}] {msg}", timestamp(now));
}

// ---- tests --------------------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    #[derive(Default)]
    struct World {
        status: BTreeMap<u16, RoomStatus>,
        dead: Vec<u16>,
        spawned: Vec<RoomSpec>,
        killed: Vec<u16>,
        fail_spawn: bool,
    }
    struct Fake(Rc<RefCell<World>>);
    struct FakeProc(u16, Rc<RefCell<World>>);
    impl Spawner for Fake {
        fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>> {
            let mut w = self.0.borrow_mut();
            if w.fail_spawn {
                return Err(io::Error::other("no binary"));
            }
            w.dead.retain(|p| *p != spec.port);
            w.spawned.push(spec.clone());
            Ok(Box::new(FakeProc(spec.port, self.0.clone())))
        }
    }
    impl RoomProcess for FakeProc {
        fn status(&mut self) -> Option<RoomStatus> {
            self.1.borrow().status.get(&self.0).copied()
        }
        fn exited(&mut self) -> bool {
            self.1.borrow().dead.contains(&self.0)
        }
        fn kill(&mut self) {
            self.1.borrow_mut().killed.push(self.0);
        }
    }

    fn manager() -> (RoomManager, Rc<RefCell<World>>) {
        let w = Rc::new(RefCell::new(World::default()));
        let mut m = RoomManager::new(ManagerConfig::default(), Box::new(Fake(w.clone())));
        m.tick(0);
        (m, w)
    }

    fn src(n: u8) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, n], 5000))
    }

    #[test]
    fn requests_and_replies_round_trip_and_stay_in_their_size_classes() {
        for r in [
            Request::Ping,
            Request::List { skip: 3 },
            Request::Create { name: "Kevin's place".into(), bots: true, kills: 25 },
        ] {
            let bytes = r.encode(0xDEAD_BEEF);
            assert!(bytes.len() >= min_request_len(&r) && bytes.len() <= MAX_REQUEST);
            assert_eq!(Request::decode(&bytes), Some((0xDEAD_BEEF, r)));
        }
        let room = RoomInfo {
            name: "Ünïcode room".into(),
            players: 3,
            capacity: 12,
            state: RoomState::Playing,
            port: 4102,
            public: false,
        };
        for r in [
            Reply::Pong,
            Reply::Created { room: room.clone() },
            Reply::Error { code: ErrorCode::Full, text: "x".repeat(300) },
            Reply::Rooms { skip: 1, total: 2, rooms: vec![room.clone(), room] },
        ] {
            let bytes = r.encode(7, 0x1234_5678);
            let p = Reply::decode(&bytes).unwrap();
            assert_eq!((p.nonce, p.build), (7, 0x1234_5678));
            match (&r, &p.reply) {
                (Reply::Error { .. }, Reply::Error { code, text }) => {
                    assert!(*code == ErrorCode::Full && text.len() == MAX_ERROR_TEXT)
                }
                _ => assert_eq!(p.reply, r),
            }
        }
    }

    #[test]
    fn garbage_and_truncated_datagrams_decode_to_nothing() {
        assert!(Request::decode(b"").is_none());
        assert!(Request::decode(b"hello world, this is not a hub request").is_none());
        let mut ok = Request::List { skip: 0 }.encode(1);
        ok[4] = VERSION + 1;
        assert!(Request::decode(&ok).is_none(), "another version is not understood");
        assert!(Request::decode(&Request::Ping.encode(1)[..9]).is_none());
        let mut create = Request::Create { name: "abcdef".into(), bots: false, kills: 0 }.encode(1);
        create.truncate(REQUEST_HEADER + 5);
        assert!(Request::decode(&create).is_none(), "a name that runs off the end");
        let mut bad_utf8 = Request::Create { name: "abcd".into(), bots: false, kills: 0 }.encode(1);
        bad_utf8[REQUEST_HEADER + 4] = 0xFF;
        assert!(Request::decode(&bad_utf8).is_none());
        assert!(Reply::decode(&Reply::Pong.encode(1, 1)[..10]).is_none());
        let mut rooms = Reply::Rooms { skip: 0, total: 1, rooms: vec![] }.encode(1, 1);
        *rooms.last_mut().unwrap() = 9;
        assert!(Reply::decode(&rooms).is_none(), "a count with no rooms behind it");
    }

    #[test]
    fn names_are_cleaned_and_bad_ones_refused() {
        assert_eq!(sanitize_name("  Kevin's   Room-2 ").as_deref(), Ok("Kevin's Room-2"));
        assert_eq!(sanitize_name("Zażółć łódź").as_deref(), Ok("Zażółć łódź"));
        assert_eq!(sanitize_name(""), Err(NameError::Empty));
        assert_eq!(sanitize_name("    "), Err(NameError::Empty));
        assert_eq!(sanitize_name("---"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("a\nb"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("tab\there"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("bell\u{7}"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("rtl\u{202e}evil"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("<script>"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name("%s%n"), Err(NameError::BadCharacter));
        assert_eq!(sanitize_name(&"a".repeat(MAX_NAME_CHARS)).map(|s| s.len()), Ok(MAX_NAME_CHARS));
        assert_eq!(sanitize_name(&"a".repeat(MAX_NAME_CHARS + 1)), Err(NameError::TooLong));
        // 24 four-byte-ish characters still fit the wire.
        let wide = "字".repeat(MAX_NAME_CHARS);
        assert_eq!(sanitize_name(&wide).as_deref(), Ok(wide.as_str()));
    }

    #[test]
    fn the_rate_limiter_refills_with_time_and_never_trusts_a_clock_that_goes_backwards() {
        let mut l = RateLimiter::new(3., 1., 100);
        let ip: IpAddr = [1, 2, 3, 4].into();
        assert!([0, 0, 0].iter().all(|t| l.allow(ip, *t)));
        assert!(!l.allow(ip, 0), "burst of three spent");
        assert!(!l.allow(ip, 500), "half a token is not enough");
        assert!(l.allow(ip, 1000), "one token per second");
        assert!(!l.allow(ip, 1000));
        assert!(!l.allow(ip, 10), "time going backwards gives nothing back");
        assert!(l.allow([1, 2, 3, 5].into(), 0), "another source has its own bucket");
        assert!(l.allow(ip, 100_000) && l.allow(ip, 100_001) && l.allow(ip, 100_002), "long idle refills to the burst");
        assert!(!l.allow(ip, 100_003), "and no further");
    }

    #[test]
    fn the_source_table_is_capped_and_ages_out_idle_sources() {
        let mut l = RateLimiter::new(2., 1., 4);
        let ip = |n: u8| IpAddr::from([9, 9, 9, n]);
        for n in 0..4 {
            assert!(l.allow(ip(n), 0));
        }
        assert_eq!(l.tracked(), 4);
        assert!(!l.allow(ip(99), 100), "full of recent sources: a fifth is refused");
        assert_eq!(l.tracked(), 4, "memory does not grow");
        assert!(l.allow(ip(99), 5_000), "after the others refill they are forgotten and the newcomer fits");
        assert!(l.tracked() <= 4);
    }

    #[test]
    fn status_lines_parse() {
        assert_eq!(
            parse_status_line("STATUS players=2 stage=lobby"),
            Some(RoomStatus { players: 2, state: RoomState::Lobby })
        );
        assert_eq!(
            parse_status_line("STATUS players=12 stage=playing"),
            Some(RoomStatus { players: 12, state: RoomState::Playing })
        );
        assert_eq!(
            parse_status_line("STATUS stage=results players=0"),
            Some(RoomStatus { players: 0, state: RoomState::Lobby })
        );
        assert_eq!(parse_status_line("[Server] tick 5 | stage Lobby | players 1"), None);
        assert_eq!(parse_status_line("STATUS players=x stage=lobby"), None);
        assert_eq!(parse_status_line("STATUS players=1"), None);
    }

    #[test]
    fn server_arguments_carry_the_settings() {
        let spec = RoomSpec {
            name: "n".into(),
            port: 4102,
            listen: "0.0.0.0:4102".into(),
            settings: RoomSettings { bots: true, kills: 25, skill: 2, auto_start: 10 },
            report_dir: Some("data/port-4102".into()),
            public: false,
        };
        let a = ProcessSpawner::args(&spec).join(" ");
        assert_eq!(
            a,
            "--listen 0.0.0.0:4102 --status-lines --exit-on-stdin-eof --auto-start 10 --kills 25 --bots --skill 2 --report-dir data/port-4102"
        );
        let plain = RoomSpec { settings: RoomSettings::default(), report_dir: None, ..spec };
        assert!(!ProcessSpawner::args(&plain).join(" ").contains("--bots"));
    }

    #[test]
    fn the_public_room_is_listed_first_on_the_next_port() {
        let (m, w) = manager();
        let rooms = m.rooms();
        assert_eq!(rooms.len(), 1);
        assert_eq!((rooms[0].name.as_str(), rooms[0].port, rooms[0].public), ("Public", 4101, true));
        assert!(w.borrow().spawned[0].settings.bots);
    }

    #[test]
    fn rooms_get_the_pool_ports_and_the_cap_and_name_rules_hold() {
        let (mut m, w) = manager();
        let a = m.create("Alpha", false, 0, 0).unwrap();
        assert_eq!(a.port, 4102);
        assert_eq!(
            m.create("alpha", false, 0, 0).unwrap_err().code,
            ErrorCode::NameTaken,
            "names are not case-sensitive"
        );
        assert_eq!(m.create("PUBLIC", false, 0, 0).unwrap_err().code, ErrorCode::NameTaken);
        assert_eq!(m.create("   ", false, 0, 0).unwrap_err().code, ErrorCode::BadName);
        for (i, n) in ["B", "C", "D", "E", "F"].iter().enumerate() {
            assert_eq!(m.create(n, true, 10, 0).unwrap().port, 4103 + i as u16);
        }
        assert_eq!(m.create("Seventh", false, 0, 0).unwrap_err().code, ErrorCode::Full);
        assert_eq!(m.rooms().len(), 7, "six user rooms plus Public");
        let spawned = w.borrow().spawned.clone();
        assert_eq!(spawned.last().unwrap().settings, RoomSettings { bots: true, kills: 10, skill: 1, auto_start: 30 });
        w.borrow_mut().fail_spawn = true;
        m.rooms.pop();
        assert_eq!(m.create("Broken", false, 0, 0).unwrap_err().code, ErrorCode::Unavailable);
    }

    #[test]
    fn a_closed_rooms_port_is_reused() {
        let (mut m, w) = manager();
        m.create("A", false, 0, 0).unwrap();
        m.create("B", false, 0, 0).unwrap();
        w.borrow_mut().dead.push(4102);
        m.tick(1000);
        assert_eq!(m.rooms().len(), 2, "the dead room is gone");
        assert_eq!(m.create("C", false, 0, 1000).unwrap().port, 4102);
    }

    #[test]
    fn counts_and_states_come_from_the_servers_and_empty_rooms_are_reaped() {
        let (mut m, w) = manager();
        let a = m.create("Busy", false, 0, 0).unwrap();
        let b = m.create("Idle", false, 0, 0).unwrap();
        w.borrow_mut().status.insert(a.port, RoomStatus { players: 2, state: RoomState::Playing });
        w.borrow_mut().status.insert(b.port, RoomStatus { players: 0, state: RoomState::Lobby });
        m.tick(60_000);
        let listed: Vec<_> = m.rooms().iter().map(|r| (r.name.clone(), r.players, r.state)).collect();
        assert!(listed.contains(&("Busy".into(), 2, RoomState::Playing)));
        m.tick(119_999);
        assert_eq!(m.rooms().len(), 3, "not yet");
        m.tick(120_000);
        let names: Vec<_> = m.rooms().into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["Public", "Busy"], "the empty user room is gone; Public is permanent");
        assert_eq!(w.borrow().killed, vec![b.port]);
        // Someone leaves Busy: its timer starts then, not at creation.
        w.borrow_mut().status.insert(a.port, RoomStatus { players: 0, state: RoomState::Lobby });
        m.tick(130_000);
        m.tick(249_999);
        assert_eq!(m.rooms().len(), 2);
        m.tick(250_000);
        assert_eq!(m.rooms().len(), 1);
        // A room someone is in never expires, however long.
        let c = m.create("Held", false, 0, 250_000).unwrap();
        w.borrow_mut().status.insert(c.port, RoomStatus { players: 1, state: RoomState::Lobby });
        m.tick(900_000);
        assert_eq!(m.rooms().len(), 2);
    }

    #[test]
    fn a_dead_public_room_is_restarted_after_a_pause_and_a_dead_user_room_is_not() {
        let (mut m, w) = manager();
        m.create("User", false, 0, 0).unwrap();
        w.borrow_mut().dead.extend([4101, 4102]);
        m.tick(10_000);
        assert!(m.rooms().is_empty(), "both gone, restart waits");
        assert_eq!(w.borrow().spawned.len(), 2);
        m.tick(11_999);
        assert!(m.rooms().is_empty());
        m.tick(12_000);
        let rooms = m.rooms();
        assert_eq!((rooms.len(), rooms[0].name.as_str(), rooms[0].port), (1, "Public", 4101));
        assert_eq!(w.borrow().spawned.len(), 3);
        // A spawn failure is retried, not fatal.
        w.borrow_mut().dead.push(4101);
        w.borrow_mut().fail_spawn = true;
        m.tick(20_000);
        m.tick(30_000);
        assert!(m.rooms().is_empty());
        w.borrow_mut().fail_spawn = false;
        m.tick(40_000);
        assert_eq!(m.rooms().len(), 1);
    }

    #[test]
    fn shutdown_and_drop_kill_every_child() {
        let (mut m, w) = manager();
        m.create("A", false, 0, 0).unwrap();
        m.shutdown();
        let mut killed = w.borrow().killed.clone();
        killed.sort();
        assert_eq!(killed, vec![4101, 4102]);
        assert!(m.rooms().is_empty());
    }

    fn hub() -> (Hub, Rc<RefCell<World>>) {
        let w = Rc::new(RefCell::new(World::default()));
        (Hub::new(ManagerConfig::default(), Limits::default(), Box::new(Fake(w.clone()))), w)
    }

    fn ask(h: &mut Hub, from: u8, r: &Request, now: u64) -> Option<ReplyPacket> {
        // A fresh nonce each time, like a client making separate requests (a repeated nonce is a retry).
        static NONCE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(100);
        let nonce = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = h.handle(src(from), &r.encode(nonce), now).map(|b| Reply::decode(&b).expect("a valid reply"))?;
        assert_eq!(p.nonce, nonce);
        Some(p)
    }

    #[test]
    fn the_hub_lists_creates_pings_and_stamps_every_reply_with_the_build() {
        let (mut h, _) = hub();
        let build = crate::netgame::fingerprint();
        let p = ask(&mut h, 1, &Request::List { skip: 0 }, 0).unwrap();
        assert_eq!(p.build, build);
        let Reply::Rooms { total, rooms, .. } = p.reply else { panic!() };
        assert_eq!((total, rooms.len(), rooms[0].name.as_str()), (1, 1, "Public"));
        let p = ask(&mut h, 1, &Request::Create { name: " Test  room ".into(), bots: false, kills: 0 }, 1).unwrap();
        let Reply::Created { room } = p.reply else { panic!("{:?}", p.reply) };
        assert_eq!((room.name.as_str(), room.port, room.capacity), ("Test room", 4102, ROOM_CAPACITY));
        let p = ask(&mut h, 2, &Request::Create { name: "test room".into(), bots: false, kills: 0 }, 2).unwrap();
        assert!(matches!(p.reply, Reply::Error { code: ErrorCode::NameTaken, .. }));
        assert_eq!(ask(&mut h, 2, &Request::Ping, 3).unwrap().reply, Reply::Pong);
        assert_eq!(ask(&mut h, 2, &Request::Ping, 3).unwrap().build, build);
    }

    #[test]
    fn a_repeated_create_with_the_same_nonce_returns_the_same_room_instead_of_name_taken() {
        let (mut h, w) = hub();
        let req = Request::Create { name: "Retry".into(), bots: false, kills: 0 }.encode(5);
        let first = Reply::decode(&h.handle(src(1), &req, 0).unwrap()).unwrap().reply;
        let again = Reply::decode(&h.handle(src(1), &req, 900).unwrap()).unwrap().reply;
        assert_eq!(first, again);
        assert_eq!(w.borrow().spawned.len(), 2, "Public and one room: no second server");
        let other = Reply::decode(&h.handle(src(2), &req, 1000).unwrap()).unwrap().reply;
        assert!(matches!(other, Reply::Error { code: ErrorCode::NameTaken, .. }), "another source is not a retry");
    }

    #[test]
    fn garbage_short_and_oversized_datagrams_get_no_reply() {
        let (mut h, _) = hub();
        assert!(h.handle(src(1), b"", 0).is_none());
        assert!(h.handle(src(1), b"GET / HTTP/1.1\r\n\r\n", 0).is_none());
        assert!(h.handle(src(1), &[0xFF; 600], 0).is_none());
        let unpadded = &Request::List { skip: 0 }.encode(1)[..REQUEST_HEADER + 1];
        assert!(h.handle(src(1), unpadded, 0).is_none(), "an unpadded list request could amplify");
        let big = [Request::List { skip: 0 }.encode(1), vec![0; 400]].concat();
        assert!(h.handle(src(1), &big, 0).is_none());
        // None of that spent the source's budget.
        assert!(ask(&mut h, 1, &Request::Ping, 0).is_some());
    }

    #[test]
    fn a_flood_from_one_source_is_cut_off_and_others_are_unaffected() {
        let (mut h, _) = hub();
        let answered = (0..100).filter(|_| ask(&mut h, 1, &Request::List { skip: 0 }, 0).is_some()).count();
        assert_eq!(answered, 10, "the burst, then silence");
        assert!(ask(&mut h, 2, &Request::List { skip: 0 }, 0).is_some());
        assert!(ask(&mut h, 1, &Request::List { skip: 0 }, 1000).is_some(), "it recovers");
    }

    #[test]
    fn creating_rooms_has_its_own_slow_limit_and_a_global_limit_caps_everyone() {
        let (mut h, _) = hub();
        let create = |n: &str| Request::Create { name: n.into(), bots: false, kills: 0 };
        assert!(matches!(ask(&mut h, 1, &create("A"), 0).unwrap().reply, Reply::Created { .. }));
        assert!(matches!(ask(&mut h, 1, &create("B"), 1).unwrap().reply, Reply::Created { .. }));
        assert!(matches!(ask(&mut h, 1, &create("C"), 2).unwrap().reply, Reply::Created { .. }));
        assert!(ask(&mut h, 1, &create("D"), 3).is_none(), "a fourth create in a few seconds is dropped");
        assert!(ask(&mut h, 1, &Request::Ping, 3).is_some(), "but listing still works");
        let (mut h, _) = hub();
        let answered = (0..2000u32)
            .filter(|i| {
                h.handle(SocketAddr::from(([10, (i >> 8) as u8, *i as u8, 1], 1)), &Request::Ping.encode(1), 0)
                    .is_some()
            })
            .count();
        assert_eq!(answered, 300, "spoofed sources cannot make the hub send more than the global burst");
    }

    #[test]
    fn a_long_room_list_is_truncated_to_one_datagram_and_can_be_paged() {
        let w = Rc::new(RefCell::new(World::default()));
        let cfg = ManagerConfig { max_user_rooms: MAX_ROOMS_HARD, public_name: "P".into(), ..Default::default() };
        let mut h = Hub::new(cfg, Limits { burst: 1e6, create_burst: 1e6, ..Default::default() }, Box::new(Fake(w)));
        for i in 0..MAX_ROOMS_HARD {
            let name = format!("{i:02}{}", "字".repeat(MAX_NAME_CHARS - 2));
            assert!(matches!(
                ask(&mut h, 1, &Request::Create { name, bots: false, kills: 0 }, 0).unwrap().reply,
                Reply::Created { .. }
            ));
        }
        let first = h.handle(src(1), &Request::List { skip: 0 }.encode(1), 0).unwrap();
        assert!(first.len() <= MAX_REPLY, "{} bytes", first.len());
        let Reply::Rooms { total, rooms, .. } = Reply::decode(&first).unwrap().reply else { panic!() };
        assert_eq!(total as usize, MAX_ROOMS_HARD + 1);
        assert!(rooms.len() < total as usize, "the first page is a truncation");
        let second = h.handle(src(1), &Request::List { skip: rooms.len() as u8 }.encode(2), 0).unwrap();
        let Reply::Rooms { rooms: more, .. } = Reply::decode(&second).unwrap().reply else { panic!() };
        assert_eq!(rooms.len() + more.len(), total as usize, "two pages cover everything");
    }

    #[test]
    fn timestamps_are_utc_dates() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00Z");
        assert_eq!(timestamp(951_782_400 + 3661), "2000-02-29 01:01:01Z");
        assert_eq!(timestamp(1_790_000_000), "2026-09-21 14:13:20Z");
    }
}
