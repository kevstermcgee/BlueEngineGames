//! The dedicated Spooky Kart server: a lobby, then a race, then results, forever.
//!
//!   spooky-kart-server [--listen ADDR] [--transport development|production] [--join-key KEY]
//!                      [--racers N] [--auto-start SECONDS] [--report-dir DIR] [--seed N]
//!
//! `development` is raw UDP for a LAN or testing. `production` is QUIC/TLS 1.3 with the engine's pinned
//! certificate (`BLUE_TLS_KEY_FILE` for the key, `BLUE_TLS_CERT_FILE` for a certificate of your own); use it
//! for anything reachable from the internet, together with a join key (`--join-key` or SPOOKY_KART_JOIN_KEY).
//! Every finished race appends a line to `DIR/races.jsonl`: results, per-character statistics and network
//! quality, the data used to tune the game.
use spooky_kart::server::{KartServer, ServerConfig};
use spooky_kart::transport::server_transport;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use vesper3d::viewer::net::TransportProfile;

fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = ServerConfig { report_dir: Some("spooky-kart-data".into()), ..Default::default() };
    cfg.join_key = std::env::var("SPOOKY_KART_JOIN_KEY").ok().filter(|k| !k.is_empty());
    let mut listen = "0.0.0.0:4100".to_string();
    let mut profile = TransportProfile::Development;
    let mut i = 0;
    let value = |i: &mut usize, flag: &str| -> vesper3d::Result<String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| format!("{flag} needs a value").into())
    };
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => listen = value(&mut i, "--listen")?,
            "--transport" => profile = value(&mut i, "--transport")?.parse()?,
            "--join-key" => cfg.join_key = Some(value(&mut i, "--join-key")?),
            "--racers" => cfg.racers = value(&mut i, "--racers")?.parse::<usize>()?.clamp(1, 8),
            "--auto-start" => cfg.auto_start_seconds = value(&mut i, "--auto-start")?.parse()?,
            "--report-dir" => cfg.report_dir = Some(value(&mut i, "--report-dir")?.into()),
            "--seed" => cfg.seed = Some(value(&mut i, "--seed")?.parse()?),
            "--help" | "-h" => {
                println!(
                    "{}",
                    include_str!("spooky-kart-server.rs")
                        .lines()
                        .take_while(|l| l.starts_with("//"))
                        .map(|l| l.trim_start_matches("//! ").trim_start_matches("//!"))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                return Ok(());
            }
            other => return Err(format!("unknown argument {other} (try --help)").into()),
        }
        i += 1;
    }
    println!(
        "[Server] Selected {profile} transport, up to {} racers, join key {}",
        cfg.racers,
        if cfg.join_key.is_some() { "required" } else { "not required" }
    );
    if profile == TransportProfile::Development && cfg.join_key.is_none() && !listen.starts_with("127.") {
        println!("[Server] Warning: development UDP is unencrypted; use --transport production for the internet");
    }
    let transport = server_transport(profile, &listen)?;
    let mut server = KartServer::new(transport, cfg)?;
    server.run_realtime(Arc::new(AtomicBool::new(false)), None)
}
