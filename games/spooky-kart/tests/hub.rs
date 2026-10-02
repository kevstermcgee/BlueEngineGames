//! Spooky Kart on the real hub, over loopback UDP: the engine's `be2-hub` binary starts the real
//! `spooky-kart-server` as rooms; a `HubClient` lists them, makes a named one, and two real `NetClient`s (raw UDP, the
//! way the Play Online screen joins) reach the lobby of that room, and one reaches the Public room.
//!
//! Needs the hub binary, which is the engine's and is not built by this repository: set `BE2_HUB_BIN` to it, or build it
//! (`cd ../BlueEngine && cargo build --profile fast --bin be2-hub`; the usual target directories are searched). Without
//! it the test prints why and passes, so a checkout that has no engine binary still goes green.
//! Ports come from 43000-44999 and are checked free first; nothing else (the live hub's 4100-4107 included) is touched,
//! and every process this test starts is stopped at the end.
use spooky_kart::KartGame;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, TransportProfile};
use vesper3d::viewer::netplay::hub::wire::{RoomInfo, RoomState};
use vesper3d::viewer::netplay::hub::{local_build, room_addr, HubClient, HubEvent};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient};

const SERVER: &str = env!("CARGO_BIN_EXE_spooky-kart-server");
const GAME: &str = "spooky-kart";

fn hub_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BE2_HUB_BIN").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(t) = std::env::var_os("BE2_TARGET_DIR") {
        roots.push(t.into());
    }
    roots.push(manifest.join("../BlueEngine/target"));
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(Path::new(&home).join(".cache/be-engine-target"));
    }
    roots.iter().flat_map(|r| ["fast", "release", "debug"].map(|p| r.join(p).join("be2-hub"))).find(|p| p.is_file())
}

/// A run of `n + 1` consecutive loopback UDP ports in 43000-44999 that are free right now.
fn free_ports(n: u16) -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let seed = (std::process::id() % 40) as u16 * 40;
    for _ in 0..200 {
        let base = 43_000 + (seed + NEXT.fetch_add(1, Ordering::SeqCst) * 24) % 1_900;
        let held: Vec<_> = (0..=n).filter_map(|i| UdpSocket::bind(("127.0.0.1", base + i)).ok()).collect();
        if held.len() == n as usize + 1 {
            return base;
        }
    }
    panic!("no free run of {} loopback ports in 43000-44999", n + 1);
}

/// The hub process, stopped (SIGTERM, then SIGKILL) when dropped. Its room servers exit when it does.
struct Hub {
    child: Child,
    dir: PathBuf,
}

