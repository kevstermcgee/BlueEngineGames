//! Deadfall online without sockets: the engine's netplay server and real clients on an in-memory network with
//! delay and loss, driven by scripted players. Checks prediction, killcam data, events and snapshot size.
use deadfall::input::{Input, FIRE};
use deadfall::netgame::DeadfallGame;
use deadfall::sim::{set_settings, EndRule, Event, Settings};
use std::net::SocketAddr;
use std::time::Instant;
use vesper3d::viewer::net::loopback::{LoopEnd, LoopNet};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};
// NetGame room settings are process-global, so fixtures in this test process must not race.
static SETTINGS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn addr(port: u16) -> SocketAddr {
    format!("10.0.0.{}:{}", port % 200 + 1, 4000 + port).parse().unwrap()
}

struct World {
    net: LoopNet,
    server: NetServer<DeadfallGame, LoopEnd>,
    clients: Vec<NetClient<DeadfallGame, LoopEnd>>,
    tick: u64,
    kills_seen: Vec<u32>,
    max_error: f32,
}

fn world(latency: u64, jitter: u64, loss: f32, teams: &[u8], bots: bool) -> World {
    set_settings(Settings { end: EndRule::Kills { target: 500 }, bots, bot_skill: 1, ..Default::default() });
    let net = LoopNet::new(latency, jitter, loss, 99);
    let cfg = ServerConfig {
        participants: 12,
        countdown_seconds: 1,
        results_seconds: 2,
        auto_start_seconds: 0,
        seed: Some(5),
        ..Default::default()
    };
    let server = NetServer::new(net.endpoint(addr(0)), cfg).unwrap();
    let clients = teams
        .iter()
        .enumerate()
        .map(|(i, t)| {
            NetClient::new(
                net.endpoint(addr(i as u16 + 1)),
                addr(0),
                ClientConfig { name: format!("Player {i}"), key: String::new(), choice: *t },
            )
            .unwrap()
        })
        .collect();
    World { net, server, clients, tick: 0, kills_seen: vec![0; teams.len()], max_error: 0. }
}

impl World {
    fn step(&mut self) {
        self.tick += 1;
        let now = self.tick as f64 / 60.;
        self.net.advance();
        let wall = Instant::now();
        self.server.poll(wall);
        self.server.step(wall);
        let tick = self.tick;
        for (i, c) in self.clients.iter_mut().enumerate() {
            c.poll(now);
            match c.state().clone() {
                ClientState::Lobby => c.ready(true),
                ClientState::Playing => {
                    c.frame(now, 1. / 60.);
                    for e in c.drain_events() {
                        if let Event::Kill { killer, .. } = e {
                            if killer as usize == c.participant().unwrap_or(99) {
                                self.kills_seen[i] += 1;
                            }
                        }
                    }
                    let view = c.view();
                    let Some(me) = c.participant() else { continue };
                    let render = view.render_tick();
                    let players = view.players_at(render);
                    let mine = players.iter().find(|p| p.slot as usize == me).copied();
                    let enemy = players
                        .iter()
                        .filter(|p| p.slot as usize != me && p.has(deadfall::netgame::flag::ALIVE))
                        .min_by(|a, b| {
                            let m = mine.map_or(a.eye, |m| m.eye);
                            (a.eye - m).length().partial_cmp(&(b.eye - m).length()).unwrap()
                        });
                    let eye = view.eye().unwrap_or(mine.map_or(vesper3d::math::V::ZERO, |m| m.eye));
                    let mut input = Input { seen_tick: deadfall::input::wrapped_tick(render), ..Default::default() };
                    if let Some(e) = enemy {
                        let d = e.eye - eye;
                        input.yaw = d.0.atan2(-d.2);
                        input.pitch = (d.1 - 0.3).atan2((d.0 * d.0 + d.2 * d.2).sqrt());
                        if d.length() > 6. {
                            input.set_axes(if (tick / 50).is_multiple_of(2) { 0.5 } else { -0.5 }, 1.);
                        }
                        if (tick / 4).is_multiple_of(2) {
                            input.buttons |= FIRE;
                        }
                    } else {
                        input.yaw = (tick as f32) * 0.01;
                    }
                    // Keep the press counters of a real client: they only change on presses.
                    let own = view.own.as_ref();
                    if let Some(o) = own {
                        input.reload_seq = o.hands.seen.reload;
                        input.switch_seq = o.hands.seen.switch;
                        input.melee_seq = o.hands.seen.melee;
                        input.use_seq = o.hands.seen.use_;
                        input.drop_seq = o.hands.seen.drop;
                    }
                    let (y, p) = deadfall::input::quantise_angles(input.yaw, input.pitch);
                    input.yaw = y;
                    input.pitch = p;
                    c.tick(input);
                    let err = c.view().error.length();
                    self.max_error = self.max_error.max(err);
                }
                _ => {}
            }
        }
    }
}

