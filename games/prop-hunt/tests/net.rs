//! Prop Hunt online, without sockets: BlueEngine's netplay server and clients on an in-memory network
//! with delay and loss, each client driven by the bot logic in `prop_hunt::bot`. The generic behaviour
//! (loss, refusals, spoofing, timeouts, telemetry) is tested in the engine; these check what is
//! particular to Prop Hunt — above all, that the per-participant snapshot redaction actually holds up
//! over a real loopback match, not just in a single `snapshot()` call.
use prop_hunt::bot::{seeker_drive, HiderBot};
use prop_hunt::{Input, Outcome, PropHuntGame, HIDE_PHASE_TICKS, MAX_HIDERS, SEEKER_SLOT, SEEK_PHASE_TICKS};
use std::net::SocketAddr;
use std::time::Instant;
use vesper3d::math::V;
use vesper3d::viewer::net::loopback::{LoopEnd, LoopNet};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};

fn addr(port: u16) -> SocketAddr {
    format!("10.0.0.{}:{}", port % 200 + 1, 4000 + port).parse().unwrap()
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

struct World {
    net: LoopNet,
    server: NetServer<PropHuntGame, LoopEnd>,
    clients: Vec<NetClient<PropHuntGame, LoopEnd>>,
    /// One slot's hider bot, built lazily once that client learns which hider slot it drives (a seeker
    /// needs no state: `bot::seeker_drive` is a pure function of the snapshot).
    hider_bots: Vec<Option<HiderBot>>,
    tick: u64,
}

/// `prefs` is each client's lobby choice: `0` prefers hiding, `1` prefers seeking (`PropHuntGame::CHOICES`;
/// several clients may prefer the same role, exactly as a real lobby allows).
fn world(latency: u64, jitter: u64, loss: f32, prefs: &[u8]) -> World {
    let net = LoopNet::new(latency, jitter, loss, 99);
    let server = NetServer::new(net.endpoint(addr(0)), config()).unwrap();
    let clients = prefs
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
    World { net, server, clients, hider_bots: vec![None; prefs.len()], tick: 0 }
}

impl World {
    /// One 60 Hz tick of the whole system. Playing clients are driven by `prop_hunt::bot`.
    fn step(&mut self) {
        self.tick += 1;
        let now = self.tick as f64 / 60.;
        self.net.advance();
        let wall = Instant::now();
        self.server.poll(wall);
        self.server.step(wall);
        for i in 0..self.clients.len() {
            self.clients[i].poll(now);
            if *self.clients[i].state() == ClientState::Playing {
                let participant = self.clients[i].participant();
                let snapshot = self.clients[i].view().snapshot().cloned();
                let input = match (participant, snapshot) {
                    (Some(p), Some(s)) if p == SEEKER_SLOT => seeker_drive(&s),
                    (Some(p), Some(s)) if p < MAX_HIDERS => {
                        let bot = self.hider_bots[i].get_or_insert_with(|| HiderBot::new(11, p));
                        bot.drive(&s, p)
                    }
                    _ => Input::default(),
                };
                self.clients[i].tick(input);
            }
            self.clients[i].frame(now, 1. / 60.);
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

    /// A whole match length, generously bounded: the hide and seek phases plus countdown and margin.
    const MATCH_LIMIT: u64 = HIDE_PHASE_TICKS as u64 + SEEK_PHASE_TICKS as u64 + 2000;

    fn play_a_round(&mut self) {
        assert!(self.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
        for c in &mut self.clients {
            c.ready(true);
        }
        assert!(self.run_until(900, |w| w.server.stage() == Stage::Match), "the round starts");
        assert!(self.run_until(Self::MATCH_LIMIT, |w| w.server.stage() == Stage::Results), "the round ends");
    }
}

#[test]
fn exactly_one_seeker_is_assigned_and_everyone_else_drives_a_hider() {
    let mut w = world(2, 0, 0., &[0, 0, 0, 1]);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    w.run_until(30, |_| false); // let the "you are now playing slot X" message reach every client
    let seekers = w.clients.iter().filter(|c| c.participant() == Some(SEEKER_SLOT)).count();
    assert_eq!(seekers, 1);
    let mut hiders: Vec<usize> = w.clients.iter().filter_map(|c| c.participant()).filter(|&p| p < MAX_HIDERS).collect();
    hiders.sort();
    hiders.dedup();
    assert_eq!(hiders.len(), 3, "the three non-seekers each drive a distinct hider slot");
}

#[test]
fn a_full_round_with_bots_on_both_sides_reaches_a_result() {
    let mut w = world(3, 1, 0., &[0, 0, 0, 1]);
    w.play_a_round();
    let log = w.server.match_log().last().unwrap().clone();
    assert!(log.report["outcome"].is_string(), "{:?}", log.report["outcome"]);
    for p in &log.net.peers {
        println!("{}: rtt {:.0} ms", p.name, p.rtt_ms_mean);
    }
}

/// The single most important correctness property unique to this game: at no point during a real
/// loopback match does an untagged hider's own client ever receive another still-untagged hider's real
/// position or disguise. Checked against the server's ground truth (`server.current()`), tick by tick,
/// for the whole match — not just a single `snapshot()` call in isolation.
#[test]
fn an_untagged_hiders_position_never_reaches_another_untagged_hiders_client() {
    let mut w = world(2, 0, 0., &[0, 0, 0, 1]);
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
        let Some(sim) = w.server.current() else { continue };
        for c in &w.clients {
            let Some(p) = c.participant().filter(|&p| p < MAX_HIDERS) else { continue };
            if sim.hiders[p].tagged {
                continue; // a tagged hider is the open spectator view; nothing left to protect
            }
            let Some(snap) = c.view().snapshot() else { continue };
            for i in 0..MAX_HIDERS {
                if i == p || sim.hiders[i].tagged {
                    continue;
                }
                checked_any = true;
                assert_eq!(snap.hiders[i].pos, V::ZERO, "tick {}: hider {p} saw hider {i}'s real position", w.tick);
                assert_eq!(snap.hiders[i].disguise, None, "tick {}: hider {p} saw hider {i}'s real disguise", w.tick);
                assert!(!snap.hiders[i].confirmed, "tick {}: hider {p} saw hider {i}'s confirmed flag", w.tick);
            }
        }
    }
    assert!(checked_any, "the test never actually exercised the redaction path (nobody was ever untagged-vs-untagged)");
}

#[test]
fn the_seeker_always_sees_every_hiders_real_disguise_once_the_hide_phase_ends() {
    let mut w = world(2, 0, 0., &[0, 0, 0, 1]);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    for _ in 0..(HIDE_PHASE_TICKS as u64 + 100) {
        if w.server.stage() != Stage::Match {
            break;
        }
        w.step();
    }
    let Some(sim) = w.server.current() else { panic!("the match should still be running") };
    let seeker_client = w.clients.iter().find(|c| c.participant() == Some(SEEKER_SLOT)).expect("a seeker is assigned");
    let snap = seeker_client.view().snapshot().expect("the seeker has received a snapshot by now");
    for i in 0..MAX_HIDERS {
        assert_eq!(snap.hiders[i].disguise, sim.hiders[i].disguise, "the seeker must see every hider's real disguise");
    }
}

#[test]
fn a_hider_who_leaves_mid_round_is_marked_tagged_not_replaced_by_an_ai() {
    let mut w = world(2, 0, 0., &[0, 0, 1]);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    w.run_until(100, |_| false);
    let (client_index, hider_slot) = w
        .clients
        .iter()
        .enumerate()
        .find_map(|(i, c)| c.participant().filter(|&p| p < MAX_HIDERS).map(|p| (i, p)))
        .expect("at least one client drives a hider");
    w.clients[client_index].leave();
    w.run_until(30, |_| false);
    assert!(w.server.current().unwrap().hiders[hider_slot].tagged, "a departed hider counts as tagged");
}

#[test]
fn a_seeker_who_leaves_mid_round_ends_it_as_a_hiders_win_not_an_ai_takeover() {
    let mut w = world(2, 0, 0., &[1, 0, 0]);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    w.run_until(50, |_| false);
    let seeker_index =
        w.clients.iter().position(|c| c.participant() == Some(SEEKER_SLOT)).expect("a seeker is assigned");
    w.clients[seeker_index].leave();
    assert!(
        w.run_until(300, |w| w.server.stage() == Stage::Results),
        "the round ends promptly, not after a full timer"
    );
    let log = w.server.match_log().last().unwrap();
    assert_eq!(log.report["outcome"], serde_json::json!("HidersWin"));
    let _ = Outcome::HidersWin; // keep the import meaningful if the assertion above ever changes shape
}

#[test]
fn round_events_reach_every_client_once() {
    let mut w = world(3, 1, 5., &[0, 1]);
    assert!(w.run_until(400, |w| w.clients.iter().all(|c| *c.state() == ClientState::Lobby)));
    for c in &mut w.clients {
        c.ready(true);
    }
    assert!(w.run_until(900, |w| w.server.stage() == Stage::Match));
    let mut seen: Vec<Vec<prop_hunt::Event>> = vec![Vec::new(); w.clients.len()];
    let mut ticks = 0;
    while w.server.stage() != Stage::Results && ticks < World::MATCH_LIMIT {
        w.step();
        ticks += 1;
        for (i, c) in w.clients.iter_mut().enumerate() {
            seen[i].extend(c.drain_events());
        }
    }
    for events in &seen {
        let seek_begins = events.iter().filter(|e| matches!(e, prop_hunt::Event::SeekPhaseBegan)).count();
        assert_eq!(seek_begins, 1, "each client saw the seek phase begin exactly once");
        let round_overs = events.iter().filter(|e| matches!(e, prop_hunt::Event::RoundOver(_))).count();
        assert!(round_overs <= 1);
    }
}
