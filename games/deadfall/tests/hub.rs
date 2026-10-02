//! The hub over real loopback UDP: a hub thread, the real `deadfall-server` binary as the rooms, a `HubClient` asking for
//! rooms, and two real netplay clients joining the room it made. Loopback only; ports are found at run time.
use deadfall::hub::{
    self, ErrorCode, Hub, Limits, ManagerConfig, ProcessSpawner, Reply, Request, RoomProcess, RoomSettings, RoomSpec,
    RoomState, RoomStatus, Spawner,
};
use deadfall::hub_client::{build_matches, room_addr, HubClient, HubEvent};
use deadfall::netgame::DeadfallGame;
use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, TransportProfile};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient};

const SERVER_BIN: &str = env!("CARGO_BIN_EXE_deadfall-server");

/// A base port whose next `n` ports are free on loopback right now.
fn free_base(n: u16) -> u16 {
    loop {
        let probe = UdpSocket::bind("127.0.0.1:0").unwrap();
        let base = probe.local_addr().unwrap().port();
        if base > 60_000 {
            continue;
        }
        let held: Vec<_> = (1..=n).filter_map(|i| UdpSocket::bind(("127.0.0.1", base + i)).ok()).collect();
        if held.len() == n as usize {
            return base;
        }
    }
}

fn config(base_port: u16, max_user_rooms: usize) -> ManagerConfig {
    ManagerConfig {
        base_port,
        listen_ip: "127.0.0.1".into(),
        max_user_rooms,
        // No countdown from a lone test client, and only the two real clients play.
        public_settings: RoomSettings { bots: false, auto_start: 0, ..Default::default() },
        user_settings: RoomSettings { auto_start: 0, ..Default::default() },
        ..Default::default()
    }
}

/// Limits loose enough for a test that polls the hub many times a second.
fn relaxed() -> Limits {
    Limits { burst: 1000., per_sec: 1000., create_burst: 100., ..Default::default() }
}

