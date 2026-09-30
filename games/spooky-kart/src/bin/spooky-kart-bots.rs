//! Bot clients for load testing and for filling a server: N bot-driven players join a real server, ready up
//! and race in real time, exactly as people would, using the same client code as the window.
//!
//!   spooky-kart-bots ADDR [--clients N] [--races R] [--transport development|production]
//!                         [--join-key KEY] [--seconds LIMIT]
//!
//! Prints one JSON line when done: how many races completed and each client's network statistics (round-trip
//! time, snapshots, prediction corrections). The server writes the race results to its own races.jsonl.
use spooky_kart::bot;
use spooky_kart::client::{ClientConfig, ClientState, KartClient};
use spooky_kart::kart::KartInput;
use spooky_kart::transport::{client_transport, AnyTransport};
use std::net::ToSocketAddrs;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::TransportProfile;

fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let target = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .ok_or("usage: spooky-kart-bots ADDR [--clients N] [--races R]")?
        .clone();
    let address = target.to_socket_addrs()?.next().ok_or("cannot resolve the server address")?;
    let (mut clients_n, mut races, mut profile, mut key, mut limit) =
        (1usize, 1u32, TransportProfile::Development, String::new(), 900u64);
    let mut i = 1;
    while i < args.len() {
        let value = args.get(i + 1).cloned().ok_or("flag needs a value")?;
        match args[i].as_str() {
            "--clients" => clients_n = value.parse::<usize>()?.clamp(1, 8),
            "--races" => races = value.parse()?,
            "--transport" => profile = value.parse()?,
            "--join-key" => key = value,
            "--seconds" => limit = value.parse()?,
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 2;
    }
    let mut clients: Vec<KartClient<AnyTransport>> = (0..clients_n)
        .map(|n| {
            KartClient::new(
                client_transport(profile, address)?,
                address,
                ClientConfig { name: format!("Bot {}", n + 1), key: key.clone(), character: n as u8 },
            )
        })
        .collect::<Result<_, _>>()?;
    let started = Instant::now();
    let frame = Duration::from_micros(16_667);
    let mut next = Instant::now();
    let mut done = 0u32;
    let mut was_racing = false;
    while done < races && started.elapsed() < Duration::from_secs(limit) {
        let now = started.elapsed().as_secs_f64();
        for c in &mut clients {
            c.poll(now);
            match c.state().clone() {
                ClientState::Lobby => {
                    let ready =
                        c.lobby().and_then(|l| l.entries.iter().find(|e| e.slot == c.seat())).is_some_and(|e| e.ready);
                    if !ready {
                        c.ready(true);
                    }
                }
                ClientState::Racing => {
                    let input = c
                        .my_kart()
                        .filter(|k| c.sim().karts.len() > *k)
                        .map_or(KartInput::default(), |k| bot::drive(c.sim(), k));
                    c.tick(input);
                }
                ClientState::Rejected(why) | ClientState::Disconnected(why) => {
                    eprintln!("bot stopped: {why}");
                    return Err(why.into());
                }
                ClientState::Connecting => {}
            }
            c.frame(now, 1. / 60.);
        }
        let racing = clients.first().is_some_and(|c| *c.state() == ClientState::Racing);
        if was_racing && !racing {
            done += 1;
        }
        was_racing = racing;
        next += frame;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
    let per_client: Vec<_> = clients
        .iter()
        .map(|c| {
            let s = c.stats();
            serde_json::json!({
                "rtt_ms": s.rtt_ms, "snapshots": s.snapshots, "packets_in": s.packets_in, "bytes_in": s.bytes_in,
                "corrections": s.corrections, "snaps": s.snaps, "max_error_m": s.max_error_m,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({ "clients": clients_n, "races_completed": done, "wall_seconds": started.elapsed().as_secs_f32(), "per_client": per_client })
    );
    Ok(())
}
