//! The same server and clients over real UDP sockets on loopback, in real time: a lobby, a start, and a
//! short run of the match with prediction active. A whole match can run to completion quickly (bots
//! trade goals fast), but this test only needs to prove real sockets carry the protocol, so it stops early.
use slapstick::{bot, AirHockeyGame};
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, server_transport, TransportProfile};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};

#[test]
fn real_udp_sockets_carry_a_lobby_and_the_start_of_a_match() {
    let cfg = ServerConfig {
        participants: 2,
        countdown_seconds: 1,
        auto_start_seconds: 0,
        seed: Some(9),
        ..Default::default()
    };
    let transport = server_transport(TransportProfile::Development, "127.0.0.1:0").expect("bind (UDP must be allowed)");
    let mut server = NetServer::<AirHockeyGame, _>::new(transport, cfg).unwrap();
    let address = server.local_addr().unwrap();
    let mut clients: Vec<_> = (0..2)
        .map(|i| {
            NetClient::<AirHockeyGame, _>::new(
                client_transport(TransportProfile::Development, address).unwrap(),
                address,
                ClientConfig { name: format!("udp {i}"), key: String::new(), choice: i as u8 },
            )
            .unwrap()
        })
        .collect();
    let started = Instant::now();
    let frame = Duration::from_micros(16_667);
    let mut next = Instant::now();
    let mut playing_ticks = 0;
    while started.elapsed() < Duration::from_secs(12) && playing_ticks < 180 {
        let now = started.elapsed().as_secs_f64();
        server.poll(Instant::now());
        server.step(Instant::now());
        for c in clients.iter_mut() {
            c.poll(now);
            if *c.state() == ClientState::Lobby {
                c.ready(true);
            }
            if *c.state() == ClientState::Playing {
                let input = match c.participant() {
                    Some(slot) if slot < 2 => bot::paddle_ai(c.view().puck(), slot),
                    _ => Default::default(),
                };
                c.tick(input);
            }
            c.frame(now, 1. / 60.);
        }
        if server.stage() == Stage::Match {
            playing_ticks += 1;
        }
        next += frame;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
    assert_eq!(server.stage(), Stage::Match, "the match started over real sockets");
    for c in &clients {
        assert_eq!(*c.state(), ClientState::Playing);
        let stats = c.stats();
        assert!(stats.snapshots > 20, "snapshots arrived: {}", stats.snapshots);
        assert!(c.view().snapshot().is_some(), "a snapshot has arrived by now");
    }
    let participants: Vec<usize> = clients.iter().filter_map(|c| c.participant()).collect();
    assert_eq!(participants.len(), 2, "both clients were assigned a paddle, even over real sockets");
}
