//! Deadfall over real UDP sockets on loopback: a server and six clients (three a side) play a full-speed match for
//! twenty seconds of wall time, driven by scripted players, with bots filling the rest. Checks that every client
//! stays connected and in step, how much bandwidth a client needs, and that the server keeps its tick rate.
use deadfall::input::{Input, FIRE};
use deadfall::netgame::{flag, DeadfallGame};
use deadfall::sim::{set_settings, EndRule, Settings};
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, server_transport, TransportProfile};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient, NetServer, ServerConfig, Stage};

#[test]
fn six_players_and_six_bots_over_real_udp_stay_in_step_within_a_modest_bandwidth() {
    set_settings(Settings { end: EndRule::Kills { target: 500 }, bots: true, bot_skill: 1, ..Default::default() });
    let cfg = ServerConfig {
        participants: 12,
        countdown_seconds: 1,
        auto_start_seconds: 0,
        seed: Some(3),
        ..Default::default()
    };
    let mut server =
        NetServer::<DeadfallGame, _>::new(server_transport(TransportProfile::Development, "127.0.0.1:0").unwrap(), cfg)
            .unwrap();
    let addr = server.local_addr().unwrap();
    let mut clients: Vec<_> = (0..6)
        .map(|i| {
            NetClient::<DeadfallGame, _>::new(
                client_transport(TransportProfile::Development, addr).unwrap(),
                addr,
                ClientConfig { name: format!("P{i}"), key: String::new(), choice: (i % 2) as u8 },
            )
            .unwrap()
        })
        .collect();
    let start = Instant::now();
    let tick = Duration::from_micros(16_667);
    let mut next = Instant::now();
    let mut ticks = 0u64;
    let mut worst_server_ms = 0f64;
    let mut worst_tick = 0;
    let mut worst_stage = Stage::Lobby;
    while start.elapsed() < Duration::from_secs(20) {
        let t0 = Instant::now();
        server.poll(t0);
        server.step(t0);
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.;
        if elapsed_ms > worst_server_ms {
            worst_server_ms = elapsed_ms;
            worst_tick = ticks;
            worst_stage = server.stage();
        }
        ticks += 1;
        let now = start.elapsed().as_secs_f64();
        for (i, c) in clients.iter_mut().enumerate() {
            c.poll(now);
            match c.state().clone() {
                ClientState::Lobby => c.ready(true),
                ClientState::Playing => {
                    c.frame(now, 1. / 60.);
                    let _ = c.drain_events();
                    let view = c.view();
                    let me = c.participant().unwrap_or(0);
                    let render = view.render_tick();
                    let players = view.players_at(render);
                    let eye = view.eye().unwrap_or_default_v();
                    let enemy = players
                        .iter()
                        .filter(|p| p.slot as usize != me && p.has(flag::ALIVE))
                        .min_by(|a, b| (a.eye - eye).length().partial_cmp(&(b.eye - eye).length()).unwrap());
                    let mut input = Input { seen_tick: deadfall::input::wrapped_tick(render), ..Default::default() };
                    if let Some(e) = enemy {
                        let d = e.eye - eye;
                        input.yaw = d.0.atan2(-d.2);
                        input.pitch = (d.1 - 0.3).atan2((d.0 * d.0 + d.2 * d.2).sqrt());
                        input.set_axes(
                            ((ticks / 40 + i as u64) % 2) as f32 - 0.5,
                            if d.length() > 8. { 1. } else { 0. },
                        );
                        if (ticks / 5).is_multiple_of(2) {
                            input.buttons |= FIRE;
                        }
                    }
                    if let Some(o) = view.own.as_ref() {
                        input.reload_seq = o.hands.seen.reload;
                        input.switch_seq = o.hands.seen.switch;
                        input.melee_seq = o.hands.seen.melee;
                        input.use_seq = o.hands.seen.use_;
                        input.drop_seq = o.hands.seen.drop;
                    }
                    c.tick(input);
                }
                other => assert!(
                    matches!(other, ClientState::Connecting | ClientState::Lobby | ClientState::Playing),
                    "client {i}: {other:?}"
                ),
            }
        }
        next += tick;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        }
    }
    assert_eq!(server.stage(), Stage::Match);
    let m = server.current().unwrap();
    assert_eq!(m.players.len(), 12);
    assert!(ticks > 1000, "the server kept up: {ticks} ticks in 20 s");
    let seconds = start.elapsed().as_secs_f64();
    for (i, c) in clients.iter().enumerate() {
        assert_eq!(*c.state(), ClientState::Playing, "client {i} is still in the match");
        let s = c.stats();
        let kbps = s.bytes_in as f64 * 8. / 1000. / seconds;
        println!(
            "client {i}: {:.0} kbit/s down, {} snapshots, rtt {:.1} ms, corrections {} (max {:.2} m)",
            kbps, s.snapshots, s.rtt_ms, s.prediction.corrections, s.prediction.max_error
        );
        assert!(s.snapshots > 400, "client {i} received {} snapshots", s.snapshots);
        assert!(kbps < 400., "client {i} needs {kbps:.0} kbit/s");
    }
    let kills: u32 = m.players.iter().map(|p| p.kills as u32).sum();
    assert_eq!(server.send_stats().errors, 0);
    assert_eq!(server.send_stats().oversized, 0);
    assert_eq!(server.event_stats().oversized, 0);
    assert!(clients.iter().all(|c| c.event_gaps() == 0));
    println!(
        "server: worst tick {worst_server_ms:.2} ms at {worst_tick} ({worst_stage:?}), {} kills in {:.0} s",
        kills, seconds
    );
    assert!(worst_server_ms < 12., "the server's slowest tick took {worst_server_ms:.1} ms");
}

trait OrZero {
    fn unwrap_or_default_v(self) -> vesper3d::math::V;
}
impl OrZero for Option<vesper3d::math::V> {
    fn unwrap_or_default_v(self) -> vesper3d::math::V {
        self.unwrap_or(vesper3d::math::V::ZERO)
    }
}
