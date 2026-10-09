//! The engine's shared hub (`be2-hub`, ADR 0037) carrying the REAL `deadfall-server`, on loopback, over both protocols:
//!
//! * `BEHB`, with the engine `HubClient` (what this build's Play Online uses);
//! * the old `DFHB` v1, with an independent hand-written copy of the codec the already-shipped Deadfall clients use
//!   (byte layout taken from `git show 51ca553:games/deadfall/src/hub.rs`, the deleted private hub), plus the shipped
//!   client's decision rule (`build == netgame::fingerprint()`, else it shows its "update the game" message);
//! * two real `NetClient`s joining the Public room over loopback UDP and reaching the lobby.
//!
//! The hub binary is built from the engine checkout, not by this package:
//!
//! ```sh
//! cd ~/BlueEngine && CARGO_TARGET_DIR=~/.cache/be-engine-target cargo build --profile fast --bin be2-hub
//! ```
//!
//! It is found through `BE2_HUB`, else `~/.cache/be-engine-target/{fast,release,debug}/be2-hub`, else `../../../BlueEngine/target/...`;
//! when none exists these tests print why and pass without running (so a checkout without the engine build stays green).
//! Loopback only; ports come from 43000-44999 and are checked free first (never the live hub's 4100-4107). Every process
//! the test starts is killed at the end, and the room servers go with the hub (they exit when their stdin closes).
use deadfall::netgame::DeadfallGame;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, TransportProfile};
use vesper3d::viewer::netplay::hub::wire::{RoomInfo, RoomState};
use vesper3d::viewer::netplay::hub::{local_build, room_addr, HubClient, HubEvent};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetGame};

const SERVER_BIN: &str = env!("CARGO_BIN_EXE_deadfall-server");
/// `netgame::fingerprint()` of the shipped clients and the deployed server (pinned in `netgame.rs` too).
const OLD_SHIPPED_FINGERPRINT: u32 = 0x2BAF_E9C8;
const CURRENT_FINGERPRINT: u32 = 0x6D4F8D2B;

// ---- finding and running the hub ------------------------------------------------------------------------------------

fn hub_binary() -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("BE2_HUB") {
        found.push(p.into());
    }
    if let Some(home) = std::env::var_os("HOME") {
        for profile in ["itest", "fast", "release", "debug"] {
            found.push(Path::new(&home).join(".cache/be-engine-target").join(profile).join("be2-hub"));
        }
    }
    for profile in ["itest", "fast", "release", "debug"] {
        found.push(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../BlueEngine/target").join(profile).join("be2-hub"),
        );
    }
    found.into_iter().find(|p| p.is_file())
}

/// The hub binary, or a printed reason to skip.
fn hub_or_skip(test: &str) -> Option<PathBuf> {
    let bin = hub_binary();
    if bin.is_none() {
        assert!(
            std::env::var_os("CI").is_none() && std::env::var_os("BE2_HUB").is_none(),
            "real engine hub integration is mandatory in CI or when BE2_HUB is set"
        );
        eprintln!(
            "SKIPPED {test}: no be2-hub binary. Build it with: cd ~/BlueEngine && \
             CARGO_TARGET_DIR=~/.cache/be-engine-target cargo build --profile fast --bin be2-hub (or set BE2_HUB)"
        );
    }
    bin
}

