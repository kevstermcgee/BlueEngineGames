//! The game's side of the hub: ask which rooms exist, make one, work out where the hub lives.
//!
//! Std only and no window, so the tests run headless; the menu polls a [`HubClient`] once a frame:
//!
//! ```ignore
//! let hub = hub_client::resolve_ipv4(&hub_client::default_server())?;   // DNS: do this off the UI thread
//! let mut client = HubClient::new(hub)?;
//! client.request_list();
//! // every frame:
//! match client.poll() {
//!     Some(HubEvent::Rooms { build, rooms }) => { /* check build_matches(build), show rooms */ }
//!     Some(HubEvent::Created { room, .. }) => { /* join hub_client::room_addr(hub, room.port) */ }
//!     Some(HubEvent::Error { text, .. }) => { /* show text */ }
//!     Some(HubEvent::Timeout) => { /* "Could not reach the server" */ }
//!     None => {}
//! }
//! ```
use crate::hub::{ErrorCode, Reply, Request, RoomInfo, DEFAULT_BASE_PORT};
use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use crate::hub::{sanitize_name, NameError, RoomState, MAX_NAME_CHARS};

/// Where the public hub lives. The one place this is written down; `deploy/README.md` explains how to point it at the
/// real DuckDNS name. A `server.txt` next to the executable overrides it without a rebuild.
pub const DEFAULT_HUB: &str = "deadfall-kevin.duckdns.org:4100";
/// The override file, next to the executable: the first line is the hub address (`host` or `host:port`).
pub const OVERRIDE_FILE: &str = "server.txt";
/// Resend an unanswered request this often.
pub const RETRY_EVERY: Duration = Duration::from_millis(800);
/// Give up on a request after this long.
pub const TIMEOUT: Duration = Duration::from_millis(2500);
/// A long room list is fetched in at most this many datagrams.
const MAX_PAGES: u8 = 4;

/// The hub address to use: `server.txt` next to the running executable if it has one, else [`DEFAULT_HUB`].
pub fn default_server() -> String {
    let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    default_server_in(dir.as_deref())
}

/// [`default_server`] for an explicit executable directory (`None` = no override), so tests are deterministic.
/// The override is the first line of `server.txt`, trimmed; a missing, empty or unreadable file falls back.
pub fn default_server_in(exe_dir: Option<&Path>) -> String {
    exe_dir
        .and_then(|d| std::fs::read_to_string(d.join(OVERRIDE_FILE)).ok())
        .and_then(|text| text.trim_start_matches('\u{feff}').lines().next().map(|l| l.trim().to_string()))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| DEFAULT_HUB.to_string())
}

/// Why an address would not resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    Empty,
    BadPort,
    BadHost,
    /// An IPv6 literal, or a name with only IPv6 addresses: Deadfall's servers are IPv4.
    NotIpv4,
    /// DNS knows no IPv4 address for the name (or could not be reached).
    NotFound(String),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Empty => write!(f, "No server address given."),
            ResolveError::BadPort => write!(f, "The port must be a number from 1 to 65535."),
            ResolveError::BadHost => write!(f, "That is not a valid server name."),
            ResolveError::NotIpv4 => write!(f, "Only IPv4 addresses work (like 203.0.113.5 or play.example.com)."),
            ResolveError::NotFound(host) => {
                write!(f, "Could not find \"{host}\". Check the name and your internet connection.")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Split `host`, `host:port`, `1.2.3.4` or `1.2.3.4:port` into host and port (the hub's port when none is given).
pub fn parse_host_port(input: &str) -> Result<(String, u16), ResolveError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(ResolveError::Empty);
    }
    if s.starts_with('[') || s.matches(':').count() > 1 {
        return Err(ResolveError::NotIpv4);
    }
    let (host, port) = match s.split_once(':') {
        Some((h, p)) => (h, p.trim().parse::<u16>().ok().filter(|p| *p != 0).ok_or(ResolveError::BadPort)?),
        None => (s, DEFAULT_BASE_PORT),
    };
    let host = host.trim();
    if host.is_empty() {
        return Err(ResolveError::Empty);
    }
    if !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_') {
        return Err(ResolveError::BadHost);
    }
    Ok((host.to_string(), port))
}

