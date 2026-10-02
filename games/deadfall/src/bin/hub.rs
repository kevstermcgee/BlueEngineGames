//! The Deadfall hub: lists and creates rooms so players pick a room by name (no codes, no ip:port).
//!
//!   deadfall-hub [--listen ADDR] [--base-port PORT] [--server-bin PATH] [--public-name NAME]
//!                [--max-rooms N] [--report-dir DIR]
//!
//! The hub answers a small UDP protocol on `--listen` (default 0.0.0.0:4100, or 0.0.0.0:BASE-PORT when only
//! `--base-port` is given). Each room is a `deadfall-server` child process: the permanent Public room on BASE-PORT+1 and
//! up to `--max-rooms` (default 6) player-made rooms on the ports after it, closed again once empty for two minutes.
//! `--server-bin` defaults to `deadfall-server` next to this executable; `--report-dir` (default deadfall-data) gets one
//! `port-N` sub-directory of match reports per room. Forward UDP BASE-PORT to BASE-PORT+1+MAX-ROOMS on the router.
//! Ctrl-C or SIGTERM stops the hub and every room it started.
use deadfall::hub::{self, Hub, Limits, ManagerConfig, ProcessSpawner, DEFAULT_BASE_PORT, MAX_ROOMS_HARD};
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

static STOP: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
mod signals {
    use super::STOP;
    use std::sync::atomic::Ordering;
    extern "C" {
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    }
    extern "C" fn on_signal(_: i32) {
        STOP.store(true, Ordering::SeqCst);
    }
    /// SIGINT and SIGTERM stop the hub (the handler only sets a flag, which is async-signal-safe).
    pub fn install() {
        // SAFETY: `signal` is in the C library std already links; the handler only stores to an atomic.
        unsafe {
            signal(2, on_signal);
            signal(15, on_signal);
        }
    }
}

#[cfg(not(unix))]
mod signals {
    /// No handler: on Windows Ctrl-C ends the hub and the servers notice their stdin close and exit.
    pub fn install() {}
}

fn main() {
    if let Err(e) = run() {
        eprintln!("deadfall-hub: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = ManagerConfig { report_dir: Some("deadfall-data".into()), ..Default::default() };
    let mut listen: Option<String> = None;
    let mut explicit_base = false;
    let mut server_bin: Option<PathBuf> = None;
    let mut i = 0;
    let value = |i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| format!("{flag} needs a value"))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => listen = Some(value(&mut i, "--listen")?),
            "--base-port" => {
                explicit_base = true;
                cfg.base_port =
                    value(&mut i, "--base-port")?.parse().map_err(|_| "--base-port must be a port number")?
            }
            "--server-bin" => server_bin = Some(value(&mut i, "--server-bin")?.into()),
            "--public-name" => cfg.public_name = value(&mut i, "--public-name")?,
            "--max-rooms" => {
                cfg.max_user_rooms =
                    value(&mut i, "--max-rooms")?.parse().map_err(|_| "--max-rooms must be a number")?
            }
            "--report-dir" => cfg.report_dir = Some(value(&mut i, "--report-dir")?.into()),
            "--help" | "-h" => {
                println!(
                    "{}",
                    include_str!("hub.rs")
                        .lines()
                        .take_while(|l| l.starts_with("//"))
                        .map(|l| l.trim_start_matches("//! ").trim_start_matches("//!"))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                return Ok(());
            }
            other => return Err(format!("unknown argument {other} (try --help)")),
        }
        i += 1;
    }
    // The rooms bind the same IP as the hub, and (unless --base-port says otherwise) the hub's port is the base port.
    let listen = match (listen, explicit_base) {
        (Some(l), false) => {
            let addr = parse_listen(&l)?;
            cfg.base_port = addr.port();
            cfg.listen_ip = addr.ip().to_string();
            l
        }
        (Some(l), true) => {
            let addr = parse_listen(&l)?;
            cfg.listen_ip = addr.ip().to_string();
            l
        }
        (None, _) => format!("{}:{}", cfg.listen_ip, cfg.base_port),
    };
    cfg.validate()
        .map_err(|e| format!("{e} (at most {MAX_ROOMS_HARD} rooms, base port default {DEFAULT_BASE_PORT})"))?;
    let server_bin = server_bin.unwrap_or_else(ProcessSpawner::default_server_bin);
    if !server_bin.is_file() {
        return Err(format!("the server binary {} does not exist (use --server-bin)", server_bin.display()));
    }
    let socket = UdpSocket::bind(&listen).map_err(|e| {
        format!("cannot listen on {listen}: {e} (is another server, such as Spooky Kart, using that UDP port?)")
    })?;
    signals::install();
    hub::log(&format!(
        "Deadfall hub on {listen}: Public room \"{}\" on port {}, up to {} more rooms from port {}, servers from {}",
        cfg.public_name,
        cfg.public_port(),
        cfg.max_user_rooms,
        cfg.base_port + 2,
        server_bin.display()
    ));
    let mut hub = Hub::new(cfg, Limits::default(), Box::new(ProcessSpawner { server_bin }));
    hub::serve(&socket, &mut hub, &STOP).map_err(|e| format!("socket error: {e}"))?;
    hub::log("Stopped");
    Ok(())
}

fn parse_listen(l: &str) -> Result<std::net::SocketAddr, String> {
    l.parse::<std::net::SocketAddr>()
        .ok()
        .filter(|a| a.is_ipv4())
        .ok_or_else(|| format!("--listen must be an IPv4 address and port like 0.0.0.0:4100, not {l}"))
}
