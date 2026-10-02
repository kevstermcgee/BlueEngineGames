//! Play Online, without the window: the room list, the Create Room dialog, the hub conversation and the rules for
//! telling a player what went wrong. The screens in `app.rs` draw this state and feed it time; everything here is plain
//! data and std (plus the hub client), so the decisions have unit tests.
//!
//! Flow: [`Online::new`] starts resolving the hub's name on a worker thread (DNS can block for seconds), then asks the hub
//! for the rooms every [`REFRESH_EVERY`] seconds. [`Online::update`] is called once a frame and says when to join a room
//! ([`Action::Join`]): after the player picks one, or right after the hub made the one they asked for.
use crate::hub::{ErrorCode, RoomInfo};
use crate::hub_client::{
    self, build_matches, resolve_ipv4, room_addr, HubClient, HubEvent, NameError, ResolveError, RoomState,
};
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Ask the hub for a fresh room list this often while the screen is open.
pub const REFRESH_EVERY: f64 = 4.;
/// Try a refused-because-a-match-is-running join again this often.
pub const JOIN_RETRY_EVERY: f64 = 3.;
/// Stop waiting for a match to end after this long (a long match is a few minutes).
pub const JOIN_GIVE_UP: f64 = 20. * 60.;

/// [`hub_client::UPDATE_MESSAGE`], for the screen.
pub const UPDATE_TEXT: &str = hub_client::UPDATE_MESSAGE;

/// The build id this game shows and compares. `DEADFALL_FAKE_BUILD=<number>` pretends to be another build, so a
/// version mismatch can be looked at without a second binary.
pub fn local_build() -> u32 {
    std::env::var("DEADFALL_FAKE_BUILD")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(hub_client::local_build)
}

/// Is the hub (and so every room it runs) the same build as this game?
pub fn build_ok(hub_build: u32) -> bool {
    match std::env::var_os("DEADFALL_FAKE_BUILD") {
        Some(_) => hub_build == local_build(),
        None => build_matches(hub_build),
    }
}

/// The build id as shown in a corner of the menu.
pub fn build_label(build: u32) -> String {
    format!("build {build:08x}")
}

// ---- pure rules -----------------------------------------------------------------------------------------------------

/// The list as the player sees it: the Public room first, then the busiest rooms, ties by name.
pub fn order_rooms(mut rooms: Vec<RoomInfo>) -> Vec<RoomInfo> {
    rooms.sort_by(|a, b| {
        b.public
            .cmp(&a.public)
            .then(b.players.cmp(&a.players))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    rooms
}

/// The right-hand side of a room's row: "3/12  Lobby", "5/12  In match", "12/12  Full".
pub fn room_status(room: &RoomInfo) -> String {
    let state = if room.players >= room.capacity && room.capacity > 0 {
        "Full"
    } else {
        match room.state {
            RoomState::Lobby => "Lobby",
            RoomState::Playing => "In match",
        }
    };
    format!("{}/{}  {state}", room.players, room.capacity)
}

/// The room name offered in the Create Room box: "<player>'s room", cleaned so the hub accepts it.
pub fn default_room_name(player: &str) -> String {
    let who: String = player.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '\'').collect();
    let who = who.trim();
    let name = if who.chars().any(char::is_alphanumeric) { format!("{who}'s room") } else { "My room".to_string() };
    match hub_client::sanitize_name(&name) {
        Ok(n) => n,
        Err(_) => "My room".to_string(),
    }
}

/// A friendly sentence for something the hub refused.
pub fn hub_error_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::NameTaken => "A room with that name already exists. Pick another name, or join it from the list.",
        ErrorCode::Full => "All the rooms are in use right now. Join one from the list, or try again in a few minutes.",
        ErrorCode::RateLimited => "Too many requests too fast. Wait a few seconds and try again.",
        ErrorCode::BadName => "That name will not work. Use letters, numbers, spaces, apostrophes and hyphens.",
        ErrorCode::Unavailable => "The server could not start a new room right now. Try again in a moment.",
        ErrorCode::BadRequest => "The server did not understand that. Update the game and try again.",
    }
}