/// Resolve a hub address to an IPv4 socket address. Accepts a host name or IPv4 address with an optional `:port`
/// (default: the hub's port, 4100). A host name needs DNS, which blocks: call it from a worker thread, not every frame.
pub fn resolve_ipv4(input: &str) -> Result<SocketAddr, ResolveError> {
    let (host, port) = parse_host_port(input)?;
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(SocketAddr::new(IpAddr::V4(ip), port));
    }
    let found: Vec<SocketAddr> =
        (host.as_str(), port).to_socket_addrs().map_err(|_| ResolveError::NotFound(host.clone()))?.collect();
    if let Some(v4) = found.iter().find(|a| a.is_ipv4()) {
        return Ok(*v4);
    }
    Err(if found.is_empty() { ResolveError::NotFound(host) } else { ResolveError::NotIpv4 })
}

/// The address of a room's game server: the hub's host, the room's port.
pub fn room_addr(hub_addr: SocketAddr, port: u16) -> SocketAddr {
    SocketAddr::new(hub_addr.ip(), port)
}

/// This game's build id (`netgame::fingerprint()`).
pub fn local_build() -> u32 {
    crate::netgame::fingerprint()
}

/// Does a hub's build id match this game? A mismatch means the rooms would refuse us.
pub fn build_matches(build: u32) -> bool {
    build == local_build()
}

/// What to tell the player when [`build_matches`] is false.
pub const UPDATE_MESSAGE: &str = "This server runs a different version of Deadfall. Update the game, then try again.";

/// The result of a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HubEvent {
    /// The room list, Public first. `build` is the hub's build id: check [`build_matches`] before joining.
    Rooms { build: u32, rooms: Vec<RoomInfo> },
    /// The room was made; join [`room_addr`]`(hub, room.port)`.
    Created { build: u32, room: RoomInfo },
    /// The hub refused: show `text`.
    Error { build: u32, code: ErrorCode, text: String },
    /// No answer after the retries: the hub is down or unreachable.
    Timeout,
}

struct Pending {
    request: Request,
    nonce: u32,
    started: Instant,
    last_sent: Instant,
    collected: Vec<RoomInfo>,
    pages: u8,
}

/// One request at a time to one hub; poll it every frame.
pub struct HubClient {
    socket: UdpSocket,
    hub: SocketAddr,
    pending: Option<Pending>,
    retry: Duration,
    timeout: Duration,
    seq: u32,
}

impl HubClient {
    /// A client for the hub at `hub` (IPv4). Opens a UDP socket; nothing is sent until a request is made.
    pub fn new(hub: SocketAddr) -> io::Result<Self> {
        Self::with_timing(hub, RETRY_EVERY, TIMEOUT)
    }

