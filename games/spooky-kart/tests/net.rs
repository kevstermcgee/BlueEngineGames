//! Spooky Kart online, without sockets: BlueEngine's netplay server and clients on an in-memory network with
//! delay and loss, each client driven by the bot logic. The generic behaviour (loss, refusals, spoofing, timeouts,
//! telemetry) is tested in the engine; these check what is particular to karts.
use spooky_kart::bot;
use spooky_kart::kart::{Driver, KartInput};
use spooky_kart::sim::{Event, LAPS};
use spooky_kart::{Character, KartGame, ALL};
use std::net::SocketAddr;
use std::time::Instant;
use vesper3d::viewer::net::loopback::{LoopEnd, LoopNet};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};

fn addr(port: u16) -> SocketAddr {
    format!("10.0.0.{}:{}", port % 200 + 1, 4000 + port).parse().unwrap()
}

struct World {
    net: LoopNet,
    server: NetServer<KartGame, LoopEnd>,
    clients: Vec<NetClient<KartGame, LoopEnd>>,
    tick: u64,
}

fn config() -> ServerConfig {
    ServerConfig {
        participants: 8,
        countdown_seconds: 1,
        results_seconds: 2,
        auto_start_seconds: 0,
        seed: Some(11),
        ..Default::default()
    }
}

fn world(latency: u64, jitter: u64, loss: f32, wanted: &[u8]) -> World {
    let net = LoopNet::new(latency, jitter, loss, 99);
    let server = NetServer::new(net.endpoint(addr(0)), config()).unwrap();
    let clients = wanted
        .iter()
        .enumerate()
        .map(|(i, c)| {
            NetClient::new(
                net.endpoint(addr(i as u16 + 1)),
                addr(0),
                ClientConfig { name: format!("Player {i}"), key: String::new(), choice: *c },
            )
            .unwrap()
        })
        .collect();
    World { net, server, clients, tick: 0 }
}

impl World {
    /// One 60 Hz tick of the whole system. Playing clients are driven by the bot logic.
    fn step(&mut self) {
        self.tick += 1;
        let now = self.tick as f64 / 60.;
        self.net.advance();
        let wall = Instant::now();
        self.server.poll(wall);
        self.server.step(wall);
        for c in &mut self.clients {
            c.poll(now);
            if *c.state() == ClientState::Playing {
                let input = match c.participant() {
                    Some(k) if c.view().sim().karts.len() > k => {
                        let mut i = bot::drive(c.view().sim(), k);
                        i.perk = i.perk && self.tick % 7 == 0; // a human presses the perk now and then
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

    fn race(&mut self) {
        assert!(self.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
        for c in &mut self.clients {
            c.ready(true);
        }
        assert!(self.run_until(900, |w| w.server.stage() == Stage::Match), "the race starts");
        assert!(self.run_until(60 * 60 * 5, |w| w.server.stage() == Stage::Results), "the race ends");
    }
}

#[test]
fn each_player_drives_the_character_they_chose_and_bots_take_the_rest() {
    let mut w = world(2, 0, 0., &[5, 2, 7]);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    w.run_until(30, |_| false);
    let sim = w.server.current().unwrap();
    let mut characters: Vec<usize> = sim.karts.iter().map(|k| k.character.index()).collect();
    characters.sort();
    assert_eq!(characters, (0..8).collect::<Vec<_>>(), "all eight characters race, each once");
    for (client, wanted) in w.clients.iter().zip([5usize, 2, 7]) {
        let kart = &sim.karts[client.participant().expect("assigned a kart")];
        assert_eq!((kart.character.index(), kart.driver), (wanted, Driver::Human));
    }
    assert_eq!(sim.karts.iter().filter(|k| k.driver == Driver::Bot).count(), 5);
}

#[test]
fn a_full_race_with_people_and_bots_finishes_with_good_prediction_and_modest_bandwidth() {
    let mut w = world(3, 1, 0., &[0, 1, 2, 3]);
    w.race();
    let log = w.server.match_log().last().unwrap().clone();
    let racers = log.report["racers"].as_array().unwrap();
    let mut places: Vec<u64> = racers.iter().map(|r| r["place"].as_u64().unwrap()).collect();
    places.sort();
    assert_eq!(places, (1..=8).collect::<Vec<u64>>(), "eight racers, every place taken once");
    assert_eq!(racers.iter().filter(|r| r["human"] == true).count(), 4);
    w.run_until(40, |_| false);
    for c in &w.clients {
        let stats = c.stats();
        assert!(stats.snapshots > 100);
        assert_eq!(stats.prediction.snaps, 0, "no hard snaps on a clean network");
        assert!(stats.prediction.max_error < 3., "prediction error: {}", stats.prediction.max_error);
    }
    let seconds = log.report["race_seconds"].as_f64().unwrap() as f32;
    for p in &log.net.peers {
        let (down, up) = (p.stats.bytes_out as f32 / seconds / 1024., p.stats.bytes_in as f32 / seconds / 1024.);
        println!("{}: {down:.1} KB/s down, {up:.1} KB/s up, rtt {:.0} ms", p.name, p.rtt_ms_mean);
        assert!(down < 30. && up < 12., "{down:.1} down, {up:.1} up");
    }
    println!("server tick mean {:.0} us max {} us", log.net.server.tick_us_mean, log.net.server.tick_us_max);
}

#[test]
fn a_laggy_lossy_network_still_produces_a_finished_race() {
    let mut w = world(6, 4, 10., &[1, 2, 3, 4]);
    w.race();
    let (sent, dropped) = w.net.counts();
    assert!(dropped > sent / 20);
    for c in &w.clients {
        assert!(c.stats().prediction.max_error < 12., "prediction under loss: {}", c.stats().prediction.max_error);
    }
}

#[test]
fn a_player_who_leaves_mid_race_leaves_their_kart_to_the_bots() {
    let mut w = world(2, 0, 0., &[0, 1, 2]);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    w.run_until(600, |_| false);
    let kart = w.clients[1].participant().expect("knows its kart");
    w.clients[1].leave();
    w.run_until(30, |_| false);
    assert_eq!(w.server.current().unwrap().karts[kart].driver, Driver::Bot);
    assert!(w.run_until(60 * 60 * 5, |w| w.server.stage() == Stage::Results));
}

#[test]
fn race_events_reach_every_client_once() {
    let mut w = world(3, 1, 5., &[0, 1]);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
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
        assert_eq!(events.iter().filter(|e| matches!(e, Event::Go)).count(), 1, "client {i} saw the start once");
        let counts: Vec<u32> =
            events.iter().filter_map(|e| if let Event::Count(n) = e { Some(*n) } else { None }).collect();
        assert!(counts.len() <= 3 && counts.windows(2).all(|p| p[0] > p[1]), "countdown in order: {counts:?}");
        let mine = w.clients[i].participant().unwrap();
        let laps = events.iter().filter(|e| matches!(e, Event::LapDone { kart, .. } if *kart == mine)).count();
        assert!(laps <= LAPS as usize);
    }
    let _ = (Character::Vampire, ALL);
}
