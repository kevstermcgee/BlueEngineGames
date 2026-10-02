//! The dedicated Deadfall server: a lobby, then a match, then results, forever.
//!
//!   deadfall-server [--listen ADDR] [--transport development|production] [--join-key KEY]
//!                   [--minutes N | --kills N] [--bots] [--skill 0|1|2] [--auto-start SECONDS] [--report-dir DIR]
//!                   [--status-lines] [--exit-on-stdin-eof]
//!
//! `development` is raw UDP for a LAN or testing; `production` is QUIC/TLS 1.3 with the engine's pinned certificate
//! (use it for the internet, with a join key). The default is 40 kills, no bots: only the people who joined play.
//! Forward UDP port 4100 (or your --listen port) on the router so friends can reach it.
//! `--status-lines` prints `STATUS players=N stage=lobby|playing|results` once a second (the hub reads it);
//! `--exit-on-stdin-eof` makes the server quit when its parent goes away (the hub holds its stdin open).
use deadfall::netgame::DeadfallGame;
use deadfall::sim::{set_settings, EndRule, Settings};
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{server_transport, TransportProfile};
use vesper3d::viewer::netplay::{NetGame, NetServer, ServerConfig, Stage};

fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = ServerConfig {
        participants: 12,
        auto_start_seconds: 30,
        report_dir: Some("deadfall-data".into()),
        ..Default::default()
    };
    cfg.join_key = std::env::var("DEADFALL_JOIN_KEY").ok().filter(|k| !k.is_empty());
    let mut listen = "0.0.0.0:4100".to_string();
    let mut profile = TransportProfile::Development;
    let mut settings = Settings::default();
    let mut status_lines = false;
    let mut exit_on_stdin_eof = false;
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
            "--minutes" => {
                settings.end = EndRule::Time { minutes: value(&mut i, "--minutes")?.parse::<u16>()?.clamp(1, 60) }
            }
            "--kills" => {
                settings.end = EndRule::Kills { target: value(&mut i, "--kills")?.parse::<u16>()?.clamp(1, 500) }
            }
            "--bots" => settings.bots = true,
            "--skill" => settings.bot_skill = value(&mut i, "--skill")?.parse::<u8>()?.min(2),
            "--auto-start" => cfg.auto_start_seconds = value(&mut i, "--auto-start")?.parse()?,
            "--report-dir" => cfg.report_dir = Some(value(&mut i, "--report-dir")?.into()),
            "--status-lines" => status_lines = true,
            "--exit-on-stdin-eof" => exit_on_stdin_eof = true,
            "--help" | "-h" => {
                println!(
                    "{}",
                    include_str!("server.rs")
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
    set_settings(settings);
    println!(
        "[Server] Deadfall on {listen}, {profile} transport, {}, join key {}",
        deadfall::netgame::describe(&settings),
        if cfg.join_key.is_some() { "required" } else { "not required" }
    );
    if profile == TransportProfile::Development && cfg.join_key.is_none() && !listen.starts_with("127.") {
        println!("[Server] Warning: development UDP is unencrypted; use --transport production for the internet");
    }
    let transport = server_transport(profile, &listen)?;
    let mut server = NetServer::<DeadfallGame, _>::new(transport, cfg)?;
    if exit_on_stdin_eof {
        // The hub keeps our stdin open; when it dies (even by SIGKILL) the pipe closes and so do we: no orphans.
        std::thread::spawn(|| {
            let mut sink = [0u8; 256];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut sink), Ok(n) if n > 0) {}
            std::process::exit(0);
        });
    }
    if !status_lines {
        return server.run_realtime(Arc::new(AtomicBool::new(false)), None);
    }
    // The same loop as `run_realtime`, plus one machine-readable line per second for the hub.
    let frame = Duration::from_micros(1_000_000 / <DeadfallGame as NetGame>::TICK_HZ);
    let mut next = Instant::now();
    let mut last_status: Option<Instant> = None;
    loop {
        let now = Instant::now();
        server.poll(now);
        server.step(now);
        if last_status.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
            last_status = Some(Instant::now());
            let stage = match server.stage() {
                Stage::Lobby => "lobby",
                Stage::Match => "playing",
                Stage::Results => "results",
            };
            println!("STATUS players={} stage={stage}", server.players());
        }
        next += frame;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > frame * 30 {
            next = now;
        }
    }
}