    /// [`HubClient::new`] with other retry and timeout times (tests).
    pub fn with_timing(hub: SocketAddr, retry: Duration, timeout: Duration) -> io::Result<Self> {
        if !hub.is_ipv4() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "the hub must be an IPv4 address"));
        }
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_nonblocking(true)?;
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.subsec_nanos());
        Ok(Self { socket, hub, pending: None, retry, timeout, seq: seed ^ std::process::id().rotate_left(16) })
    }

    pub fn hub_addr(&self) -> SocketAddr {
        self.hub
    }

    /// A request is in flight.
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Ask for the room list. Replaces any request in flight.
    pub fn request_list(&mut self) {
        self.start(Request::List { skip: 0 });
    }

    /// Ask the hub to make a room called `name` (clean it with [`sanitize_name`] first to show errors as the player types;
    /// the hub checks again). Replaces any request in flight.
    pub fn request_create(&mut self, name: &str) {
        self.request_create_with(name, false, 0);
    }

    /// [`HubClient::request_create`] with options: `bots` fills empty slots with computer players, `kills` is the kill
    /// target (0 = the hub's default, otherwise 1 to 500).
    pub fn request_create_with(&mut self, name: &str, bots: bool, kills: u16) {
        self.start(Request::Create { name: name.to_string(), bots, kills });
    }

    /// Forget any request in flight (a late reply is ignored).
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    fn start(&mut self, request: Request) {
        // A xorshift step: nonces are for matching replies to requests, not secrecy.
        let mut x = self.seq | 1;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seq = x;
        let now = Instant::now();
        let p = Pending { request, nonce: x, started: now, last_sent: now, collected: Vec::new(), pages: 1 };
        self.send(&p.request, p.nonce);
        self.pending = Some(p);
    }

    fn send(&self, request: &Request, nonce: u32) {
        let _ = self.socket.send_to(&request.encode(nonce), self.hub);
    }

    /// Check for the answer (never blocks). Returns each request's result once.
    pub fn poll(&mut self) -> Option<HubEvent> {
        let mut buf = [0u8; 2048];
        loop {
            let Ok((n, from)) = self.socket.recv_from(&mut buf) else { break };
            let Some(p) = self.pending.as_mut() else { continue };
            if from != self.hub {
                continue;
            }
            let Some(packet) = Reply::decode(&buf[..n]) else { continue };
            if packet.nonce != p.nonce {
                continue;
            }
            let build = packet.build;
            match (&p.request, packet.reply) {
                (_, Reply::Error { code, text }) => {
                    self.pending = None;
                    return Some(HubEvent::Error { build, code, text });
                }
                (Request::List { .. }, Reply::Rooms { skip, total, rooms }) => {
                    if skip as usize != p.collected.len() {
                        continue; // a duplicate page
                    }
                    let got = rooms.len();
                    p.collected.extend(rooms);
                    if got > 0 && p.collected.len() < total as usize && p.pages < MAX_PAGES {
                        p.pages += 1;
                        p.last_sent = Instant::now();
                        let next = Request::List { skip: p.collected.len().min(255) as u8 };
                        let _ = self.socket.send_to(&next.encode(p.nonce), self.hub);
                        continue;
                    }
                    let rooms = std::mem::take(&mut p.collected);
                    self.pending = None;
                    return Some(HubEvent::Rooms { build, rooms });
                }
                (Request::Create { .. }, Reply::Created { room }) => {
                    self.pending = None;
                    return Some(HubEvent::Created { build, room });
                }
                _ => {}
            }
        }
        let p = self.pending.as_mut()?;
        if p.started.elapsed() >= self.timeout {
            self.pending = None;
            return Some(HubEvent::Timeout);
        }
        if p.last_sent.elapsed() >= self.retry {
            p.last_sent = Instant::now();
            // Resend the current page (a Create is safe to repeat: the hub recognises the nonce).
            let request = match &p.request {
                Request::List { .. } => Request::List { skip: p.collected.len().min(255) as u8 },
                other => other.clone(),
            };
            let _ = self.socket.send_to(&request.encode(p.nonce), self.hub);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::{Request, RoomInfo};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn room(name: &str, port: u16) -> RoomInfo {
        RoomInfo { name: name.into(), players: 1, capacity: 12, state: RoomState::Lobby, port, public: port == 4101 }
    }

    /// A UDP socket on loopback that answers each datagram with `script(request_index, request, nonce)` replies.
    fn fake_hub(script: impl Fn(usize, Request, u32) -> Vec<Reply> + Send + 'static) -> (SocketAddr, Arc<AtomicUsize>) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        socket.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        let seen = Arc::new(AtomicUsize::new(0));
        let count = seen.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            let mut idle = 0;
            while idle < 100 {
                match socket.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        idle = 0;
                        let i = count.fetch_add(1, Ordering::SeqCst);
                        if let Some((nonce, req)) = Request::decode(&buf[..n]) {
                            for r in script(i, req, nonce) {
                                socket.send_to(&r.encode(nonce, 0xABCD), from).unwrap();
                            }
                        }
                    }
                    Err(_) => idle += 1,
                }
            }
        });
        (addr, seen)
    }

    fn wait(c: &mut HubClient) -> HubEvent {
        let t = Instant::now();
        loop {
            if let Some(e) = c.poll() {
                return e;
            }
            assert!(t.elapsed() < Duration::from_secs(5), "no event");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn host_and_port_parsing() {
        assert_eq!(parse_host_port("example.com"), Ok(("example.com".into(), 4100)));
        assert_eq!(parse_host_port("  example.com:5000 "), Ok(("example.com".into(), 5000)));
        assert_eq!(parse_host_port("10.0.0.5:7"), Ok(("10.0.0.5".into(), 7)));
        assert_eq!(parse_host_port(""), Err(ResolveError::Empty));
        assert_eq!(parse_host_port(":4100"), Err(ResolveError::Empty));
        assert_eq!(parse_host_port("a:0"), Err(ResolveError::BadPort));
        assert_eq!(parse_host_port("a:70000"), Err(ResolveError::BadPort));
        assert_eq!(parse_host_port("a:"), Err(ResolveError::BadPort));
        assert_eq!(parse_host_port("[::1]:4100"), Err(ResolveError::NotIpv4));
        assert_eq!(parse_host_port("::1"), Err(ResolveError::NotIpv4));
        assert_eq!(parse_host_port("bad host"), Err(ResolveError::BadHost));
        assert_eq!(parse_host_port("http://x"), Err(ResolveError::BadPort));
    }

    #[test]
    fn resolving_literals_needs_no_dns_and_defaults_to_the_hub_port() {
        assert_eq!(resolve_ipv4("127.0.0.1"), Ok("127.0.0.1:4100".parse().unwrap()));
        assert_eq!(resolve_ipv4("192.168.1.9:4200"), Ok("192.168.1.9:4200".parse().unwrap()));
        assert_eq!(resolve_ipv4("localhost:4300"), Ok("127.0.0.1:4300".parse().unwrap()));
        assert!(matches!(resolve_ipv4("nonexistent.invalid"), Err(ResolveError::NotFound(_))));
        assert_eq!(resolve_ipv4("::1"), Err(ResolveError::NotIpv4));
        assert!(!ResolveError::Empty.to_string().is_empty());
    }

    #[test]
    fn room_addresses_keep_the_hub_host() {
        let hub: SocketAddr = "203.0.113.5:4100".parse().unwrap();
        assert_eq!(room_addr(hub, 4103), "203.0.113.5:4103".parse().unwrap());
    }

    #[test]
    fn the_default_server_is_the_constant_unless_server_txt_overrides_it() {
        assert_eq!(default_server_in(None), DEFAULT_HUB);
        let dir = std::env::temp_dir().join(format!("df-hub-client-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(default_server_in(Some(&dir)), DEFAULT_HUB, "no file");
        let write = |s: &str| std::fs::write(dir.join(OVERRIDE_FILE), s).unwrap();
        write("  192.168.1.20:4100  \r\nsecond line\n");
        assert_eq!(default_server_in(Some(&dir)), "192.168.1.20:4100");
        write("\u{feff}play.example.com\n");
        assert_eq!(default_server_in(Some(&dir)), "play.example.com");
        write("\n\n");
        assert_eq!(default_server_in(Some(&dir)), DEFAULT_HUB, "an empty file falls back");
        write("   ");
        assert_eq!(default_server_in(Some(&dir)), DEFAULT_HUB);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_list_request_gets_the_rooms_and_the_build() {
        let (addr, _) = fake_hub(|_, req, _| {
            assert!(matches!(req, Request::List { skip: 0 }));
            vec![Reply::Rooms { skip: 0, total: 2, rooms: vec![room("Public", 4101), room("Mine", 4102)] }]
        });
        let mut c = HubClient::new(addr).unwrap();
        assert!(c.poll().is_none() && !c.busy());
        c.request_list();
        assert!(c.busy());
        let HubEvent::Rooms { build, rooms } = wait(&mut c) else { panic!() };
        assert_eq!((build, rooms.len(), rooms[1].port), (0xABCD, 2, 4102));
        assert!(!c.busy());
        assert!(c.poll().is_none(), "an event is delivered once");
    }

    #[test]
    fn a_truncated_list_is_fetched_page_by_page_and_merged() {
        let (addr, seen) = fake_hub(|_, req, _| {
            let Request::List { skip } = req else { panic!() };
            let all: Vec<_> = (0..5).map(|i| room(&format!("R{i}"), 4101 + i)).collect();
            let page = all.iter().skip(skip as usize).take(2).cloned().collect();
            vec![Reply::Rooms { skip, total: 5, rooms: page }]
        });
        let mut c = HubClient::new(addr).unwrap();
        c.request_list();
        let HubEvent::Rooms { rooms, .. } = wait(&mut c) else { panic!() };
        assert_eq!(rooms.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["R0", "R1", "R2", "R3", "R4"]);
        assert_eq!(seen.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn create_succeeds_and_errors_surface() {
        let (addr, _) = fake_hub(|_, req, _| match req {
            Request::Create { name, .. } if name == "Taken" => {
                vec![Reply::Error { code: ErrorCode::NameTaken, text: "nope".into() }]
            }
            Request::Create { name, .. } => vec![Reply::Created { room: room(&name, 4105) }],
            _ => vec![],
        });
        let mut c = HubClient::new(addr).unwrap();
        c.request_create("Taken");
        assert!(matches!(wait(&mut c), HubEvent::Error { code: ErrorCode::NameTaken, ref text, .. } if text == "nope"));
        c.request_create_with("Fresh", true, 10);
        let HubEvent::Created { room, .. } = wait(&mut c) else { panic!() };
        assert_eq!((room.name.as_str(), room.port), ("Fresh", 4105));
    }

    #[test]
    fn replies_from_elsewhere_or_with_another_nonce_do_not_count() {
        let (addr, _) = fake_hub(|_, _, _| vec![]);
        let mut c = HubClient::with_timing(addr, Duration::from_millis(50), Duration::from_millis(300)).unwrap();
        c.request_list();
        let stranger = UdpSocket::bind("127.0.0.1:0").unwrap();
        let local = c.socket.local_addr().unwrap();
        let target: SocketAddr = format!("127.0.0.1:{}", local.port()).parse().unwrap();
        stranger
            .send_to(
                &Reply::Rooms { skip: 0, total: 0, rooms: vec![] }.encode(c.pending.as_ref().unwrap().nonce, 1),
                target,
            )
            .unwrap();
        assert_eq!(wait(&mut c), HubEvent::Timeout, "a datagram from another address is ignored");
    }

    #[test]
    fn silence_retries_then_times_out() {
        let (addr, seen) = fake_hub(|_, _, _| vec![]);
        let mut c = HubClient::with_timing(addr, Duration::from_millis(100), Duration::from_millis(450)).unwrap();
        c.request_list();
        assert_eq!(wait(&mut c), HubEvent::Timeout);
        assert!(seen.load(Ordering::SeqCst) >= 3, "retried: {}", seen.load(Ordering::SeqCst));
        assert!(!c.busy());
        // And a client can be reused afterwards.
        c.request_list();
        assert!(c.busy());
    }

    #[test]
    fn the_client_refuses_ipv6_hubs() {
        assert!(HubClient::new("[::1]:4100".parse().unwrap()).is_err());
    }
}