/// A run of `n + 1` consecutive loopback UDP ports in 43000-44999 that are free right now (the hub's, then the pool).
fn free_ports(n: u16) -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let seed = (std::process::id() % 40) as u16 * 40;
    for _ in 0..200 {
        let step = NEXT.fetch_add(1, Ordering::SeqCst);
        let base = 43_000 + (seed + step * 24) % 1_900;
        let held: Vec<_> = (0..=n).filter_map(|i| UdpSocket::bind(("127.0.0.1", base + i)).ok()).collect();
        if held.len() == n as usize + 1 {
            return base;
        }
    }
    panic!("no free run of {} loopback ports in 43000-44999", n + 1);
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("deadfall-enginehub-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The registry text: the real `deadfall-server` as game `deadfall`, shaped like `deploy/hub/hub.conf.example` of the engine
/// (Public room on, players may choose `bots` and `kills`), with the pool, rates and caps made test-sized.
fn registry(base: u16, pool: u16, dir: &Path) -> String {
    format!(
        "[hub]\nlisten = 127.0.0.1:{base}\npool_start = {}\npool_size = {pool}\nreport_dir = {}\nlegacy = serve\n\
         rate_burst = 1000\nrate_per_sec = 1000\nmax_rooms_per_ip = 10\n\n\
         [game deadfall]\nserver = {SERVER_BIN}\npublic = on\npublic_name = Public\nuser_set = kills=40\n\
         client_settings = bots,kills,skill,minutes,mode,map,duel,objective\nmax_rooms = 4\nauto_start = 0\n",
        base + 1,
        dir.join("reports").display()
    )
}

struct Hub {
    child: Child,
    addr: SocketAddr,
    pool: std::ops::RangeInclusive<u16>,
}

impl Hub {
    fn start(bin: &Path, dir: &Path, pool: u16) -> Hub {
        let base = free_ports(pool);
        let conf = dir.join("hub.conf");
        std::fs::write(&conf, registry(base, pool, dir)).unwrap();
        let child =
            Command::new(bin).arg("--config").arg(&conf).stdin(Stdio::null()).stdout(Stdio::null()).spawn().unwrap();
        let mut hub = Hub { child, addr: ([127, 0, 0, 1], base).into(), pool: base + 1..=base + pool };
        // Up when the Public room answers a list.
        let mut client = HubClient::new(hub.addr, "deadfall").unwrap();
        let up = eventually(30, || {
            assert!(hub.child.try_wait().unwrap().is_none(), "be2-hub exited early");
            matches!(list(&mut client).first(), Some(r) if r.public)
        });
        assert!(up, "the hub did not list the Public room (is the registry or the server broken?)");
        hub
    }

    /// Pids of the processes the hub started (its children): the room servers.
    fn room_servers(&self) -> Vec<(u32, Vec<String>)> {
        let me = self.child.id();
        let mut out = Vec::new();
        let Ok(dir) = std::fs::read_dir("/proc") else { return out };
        for e in dir.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
            let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else { continue };
            // "pid (comm) S ppid ...": comm may contain spaces, so count from the last ')'.
            let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else { continue };
            if rest.split_whitespace().nth(1).and_then(|p| p.parse::<u32>().ok()) != Some(me) {
                continue;
            }
            let Ok(cmd) = std::fs::read(e.path().join("cmdline")) else { continue };
            let args: Vec<String> =
                cmd.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into()).collect();
            out.push((pid, args));
        }
        out
    }

    /// The argument list of the room server listening on `port`.
    fn server_args(&self, port: u16) -> Vec<String> {
        let want = format!("127.0.0.1:{port}");
        self.room_servers()
            .into_iter()
            .map(|(_, a)| a)
            .find(|a| a.windows(2).any(|w| w[0] == "--listen" && w[1] == want))
            .unwrap_or_else(|| panic!("no room server on {port}"))
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn eventually(secs: u64, mut check: impl FnMut() -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn wait_event(c: &mut HubClient) -> HubEvent {
    let t = Instant::now();
    loop {
        if let Some(e) = c.poll() {
            return e;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "no hub event");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The rooms over BEHB, asserting the hub's build is this game's.
fn list(c: &mut HubClient) -> Vec<RoomInfo> {
    c.request_list();
    match wait_event(c) {
        HubEvent::Rooms { build, rooms } => {
            assert_eq!(build, local_build::<DeadfallGame>(), "the hub reports this game's build");
            rooms
        }
        HubEvent::Timeout => Vec::new(),
        other => panic!("expected rooms, got {other:?}"),
    }
}

// ---- the shipped client's DFHB v1 codec, written out again (independent of the engine's `hub::legacy`) -----------------
//
//   request: "DFHB" version(1)=1 kind(1) nonce(u32 le) payload..., zero-padded to 200 (list) / 96 (create) / 32 (ping)
//   reply:   "DFHB" version(1)=1 kind(1) nonce(u32 le) build(u32 le) payload...
//   kinds:   1 list(skip u8)  2 create(bots u8, kills u16 le, name: len u8 + utf-8)  3 ping
//            0x81 rooms(skip u8, total u8, n u8, n x room)  0x82 created(room)  0x83 error(code u8, text)  0x84 pong
//   room:    name(len u8 + utf-8) players u8, capacity u8, playing u8, port u16 le, public u8

fn old_request(kind: u8, nonce: u32, payload: &[u8], min_len: usize) -> Vec<u8> {
    let mut v = b"DFHB".to_vec();
    v.push(1);
    v.push(kind);
    v.extend_from_slice(&nonce.to_le_bytes());
    v.extend_from_slice(payload);
    if v.len() < min_len {
        v.resize(min_len, 0);
    }
    v
}

fn old_list(nonce: u32, skip: u8) -> Vec<u8> {
    old_request(1, nonce, &[skip], 200)
}

fn old_create(nonce: u32, name: &str, bots: bool, kills: u16) -> Vec<u8> {
    let mut p = vec![bots as u8];
    p.extend_from_slice(&kills.to_le_bytes());
    p.push(name.len() as u8);
    p.extend_from_slice(name.as_bytes());
    old_request(2, nonce, &p, 96)
}

fn old_ping(nonce: u32) -> Vec<u8> {
    old_request(3, nonce, &[], 32)
}

#[derive(Debug, PartialEq)]
struct OldRoom {
    name: String,
    players: u8,
    capacity: u8,
    playing: bool,
    port: u16,
    public: bool,
}

#[derive(Debug, PartialEq)]
enum OldReply {
    Rooms { skip: u8, total: u8, rooms: Vec<OldRoom> },
    Created(OldRoom),
    Error { code: u8, text: String },
    Pong,
}

struct Cur<'a>(&'a [u8]);
impl Cur<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        (self.0.len() >= n).then(|| {
            let (a, b) = self.0.split_at(n);
            self.0 = b;
            a
        })
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
    fn string(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    fn room(&mut self) -> Option<OldRoom> {
        Some(OldRoom {
            name: self.string()?,
            players: self.u8()?,
            capacity: self.u8()?,
            playing: self.u8()? != 0,
            port: self.u16()?,
            public: self.u8()? != 0,
        })
    }
}

/// `(nonce, build, reply)` as the shipped client decodes it; `None` for anything it would silently drop.
fn old_decode(data: &[u8]) -> Option<(u32, u32, OldReply)> {
    if data.len() < 14 || &data[..4] != b"DFHB" || data[4] != 1 {
        return None;
    }
    let kind = data[5];
    let nonce = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    let mut c = Cur(&data[10..]);
    let build = c.u32()?;
    let reply = match kind {
        0x81 => {
            let (skip, total, n) = (c.u8()?, c.u8()?, c.u8()? as usize);
            let mut rooms = Vec::new();
            for _ in 0..n {
                rooms.push(c.room()?);
            }
            OldReply::Rooms { skip, total, rooms }
        }
        0x82 => OldReply::Created(c.room()?),
        0x83 => OldReply::Error { code: c.u8()?, text: c.string()? },
        0x84 => OldReply::Pong,
        _ => return None,
    };
    Some((nonce, build, reply))
}

fn old_ask(sock: &UdpSocket, hub: SocketAddr, request: &[u8], max_reply: usize) -> (u32, u32, OldReply) {
    sock.send_to(request, hub).unwrap();
    let mut buf = [0u8; 2048];
    let (n, from) = sock.recv_from(&mut buf).expect("the hub answered the old protocol");
    assert_eq!(from, hub);
    assert!(n <= max_reply, "a reply of {n} bytes is over the old cap {max_reply}");
    old_decode(&buf[..n]).expect("a reply the shipped client can decode")
}

/// What the shipped client does with a hub's build: it compares it with its raw `netgame::fingerprint()` and shows its
/// "This server runs a different version of Deadfall. Update the game, then try again." message when they differ.
fn old_client_shows_update_message(hub_build: u32) -> bool {
    hub_build != OLD_SHIPPED_FINGERPRINT
}

// ---- the tests --------------------------------------------------------------------------------------------------------

#[test]
fn the_engine_hub_carries_the_real_server_over_behb_and_dfhb_and_two_players_join_the_public_room() {
    let Some(bin) = hub_or_skip("the_engine_hub_carries_the_real_server") else { return };
    let dir = TempDir::new("both");
    let hub = Hub::start(&bin, &dir.0, 4);
    let mut hc = HubClient::new(hub.addr, "deadfall").unwrap();

    // (c) BEHB: the Public room, and the build the engine client compares.
    let rooms = list(&mut hc);
    assert_eq!(rooms.len(), 1, "{rooms:?}");
    let public = rooms[0].clone();
    assert!(public.public && public.name == "Public", "{public:?}");
    assert_eq!((public.capacity, public.state), (12, RoomState::Lobby));
    assert!(hub.pool.contains(&public.port), "the room is on a pool port: {}", public.port);
    assert_ne!(local_build::<DeadfallGame>(), CURRENT_FINGERPRINT, "the BEHB build is not the raw fingerprint");
    assert_eq!(DeadfallGame::fingerprint(), CURRENT_FINGERPRINT);

    // The hub started the real server with the typed settings (never player text) and the supervision flags.
    if cfg!(target_os = "linux") {
        let args = hub.server_args(public.port);
        for want in ["--status-lines", "--exit-on-stdin-eof", "--transport"] {
            assert!(args.iter().any(|a| a == want), "{want} in {args:?}");
        }
        assert!(args.iter().any(|a| a.contains("deadfall-server")), "{args:?}");
    }

    // (d) DFHB v1 from a shipped client's point of view.
    let old = UdpSocket::bind("127.0.0.1:0").unwrap();
    old.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let (nonce, build, reply) = old_ask(&old, hub.addr, &old_ping(77), 32);
    assert_eq!((nonce, build, reply), (77, CURRENT_FINGERPRINT, OldReply::Pong));
    let (nonce, build, reply) = old_ask(&old, hub.addr, &old_list(78, 0), 1000);
    assert_eq!(nonce, 78);
    assert_eq!(build, deadfall::netgame::fingerprint(), "the hub tells old clients the raw fingerprint");
    assert!(old_client_shows_update_message(build), "old clients must ask the player to update for the new protocol");
    let OldReply::Rooms { skip, total, rooms } = reply else { panic!("{reply:?}") };
    assert_eq!((skip, total, rooms.len()), (0, 1, 1));
    assert_eq!(
        rooms[0],
        OldRoom { name: "Public".into(), players: 0, capacity: 12, playing: false, port: public.port, public: true }
    );
    // A reply to a request of the wrong version or game is silence, as before.
    let mut bad = old_list(79, 0);
    bad[4] = 2;
    old.send_to(&bad, hub.addr).unwrap();
    old.set_read_timeout(Some(Duration::from_millis(400))).unwrap();
    assert!(old.recv_from(&mut [0u8; 64]).is_err(), "DFHB version 2 gets no answer");
    old.set_read_timeout(Some(Duration::from_secs(3))).unwrap();

    // (e) Two real clients join the Public room on the hub's host and the room's port, and reach the lobby.
    let server = room_addr(hub.addr, public.port);
    let mut clients: Vec<_> = (0..2)
        .map(|i| {
            NetClient::<DeadfallGame, _>::new(
                client_transport(TransportProfile::Development, server).unwrap(),
                server,
                ClientConfig { name: format!("P{i}"), key: String::new(), choice: i as u8 },
            )
            .unwrap()
        })
        .collect();
    let start = Instant::now();
    let mut in_lobby = false;
    while start.elapsed() < Duration::from_secs(20) && !in_lobby {
        for c in clients.iter_mut() {
            c.poll(start.elapsed().as_secs_f64());
        }
        in_lobby = clients.iter().all(|c| *c.state() == ClientState::Lobby);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        in_lobby,
        "both clients reached the lobby: {:?}",
        clients.iter().map(|c| c.state().clone()).collect::<Vec<_>>()
    );
    // The server prints a status line a second; both protocols show the count.
    let mut seen = (0, 0);
    let found = eventually(15, || {
        for c in clients.iter_mut() {
            c.poll(start.elapsed().as_secs_f64());
        }
        seen.0 = list(&mut hc).first().map_or(0, |r| r.players);
        if let (_, _, OldReply::Rooms { rooms, .. }) = old_ask(&old, hub.addr, &old_list(80, 0), 1000) {
            seen.1 = rooms.first().map_or(0, |r| r.players);
        }
        seen == (2, 2)
    });
    assert!(found, "the Public room reports 2 players over BEHB and DFHB, saw {seen:?}");

    // Everything the test started goes away: the hub, then the room servers behind it.
    drop(clients);
    let ports: Vec<u16> = hub.pool.clone().collect();
    drop(hub);
    for port in ports {
        assert!(eventually(10, || UdpSocket::bind(("127.0.0.1", port)).is_ok()), "port {port} still held by an orphan");
    }
}

#[test]
fn rooms_made_by_a_shipped_client_and_by_an_engine_client_reach_the_server_as_typed_settings() {
    let Some(bin) = hub_or_skip("rooms_made_by_a_shipped_client") else { return };
    let dir = TempDir::new("create");
    let hub = Hub::start(&bin, &dir.0, 6);
    let old = UdpSocket::bind("127.0.0.1:0").unwrap();
    old.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

    // The shipped client's Create{bots, kills, name}: the hub maps it to the settings named `bots` and `kills`
    // (ids 1 and 2 of deadfall-server's --info) and starts the room with `--set`.
    let (nonce, build, reply) = old_ask(&old, hub.addr, &old_create(5, "Old crew", true, 20), 128);
    assert_eq!((nonce, build), (5, CURRENT_FINGERPRINT));
    let OldReply::Created(room) = reply else { panic!("{reply:?}") };
    assert_eq!((room.name.as_str(), room.public, room.capacity), ("Old crew", false, 12));
    if cfg!(target_os = "linux") {
        let args = hub.server_args(room.port);
        let sets: Vec<&str> = args.windows(2).filter(|w| w[0] == "--set").map(|w| w[1].as_str()).collect();
        assert!(sets.contains(&"1=1") && sets.contains(&"2=20"), "bots=1, kills=20 in {args:?}");
        assert!(!args.iter().any(|a| a.contains("Old crew")), "the room name never reaches a command line: {args:?}");
    }

    // The shipped dialog sends only a name: kills 0 means the registry default (user_set kills=40), no bots.
    let (_, _, reply) = old_ask(&old, hub.addr, &old_create(6, "Plain", false, 0), 128);
    let OldReply::Created(plain) = reply else { panic!("{reply:?}") };
    if cfg!(target_os = "linux") {
        let args = hub.server_args(plain.port);
        let sets: Vec<&str> = args.windows(2).filter(|w| w[0] == "--set").map(|w| w[1].as_str()).collect();
        assert!(sets.contains(&"2=40") && !sets.contains(&"1=1"), "{args:?}");
    }

    // An engine client (this build) chooses kills by id and the room appears in both lists.
    let mut hc = HubClient::new(hub.addr, "deadfall").unwrap();
    hc.request_create_with("New crew", &[(deadfall::netgame::SETTING_KILLS, 25), (deadfall::netgame::SETTING_BOTS, 1)]);
    let HubEvent::Created { room: made, build } = wait_event(&mut hc) else { panic!("create failed") };
    assert_eq!(build, local_build::<DeadfallGame>());
    if cfg!(target_os = "linux") {
        let args = hub.server_args(made.port);
        let sets: Vec<&str> = args.windows(2).filter(|w| w[0] == "--set").map(|w| w[1].as_str()).collect();
        assert!(sets.contains(&"2=25") && sets.contains(&"1=1"), "{args:?}");
    }

    let names: Vec<String> = list(&mut hc).into_iter().map(|r| r.name).collect();
    for n in ["Public", "Old crew", "Plain", "New crew"] {
        assert!(names.iter().any(|x| x == n), "{n} listed in {names:?}");
    }
    let (_, _, OldReply::Rooms { rooms, .. }) = old_ask(&old, hub.addr, &old_list(7, 0), 1000) else { panic!() };
    assert_eq!(rooms.len(), 4, "the shipped client lists every Deadfall room");

    // A setting the registry does not let players choose is refused, not passed on.
    hc.request_create_with("Too clever", &[(99, 5)]);
    assert!(matches!(wait_event(&mut hc), HubEvent::Error { .. }), "unknown setting IDs must be refused");
}

#[test]
fn online_duels_wait_for_two_players_and_start_each_mode_on_the_requested_map() {
    let Some(bin) = hub_or_skip("online_duels") else { return };
    let dir = TempDir::new("duels");
    let hub = Hub::start(&bin, &dir.0, 6);
    let mut hc = HubClient::new(hub.addr, "deadfall").unwrap();
    // Public occupies one slot; three private rooms exercise all maps, with a second run for FFA.
    for (mode, map) in [(0, 0), (1, 1), (2, 2)] {
        hc.request_create_with(
            &format!("Cousins {mode}"),
            &[(1, 0), (2, 10), (3, 1), (4, 0), (5, mode), (6, map), (7, 1), (8, 3)],
        );
        let HubEvent::Created { room, build } = wait_event(&mut hc) else { panic!("could not create duel") };
        assert_eq!(build, local_build::<DeadfallGame>());
        // Created may use --info's maximum until the first dynamic STATUS arrives.
        assert!(eventually(10, || list(&mut hc).iter().any(|r| r.port == room.port && r.capacity == 2)));
        let server = room_addr(hub.addr, room.port);
        let start = Instant::now();
        let make = |name: &str, choice: u8| {
            NetClient::<DeadfallGame, _>::new(
                client_transport(TransportProfile::Development, server).unwrap(),
                server,
                ClientConfig { name: name.into(), key: String::new(), choice },
            )
            .unwrap()
        };
        let mut first = make("Kevin", 30);
        assert!(eventually(10, || {
            first.poll(start.elapsed().as_secs_f64());
            *first.state() == ClientState::Lobby
        }));
        first.ready(true);
        // A ready host cannot start a duel while their friend is still connecting.
        for _ in 0..30 {
            first.poll(start.elapsed().as_secs_f64());
            assert_eq!(*first.state(), ClientState::Lobby);
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut second = make("Cousin", 2);
        assert!(eventually(10, || {
            first.poll(start.elapsed().as_secs_f64());
            second.poll(start.elapsed().as_secs_f64());
            *second.state() == ClientState::Lobby
        }));
        let mut third = make("Extra", 0);
        assert!(eventually(10, || {
            first.poll(start.elapsed().as_secs_f64());
            second.poll(start.elapsed().as_secs_f64());
            third.poll(start.elapsed().as_secs_f64());
            matches!(third.state(), ClientState::Rejected(_))
        }));
        second.ready(true);
        assert!(eventually(15, || {
            first.poll(start.elapsed().as_secs_f64());
            second.poll(start.elapsed().as_secs_f64());
            first.view().latest().is_some() && second.view().latest().is_some()
        }));
        for client in [&first, &second] {
            let snapshot = &client.view().latest().unwrap().snap;
            assert_eq!(snapshot.mode as u32, mode);
            assert_eq!(snapshot.map as u32, map);
            assert_eq!(snapshot.players.len(), 2);
            assert_ne!(snapshot.players[0].appearance & 1, snapshot.players[1].appearance & 1);
            let appearance: Vec<_> = snapshot.players.iter().map(|p| p.appearance & 30).collect();
            assert_eq!(appearance, [30, 2]);
            assert_eq!(client.event_gaps(), 0);
        }
        first.leave();
        second.leave();
    }
    drop(hub);
    // Fresh hub permits another private room without relying on idle-process expiry.
    let hub = Hub::start(&bin, &dir.0, 4);
    let mut hc = HubClient::new(hub.addr, "deadfall").unwrap();
    hc.request_create_with("FFA duel", &[(1, 0), (2, 10), (5, 3), (6, 1), (7, 1)]);
    let HubEvent::Created { room, .. } = wait_event(&mut hc) else { panic!("could not create FFA duel") };
    let server = room_addr(hub.addr, room.port);
    let mut clients: Vec<_> = (0..2)
        .map(|i| {
            NetClient::<DeadfallGame, _>::new(
                client_transport(TransportProfile::Development, server).unwrap(),
                server,
                ClientConfig { name: format!("FFA {i}"), key: String::new(), choice: 0 },
            )
            .unwrap()
        })
        .collect();
    let start = Instant::now();
    assert!(eventually(15, || {
        for c in &mut clients {
            c.poll(start.elapsed().as_secs_f64());
            if *c.state() == ClientState::Lobby {
                c.ready(true);
            }
        }
        clients.iter().all(|c| c.view().latest().is_some())
    }));
    assert!(clients.iter().all(|c| c.view().latest().unwrap().snap.mode == deadfall::modes::GameMode::FreeForAll));
}
