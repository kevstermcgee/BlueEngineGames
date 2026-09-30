//! The netcode end to end, without sockets: a real server and real clients on an in-memory network with
//! configurable delay and loss, each client driven by the bot logic.
use spooky_kart::bot;
use spooky_kart::client::{ClientConfig, ClientState, KartClient};
use spooky_kart::kart::{Driver, KartInput};
use spooky_kart::server::{KartServer, ServerConfig, Stage};
use spooky_kart::sim::{Event, Phase, LAPS};
use spooky_kart::transport::{LoopEnd, LoopNet};
use spooky_kart::wire::{ClientMsg, ServerMsg};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::DatagramTransport;

fn addr(port: u16) -> SocketAddr {
    format!("10.0.0.{}:{}", port % 200 + 1, 4000 + port).parse().unwrap()
}

struct World {
    net: LoopNet,
    server: KartServer<LoopEnd>,
    clients: Vec<KartClient<LoopEnd>>,
    tick: u64,
}

fn config() -> ServerConfig {
    ServerConfig {
        countdown_seconds: 1,
        results_seconds: 2,
        auto_start_seconds: 0,
        seed: Some(11),
        ..Default::default()
    }
}

fn world(latency: u64, jitter: u64, loss: f32, wanted: &[u8], cfg: ServerConfig) -> World {
    let net = LoopNet::new(latency, jitter, loss, 99);
    let server = KartServer::new(net.endpoint(addr(0)), cfg.clone()).unwrap();
    let clients = wanted
        .iter()
        .enumerate()
        .map(|(i, c)| {
            KartClient::new(
                net.endpoint(addr(i as u16 + 1)),
                addr(0),
                ClientConfig {
                    name: format!("Player {i}"),
                    key: cfg.join_key.clone().unwrap_or_default(),
                    character: *c,
                },
            )
            .unwrap()
        })
        .collect();
    World { net, server, clients, tick: 0 }
}

impl World {
    /// One 60 Hz tick of the whole system. Racing clients are driven by the bot logic.
    fn step(&mut self) {
        self.tick += 1;
        let now = self.tick as f64 / 60.;
        self.net.advance();
        let wall = Instant::now();
        self.server.poll(wall);
        self.server.step(wall);
        for c in &mut self.clients {
            c.poll(now);
            if *c.state() == ClientState::Racing {
                let input = match c.my_kart() {
                    Some(k) if c.sim().karts.len() > k => {
                        let mut i = bot::drive(c.sim(), k);
                        // A human would press the perk only now and then.
                        i.perk = i.perk && self.tick % 7 == 0;
                        i
                    }
                    _ => KartInput::default(),
                };
                c.tick(input);
            }
            c.frame(now, 1. / 60.);
        }
    }

    fn run_until(&mut self, limit: u64, done: impl Fn(&World) -> bool) -> bool {
        for _ in 0..limit {
            if done(self) {
                return true;
            }
            self.step();
        }
        done(self)
    }

    fn all_in_lobby(&self) -> bool {
        self.clients.iter().all(|c| *c.state() == ClientState::Lobby)
    }
}

fn play_a_race(w: &mut World) {
    assert!(w.run_until(300, |w| w.all_in_lobby()), "everyone reaches the lobby");
    for i in 0..w.clients.len() {
        w.clients[i].ready(true);
    }
    assert!(w.run_until(600, |w| w.server.stage() == Stage::Race), "the race starts once everyone is ready");
    assert!(w.run_until(60 * 60 * 5, |w| w.server.stage() == Stage::Results), "the race ends");
}

