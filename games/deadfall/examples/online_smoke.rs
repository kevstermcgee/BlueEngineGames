//! Explicit live-host check: create an empty private duel, join with two clients,
//! start a match, receive snapshots, and leave. Never joins an existing room.
use deadfall::netgame::DeadfallGame;
use std::error::Error;
use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};
use vesper3d::viewer::net::{client_transport, TransportProfile};
use vesper3d::viewer::netplay::hub::{local_build, room_addr, HubClient, HubEvent};
use vesper3d::viewer::netplay::{ClientConfig, ClientState, NetClient};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let address: SocketAddr = std::env::args().nth(1).ok_or("usage: online_smoke HUB_IP:PORT")?.parse()?;
    let build = local_build::<DeadfallGame>();
    let mut hub = HubClient::new(address, "deadfall")?;
    hub.request_list();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match hub.poll() {
            Some(HubEvent::Rooms { build: host_build, .. }) => {
                if host_build != build {
                    return Err(format!("build mismatch: client={build:08x} host={host_build:08x}").into());
                }
                break;
            }
            Some(event) => return Err(format!("room list failed: {event:?}").into()),
            None if Instant::now() >= deadline => return Err("room list timed out".into()),
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
    let name = format!("Version check {}", std::process::id());
    hub.request_create_with(&name, &[(1, 0), (2, 10), (5, 0), (6, 0), (7, 1)]);
    let deadline = Instant::now() + Duration::from_secs(10);
    let room = loop {
        match hub.poll() {
            Some(HubEvent::Created { room, build: host_build }) if host_build == build => break room,
            Some(event) => return Err(format!("room creation failed: {event:?}").into()),
            None if Instant::now() >= deadline => return Err("room creation timed out".into()),
            None => thread::sleep(Duration::from_millis(10)),
        }
    };
    let server = room_addr(address, room.port);
    let mut clients = (0..2)
        .map(|i| {
            NetClient::<DeadfallGame, _>::new(
                client_transport(TransportProfile::Development, server)?,
                server,
                ClientConfig { name: format!("VersionCheck{i}"), key: String::new(), choice: i },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let started = Instant::now();
    let result = (|| -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut ready = false;
        while started.elapsed() < Duration::from_secs(45) {
            for client in &mut clients {
                client.poll(started.elapsed().as_secs_f64());
                if matches!(client.state(), ClientState::Rejected(_) | ClientState::Disconnected(_)) {
                    return Err(format!("join failed: {:?}", client.state()).into());
                }
            }
            if !ready && clients.iter().all(|c| *c.state() == ClientState::Lobby) {
                for client in &mut clients {
                    client.ready(true);
                }
                ready = true;
            }
            if clients.iter().all(|c| c.view().latest().is_some()) {
                for client in &clients {
                    let snapshot = &client.view().latest().unwrap().snap;
                    if snapshot.players.len() != 2 || client.event_gaps() != 0 {
                        return Err("invalid duel snapshot or event gap".into());
                    }
                }
                println!(
                    "PASS build={build:08x}: created private room, two clients joined and received match snapshots"
                );
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        Err("two-player match timed out".into())
    })();
    for client in &mut clients {
        client.leave();
    }
    result
}
