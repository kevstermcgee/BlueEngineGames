//! Slapstick online, without sockets: BlueEngine's netplay server and clients on an in-memory network
//! with delay and loss, each client driven by `slapstick::bot::paddle_ai`. The generic behaviour (loss,
//! refusals, spoofing, timeouts, telemetry) is tested in the engine; these check what is particular to
//! Slapstick — above all, that both clients' snapshots are genuinely identical over a real loopback
//! match, the mirror image of Prop Hunt's "never leaks" test: here the property is "never differs".
use slapstick::{bot, AirHockeyGame, Event};
use std::net::SocketAddr;
use std::time::Instant;
use vesper3d::viewer::net::loopback::{LoopEnd, LoopNet};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};

fn addr(port: u16) -> SocketAddr {
    format!("10.0.1.{}:{}", port % 200 + 1, 4000 + port).parse().unwrap()
}

fn config() -> ServerConfig {
    ServerConfig {
        participants: 2,
        countdown_seconds: 1,
        results_seconds: 2,
        auto_start_seconds: 0,
        seed: Some(21),
        ..Default::default()
    }
}

struct World {
    net: LoopNet,
    server: NetServer<AirHockeyGame, LoopEnd>,
    clients: Vec<NetClient<AirHockeyGame, LoopEnd>>,
    tick: u64,
}

fn world(latency: u64, jitter: u64, loss: f32, n: usize) -> World {
    let net = LoopNet::new(latency, jitter, loss, 7);
    let server = NetServer::new(net.endpoint(addr(0)), config()).unwrap();
    let clients = (0..n)
        .map(|i| {
            NetClient::new(
                net.endpoint(addr(i as u16 + 1)),
                addr(0),
                ClientConfig { name: format!("Player {i}"), key: String::new(), choice: i as u8 },
            )
            .unwrap()
        })
        .collect();
    World { net, server, clients, tick: 0 }
}

impl World {
    /// One 60 Hz tick of the whole system. Playing clients are driven by `slapstick::bot`.
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
                    Some(slot) if slot < 2 => bot::paddle_ai(c.view().puck(), slot),
                    _ => Default::default(),
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

    /// A whole match length, generously bounded: two players' bots trading goals to 7, plus countdown.
    const MATCH_LIMIT: u64 = 60 * 60 * 10; // ten minutes of ticks

    fn play_a_match(&mut self) {
        assert!(self.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
        for c in &mut self.clients {
            c.ready(true);
        }
        assert!(self.run_until(900, |w| w.server.stage() == Stage::Match), "the match starts");
        assert!(self.run_until(Self::MATCH_LIMIT, |w| w.server.stage() == Stage::Results), "the match ends");
    }
}

#[test]
fn two_bot_clients_play_a_full_match_to_a_result() {
    let mut w = world(3, 1, 0., 2);
    w.play_a_match();
    let log = w.server.match_log().last().unwrap().clone();
    assert!(log.report["winner"].is_number() || log.report["winner"].is_null());
    for p in &log.net.peers {
        println!("{}: rtt {:.0} ms", p.name, p.rtt_ms_mean);
    }
}

/// This game's key networking invariant, the mirror image of Prop Hunt's "never leaks" test: for every
/// tick of a loopback match, both clients' own received snapshot must be byte-identical (paddles, puck,
/// score, phase all equal) — there is no hidden information in air hockey.
#[test]
fn both_clients_snapshot_identical_state_every_tick() {
    let mut w = world(2, 0, 0., 2);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    let mut checked_any = false;
    for _ in 0..World::MATCH_LIMIT {
        w.step();
        if w.server.stage() != Stage::Match {
            break;
        }
        let snaps: Vec<_> = w.clients.iter().filter_map(|c| c.view().snapshot()).collect();
        if snaps.len() == 2 {
            checked_any = true;
            assert_eq!(snaps[0], snaps[1], "tick {}: the two clients' snapshots differ", w.tick);
        }
    }
    assert!(checked_any, "the test never actually compared two live snapshots");
}

#[test]
fn a_player_who_leaves_mid_match_is_taken_over_by_the_bot_ai_and_the_match_continues() {
    let mut w = world(2, 0, 0., 2);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    w.run_until(60, |_| false);
    w.clients[0].leave();
    w.run_until(30, |_| false);
    let sim = w.server.current().expect("the match should still be running with one bot-driven paddle");
    assert!(sim.paddles[0].ai, "the departed player's paddle is now driven by the bot");
    // The match keeps going and still reaches a result (nobody is stuck waiting on a dead seat).
    assert!(w.run_until(World::MATCH_LIMIT, |w| w.server.stage() == Stage::Results), "the match still finishes");
}

#[test]
fn round_events_reach_every_client_once() {
    let mut w = world(3, 1, 5., 2);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    let mut seen: Vec<Vec<Event>> = vec![Vec::new(); w.clients.len()];
    let mut ticks = 0;
    while w.server.stage() != Stage::Results && ticks < World::MATCH_LIMIT {
        w.step();
        ticks += 1;
        for (i, c) in w.clients.iter_mut().enumerate() {
            seen[i].extend(c.drain_events());
        }
    }
    for events in &seen {
        let game_overs = events.iter().filter(|e| matches!(e, Event::GameOver { .. })).count();
        assert!(game_overs <= 1, "a client saw GameOver more than once: {game_overs}");
        assert!(!events.is_empty(), "a client saw no events across a whole match");
    }
}