/// A friendly sentence for a room name that failed the local check.
pub fn name_error_message(e: NameError) -> &'static str {
    e.text()
}

/// Why a connection to a game server failed, from the reason text the engine client gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectFailure {
    /// The server never answered (the engine reports this as "Could not reach the server").
    NoReply,
    /// A match is running; the lobby will accept us after it.
    MatchInProgress,
    /// The server answered and said no.
    Refused(String),
}

/// Sort a `ClientState::Rejected` reason into what it really means. Matching is by phrase, case-insensitive, so small
/// wording changes in the engine do not turn a silent server into a "turned away" message.
pub fn classify_rejection(reason: &str) -> ConnectFailure {
    let lower = reason.to_lowercase();
    if lower.contains("match is in progress") || lower.contains("match in progress") {
        ConnectFailure::MatchInProgress
    } else if lower.contains("could not reach") || lower.contains("couldn't reach") || lower.contains("no reply") {
        ConnectFailure::NoReply
    } else {
        ConnectFailure::Refused(reason.trim().to_string())
    }
}

/// How long the engine client waits before it gives up (`CONNECT_TIMEOUT` in the engine), for the message.
pub const CONNECT_WAIT_SECONDS: u32 = 8;

/// The words for [`ConnectFailure::NoReply`]. `room` is true when the address came from the room list, where the player
/// never typed it and port forwarding is not their problem.
pub fn no_reply_message(addr: &str, room: bool) -> String {
    if room {
        format!(
            "No reply from the room at {addr} after {CONNECT_WAIT_SECONDS} s. The room may have just closed, or the server is \
             having trouble. Go back and refresh the list, or try again."
        )
    } else {
        format!(
            "No reply from {addr} after {CONNECT_WAIT_SECONDS} s. The server may be offline, the address may be wrong, or the \
             host's router isn't forwarding the port."
        )
    }
}

/// The words for a real refusal.
pub fn refused_message(reason: &str) -> String {
    format!("The server turned you away: {reason}")
}

/// What to do about a join that waits for a running match to end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetryStep {
    /// Not yet.
    Wait,
    /// Ask the lobby again now.
    TryNow,
    /// Waited long enough.
    GiveUp,
}

/// The retry schedule for "A match is running".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JoinWait {
    pub since: f64,
    pub next_at: f64,
    pub attempts: u32,
}

impl JoinWait {
    pub fn new(now: f64) -> JoinWait {
        JoinWait { since: now, next_at: now + JOIN_RETRY_EVERY, attempts: 0 }
    }

    pub fn step(&self, now: f64) -> RetryStep {
        if now - self.since >= JOIN_GIVE_UP {
            RetryStep::GiveUp
        } else if now >= self.next_at {
            RetryStep::TryNow
        } else {
            RetryStep::Wait
        }
    }

    /// An attempt was made at `now` and was turned away again: schedule the next one.
    pub fn attempted(&mut self, now: f64) {
        self.attempts += 1;
        self.next_at = now + JOIN_RETRY_EVERY;
    }
}

/// How many list rows to show at once for a window `height_px` tall at `ui` scale; the rest scroll.
pub fn visible_rows(height_px: f32, ui: f32) -> usize {
    let free = height_px / ui - 215. - 4. * 46. - 23. - 36.;
    ((free / 46.).floor() as i32).clamp(2, 10) as usize
}

/// Keep the highlighted row inside the visible window. `sel` is the selected room's index in the whole list.
pub fn scroll_to(sel: usize, offset: usize, visible: usize, total: usize) -> usize {
    let max_offset = total.saturating_sub(visible);
    let offset = if sel < offset {
        sel
    } else if sel >= offset + visible {
        sel + 1 - visible
    } else {
        offset
    };
    offset.min(max_offset)
}

// ---- the state machine ----------------------------------------------------------------------------------------------