#[test]
fn players_join_choose_drivers_ready_up_and_race_to_the_end() {
    let mut w = world(3, 1, 0., &[0, 0, 5], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    // Two asked for the Vampire: the second gets another driver, and the lobby says so.
    let lobby = w.clients[0].lobby().unwrap().clone();
    let mut chars: Vec<u8> = lobby.entries.iter().map(|e| e.character).collect();
    chars.sort();
    chars.dedup();
    assert_eq!(chars.len(), 3, "every player has a different driver: {lobby:?}");
    // Change driver, and ready up.
    w.clients[2].select(7);
    w.run_until(30, |_| false);
    assert!(w.clients[0].lobby().unwrap().entries.iter().any(|e| e.character == 7));
    play_a_race(&mut w);
    let log = w.server.race_log().last().expect("the server logged the race").clone();
    let mut places: Vec<u32> = log.race.racers.iter().map(|r| r.place).collect();
    places.sort();
    assert_eq!(places, (1..=8).collect::<Vec<u32>>(), "eight racers, every place taken once");
    assert_eq!(log.race.racers.iter().filter(|r| r.human).count(), 3);
    assert_eq!(log.net.peers.len(), 3);
    // What the clients saw agrees with the server.
    w.run_until(120, |_| false);
    for c in &w.clients {
        assert!(c.stats().snapshots > 100, "snapshots kept arriving");
        assert_eq!(c.stats().snaps, 0, "no hard snaps on a clean network");
        assert!(c.stats().max_error_m < 3., "prediction error stays small: {}", c.stats().max_error_m);
    }
    println!("net: {:?}", log.net.server);
    for p in &log.net.peers {
        println!(
            "peer {} rtt {:.0}/{} ms, repeated {}, skipped {}, out {} B",
            p.name, p.rtt_ms_mean, p.rtt_ms_max, p.stats.ticks_repeated, p.stats.inputs_skipped, p.stats.bytes_out
        );
    }
}

#[test]
fn a_laggy_lossy_network_still_produces_a_finished_race() {
    // 100 ms each way, plenty of jitter, one datagram in ten lost.
    let mut w = world(6, 4, 10., &[1, 2, 3, 4], config());
    play_a_race(&mut w);
    let log = w.server.race_log().last().unwrap().clone();
    assert_eq!(log.race.racers.len(), 8);
    let (sent, dropped) = w.net.counts();
    assert!(dropped > sent / 20, "the network really was lossy: {dropped} of {sent}");
    for p in &log.net.peers {
        assert!(p.stats.inputs_received > 1000, "{} sent inputs", p.name);
        assert!(p.rtt_ms_mean > 60. && p.rtt_ms_mean < 400., "rtt reported: {}", p.rtt_ms_mean);
    }
    // Redundant input bundles mean loss barely costs the server any input.
    let received: u64 = log.net.peers.iter().map(|p| p.stats.inputs_received).sum();
    let repeated: u64 = log.net.peers.iter().map(|p| p.stats.ticks_repeated).sum();
    println!("lossy: inputs {received}, repeated ticks {repeated}, drops {dropped}/{sent}");
    assert!(repeated * 10 < received, "under a tenth of ticks ran on a repeated input");
    for c in &w.clients {
        assert!(c.stats().max_error_m < 12., "prediction stays sane under loss: {}", c.stats().max_error_m);
    }
}

#[test]
fn bandwidth_stays_inside_a_home_connection() {
    let mut w = world(2, 0, 0., &[0, 1, 2, 3], config());
    play_a_race(&mut w);
    let log = w.server.race_log().last().unwrap().clone();
    let seconds = log.race.race_seconds.max(1.);
    for p in &log.net.peers {
        let down = p.stats.bytes_out as f32 / seconds / 1024.;
        let up = p.stats.bytes_in as f32 / seconds / 1024.;
        println!("{}: server to client {down:.1} KB/s, client to server {up:.1} KB/s", p.name);
        assert!(down < 30., "{down:.1} KB/s down");
        assert!(up < 12., "{up:.1} KB/s up");
    }
    assert!(log.net.server.snapshot_bytes_max <= 1200);
    println!("server tick mean {:.0} us max {} us", log.net.server.tick_us_mean, log.net.server.tick_us_max);
}

#[test]
fn a_ninth_player_a_wrong_key_and_a_wrong_version_are_turned_away() {
    let cfg = ServerConfig { join_key: Some("hunter2".into()), ..config() };
    let mut w = world(1, 0, 0., &[0, 1, 2, 3, 4, 5, 6, 7], cfg);
    assert!(w.run_until(400, |w| w.all_in_lobby()));
    // A ninth arrives with the right key.
    let late = w.net.endpoint(addr(50));
    let mut ninth =
        KartClient::new(late, addr(0), ClientConfig { name: "Late".into(), key: "hunter2".into(), character: 0 })
            .unwrap();
    // A stranger with the wrong key, and one with an old build.
    let wrong_key = w.net.endpoint(addr(51));
    let old_build = w.net.endpoint(addr(52));
    wrong_key
        .send(
            addr(0),
            &ClientMsg::Hello {
                key: "nope".into(),
                name: "x".into(),
                character: 0,
                nonce: [1, 1],
                fingerprint: spooky_kart::content_fingerprint(),
            }
            .encode(),
        )
        .unwrap();
    old_build
        .send(
            addr(0),
            &ClientMsg::Hello {
                key: "hunter2".into(),
                name: "x".into(),
                character: 0,
                nonce: [2, 2],
                fingerprint: 12345,
            }
            .encode(),
        )
        .unwrap();
    for i in 0..120 {
        w.step();
        ninth.poll((w.tick + i) as f64 / 60.);
    }
    assert!(matches!(ninth.state(), ClientState::Rejected(r) if r.contains("full")), "{:?}", ninth.state());
    assert_eq!(w.server.players(), 8);
    let mut reasons = Vec::new();
    for (name, mut end) in [("key", wrong_key), ("build", old_build)] {
        for d in end.receive().unwrap() {
            if let Ok(ServerMsg::Rejected { reason }) = ServerMsg::decode(&d.data) {
                reasons.push((name, reason));
            }
        }
    }
    assert!(reasons.iter().any(|(n, r)| *n == "key" && r.contains("key")), "{reasons:?}");
    assert!(reasons.iter().any(|(n, r)| *n == "build" && r.contains("version")), "{reasons:?}");
}

#[test]
fn another_address_cannot_use_a_players_token() {
    let mut w = world(1, 0, 0., &[0, 1], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    let victim = w.clients[0].token().unwrap();
    let attacker = w.net.endpoint(addr(60));
    attacker.send(addr(0), &ClientMsg::Ready { token: victim, ready: true }.encode()).unwrap();
    attacker.send(addr(0), &ClientMsg::Select { token: victim, character: 6 }.encode()).unwrap();
    attacker.send(addr(0), &ClientMsg::Leave { token: victim }.encode()).unwrap();
    w.run_until(60, |_| false);
    let lobby = w.clients[1].lobby().unwrap();
    assert_eq!(lobby.entries.len(), 2, "the victim was not removed");
    assert!(lobby.entries.iter().all(|e| !e.ready && e.character != 6), "nothing was changed: {lobby:?}");
}

#[test]
fn garbage_datagrams_are_counted_and_the_race_carries_on() {
    let mut w = world(2, 0, 0., &[0, 1], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    let junk = w.net.endpoint(addr(70));
    let mut x = 7u64;
    for i in 0..300 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let bytes: Vec<u8> = (0..(x % 300)).map(|j| (x >> (j % 56)) as u8).collect();
        junk.send(addr(0), &bytes).unwrap();
        if i % 50 == 0 {
            junk.send(addr(0), b"SK\x01\x04truncated").unwrap();
        }
    }
    for c in 0..w.clients.len() {
        w.clients[c].ready(true);
    }
    assert!(w.run_until(600, |w| w.server.stage() == Stage::Race));
    assert!(w.run_until(60 * 60 * 5, |w| w.server.stage() == Stage::Results));
    let log = w.server.race_log().last().unwrap();
    assert!(log.net.server.bad_datagrams >= 300 || w.server.race_log().len() == 1, "junk was counted");
}

#[test]
fn a_player_who_leaves_mid_race_is_replaced_by_a_bot_and_the_race_finishes() {
    let mut w = world(2, 0, 0., &[0, 1, 2], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    for i in 0..3 {
        w.clients[i].ready(true);
    }
    assert!(w.run_until(600, |w| w.server.stage() == Stage::Race));
    w.run_until(600, |_| false);
    let leaver_kart = w.clients[1].my_kart().expect("knows its kart");
    w.clients[1].leave();
    w.run_until(30, |_| false);
    let sim = w.server.sim().unwrap();
    assert_eq!(sim.karts[leaver_kart].driver, Driver::Bot, "their kart is now a bot");
    assert_eq!(w.server.players(), 2);
    assert!(w.run_until(60 * 60 * 5, |w| w.server.stage() == Stage::Results));
    let log = w.server.race_log().last().unwrap();
    assert!(log.net.peers.iter().any(|p| p.left_early), "the report records who left");
}

#[test]
fn a_silent_player_times_out_and_is_replaced_by_a_bot() {
    let cfg = ServerConfig { session_timeout: Duration::from_millis(150), ..config() };
    let mut w = world(1, 0, 0., &[0, 1], cfg);
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    for i in 0..2 {
        w.clients[i].ready(true);
    }
    assert!(w.run_until(600, |w| w.server.stage() == Stage::Race));
    w.run_until(120, |_| false);
    // Client 1 goes dark: stop stepping it, let its last datagrams land, then let real time pass.
    let dark = w.clients.pop().unwrap();
    w.run_until(15, |_| false);
    std::thread::sleep(Duration::from_millis(250));
    w.run_until(5, |_| false);
    assert_eq!(w.server.players(), 1, "the silent player was dropped");
    assert!(w.server.sim().unwrap().karts.iter().filter(|k| k.driver == Driver::Human).count() == 1);
    drop(dark);
}

#[test]
fn a_late_joiner_waits_out_the_race_and_is_welcome_afterwards() {
    let mut w = world(1, 0, 0., &[0, 1], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    for i in 0..2 {
        w.clients[i].ready(true);
    }
    assert!(w.run_until(600, |w| w.server.stage() == Stage::Race));
    let late = w.net.endpoint(addr(80));
    let mut newcomer =
        KartClient::new(late, addr(0), ClientConfig { name: "Late".into(), key: String::new(), character: 3 }).unwrap();
    for i in 0..30 {
        w.step();
        newcomer.poll((w.tick + i) as f64 / 60.);
    }
    assert!(matches!(newcomer.state(), ClientState::Rejected(r) if r.contains("race")), "{:?}", newcomer.state());
    assert!(w.run_until(60 * 60 * 5, |w| w.server.stage() == Stage::Results));
    assert!(w.run_until(600, |w| w.server.stage() == Stage::Lobby), "results give way to the lobby");
    // Now a fresh Hello is accepted.
    let again = w.net.endpoint(addr(81));
    let mut second =
        KartClient::new(again, addr(0), ClientConfig { name: "Later".into(), key: String::new(), character: 3 })
            .unwrap();
    for i in 0..90 {
        w.step();
        second.poll((w.tick + i) as f64 / 60.);
    }
    assert_eq!(*second.state(), ClientState::Lobby);
    assert_eq!(w.server.players(), 3);
}

#[test]
fn events_reach_every_client_once() {
    let mut w = world(3, 1, 5., &[0, 1], config());
    assert!(w.run_until(300, |w| w.all_in_lobby()));
    for i in 0..2 {
        w.clients[i].ready(true);
    }
    assert!(w.run_until(600, |w| w.server.stage() == Stage::Race));
    let mut seen: Vec<Vec<Event>> = vec![Vec::new(); 2];
    let mut ticks = 0;
    while w.server.stage() != Stage::Results && ticks < 60 * 60 * 5 {
        w.step();
        ticks += 1;
        for (i, c) in w.clients.iter_mut().enumerate() {
            seen[i].extend(c.drain_events());
        }
    }
    for (i, events) in seen.iter().enumerate() {
        let go = events.iter().filter(|e| matches!(e, Event::Go)).count();
        assert_eq!(go, 1, "client {i} saw the start signal {go} times");
        let counts: Vec<u32> =
            events.iter().filter_map(|e| if let Event::Count(n) = e { Some(*n) } else { None }).collect();
        assert!(
            counts.len() <= 3 && counts.windows(2).all(|p| p[0] > p[1]),
            "countdown in order, at most once each: {counts:?}"
        );
        let mine = w.clients[i].my_kart().unwrap();
        let laps = events.iter().filter(|e| matches!(e, Event::LapDone { kart, .. } if *kart == mine)).count();
        assert!(laps <= LAPS as usize, "client {i} saw {laps} lap events for its own kart");
    }
    w.run_until(40, |_| false); // results keep coming at 10 Hz, well inside the 2 s results window
    assert_eq!(w.clients[0].sim().phase, Phase::Finished);
}
