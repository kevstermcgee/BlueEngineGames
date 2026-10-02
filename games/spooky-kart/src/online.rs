//! The rules behind the Play Online screen: which hub to ask, what to call a room, which buttons a state has, and
//! what to tell a player when joining does not work. Pure std (no window, no clock), so each decision has a test;
//! `main.rs` only draws and feeds it frames. The room list, the hub conversation and the retry schedule come from the
//! engine's `netplay::hub` (`Online`, `JoinWait`, ...); this file is what is particular to Spooky Kart.
use crate::netgame::KartGame;
use std::path::Path;
use vesper3d::viewer::devkit::{ServerChoice, ServerSource};
use vesper3d::viewer::netplay::hub::{self, client::Pane, wire::sanitize_name, wire::MAX_NAME_CHARS};
use vesper3d::viewer::netplay::ConnectFailure;

/// The game's title in player-facing sentences.
pub const TITLE: &str = "Spooky Kart";

/// The file beside the executable that names a hub (line 1, `host` or `host:port`; `#` comments). It is **not**
/// `server.txt`: that file already means "a game server to connect to directly" (address, join key, transport) and
/// must keep meaning that.
pub const HUB_FILE: &str = "hub.txt";

/// The environment variable that pretends to be another build, so the version-mismatch screen can be looked at
/// without a second executable (a number; anything else is ignored).
pub const FAKE_BUILD_ENV: &str = "SPOOKY_KART_FAKE_BUILD";

/// Which hub to ask: `--hub HOST:PORT`, else [`HUB_FILE`] beside the executable, else the hub the player used last
/// (`Settings::last_server`), else the built-in public one (`hub::DEFAULT_HUB`). Same chain as the engine's
/// `hub::default_hub`, except that file: that helper reads `server.txt`, which for Spooky Kart is a direct-connect
/// file. `dir` is the executable's directory.
pub fn hub_choice(dir: &Path, cli_arg: Option<&str>, last_used: Option<&str>) -> ServerChoice {
    ServerChoice::first_of(
        dir,
        &[
            ServerSource::CliArg(cli_arg),
            ServerSource::FileBesideExe(HUB_FILE),
            ServerSource::LastUsed(last_used),
            ServerSource::Builtin(hub::DEFAULT_HUB),
        ],
    )
    .expect("the built-in hub is never empty")
}

/// This executable's build id, or the one in [`FAKE_BUILD_ENV`] when it holds a number.
pub fn local_build(fake: Option<&str>) -> u32 {
    fake.and_then(|v| v.trim().parse().ok()).unwrap_or_else(hub::local_build::<KartGame>)
}

/// The room a lobby belongs to (`None` for a direct `--connect` / `server.txt` game, where nothing changes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomTag {
    pub name: String,
    pub public: bool,
}

impl RoomTag {
    /// The two lines the lobby shows: the room, and for a room a player made, how friends find it.
    pub fn lobby_lines(&self) -> (String, Option<String>) {
        let friends = (!self.public).then(|| format!("Friends: open {TITLE} > Play Online > pick {}", self.name));
        (format!("Room: {}", self.name), friends)
    }
}

/// The name offered in the Create Room box: "PLAYER's race", cleaned so the hub accepts it.
pub fn default_room_name(player: &str) -> String {
    let who: String = player.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '\'').collect();
    let who = who.trim();
    // "'s race" adds 7 characters; the name must still fit what the hub allows.
    let who: String = who.chars().take(MAX_NAME_CHARS - 7).collect();
    let name = if who.chars().any(char::is_alphanumeric) { format!("{}'s race", who.trim()) } else { "My race".into() };
    sanitize_name(&name).unwrap_or_else(|_| "My race".into())
}

/// The right-hand side of a room's row: the engine's `room_status` ("3/8  Lobby", "8/8  Full"), with a match
/// called a race, as it is everywhere else in this game.
pub fn room_label(room: &hub::wire::RoomInfo) -> String {
    hub::client::room_status(room).replace("In match", "In race")
}

/// The buttons under the room list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Join,
    Create,
    Refresh,
    Retry,
    Back,
}