struct TestHub {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TestHub {
    fn start(cfg: ManagerConfig, limits: Limits, spawner: impl Spawner + Send + 'static) -> TestHub {
        let base = cfg.base_port;
        let socket = UdpSocket::bind(("127.0.0.1", base)).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut hub = Hub::new(cfg, limits, Box::new(spawner));
            hub::serve(&socket, &mut hub, &flag).unwrap();
        });
        TestHub { addr: ([127, 0, 0, 1], base).into(), stop, thread: Some(thread) }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
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

fn list(c: &mut HubClient) -> Vec<hub::RoomInfo> {
    c.request_list();
    match wait_event(c) {
        HubEvent::Rooms { build, rooms } => {
            assert!(build_matches(build), "the hub reports this build");
            rooms
        }
        other => panic!("expected rooms, got {other:?}"),
    }
}

/// Poll `check` until it holds or `secs` pass.
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

// ---- a spawner that runs real servers but can be told to crash one -------------------------------------------------

#[derive(Clone, Default)]
struct Crash(Arc<Mutex<Vec<u16>>>);
struct Crashable {
    inner: Box<dyn RoomProcess>,
    port: u16,
    crash: Crash,
}
struct CrashSpawner {
    inner: ProcessSpawner,
    crash: Crash,
    spawned: Arc<Mutex<Vec<u16>>>,
}
impl Spawner for CrashSpawner {
    fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>> {
        self.spawned.lock().unwrap().push(spec.port);
        Ok(Box::new(Crashable { inner: self.inner.spawn(spec)?, port: spec.port, crash: self.crash.clone() }))
    }
}
impl RoomProcess for Crashable {
    fn status(&mut self) -> Option<RoomStatus> {
        self.inner.status()
    }
    fn exited(&mut self) -> bool {
        let mut c = self.crash.0.lock().unwrap();
        if let Some(i) = c.iter().position(|p| *p == self.port) {
            c.remove(i);
            self.inner.kill(); // SIGKILL: the server dies without a word
        }
        drop(c);
        self.inner.exited()
    }
    fn kill(&mut self) {
        self.inner.kill();
    }
}

// ---- a spawner with no processes at all ----------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Pretend(Arc<Mutex<HashMap<u16, RoomStatus>>>);
struct PretendProc(u16, Pretend);
impl Spawner for Pretend {
    fn spawn(&mut self, spec: &RoomSpec) -> io::Result<Box<dyn RoomProcess>> {
        Ok(Box::new(PretendProc(spec.port, self.clone())))
    }
}
impl RoomProcess for PretendProc {
    fn status(&mut self) -> Option<RoomStatus> {
        self.1 .0.lock().unwrap().get(&self.0).copied()
    }
    fn exited(&mut self) -> bool {
        false
    }
    fn kill(&mut self) {}
}

// ---- tests -------------------------------------------------------------------------------------------------------------

#[test]
fn a_room_made_through_the_hub_takes_two_real_players_and_reports_them() {
    let base = free_base(4);
    let mut hub = TestHub::start(config(base, 2), relaxed(), ProcessSpawner { server_bin: SERVER_BIN.into() });
    let mut hc = HubClient::new(hub.addr).unwrap();

    let rooms = list(&mut hc);
    assert_eq!(rooms.len(), 1, "{rooms:?}");
    assert!(rooms[0].public && rooms[0].name == "Public" && rooms[0].port == base + 1);

    hc.request_create("Test room");
    let HubEvent::Created { room, build } = wait_event(&mut hc) else { panic!("create failed") };
    assert!(build_matches(build));
    assert_eq!((room.name.as_str(), room.port, room.public), ("Test room", base + 2, false));

    let rooms = list(&mut hc);
    let listed = rooms.iter().find(|r| r.name == "Test room").expect("the new room is listed");
    assert_eq!((listed.port, listed.players, listed.state), (base + 2, 0, RoomState::Lobby));

    // Two real clients join the room's port (the hub's host, the room's port).
    let server = room_addr(hub.addr, room.port);
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

    // The server prints a status line a second; the hub lists the count.
    let mut seen = 0;
    let found = eventually(15, || {
        for c in clients.iter_mut() {
            c.poll(start.elapsed().as_secs_f64());
        }
        seen = list(&mut hc).iter().find(|r| r.name == "Test room").map_or(0, |r| r.players);
        seen == 2
    });
    assert!(found, "the room reports players=2, saw {seen}");

    // Stopping the hub takes every room server with it: the ports are free again.
    hub.stop();
    drop(clients);
    for port in [base + 1, base + 2] {
        assert!(eventually(5, || UdpSocket::bind(("127.0.0.1", port)).is_ok()), "port {port} still held by an orphan");
    }
}

#[test]
fn a_crashed_public_room_is_restarted() {
    let base = free_base(3);
    let crash = Crash::default();
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let spawner = CrashSpawner {
        inner: ProcessSpawner { server_bin: SERVER_BIN.into() },
        crash: crash.clone(),
        spawned: spawned.clone(),
    };
    let cfg = ManagerConfig { public_restart_ms: 300, ..config(base, 1) };
    let hub = TestHub::start(cfg, relaxed(), spawner);
    let mut hc = HubClient::new(hub.addr).unwrap();
    assert_eq!(list(&mut hc).len(), 1);
    crash.0.lock().unwrap().push(base + 1);
    assert!(eventually(10, || spawned.lock().unwrap().len() >= 2), "the hub started Public again");
    assert!(eventually(10, || list(&mut hc).iter().any(|r| r.public)), "and lists it");
    assert_eq!(*spawned.lock().unwrap(), vec![base + 1, base + 1]);
}

#[test]
fn the_server_prints_status_lines_and_exits_when_its_parent_goes_away() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let port = free_base(1);
    let mut child = Command::new(SERVER_BIN)
        .args(["--listen", &format!("127.0.0.1:{port}"), "--status-lines", "--exit-on-stdin-eof", "--auto-start", "0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let status = loop {
        let line = lines.next().expect("the server printed a status").unwrap();
        if let Some(s) = hub::parse_status_line(&line) {
            break s;
        }
    };
    assert_eq!(status, RoomStatus { players: 0, state: RoomState::Lobby });
    drop(child.stdin.take()); // what happens when the hub dies
    assert!(eventually(10, || child.try_wait().unwrap().is_some()), "the server quit when its stdin closed");
}

#[test]
fn a_flood_gets_a_burst_of_answers_and_then_silence_while_others_are_served() {
    let base = free_base(2);
    let hub = TestHub::start(config(base, 1), Limits::default(), Pretend::default());
    let flooder = UdpSocket::bind("127.0.0.1:0").unwrap();
    flooder.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    let packet = Request::List { skip: 0 }.encode(1);
    for _ in 0..60 {
        flooder.send_to(&packet, hub.addr).unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));
    let mut buf = [0u8; 2048];
    let mut answered = 0;
    while flooder.recv_from(&mut buf).is_ok() {
        answered += 1;
    }
    assert!((8..=14).contains(&answered), "about the burst of 10 (plus a refill or two): {answered}");
    // The flooder is bucketed by address, so a second socket on the same IP shares the empty bucket; the HubClient too.
    // A different source address is a different bucket: use the unspecified-to-127.0.0.2 route if the OS allows it.
    if let Ok(other) = UdpSocket::bind("127.0.0.2:0") {
        other.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
        other.send_to(&Request::Ping.encode(2), hub.addr).unwrap();
        let (n, _) = other.recv_from(&mut buf).expect("another address is still served");
        assert!(matches!(Reply::decode(&buf[..n]).unwrap().reply, Reply::Pong));
    }
    // Rates recover.
    std::thread::sleep(Duration::from_millis(1200));
    flooder.send_to(&Request::Ping.encode(3), hub.addr).unwrap();
    flooder.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    assert!(flooder.recv_from(&mut buf).is_ok(), "answered again after a pause");
}

