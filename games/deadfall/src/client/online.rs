//! Play Online: the rules and the state machine now live in the engine (`netplay::hub::client`, ADR 0037); this file is
//! what is Deadfall's own. The screens in `app.rs` draw that state and feed it time.
//!
//! * the build id this game shows and compares, with the `DEADFALL_FAKE_BUILD` switch for looking at the version-mismatch
//!   screen without a second binary ([`local_build`]);
//! * the words ([`UPDATE_TEXT`], [`failure_message`]) and the layout rule [`visible_rows`], which depend on how the menu
//!   kit in `ui.rs` is drawn.
//!
//! Flow, as before: [`Online::new`] resolves the hub's name on a worker thread, asks it for this game's rooms every
//! [`REFRESH_EVERY`] seconds, and [`Online::update`] says when to join a room ([`Action::Join`]).
use crate::netgame::DeadfallGame;
use vesper3d::viewer::netplay::{hub, NetGame};

pub use hub::client::{
    build_label, order_rooms, room_status, scroll_to, Action, DialogPhase, JoinWait, Online, Pane, RetryStep,
    REFRESH_EVERY,
};
pub use vesper3d::viewer::netplay::ConnectFailure;

/// The id this game is listed under on a hub (and `NetGame::NAME`).
pub const GAME_ID: &str = <DeadfallGame as NetGame>::NAME;

/// What to tell the player when the hub's build is not this game's.
pub const UPDATE_TEXT: &str = "This server runs a different version of Deadfall. Update the game, then try again.";

/// The build id this game shows and compares: [`hub::local_build`]. `DEADFALL_FAKE_BUILD=<number>` pretends to be
/// another build, so a version mismatch can be looked at without a second binary.
pub fn local_build() -> u32 {
    build_from(std::env::var("DEADFALL_FAKE_BUILD").ok().as_deref())
}

/// [`local_build`] with the environment value given.
pub fn build_from(fake: Option<&str>) -> u32 {
    fake.and_then(|v| v.trim().parse().ok()).unwrap_or_else(hub::local_build::<DeadfallGame>)
}

/// Open the Play Online screen's state for the hub at `spec` (`host`, `host:port`).
pub fn open(spec: &str, now: f64) -> Online {
    Online::new(spec, GAME_ID, local_build(), now)
}

/// What to show for a failed connection. `room` is true when the address came from the room list, where the player never
/// typed it and port forwarding is not their problem.
pub fn failure_message(failure: &ConnectFailure, room: bool) -> String {
    hub::client::connect_failure_message(failure, room)
}

/// How many list rows to show at once for a window `height_px` tall at `ui` scale; the rest scroll.
pub fn visible_rows(height_px: f32, ui: f32) -> usize {
    let free = height_px / ui - 215. - 4. * 46. - 23. - 36.;
    ((free / 46.).floor() as i32).clamp(2, 10) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use hub::wire::{Reply, Request, RoomInfo, RoomState};
    use std::net::{SocketAddr, UdpSocket};
    use std::time::{Duration, Instant};

    #[test]
    fn the_fake_build_switch_replaces_the_build_and_nonsense_is_ignored() {
        let real = hub::local_build::<DeadfallGame>();
        assert_eq!(build_from(None), real);
        assert_eq!(build_from(Some("7")), 7);
        assert_eq!(build_from(Some(" 12 ")), 12);
        assert_eq!(build_from(Some("lots")), real);
        assert_ne!(real, <DeadfallGame as NetGame>::fingerprint(), "the BEHB build is not the raw fingerprint");
    }

    #[test]
    fn the_list_window_fits_the_screen() {
        assert!(visible_rows(720., 1.) >= 4);
        assert_eq!(visible_rows(100., 0.55), 2);
        assert!(visible_rows(5000., 2.4) <= 10);
    }

    #[test]
    fn the_update_text_is_the_engines() {
        assert_eq!(UPDATE_TEXT, hub::client::update_message("Deadfall"));
    }

    #[test]
    fn a_room_failure_reads_as_before_and_a_running_match_is_the_one_to_retry() {
        let addr: SocketAddr = "203.0.113.5:4102".parse().unwrap();
        let f = ConnectFailure::Unreachable { addr, waited_secs: 8 };
        let m = failure_message(&f, true);
        assert!(m.starts_with("No reply from the room at 203.0.113.5:4102 after 8 s."), "{m}");
        assert!(failure_message(&f, false).contains("after 8 s"));
        assert!(hub::client::should_retry_join(&ConnectFailure::MatchInProgress));
        assert!(!hub::client::should_retry_join(&ConnectFailure::WrongKey));
    }

    /// A BEHB hub on loopback that lists two rooms.
    fn fake_hub(build: u32) -> SocketAddr {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        socket.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            for _ in 0..400 {
                let Ok((n, from)) = socket.recv_from(&mut buf) else { continue };
                if let Some((nonce, Request::List { game, .. })) = Request::decode(&buf[..n]) {
                    assert_eq!(game, "deadfall");
                    let room = |name: &str, players, port, public| RoomInfo {
                        name: name.into(),
                        players,
                        capacity: 12,
                        state: RoomState::Lobby,
                        port,
                        public,
                    };
                    let reply = Reply::Rooms {
                        game,
                        skip: 0,
                        total: 2,
                        rooms: vec![room("Busy", 5, 43_105, false), room("Public", 0, 43_102, true)],
                    };
                    let _ = socket.send_to(&reply.encode(nonce, build), from);
                }
            }
        });
        addr
    }

    fn lists(o: &mut Online) {
        let t = Instant::now();
        while o.view != Pane::Rooms {
            o.update(t.elapsed().as_secs_f64());
            assert!(t.elapsed() < Duration::from_secs(5), "no list");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_engine_screen_state_lists_public_first_and_joins_the_room_port_on_the_hub_host() {
        let hub = fake_hub(hub::local_build::<DeadfallGame>());
        let mut o = Online::at(hub, GAME_ID, local_build(), 0.);
        lists(&mut o);
        let names: Vec<_> = o.rooms.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Public", "Busy"]);
        assert!(o.can_join() && !o.mismatch());
        let Some(Action::Join { addr, room }) = o.join_index(1) else { panic!("join") };
        assert_eq!((room.name.as_str(), addr), ("Busy", "127.0.0.1:43105".parse().unwrap()));
    }

    #[test]
    fn a_hub_of_another_build_blocks_joining_and_creating() {
        let hub = fake_hub(1234);
        let mut o = Online::at(hub, GAME_ID, local_build(), 0.);
        lists(&mut o);
        assert!(o.mismatch() && !o.can_join());
        assert!(o.join_selected().is_none());
        o.open_dialog("Kev");
        assert!(o.dialog.is_none(), "no Create Room against a different build");
    }
}