impl Btn {
    pub fn label(self) -> &'static str {
        match self {
            Btn::Join => "JOIN",
            Btn::Create => "CREATE ROOM",
            Btn::Refresh => "REFRESH",
            Btn::Retry => "RETRY",
            Btn::Back => "BACK",
        }
    }
}

/// Which buttons the screen offers. Joining and creating are only offered when the hub answered and speaks this
/// game's build; an unreachable hub gets Retry (never a claim that anyone refused us).
pub fn buttons(view: &Pane, mismatch: bool, rooms: usize) -> Vec<Btn> {
    match view {
        Pane::Looking => vec![Btn::Back],
        Pane::Rooms if mismatch => vec![Btn::Refresh, Btn::Back],
        Pane::Rooms if rooms == 0 => vec![Btn::Create, Btn::Refresh, Btn::Back],
        Pane::Rooms => vec![Btn::Join, Btn::Create, Btn::Refresh, Btn::Back],
        Pane::Offline | Pane::Problem(_) => vec![Btn::Retry, Btn::Back],
    }
}

/// What the lobby panel says when the connection to a room did not work (or is being retried).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureView {
    pub title: &'static str,
    pub text: String,
    /// The game retries by itself ("A race is running. You'll be in the next one.").
    pub waiting: bool,
}

/// Words for a failed connection. `room` is true for a room from the list; then the reason is classified (`failure`)
/// and shown through the engine's `connect_failure_message`. A direct connection keeps the server's own words, as
/// it always did. `waiting` is whether a running-match retry is still scheduled.
pub fn failure_view(failure: Option<&ConnectFailure>, why: &str, room: bool, waiting: bool) -> FailureView {
    match (room, failure) {
        (false, _) | (true, None) => FailureView { title: "Cannot play", text: why.to_string(), waiting: false },
        (true, Some(ConnectFailure::MatchInProgress)) if waiting => FailureView {
            title: "Race in progress",
            text: "A race is running. You'll be in the next one.".into(),
            waiting: true,
        },
        (true, Some(f)) => FailureView {
            title: "Cannot join",
            text: match f {
                ConnectFailure::VersionMismatch => hub::client::update_message(TITLE),
                other => hub::client::connect_failure_message(other, true),
            },
            waiting: false,
        },
    }
}