#[test]
fn two_players_on_a_laggy_lossy_network_fight_and_the_predicted_body_keeps_up() {
    let _lock = SETTINGS_LOCK.lock().unwrap();
    let mut w = world(3, 1, 4., &[0, 1], false);
    for _ in 0..60 * 50 {
        w.step();
    }
    assert_eq!(w.server.stage(), Stage::Match, "the match started");
    let m = w.server.current().unwrap();
    assert_eq!(m.players.len(), 2, "no bots unless asked for");
    assert!(m.tick > 60 * 40, "the match ran: {}", m.tick);
    let total: u32 = m.players.iter().map(|p| p.kills as u32).sum();
    assert!(total >= 1, "two scripted fighters should kill each other at least once in 50 s: {:?}", m.scores);
    assert!(w.kills_seen.iter().sum::<u32>() >= 1, "kill events reached the clients");
    // Prediction: the local body agrees with the server's.
    for (i, c) in w.clients.iter().enumerate() {
        let me = c.participant().unwrap();
        let server_eye = m.players[me].eye();
        let client_eye = c.view().eye().unwrap();
        // The client is ahead by the inputs the server has not applied yet (a few ticks of walking).
        assert!(
            (server_eye - client_eye).length() < 2.0,
            "client {i}: server {:?} vs predicted {:?}",
            server_eye,
            client_eye
        );
        let stats = c.view().history.len();
        assert!(stats > 100, "the killcam has history: {stats} snapshots");
    }
    assert!(w.max_error < 1.5, "corrections stayed small: {}", w.max_error);
}

#[test]
fn teams_are_balanced_to_six_a_side_and_bots_fill_the_rest_only_when_asked() {
    let _lock = SETTINGS_LOCK.lock().unwrap();
    let mut w = world(2, 0, 0., &[0, 0, 0, 0, 0, 0, 0, 0], false);
    for _ in 0..60 * 4 {
        w.step();
    }
    let m = w.server.current().expect("the match started");
    let ironclad = m.players.iter().filter(|p| p.team == deadfall::Team::Ironclad).count();
    assert_eq!((ironclad, m.players.len()), (4, 8), "eight humans choosing one team are balanced four against four");
    let mut w = world(2, 0, 0., &[0, 1], true);
    for _ in 0..60 * 4 {
        w.step();
    }
    let m = w.server.current().expect("the match started");
    assert_eq!(m.players.len(), 12);
    assert_eq!(m.players.iter().filter(|p| !p.human).count(), 10);
}

#[test]
fn a_client_that_leaves_is_replaced_by_a_bot() {
    let _lock = SETTINGS_LOCK.lock().unwrap();
    let mut w = world(2, 0, 0., &[0, 1], false);
    for _ in 0..60 * 4 {
        w.step();
    }
    w.clients[1].leave();
    for _ in 0..60 * 10 {
        w.step();
    }
    let m = w.server.current().unwrap();
    assert!(!m.players[1].human && m.players[1].bot.is_some());
}