/// What the room list screen is showing.
#[derive(Clone, Debug, PartialEq)]
pub enum Pane {
    /// Waiting for the first answer.
    Looking,
    Rooms,
    /// The hub does not answer (or its name did not resolve).
    Offline,
    /// The hub answered with a refusal to the list request.
    Problem(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DialogPhase {
    Editing,
    Creating,
}

/// The Create Room box.
#[derive(Clone, Debug)]
pub struct Dialog {
    pub name: String,
    pub phase: DialogPhase,
    pub error: Option<String>,
}

/// What the screen should do next.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Connect to this room's game server.
    Join { addr: SocketAddr, room: RoomInfo },
}

pub struct Online {
    spec: String,
    resolving: Option<Receiver<Result<SocketAddr, ResolveError>>>,
    client: Option<HubClient>,
    pub view: Pane,
    /// Why the hub is unreachable, for a dim detail line.
    pub detail: Option<String>,
    pub rooms: Vec<RoomInfo>,
    /// The hub's build id, once it has answered.
    pub build: Option<u32>,
    next_refresh: f64,
    pub dialog: Option<Dialog>,
    /// The highlighted room, by name (the list can reorder under it).
    pub selected: Option<String>,
    pub offset: usize,
}

fn spawn_resolve(spec: &str) -> Receiver<Result<SocketAddr, ResolveError>> {
    let (tx, rx) = mpsc::channel();
    let spec = spec.to_string();
    // If the thread cannot start the sender is dropped and the receiver reports a disconnect: the screen shows offline.
    let _ = std::thread::Builder::new().name("deadfall-dns".into()).spawn(move || {
        let _ = tx.send(resolve_ipv4(&spec));
    });
    rx
}

impl Online {
    /// Open the screen for the hub at `spec` (`host`, `host:port`): resolves it off the calling thread.
    pub fn new(spec: &str, now: f64) -> Online {
        Online {
            spec: spec.to_string(),
            resolving: Some(spawn_resolve(spec)),
            client: None,
            view: Pane::Looking,
            detail: None,
            rooms: Vec::new(),
            build: None,
            next_refresh: now,
            dialog: None,
            selected: None,
            offset: 0,
        }
    }

    /// [`Online::new`] for a hub whose address is already known (tests).
    pub fn at(hub: SocketAddr, now: f64) -> Online {
        let mut o = Online::new(&hub.to_string(), now);
        o.resolving = None;
        o.attach(hub, now);
        o
    }

    pub fn hub_spec(&self) -> &str {
        &self.spec
    }

    /// The hub is a different build than this game: rooms would turn us away.
    pub fn mismatch(&self) -> bool {
        self.build.is_some_and(|b| !build_ok(b))
    }

    /// Joining and creating are allowed.
    pub fn can_join(&self) -> bool {
        self.view == Pane::Rooms && !self.mismatch()
    }

    /// Nothing is happening that a Retry should wait for.
    pub fn is_busy(&self) -> bool {
        self.resolving.is_some() || self.client.as_ref().is_some_and(HubClient::busy)
    }

    fn attach(&mut self, addr: SocketAddr, now: f64) {
        match HubClient::new(addr) {
            Ok(mut c) => {
                c.request_list();
                self.client = Some(c);
            }
            Err(e) => {
                self.detail = Some(e.to_string());
                self.view = Pane::Offline;
                self.next_refresh = now + REFRESH_EVERY;
            }
        }
    }

    /// Ask again now (the Refresh and Retry buttons). Looks the hub up again if it never resolved.
    pub fn refresh(&mut self, now: f64) {
        if self.resolving.is_some() {
            return;
        }
        match self.client.as_mut() {
            None => {
                self.resolving = Some(spawn_resolve(&self.spec));
                self.view = Pane::Looking;
            }
            Some(c) => {
                if self.dialog.as_ref().is_some_and(|d| d.phase == DialogPhase::Creating) {
                    return;
                }
                c.request_list();
                if self.view == Pane::Offline || matches!(self.view, Pane::Problem(_)) {
                    self.view = Pane::Looking;
                }
            }
        }
        self.next_refresh = now + REFRESH_EVERY;
    }

    pub fn selected_room(&self) -> Option<&RoomInfo> {
        self.selected.as_ref().and_then(|n| self.rooms.iter().find(|r| &r.name == n)).or(self.rooms.first())
    }

