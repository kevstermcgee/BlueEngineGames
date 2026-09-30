//! The same server and clients over real UDP sockets on loopback, in real time: a lobby, a start, and the
//! first seconds of a race with prediction running. (A whole race takes two minutes, so this stops early.)
use spooky_kart::bot;
use spooky_kart::client::{ClientConfig, ClientState, KartClient};
use spooky_kart::kart::KartInput;
use spooky_kart::server::{KartServer, ServerConfig, Stage};
use spooky_kart::transport::{client_transport, server_transport};
use std::time::{Duration, Instant};
use vesper3d::viewer::net::TransportProfile;

#[test]
fn real_udp_sockets_carry_a_lobby_and_the_start_of_a_race() {
    let cfg = ServerConfig { countdown_seconds: 1, auto_start_seconds: 0, seed: Some(5), ..Default::default() };
    let transport = server_transport(TransportProfile::Development, "127.0.0.1:0").expect("bind (UDP must be allowed)");
    let mut server = KartServer::new(transport, cfg).unwrap();
    let address = server.local_addr().unwrap();
    let mut clients: Vec<_> = (0..2u8)
        .map(|i| {
            KartClient::new(
                client_transport(TransportProfile::Development, address).unwrap(),
                address,
                ClientConfig { name: format!("udp {i}"), key: String::new(), character: i },
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
            if *c.state() == ClientState::Racing {
                let input = c
                    .my_kart()
                    .filter(|k| c.sim().karts.len() > *k)
                    .map_or(KartInput::default(), |k| bot::drive(c.sim(), k));
                c.tick(input);
            }
            c.frame(now, 1. / 60.);
        }
        if server.stage() == Stage::Race {
            raced_ticks += 1;
        }
        next += frame;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
    assert_eq!(server.stage(), Stage::Race, "the race started over real sockets");
    for c in &clients {
        assert_eq!(*c.state(), ClientState::Racing);
        assert!(c.stats().snapshots > 100, "snapshots arrived: {}", c.stats().snapshots);
        assert!(c.sim().karts.len() == 8);
        let mine = &c.sim().karts[c.my_kart().unwrap()];
        assert!(mine.speed() > 5., "the predicted kart is driving: {:.1} m/s", mine.speed());
        assert!(c.stats().max_error_m < 3., "prediction error over UDP: {:.2} m", c.stats().max_error_m);
    }
    println!("udp: rtt {:.2} ms", clients[0].stats().rtt_ms);
}
