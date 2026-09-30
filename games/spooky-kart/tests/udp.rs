//! The same server and clients over real UDP sockets on loopback, in real time: a lobby, a start, and the
//! first seconds of a race with prediction running. (A whole race takes two minutes, so this stops early.)
use spooky_kart::bot;
use spooky_kart::kart::KartInput;
use spooky_kart::KartGame;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, server_transport, TransportProfile};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};

#[test]
fn real_udp_sockets_carry_a_lobby_and_the_start_of_a_race() {
    let cfg = ServerConfig { countdown_seconds: 1, auto_start_seconds: 0, seed: Some(5), ..Default::default() };
    let transport = server_transport(TransportProfile::Development, "127.0.0.1:0").expect("bind (UDP must be allowed)");
    let mut server = NetServer::<KartGame, _>::new(transport, cfg).unwrap();
    let address = server.local_addr().unwrap();
    let mut clients: Vec<_> = (0..2u8)
        .map(|i| {
            NetClient::<KartGame, _>::new(
                client_transport(TransportProfile::Development, address).unwrap(),
                address,
                ClientConfig { name: format!("udp {i}"), key: String::new(), choice: i },
            )
            .unwrap()
        })
        .collect();
    let started = Instant::now();
    let frame = Duration::from_micros(16_667);
    let mut next = Instant::now();
    let mut raced_ticks = 0;
    while started.elapsed() < Duration::from_secs(12) && raced_ticks < 360 {
        let now = started.elapsed().as_secs_f64();
        server.poll(Instant::now());
        server.step(Instant::now());
        for c in &mut clients {
            c.poll(now);
            if *c.state() == ClientState::Lobby {
                c.ready(true);
            }
            if *c.state() == ClientState::Playing {
                let input = c
                    .participant()
                    .filter(|k| c.view().sim().karts.len() > *k)
                    .map_or(KartInput::default(), |k| bot::drive(c.view().sim(), k));
                c.tick(input);
            }
            c.frame(now, 1. / 60.);
        }
        if server.stage() == Stage::Match {
            raced_ticks += 1;
        }
        next += frame;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
    assert_eq!(server.stage(), Stage::Match, "the race started over real sockets");
    for c in &clients {
        assert_eq!(*c.state(), ClientState::Playing);
        let stats = c.stats();
        assert!(stats.snapshots > 100, "snapshots arrived: {}", stats.snapshots);
        let sim = c.view().sim();
        assert_eq!(sim.karts.len(), 8);
        let mine = &sim.karts[c.participant().unwrap()];
        assert!(mine.speed() > 5., "the predicted kart is driving: {:.1} m/s", mine.speed());
        assert!(stats.prediction.max_error < 3., "prediction error over UDP: {:.2} m", stats.prediction.max_error);
    }
}