    /// Join the highlighted room (the Join button).
    pub fn join_selected(&self) -> Option<Action> {
        if !self.can_join() {
            return None;
        }
        let hub = self.client.as_ref()?.hub_addr();
        let room = self.selected_room()?.clone();
        Some(Action::Join { addr: room_addr(hub, room.port), room })
    }

    /// Join a room by its place in the list (Enter or a click on its row).
    pub fn join_index(&self, index: usize) -> Option<Action> {
        if !self.can_join() {
            return None;
        }
        let hub = self.client.as_ref()?.hub_addr();
        let room = self.rooms.get(index)?.clone();
        Some(Action::Join { addr: room_addr(hub, room.port), room })
    }

    pub fn open_dialog(&mut self, player: &str) {
        if self.can_join() {
            self.dialog = Some(Dialog { name: default_room_name(player), phase: DialogPhase::Editing, error: None });
        }
    }

    /// The Cancel button of the dialog (also abandons a request in flight).
    pub fn close_dialog(&mut self) {
        if self.dialog.take().is_some_and(|d| d.phase == DialogPhase::Creating) {
            if let Some(c) = self.client.as_mut() {
                c.cancel();
            }
            self.view = Pane::Looking;
            self.next_refresh = 0.;
        }
    }

    /// The Create button: check the name, ask the hub. The answer comes through [`Online::update`].
    pub fn create(&mut self) {
        let allowed = self.can_join_ignoring_dialog();
        let Some(d) = self.dialog.as_mut() else { return };
        if d.phase != DialogPhase::Editing || !allowed {
            return;
        }
        match hub_client::sanitize_name(&d.name) {
            Err(e) => d.error = Some(name_error_message(e).to_string()),
            Ok(name) => {
                d.name = name.clone();
                d.error = None;
                if let Some(c) = self.client.as_mut() {
                    c.request_create(&name);
                    d.phase = DialogPhase::Creating;
                }
            }
        }
    }

    fn can_join_ignoring_dialog(&self) -> bool {
        self.view == Pane::Rooms && !self.mismatch()
    }