/// Move a focus through `count` entries (up and down never wrap, so a held stick does not loop past the ends).
pub fn step_focus(focus: usize, count: usize, up: bool, down: bool) -> usize {
    if count == 0 {
        return 0;
    }
    let focus = focus.min(count - 1);
    if down && focus + 1 < count {
        focus + 1
    } else if up && focus > 0 {
        focus - 1
    } else {
        focus
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vesper3d::viewer::netplay::hub::wire::{RoomInfo, RoomState};

    fn temp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sk-online-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_hub_comes_from_the_flag_then_hub_txt_then_last_used_then_the_built_in_one() {
        let dir = temp("chain");
        let built_in = hub_choice(&dir, None, None);
        assert_eq!(built_in.address, hub::DEFAULT_HUB);
        assert_eq!(hub_choice(&dir, None, Some("last.example:4100")).address, "last.example:4100");
        std::fs::write(dir.join(HUB_FILE), "# my hub\nfile.example:4100\n").unwrap();
        assert_eq!(hub_choice(&dir, None, Some("last.example:4100")).address, "file.example:4100");
        assert_eq!(hub_choice(&dir, Some("127.0.0.1:43001"), Some("last.example")).address, "127.0.0.1:43001");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn server_txt_is_never_read_as_a_hub() {
        let dir = temp("server-txt");
        std::fs::write(dir.join("server.txt"), "203.0.113.9:4100\nsecret-key\ndevelopment\n").unwrap();
        assert_eq!(hub_choice(&dir, None, None).address, hub::DEFAULT_HUB);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fake_build_is_a_number_and_otherwise_the_real_one() {
        let real = hub::local_build::<KartGame>();
        assert_eq!(local_build(None), real);
        assert_eq!(local_build(Some("12345")), 12345);
        assert_eq!(local_build(Some(" 7 ")), 7);
        assert_eq!(local_build(Some("nope")), real);
    }

    #[test]
    fn default_room_names_are_the_players_race_and_always_pass_the_hub_check() {
        assert_eq!(default_room_name("kevin"), "kevin's race");
        assert_eq!(default_room_name("Frank N. Stein"), "Frank N Stein's race");
        assert_eq!(default_room_name(""), "My race");
        assert_eq!(default_room_name("!!!"), "My race");
        let long = default_room_name(&"x".repeat(60));
        assert!(long.chars().count() <= MAX_NAME_CHARS && long.ends_with("'s race"), "{long}");
        for who in ["kevin", "A B", "名前", "x".repeat(40).as_str(), "  spaced  out  "] {
            assert!(sanitize_name(&default_room_name(who)).is_ok(), "{who}");
        }
    }

    #[test]
    fn a_named_room_tells_friends_how_to_find_it_and_the_public_room_does_not() {
        let own = RoomTag { name: "Kevin's race".into(), public: false };
        assert_eq!(
            own.lobby_lines(),
            (
                "Room: Kevin's race".to_string(),
                Some("Friends: open Spooky Kart > Play Online > pick Kevin's race".to_string())
            )
        );
        assert_eq!(RoomTag { name: "Public".into(), public: true }.lobby_lines(), ("Room: Public".into(), None));
    }

    #[test]
    fn buttons_never_offer_join_or_create_when_the_hub_cannot_be_joined() {
        assert_eq!(buttons(&Pane::Rooms, false, 2), vec![Btn::Join, Btn::Create, Btn::Refresh, Btn::Back]);
        assert_eq!(buttons(&Pane::Rooms, false, 0), vec![Btn::Create, Btn::Refresh, Btn::Back]);
        assert_eq!(buttons(&Pane::Rooms, true, 2), vec![Btn::Refresh, Btn::Back]);
        assert_eq!(buttons(&Pane::Offline, false, 0), vec![Btn::Retry, Btn::Back]);
        assert_eq!(buttons(&Pane::Problem("x".into()), false, 0), vec![Btn::Retry, Btn::Back]);
        assert_eq!(buttons(&Pane::Looking, false, 0), vec![Btn::Back]);
    }

    #[test]
    fn failures_are_honest_per_cause() {
        let wait = failure_view(Some(&ConnectFailure::MatchInProgress), "x", true, true);
        assert_eq!(wait.text, "A race is running. You'll be in the next one.");
        assert!(wait.waiting);
        let gave_up = failure_view(Some(&ConnectFailure::MatchInProgress), "x", true, false);
        assert!(!gave_up.waiting && gave_up.text.contains("in progress"), "{gave_up:?}");
        let silent = ConnectFailure::Unreachable { addr: "10.0.0.1:4101".parse().unwrap(), waited_secs: 8 };
        let v = failure_view(Some(&silent), "x", true, false);
        assert!(v.text.contains("No reply from the room"), "{}", v.text);
        assert!(!v.text.to_lowercase().contains("refus") && !v.text.contains("turned you away"), "{}", v.text);
        assert!(failure_view(Some(&ConnectFailure::VersionMismatch), "x", true, false)
            .text
            .contains("Update the game"));
        assert!(failure_view(Some(&ConnectFailure::Full), "x", true, false).text.to_lowercase().contains("full"));
        // A direct connection keeps the server's own words.
        let direct = failure_view(Some(&ConnectFailure::MatchInProgress), "A match is running", false, false);
        assert_eq!((direct.title, direct.text.as_str()), ("Cannot play", "A match is running"));
    }

    #[test]
    fn focus_stops_at_the_ends() {
        assert_eq!(step_focus(0, 3, true, false), 0);
        assert_eq!(step_focus(0, 3, false, true), 1);
        assert_eq!(step_focus(2, 3, false, true), 2);
        assert_eq!(step_focus(9, 3, false, false), 2);
        assert_eq!(step_focus(0, 0, true, true), 0);
    }

    #[test]
    fn the_room_status_the_screen_shows_is_the_engines_with_race_for_match() {
        let room =
            |players, state| RoomInfo { name: "r".into(), players, capacity: 8, state, port: 4101, public: false };
        assert_eq!(room_label(&room(3, RoomState::Lobby)), "3/8  Lobby");
        assert_eq!(room_label(&room(5, RoomState::Playing)), "5/8  In race");
        assert_eq!(room_label(&room(8, RoomState::Lobby)), "8/8  Full");
    }
}
