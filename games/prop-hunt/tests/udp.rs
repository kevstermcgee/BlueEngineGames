//! The same server and clients over real UDP sockets on loopback, in real time: a lobby, a start, and
//! the first seconds of the hide phase with prediction running. (A whole round is well over three
//! minutes, so this stops early — see `tests/net.rs` for a full round on the in-memory network instead.)
use prop_hunt::bot::{seeker_drive, HiderBot};
use prop_hunt::{Input, PropHuntGame, MAX_HIDERS, SEEKER_SLOT};
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, server_transport, TransportProfile};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};

#[test]
fn real_udp_sockets_carry_a_lobby_and_the_start_of_a_round() {
    let cfg = ServerConfig { countdown_seconds: 1, auto_start_seconds: 0, seed: Some(5), ..Default::default() };
    let transport = server_transport(TransportProfile::Development, "127.0.0.1:0").expect("bind (UDP must be allowed)");
    let mut server = NetServer::<PropHuntGame, _>::new(transport, cfg).unwrap();
    let address = server.local_addr().unwrap();
    let prefs = [0u8, 0, 1]; // two hider-preferring, one seeker-preferring
    let mut clients: Vec<_> = prefs
        .iter()
        .enumerate()
        .map(|(i, c)| {
            NetClient::<PropHuntGame, _>::new(
                client_transport(TransportProfile::Development, address).unwrap(),
                address,
                ClientConfig { name: format!("udp {i}"), key: String::new(), choice: *c },
            )
            .unwrap()
        })
        .collect();
    let mut hider_bots: Vec<Option<HiderBot>> = vec![None; clients.len()];
    let started = Instant::now();
    let frame = Duration::from_micros(16_667);
    let mut next = Instant::now();
    let mut playing_ticks = 0;
    while started.elapsed() < Duration::from_secs(12) && playing_ticks < 180 {
        let now = started.elapsed().as_secs_f64();
        server.poll(Instant::now());
        server.step(Instant::now());
        for (i, c) in clients.iter_mut().enumerate() {
            c.poll(now);
            if *c.state() == ClientState::Lobby {
                c.ready(true);
            }
            if *c.state() == ClientState::Playing {
                let input = match (c.participant(), c.view().snapshot()) {
                    (Some(p), Some(s)) if p == SEEKER_SLOT => seeker_drive(s),
                    (Some(p), Some(s)) if p < MAX_HIDERS => {
                        let s = s.clone();
                        let bot = hider_bots[i].get_or_insert_with(|| HiderBot::new(5, p));
                        bot.drive(&s, p)
                    }
                    _ => Input::default(),
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
    assert_eq!(server.stage(), Stage::Match, "the round started over real sockets");
    for c in &clients {
        assert_eq!(*c.state(), ClientState::Playing);
        let stats = c.stats();
        assert!(stats.snapshots > 20, "snapshots arrived: {}", stats.snapshots);
        let snap = c.view().snapshot().expect("a snapshot has arrived by now");
        assert_eq!(snap.hiders.len(), MAX_HIDERS);
    }
    let seekers = clients.iter().filter(|c| c.participant() == Some(SEEKER_SLOT)).count();
    assert_eq!(seekers, 1, "exactly one seeker, even over real sockets");
}