impl Hub {
    fn stop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(None)) {
            return; // already gone
        }
        #[cfg(unix)]
        {
            let _ = Command::new("kill").args(["-TERM", &self.child.id().to_string()]).status();
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until && matches!(self.child.try_wait(), Ok(None)) {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn wait_event(c: &mut HubClient) -> HubEvent {
    let t = Instant::now();
    loop {
        if let Some(e) = c.poll() {
            return e;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "no answer from the hub");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn list(c: &mut HubClient) -> (u32, Vec<RoomInfo>) {
    c.request_list();
    match wait_event(c) {
        HubEvent::Rooms { build, rooms } => (build, rooms),
        other => panic!("expected the room list, got {other:?}"),
    }
}

fn join(addr: SocketAddr, name: &str, choice: u8) -> NetClient<KartGame, vesper3d::viewer::net::AnyTransport> {
    NetClient::new(
        client_transport(TransportProfile::Development, addr).unwrap(),
        addr,
        ClientConfig { name: name.into(), key: String::new(), choice },
    )
    .unwrap()
}

type Client = NetClient<KartGame, vesper3d::viewer::net::AnyTransport>;

/// Poll `clients` in real time until `done`, within `secs`.
fn run_until(clients: &mut [&mut Client], secs: u64, done: impl Fn(&[&mut Client]) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        let now = start.elapsed().as_secs_f64();
        for c in clients.iter_mut() {
            c.poll(now);
            c.frame(now, 1. / 60.);
        }
        if done(clients) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn the_real_hub_lists_the_public_room_makes_a_named_one_and_two_clients_reach_its_lobby() {
    let Some(hub_bin) = hub_binary() else {
        eprintln!(
            "SKIPPED the_real_hub_...: no be2-hub binary. Build it (cd ../BlueEngine && cargo build --profile fast --bin \
             be2-hub) or set BE2_HUB_BIN=/path/to/be2-hub."
        );
        return;
    };
    let pool = 6;
    let base = free_ports(pool);
    let dir = std::env::temp_dir().join(format!("sk-hub-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The registry a real box would have: the spooky-kart-server of this build as the game "spooky-kart", with a Public room.
    let conf = format!(
        "[hub]\nlisten = 127.0.0.1:{base}\npool_start = {}\npool_size = {pool}\nreport_dir = {}\nmax_rooms_per_ip = 10\n\
         rate_burst = 1000\nrate_per_sec = 1000\n\n[game {GAME}]\nserver = {SERVER}\npublic = on\nmax_rooms = 3\nauto_start = 0\n",
        base + 1,
        dir.join("reports").display()
    );
    std::fs::write(dir.join("hub.conf"), conf).unwrap();
    let log = std::fs::File::create(dir.join("hub.log")).unwrap();
    let child = Command::new(&hub_bin)
        .args(["--config"])
        .arg(dir.join("hub.conf"))
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .expect("start be2-hub");
    let mut hub = Hub { child, dir: dir.clone() };
    let hub_addr: SocketAddr = ([127, 0, 0, 1], base).into();
    let mut client = HubClient::new(hub_addr, GAME).unwrap();

    // The hub is up once it answers; the Public room is started with it.
    let started = Instant::now();
    let (build, rooms) = loop {
        client.request_list();
        match wait_event(&mut client) {
            HubEvent::Rooms { build, rooms } if !rooms.is_empty() => break (build, rooms),
            _ if started.elapsed() < Duration::from_secs(15) => std::thread::sleep(Duration::from_millis(200)),
            other => panic!(
                "the hub never listed a room: {other:?}\n{}",
                std::fs::read_to_string(dir.join("hub.log")).unwrap_or_default()
            ),
        }
    };
    assert_eq!(build, local_build::<KartGame>(), "the hub reports this game's build");
    let public = rooms.iter().find(|r| r.public).expect("a Public room");
    assert_eq!((public.name.as_str(), public.capacity, public.state), ("Public", 8, RoomState::Lobby));
    assert!((base + 1..base + 1 + pool).contains(&public.port), "rooms use the pool, not the hub port 4100");

    // A named room, with the rivals set to Hard (setting 1 of the game's own schema).
    client.request_create_with("Test friends", &[(spooky_kart::netgame::SETTING_DIFFICULTY, 2)]);
    let room = match wait_event(&mut client) {
        HubEvent::Created { build, room } => {
            assert_eq!(build, local_build::<KartGame>());
            room
        }
        other => panic!("expected Created, got {other:?}"),
    };
    assert_eq!((room.name.as_str(), room.public, room.capacity), ("Test friends", false, 8));
    assert_ne!(room.port, public.port);
    let (_, rooms) = list(&mut client);
    assert!(rooms.iter().any(|r| r.name == "Test friends") && rooms.iter().any(|r| r.public), "{rooms:?}");

    // Two real clients join the named room over raw UDP, and one more joins the Public room.
    let mut a = join(room_addr(hub_addr, room.port), "Ann", 0);
    let mut b = join(room_addr(hub_addr, room.port), "Bob", 1);
    let mut p = join(room_addr(hub_addr, public.port), "Pat", 2);
    let both_in = run_until(&mut [&mut a, &mut b, &mut p], 15, |cs| {
        cs.iter().all(|c| *c.state() == ClientState::Lobby)
            && cs[0].lobby().is_some_and(|l| l.entries.len() == 2)
            && cs[2].lobby().is_some_and(|l| l.entries.len() == 1)
    });
    let states: Vec<_> = [&a, &b, &p].iter().map(|c| c.state().clone()).collect();
    assert!(
        both_in,
        "clients reached the lobbies: {states:?}\n{}",
        std::fs::read_to_string(dir.join("hub.log")).unwrap_or_default()
    );
    let names: Vec<String> = a.lobby().unwrap().entries.iter().map(|e| e.name.clone()).collect();
    assert!(names.contains(&"Ann".to_string()) && names.contains(&"Bob".to_string()), "{names:?}");
    // The hub learns player counts from each server's once-a-second status line.
    let until = Instant::now() + Duration::from_secs(10);
    let counts = loop {
        let (_, rooms) = list(&mut client);
        let n = |name: &str| rooms.iter().find(|r| r.name == name).map(|r| r.players);
        let counts = (n("Test friends"), n("Public"));
        if counts == (Some(2), Some(1)) || Instant::now() > until {
            break counts;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    assert_eq!(counts, (Some(2), Some(1)), "the hub shows who is in each room");

    // Leave politely, then stop everything this test started and prove the ports are free again.
    for c in [&mut a, &mut b, &mut p] {
        c.leave();
    }
    drop((a, b, p, client));
    hub.stop();
    let freed = (base..base + 1 + pool).all(|port| {
        (0..50).any(|_| {
            UdpSocket::bind(("127.0.0.1", port)).is_ok() || {
                std::thread::sleep(Duration::from_millis(100));
                false
            }
        })
    });
    assert!(freed, "the hub and its rooms are gone and their ports are free");
}