#[test]
fn garbage_gets_no_reply_and_does_not_hurt_the_hub() {
    let base = free_base(2);
    let hub = TestHub::start(config(base, 1), Limits::default(), Pretend::default());
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_millis(400))).unwrap();
    let mut buf = [0u8; 2048];
    for junk in [&b"GET / HTTP/1.1\r\n\r\n"[..], &[0u8; 300][..], b"DFHB", b"DFHB\x01\x01", &[0xFF; 1500][..]] {
        s.send_to(junk, hub.addr).unwrap();
        assert!(s.recv_from(&mut buf).is_err(), "no reply to {} junk bytes", junk.len());
    }
    // A valid request gets through the same socket straight after.
    s.send_to(&Request::Ping.encode(9), hub.addr).unwrap();
    assert!(s.recv_from(&mut buf).is_ok());
}

#[test]
fn the_room_cap_is_enforced_with_a_clear_error() {
    let base = free_base(5);
    let limits = Limits { create_burst: 100., ..Default::default() };
    let hub = TestHub::start(config(base, 3), limits, Pretend::default());
    let mut hc = HubClient::new(hub.addr).unwrap();
    for i in 0..3 {
        hc.request_create(&format!("Room {i}"));
        let HubEvent::Created { room, .. } = wait_event(&mut hc) else { panic!() };
        assert_eq!(room.port, base + 2 + i);
    }
    hc.request_create("One too many");
    let HubEvent::Error { code, text, .. } = wait_event(&mut hc) else { panic!("expected an error") };
    assert_eq!(code, ErrorCode::Full);
    assert!(text.contains("rooms"), "{text}");
    hc.request_create("room 1");
    assert!(matches!(wait_event(&mut hc), HubEvent::Error { code: ErrorCode::Full | ErrorCode::NameTaken, .. }));
    hc.request_create("bad\nname");
    assert!(matches!(wait_event(&mut hc), HubEvent::Error { code: ErrorCode::BadName | ErrorCode::Full, .. }));
    assert_eq!(list(&mut hc).len(), 4, "Public and three rooms");
}

#[test]
fn empty_rooms_are_reaped_but_occupied_ones_and_public_stay() {
    let base = free_base(4);
    let world = Pretend::default();
    let cfg = ManagerConfig { empty_timeout_ms: 1500, ..config(base, 2) };
    let hub = TestHub::start(cfg, relaxed(), world.clone());
    let mut hc = HubClient::new(hub.addr).unwrap();
    for name in ["Empty", "Busy"] {
        hc.request_create(name);
        assert!(matches!(wait_event(&mut hc), HubEvent::Created { .. }));
    }
    world.0.lock().unwrap().insert(base + 3, RoomStatus { players: 1, state: RoomState::Playing });
    world.0.lock().unwrap().insert(base + 2, RoomStatus { players: 0, state: RoomState::Lobby });
    assert!(eventually(10, || list(&mut hc).iter().all(|r| r.name != "Empty")), "the empty room was reaped");
    let rooms = list(&mut hc);
    let names: Vec<_> = rooms.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Public", "Busy"]);
    assert_eq!((rooms[1].players, rooms[1].state), (1, RoomState::Playing));
    // The freed port is handed to the next room.
    hc.request_create("Next");
    let HubEvent::Created { room, .. } = wait_event(&mut hc) else { panic!() };
    assert_eq!(room.port, base + 2);
}