    /// Once a frame: collect the DNS answer and the hub's replies, refresh on schedule.
    pub fn update(&mut self, now: f64) -> Option<Action> {
        if let Some(rx) = &self.resolving {
            match rx.try_recv() {
                Ok(Ok(addr)) => {
                    self.resolving = None;
                    self.attach(addr, now);
                }
                Ok(Err(e)) => {
                    self.resolving = None;
                    self.detail = Some(e.to_string());
                    self.view = Pane::Offline;
                    self.next_refresh = now + REFRESH_EVERY;
                }
                Err(TryRecvError::Disconnected) => {
                    self.resolving = None;
                    self.detail = Some("The hub's name could not be looked up.".into());
                    self.view = Pane::Offline;
                    self.next_refresh = now + REFRESH_EVERY;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let event = self.client.as_mut().and_then(HubClient::poll);
        let mut action = None;
        if let Some(event) = event {
            match event {
                HubEvent::Rooms { build, rooms } => {
                    self.build = Some(build);
                    self.rooms = order_rooms(rooms);
                    self.view = Pane::Rooms;
                    self.detail = None;
                    self.next_refresh = now + REFRESH_EVERY;
                }
                HubEvent::Created { build, room } => {
                    self.build = Some(build);
                    self.dialog = None;
                    if let Some(c) = self.client.as_ref() {
                        action = Some(Action::Join { addr: room_addr(c.hub_addr(), room.port), room });
                    }
                }
                HubEvent::Error { build, code, .. } => {
                    self.build = Some(build);
                    let text = hub_error_message(code).to_string();
                    match self.dialog.as_mut() {
                        Some(d) => {
                            d.phase = DialogPhase::Editing;
                            d.error = Some(text);
                            self.next_refresh = now + REFRESH_EVERY;
                        }
                        None => {
                            if self.rooms.is_empty() {
                                self.view = Pane::Problem(text);
                            }
                            self.next_refresh = now + REFRESH_EVERY;
                        }
                    }
                }
                HubEvent::Timeout => {
                    self.next_refresh = now + REFRESH_EVERY;
                    match self.dialog.as_mut() {
                        Some(d) if d.phase == DialogPhase::Creating => {
                            d.phase = DialogPhase::Editing;
                            d.error = Some("The server did not answer. Check your connection and try again.".into());
                        }
                        _ => {
                            self.view = Pane::Offline;
                            self.rooms.clear();
                            self.detail = None;
                        }
                    }
                }
            }
        }
        if let Some(c) = self.client.as_mut() {
            if !c.busy() && now >= self.next_refresh {
                c.request_list();
                self.next_refresh = now + REFRESH_EVERY;
            }
        } else if self.resolving.is_none() && self.view == Pane::Offline && now >= self.next_refresh {
            // The name never resolved: look again now and then, so coming back online fixes the screen by itself.
            self.resolving = Some(spawn_resolve(&self.spec));
            self.next_refresh = now + REFRESH_EVERY;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::{Reply, Request};
    use crate::hub_client::MAX_NAME_CHARS;
    use std::net::UdpSocket;
    use std::time::Duration;

    fn room(name: &str, players: u8, public: bool) -> RoomInfo {
        RoomInfo { name: name.into(), players, capacity: 12, state: RoomState::Lobby, port: 4100, public }
    }

    #[test]
    fn rooms_are_ordered_public_first_then_busiest_then_by_name() {
        let list = order_rooms(vec![
            room("zed", 1, false),
            room("Alpha", 3, false),
            room("beta", 3, false),
            room("Public", 0, true),
            room("Mid", 5, false),
        ]);
        let names: Vec<_> = list.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Public", "Mid", "Alpha", "beta", "zed"]);
        // A busy room never outranks Public, whatever the input order.
        let list = order_rooms(vec![room("Big", 11, false), room("Public", 0, true)]);
        assert_eq!(list[0].name, "Public");
        assert!(order_rooms(vec![]).is_empty());
    }

    #[test]
    fn room_rows_say_players_and_state() {
        let mut r = room("x", 3, false);
        assert_eq!(room_status(&r), "3/12  Lobby");
        r.state = RoomState::Playing;
        assert_eq!(room_status(&r), "3/12  In match");
        r.players = 12;
        assert_eq!(room_status(&r), "12/12  Full");
    }

    #[test]
    fn the_default_room_name_is_always_acceptable_to_the_hub() {
        assert_eq!(default_room_name("Kevin"), "Kevin's room");
        assert_eq!(default_room_name("  "), "My room");
        assert_eq!(default_room_name("!!!"), "My room");
        assert_eq!(default_room_name("kevin_99"), "kevin99's room");
        for who in ["Kevin", "A very long player name", "字字字字", "x-y", "O'Neil", ""] {
            let n = default_room_name(who);
            assert_eq!(hub_client::sanitize_name(&n).as_deref(), Ok(n.as_str()), "{who}");
            assert!(n.chars().count() <= MAX_NAME_CHARS);
        }
    }

    #[test]
    fn every_hub_error_has_a_friendly_sentence() {
        use ErrorCode::*;
        for code in [BadName, NameTaken, Full, RateLimited, Unavailable, BadRequest] {
            let m = hub_error_message(code);
            assert!(m.ends_with('.') && m.len() > 20, "{code:?}: {m}");
            assert!(!m.to_lowercase().contains("turned you away"));
        }
        assert!(hub_error_message(NameTaken).contains("already exists"));
        assert!(hub_error_message(Full).contains("in use"));
        assert!(hub_error_message(RateLimited).contains("Wait"));
        assert!(hub_error_message(Unavailable).contains("could not start"));
        assert!(hub_error_message(BadName).contains("letters"));
    }

    #[test]
    fn rejections_are_told_apart() {
        assert_eq!(classify_rejection("Could not reach the server"), ConnectFailure::NoReply);
        assert_eq!(classify_rejection("could not reach the server."), ConnectFailure::NoReply);
        assert_eq!(
            classify_rejection("A match is in progress; try again in a moment"),
            ConnectFailure::MatchInProgress
        );
        assert_eq!(classify_rejection("a Match Is In Progress"), ConnectFailure::MatchInProgress);
        assert_eq!(classify_rejection("Wrong password"), ConnectFailure::Refused("Wrong password".into()));
        assert_eq!(classify_rejection(" The server is full "), ConnectFailure::Refused("The server is full".into()));
        assert_eq!(classify_rejection(""), ConnectFailure::Refused(String::new()));
    }

    #[test]
    fn no_reply_and_refusal_read_differently() {
        let typed = no_reply_message("1.2.3.4:4100", false);
        assert!(typed.starts_with("No reply from 1.2.3.4:4100 after 8 s."));
        assert!(typed.contains("router isn't forwarding the port"));
        assert!(!typed.contains("turned you away"));
        let listed = no_reply_message("1.2.3.4:4101", true);
        assert!(listed.contains("1.2.3.4:4101") && !listed.contains("forwarding"));
        assert_eq!(refused_message("Wrong password"), "The server turned you away: Wrong password");
    }

    #[test]
    fn a_running_match_is_retried_every_three_seconds_until_the_give_up_time() {
        let mut w = JoinWait::new(100.);
        assert_eq!(w.step(100.), RetryStep::Wait);
        assert_eq!(w.step(102.9), RetryStep::Wait);
        assert_eq!(w.step(103.), RetryStep::TryNow);
        w.attempted(103.2);
        assert_eq!((w.attempts, w.step(105.), w.step(106.2)), (1, RetryStep::Wait, RetryStep::TryNow));
        assert_eq!(w.step(100. + JOIN_GIVE_UP), RetryStep::GiveUp);
        assert_eq!(w.step(100. + JOIN_GIVE_UP - 1.), RetryStep::TryNow, "an overdue attempt is not postponed");
    }

    #[test]
    fn the_list_scrolls_to_keep_the_highlight_in_view() {
        assert_eq!(scroll_to(0, 0, 5, 3), 0);
        assert_eq!(scroll_to(5, 0, 5, 10), 1);
        assert_eq!(scroll_to(9, 1, 5, 10), 5);
        assert_eq!(scroll_to(2, 5, 5, 10), 2);
        assert_eq!(scroll_to(4, 2, 5, 10), 2);
        assert_eq!(scroll_to(0, 7, 5, 6), 0);
        assert!(visible_rows(720., 1.) >= 4);
        assert_eq!(visible_rows(100., 0.55), 2);
        assert!(visible_rows(5000., 2.4) <= 10);
    }

    /// A UDP hub on loopback that answers each request as `answer` says (nothing for `None`).
    fn fake_hub(answer: impl Fn(Request) -> Option<Reply> + Send + 'static) -> SocketAddr {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        socket.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            for _ in 0..400 {
                if let Ok((n, from)) = socket.recv_from(&mut buf) {
                    if let Some((nonce, req)) = Request::decode(&buf[..n]) {
                        if let Some(r) = answer(req) {
                            let _ = socket.send_to(&r.encode(nonce, local_build()), from);
                        }
                    }
                }
            }
        });
        addr
    }

    fn run_until(o: &mut Online, mut done: impl FnMut(&Online) -> bool) -> Option<Action> {
        let start = std::time::Instant::now();
        let mut action = None;
        while !done(o) {
            assert!(start.elapsed() < Duration::from_secs(6), "timed out in state {:?}", o.view);
            let now = start.elapsed().as_secs_f64();
            action = o.update(now).or(action);
            std::thread::sleep(Duration::from_millis(10));
        }
        action
    }

    fn info(name: &str, port: u16, public: bool) -> RoomInfo {
        RoomInfo { name: name.into(), players: 1, capacity: 12, state: RoomState::Lobby, port, public }
    }

    #[test]
    fn listing_creating_and_joining_through_a_hub() {
        let hub = fake_hub(|req| match req {
            Request::List { .. } => Some(Reply::Rooms {
                skip: 0,
                total: 2,
                rooms: vec![info("Mine", 4102, false), info("Public", 4101, true)],
            }),
            Request::Create { name, .. } if name == "Taken" => {
                Some(Reply::Error { code: ErrorCode::NameTaken, text: "raw hub text".into() })
            }
            Request::Create { name, .. } => Some(Reply::Created { room: info(&name, 4105, false) }),
            _ => None,
        });
        let mut o = Online::at(hub, 0.);
        assert_eq!(o.view, Pane::Looking);
        run_until(&mut o, |o| o.view == Pane::Rooms);
        assert_eq!(o.rooms[0].name, "Public", "public first");
        assert!(o.can_join());
        assert_eq!(o.selected_room().unwrap().name, "Public");
        let Some(Action::Join { addr, room }) = o.join_selected() else { panic!() };
        assert_eq!((addr, room.public), (SocketAddr::new(hub.ip(), 4101), true));
        let Some(Action::Join { addr, .. }) = o.join_index(1) else { panic!() };
        assert_eq!(addr.port(), 4102);
        // A bad name is caught locally; a taken one comes back as a friendly sentence and the dialog stays.
        o.open_dialog("Kevin");
        assert_eq!(o.dialog.as_ref().unwrap().name, "Kevin's room");
        o.dialog.as_mut().unwrap().name = "***".into();
        o.create();
        assert!(o.dialog.as_ref().unwrap().error.as_ref().unwrap().contains("letters"));
        o.dialog.as_mut().unwrap().name = "Taken".into();
        o.create();
        assert_eq!(o.dialog.as_ref().unwrap().phase, DialogPhase::Creating);
        run_until(&mut o, |o| o.dialog.as_ref().is_some_and(|d| d.phase == DialogPhase::Editing));
        assert!(o.dialog.as_ref().unwrap().error.as_ref().unwrap().contains("already exists"));
        // A good one is made and joined at once.
        o.dialog.as_mut().unwrap().name = "  Fresh   room ".into();
        o.create();
        let action = run_until(&mut o, |o| o.dialog.is_none());
        let Some(Action::Join { addr, room }) = action else { panic!("no join: {action:?}") };
        assert_eq!((room.name.as_str(), addr.port()), ("Fresh room", 4105));
    }

    #[test]
    fn a_silent_hub_is_offline_and_a_different_build_blocks_joining() {
        let silent = fake_hub(|_| None);
        let mut o = Online::at(silent, 0.);
        run_until(&mut o, |o| o.view == Pane::Offline);
        assert!(!o.can_join() && o.join_selected().is_none() && o.rooms.is_empty());
        o.open_dialog("Kevin");
        assert!(o.dialog.is_none(), "no Create Room while offline");

        // A hub that reports another build: the list shows, but Join and Create are off.
        let other = {
            let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
            let addr = socket.local_addr().unwrap();
            socket.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                for _ in 0..200 {
                    if let Ok((n, from)) = socket.recv_from(&mut buf) {
                        if let Some((nonce, _)) = Request::decode(&buf[..n]) {
                            let reply = Reply::Rooms { skip: 0, total: 1, rooms: vec![info("Public", 4101, true)] };
                            let _ = socket.send_to(&reply.encode(nonce, local_build() ^ 0xFFFF), from);
                        }
                    }
                }
            });
            addr
        };
        let mut o = Online::at(other, 0.);
        run_until(&mut o, |o| o.view == Pane::Rooms);
        assert!(o.mismatch() && !o.can_join());
        assert!(o.join_selected().is_none() && o.join_index(0).is_none());
        o.open_dialog("Kevin");
        assert!(o.dialog.is_none());
    }

    #[test]
    fn an_unresolvable_hub_name_is_offline_not_a_hang() {
        let mut o = Online::new("nonexistent.invalid", 0.);
        assert_eq!(o.view, Pane::Looking);
        run_until(&mut o, |o| o.view == Pane::Offline);
        assert!(o.detail.as_deref().is_some_and(|d| d.contains("nonexistent.invalid")));
        let mut o = Online::new("bad host name", 0.);
        run_until(&mut o, |o| o.view == Pane::Offline);
    }
}
