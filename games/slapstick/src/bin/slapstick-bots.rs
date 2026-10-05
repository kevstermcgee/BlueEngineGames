//! Bot clients for load testing and for filling a server: N bot-driven players join a real server, ready
//! up and play a match in real time, exactly as people would, using the same client code as the window
//! and the simple AI in `slapstick::bot`.
//!
//!   slapstick-bots ADDR [--clients N] [--matches R] [--transport development|production]
//!                        [--join-key KEY] [--seconds LIMIT]
//!
//! Prints one JSON line when done: how many matches completed and each client's network statistics
//! (round-trip time, snapshots, prediction corrections). The server writes match results to its own
//! matches.jsonl.
use slapstick::{bot, AirHockeyGame};
use std::net::ToSocketAddrs;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, AnyTransport, TransportProfile};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient};

fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let target = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .ok_or("usage: slapstick-bots ADDR [--clients N] [--matches R]")?
        .clone();
    let address = target.to_socket_addrs()?.next().ok_or("cannot resolve the server address")?;
    let (mut clients_n, mut matches, mut profile, mut key, mut limit) =
        (2usize, 1u32, TransportProfile::Development, String::new(), 900u64);
    let mut i = 1;
    while i < args.len() {
        let value = args.get(i + 1).cloned().ok_or("flag needs a value")?;
        match args[i].as_str() {
            "--clients" => clients_n = value.parse::<usize>()?.clamp(1, 2),
            "--matches" => matches = value.parse()?,
            "--transport" => profile = value.parse()?,
            "--join-key" => key = value,
            "--seconds" => limit = value.parse()?,
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 2;
    }
    let mut clients: Vec<NetClient<AirHockeyGame, AnyTransport>> = (0..clients_n)
        .map(|n| {
            NetClient::<AirHockeyGame, _>::new(
                client_transport(profile, address)?,
                address,
                ClientConfig { name: format!("Bot {}", n + 1), key: key.clone(), choice: (n % 4) as u8 },
            )
        })
        .collect::<Result<_, _>>()?;
    let started = Instant::now();
    let frame = Duration::from_micros(16_667);
    let mut next = Instant::now();
    let mut done = 0u32;
    let mut was_playing = false;
    while done < matches && started.elapsed() < Duration::from_secs(limit) {
        let now = started.elapsed().as_secs_f64();
        for c in clients.iter_mut() {
            c.poll(now);
            match c.state().clone() {
                ClientState::Lobby => {
                    let ready =
                        c.lobby().and_then(|l| l.entries.iter().find(|e| e.slot == c.seat())).is_some_and(|e| e.ready);
                    if !ready {
                        c.ready(true);
                    }
                }
                ClientState::Playing => {
                    let input = match c.participant() {
                        Some(slot) if slot < 2 => bot::paddle_ai(c.view().puck(), slot),
                        _ => Default::default(),
                    };
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
        let playing = clients.first().is_some_and(|c| *c.state() == ClientState::Playing);
        if was_playing && !playing {
            done += 1;
        }
        was_playing = playing;
        next += frame;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
    let per_client: Vec<_> = clients
        .iter()
        .map(|c| {
            let s = c.stats();
            serde_json::json!({
                "rtt_ms": s.rtt_ms, "snapshots": s.snapshots, "packets_in": s.packets_in, "bytes_in": s.bytes_in,
                "corrections": s.prediction.corrections, "snaps": s.prediction.snaps, "max_error_m": s.prediction.max_error,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({ "clients": clients_n, "matches_completed": done, "wall_seconds": started.elapsed().as_secs_f32(), "per_client": per_client })
    );
    Ok(())
}
