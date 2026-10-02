//! The whole game window: screens, the network session, and the frame loop that ties input, prediction, drawing and
//! sound together.
use super::audio::{self, Sfx};
use super::controls::Controls;
use super::online::{self, Action, ConnectFailure, DialogPhase, JoinWait, Online, Pane, RetryStep};
use super::overlay::{self, Feed};
use super::render::{self, Figure, Renderer};
use super::sound::{Audio, Listener};
use super::ui::{self, Hit, Item, Menu, Nav, TextRules, ACCENT, DIM, TEXT};
use crate::hands::{Busy, Hands};
use crate::netgame::{flag, DeadfallGame, Local, Snapshot};
use crate::prefs::Prefs;
use crate::sim::{self, hit, EndRule, Event, Settings};
use crate::stats::{MatchTracker, Stats};
use crate::weapons;
use macroquad::prelude::*;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use vesper3d::math::V;
use vesper3d::viewer::{
    devkit::{flag_value, has_flag, ServerOrigin, ShadowQuality},
    game_client::{self, GameShell},
    game_input::ClientInput,
    kit::{capture, hud, View},
    net::{client_transport, server_transport, AnyTransport, TransportProfile},
    netplay::{hub, ClientConfig, ClientState, NetClient, NetServer, ServerConfig},
};

const PORT: u16 = 4100;
const TICK: f32 = 1. / 60.;

#[derive(Clone, Debug, PartialEq)]
enum Screen {
    Main,
    Solo,
    Host,
    Join,
    Online,
    Stats,
    Settings,
    Connecting,
    /// A running match refused us; asking again every few seconds.
    Waiting,
    Lobby,
    Playing,
    Error(String),
    /// Joining a server failed: the message, with Retry.
    ConnectFailed(String),
}

/// A room from the Play Online list, as the session and the lobby know it.
#[derive(Clone, Debug, PartialEq)]
struct RoomTag {
    name: String,
    public: bool,
}

/// Where a join goes, kept so Retry and the "match in progress" wait can ask again.
#[derive(Clone, Debug, PartialEq)]
struct JoinTarget {
    addr: String,
    key: String,
    team: u8,
    room: Option<RoomTag>,
}

/// A server running inside this process (solo and hosted games).
struct LocalServer {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub addr: SocketAddr,
}

impl LocalServer {
    fn start(listen: &str, cfg: ServerConfig, settings: Settings) -> Result<LocalServer, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let listen = listen.to_string();
        let thread = std::thread::Builder::new()
            .name("deadfall-server".into())
            .spawn(move || {
                sim::set_settings(settings);
                let made = server_transport(TransportProfile::Development, &listen)
                    .and_then(|t| NetServer::<DeadfallGame, _>::new(t, cfg));
                match made {
                    Ok(mut server) => {
                        let _ = tx.send(server.local_addr().map_err(|e| e.to_string()));
                        let _ = server.run_realtime(flag, None);
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(addr)) => Ok(LocalServer { stop, thread: Some(thread), addr }),
            Ok(Err(e)) => Err(format!("the server could not start: {e}")),
            Err(_) => Err("the server did not start".into()),
        }
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Killcam {
    started: f64,
    elapsed0: f32,
    killer: u8,
    from: f32,
    last_tick: f32,
}

struct Session {
    client: NetClient<DeadfallGame, AnyTransport>,
    _server: Option<LocalServer>,
    tracker: MatchTracker,
    feed: Vec<Feed>,
    hit_marker: (f32, bool),
    damage: Vec<(f32, f32)>,
    notice: (String, f32),
    killcam: Option<Killcam>,
    acc: f32,
    shake: f32,
    want_again: bool,
    over_handled: bool,
    solo: bool,
    hosting: Option<String>,
    /// The Play Online room this session joined.
    room: Option<RoomTag>,
    skins: [u8; 16],
    last_alive: bool,
    motion: render::ViewMotion,
    last_angles: (f32, f32),
    my_team: usize,
    kills_by_me: Vec<(u8, f32)>,
    died_by_headshot: bool,
}

pub struct App {
    prefs: Prefs,
    /// The shadow tier the Settings row last showed or set (a `--shadows` flag can differ from `prefs`).
    shadow_choice: ShadowQuality,
    stats: Stats,
    renderer: Renderer,
    audio: Audio,
    shell: GameShell,
    input: ClientInput,
    controls: Controls,
    menu: Menu,
    last_mouse: Vec2,
    screen: Screen,
    session: Option<Session>,
    time: f32,
    // Menu state.
    host_port: String,
    host_key: String,
    join_addr: String,
    join_key: String,
    solo_items: Vec<Item>,
    host_items: Vec<Item>,
    join_items: Vec<Item>,
    settings_items: Vec<Item>,
    results_menu: Menu,
    // Play Online.
    hub_spec: String,
    /// Where `hub_spec` came from: only a hub the player named on the command line is remembered.
    hub_origin: ServerOrigin,
    online: Option<Online>,
    online_menu: Menu,
    dialog_menu: Menu,
    /// The highlight is on a room row (so it follows the room, not the position, when the list changes).
    online_on_row: bool,
    last_join: Option<JoinTarget>,
    wait: Option<JoinWait>,
    now: f64,
    vignette: Texture2D,
    play_seconds_unsaved: f32,
    // Capture and scripted runs.
    capture_dir: Option<std::path::PathBuf>,
    capture_frames: Vec<u32>,
    frame: u32,
    quit: bool,
    go_home: bool,
    autostart: Option<Screen>,
    pub autopilot: bool,
}

fn local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "your-ip".into())
}

fn end_choices() -> Vec<String> {
    vec!["First to a kill target".into(), "Timed match".into()]
}

const KILL_TARGETS: [u16; 6] = [10, 20, 40, 60, 100, 200];
const MINUTES: [u16; 6] = [3, 5, 10, 15, 20, 30];

fn target_labels(time: bool) -> Vec<String> {
    if time {
        MINUTES.iter().map(|m| format!("{m} minutes")).collect()
    } else {
        KILL_TARGETS.iter().map(|k| format!("{k} kills")).collect()
    }
}

fn pick(options: &[u16], v: u16) -> usize {
    options.iter().position(|o| *o == v).unwrap_or(options.len() / 2)
}

impl App {
    pub async fn new(args: &[String]) -> App {
        let prefs = Prefs::load();
        let hub = hub::default_hub(flag_value(args, "--hub"), prefs.last_hub.as_deref());
        let level = crate::map();
        let mut renderer = Renderer::new(&level);
        // `--shadows off|simple|full` wins for this run without changing what is remembered.
        let shadows = match ShadowQuality::from_flag(args) {
            Ok(q) => q.unwrap_or(prefs.shadows),
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(2);
            }
        };
        renderer.set_shadows(shadows);
        let audio = Audio::start(has_flag(args, "--mute"), prefs.volume).await;
        let vignette = hud::make_vignette();
        let mut app = App {
            stats: Stats::load(),
            renderer,
            audio,
            shell: GameShell::new(),
            input: ClientInput::new(),
            controls: Controls::new(),
            menu: Menu::new(),
            last_mouse: Vec2::ZERO,
            screen: Screen::Main,
            session: None,
            time: 0.,
            host_port: PORT.to_string(),
            host_key: prefs.key.clone(),
            join_addr: prefs.address.clone(),
            // A password is never carried over from what was typed or hosted before.
            join_key: String::new(),
            solo_items: Vec::new(),
            host_items: Vec::new(),
            join_items: Vec::new(),
            settings_items: Vec::new(),
            results_menu: Menu::new(),
            hub_spec: hub.address,
            hub_origin: hub.origin,
            online: None,
            online_menu: Menu::new(),
            dialog_menu: Menu::new(),
            online_on_row: false,
            last_join: None,
            wait: None,
            now: 0.,
            vignette,
            play_seconds_unsaved: 0.,
            capture_dir: flag_value(args, "--capture").map(Into::into),
            capture_frames: flag_value(args, "--frames")
                .map(|v| v.split(',').filter_map(|p| p.trim().parse().ok()).collect())
                .unwrap_or_default(),
            frame: 0,
            quit: false,
            go_home: false,
            autostart: None,
            autopilot: std::env::var_os("DEADFALL_AUTOPILOT").is_some(),
            shadow_choice: shadows,
            prefs,
        };
        app.rebuild_items();
        if let Some(text) = flag_value(args, "--script") {
            app.controls.script = Some(super::controls::Script::parse(text));
        }
        if let Some(dir) = &app.capture_dir {
            let _ = std::fs::create_dir_all(dir);
        }
        match flag_value(args, "--screen") {
            Some("stats") => app.screen = Screen::Stats,
            Some("settings") => app.screen = Screen::Settings,
            Some("solo") => app.screen = Screen::Solo,
            Some("host") => app.screen = Screen::Host,
            Some("join") => app.screen = Screen::Join,
            Some("online") => app.open_online(),
            _ => {}
        }
        if has_flag(args, "--solo") {
            app.prefs.bots = true;
            app.autostart = Some(Screen::Solo);
        }
        if let Some(addr) = flag_value(args, "--connect") {
            app.join_addr = addr.to_string();
            app.autostart = Some(Screen::Join);
        }
        app
    }

    fn rebuild_items(&mut self) {
        let p = &self.prefs;
        let team = vec!["Ironclad".to_string(), "Nightwatch".to_string()];
        let skill = vec!["Easy".to_string(), "Normal".to_string(), "Hard".to_string()];
        let options = |time: bool| {
            Item::Choice(
                if time { "Length".into() } else { "Kills to win".into() },
                target_labels(time),
                if time { pick(&MINUTES, p.minutes) } else { pick(&KILL_TARGETS, p.kills_target) },
            )
        };
        self.solo_items = vec![
            Item::Choice("Team".into(), team.clone(), p.team as usize),
            Item::Choice("Ends".into(), end_choices(), p.end_by_time as usize),
            options(p.end_by_time),
            Item::Choice("Bot skill".into(), skill.clone(), p.bot_skill as usize),
            Item::Gap,
            Item::Button("Start".into()),
            Item::Button("Back".into()),
        ];
        self.host_items = vec![
            Item::Text("Port".into(), self.host_port.clone(), "4100", TextRules { max: 5, charset: ui::Charset::Any }),
            Item::Text("Password (optional)".into(), self.host_key.clone(), "none", TextRules::ANY),
            Item::Choice("Team".into(), team.clone(), p.team as usize),
            Item::Choice("Ends".into(), end_choices(), p.end_by_time as usize),
            options(p.end_by_time),
            Item::Toggle("Bots fill the teams".into(), p.bots),
            Item::Choice("Bot skill".into(), skill, p.bot_skill as usize),
            Item::Gap,
            Item::Button("Start hosting".into()),
            Item::Button("Back".into()),
        ];
        self.join_items = vec![
            Item::Text("Server".into(), self.join_addr.clone(), "address:port", TextRules::ADDRESS),
            Item::Text("Password".into(), self.join_key.clone(), "none", TextRules::ANY),
            Item::Choice("Team".into(), team, p.team as usize),
            Item::Gap,
            Item::Button("Connect".into()),
            Item::Button("Back".into()),
        ];
        self.settings_items = vec![
            Item::Text("Name".into(), p.name.clone(), "your name", TextRules::NAME),
            Item::Slider("Mouse sensitivity".into(), p.sensitivity, 0.2, 4.),
            Item::Slider("Stick sensitivity".into(), p.stick_sensitivity, 0.3, 3.),
            Item::Toggle("Invert look".into(), p.invert_y),
            Item::Slider("Volume".into(), p.volume, 0., 1.),
            Item::Toggle("Fullscreen".into(), p.fullscreen),
            Item::Choice(
                "Shadows".into(),
                ShadowQuality::ALL.iter().map(|q| q.label().to_string()).collect(),
                ShadowQuality::ALL.iter().position(|q| *q == self.shadow_choice).unwrap_or(1),
            ),
            Item::Gap,
            Item::Button("Back".into()),
        ];
    }

    fn settings_from(&self, items: &[Item], time_idx: usize, target_idx: usize) -> (bool, u16, u16) {
        let by_time = matches!(items.get(time_idx), Some(Item::Choice(_, _, 1)));
        let (mut kills, mut minutes) = (self.prefs.kills_target, self.prefs.minutes);
        if let Some(Item::Choice(_, _, k)) = items.get(target_idx) {
            if by_time {
                minutes = MINUTES[*k % MINUTES.len()];
            } else {
                kills = KILL_TARGETS[*k % KILL_TARGETS.len()];
            }
        }
        (by_time, kills, minutes)
    }

    // ---- lifecycle ---------------------------------------------------------------------------------------------

    fn save_prefs(&mut self) {
        self.prefs.key = self.host_key.clone();
        self.prefs.address = self.join_addr.clone();
        self.prefs.store();
    }

    fn leave(&mut self) {
        if let Some(mut s) = self.session.take() {
            s.client.leave();
            self.flush_match(&mut s, None);
        }
        self.audio.clear_match_state();
        self.renderer.clear_match();
        self.last_join = None;
        self.wait = None;
        self.online = None;
        self.screen = Screen::Main;
        self.rebuild_items();
    }

    fn open_online(&mut self) {
        self.online = Some(online::open(&self.hub_spec, self.now));
        self.online_menu = Menu::new();
        self.online_on_row = false;
        self.screen = Screen::Online;
    }

    /// Join a game server somewhere else (a room from the list, or an address typed in). Remembers where, so Retry and
    /// the wait for a running match can ask again.
    fn join(&mut self, target: JoinTarget) {
        self.last_join = Some(target.clone());
        self.connect(&target.addr, &target.key, target.team, None, false, None);
        if let Some(s) = self.session.as_mut() {
            s.room = target.room;
        }
    }

    /// Show why joining failed: with Retry when there is somewhere to retry.
    fn show_failure(&mut self, message: String) {
        self.wait = None;
        self.session = None;
        self.screen = if self.last_join.is_some() { Screen::ConnectFailed(message) } else { Screen::Error(message) };
    }

    /// Fold the match into the lifetime stats (once) and store them.
    fn flush_match(&mut self, s: &mut Session, winner: Option<u8>) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        if s.tracker.finish(&mut self.stats, Some(s.my_team as u8), winner, now) {
            self.stats.store();
        }
    }

    fn connect(
        &mut self,
        addr: &str,
        key: &str,
        team: u8,
        server: Option<LocalServer>,
        solo: bool,
        hosting: Option<String>,
    ) {
        let target = if addr.contains(':') { addr.to_string() } else { format!("{addr}:{PORT}") };
        let resolved = target.to_socket_addrs().ok().and_then(|mut a| a.next());
        let Some(address) = resolved else {
            self.show_failure(format!("Cannot find a server at {target}"));
            return;
        };
        let name = self.prefs.display_name();
        let made = client_transport(TransportProfile::Development, address)
            .and_then(|t| NetClient::new(t, address, ClientConfig { name, key: key.to_string(), choice: team }));
        match made {
            Ok(client) => {
                let skin = (std::process::id() as u8) % 4;
                let mut skins = [0u8; 16];
                for (i, s) in skins.iter_mut().enumerate() {
                    *s = (i as u8 * 3 + skin) % 4;
                }
                self.session = Some(Session {
                    client,
                    _server: server,
                    tracker: MatchTracker::new(None),
                    feed: Vec::new(),
                    hit_marker: (0., false),
                    damage: Vec::new(),
                    notice: (String::new(), 0.),
                    killcam: None,
                    acc: 0.,
                    shake: 0.,
                    want_again: solo,
                    over_handled: false,
                    solo,
                    hosting,
                    room: None,
                    skins,
                    last_alive: false,
                    motion: render::ViewMotion::default(),
                    last_angles: (0., 0.),
                    my_team: team as usize,
                    kills_by_me: Vec::new(),
                    died_by_headshot: false,
                });
                self.screen = if self.wait.is_some() { Screen::Waiting } else { Screen::Connecting };
            }
            Err(e) => self.show_failure(format!("Could not start the network: {e}")),
        }
    }

    fn start_solo(&mut self, items: &[Item]) {
        let team = if let Some(Item::Choice(_, _, t)) = items.first() { *t as u8 } else { 0 };
        let (by_time, kills, minutes) = self.settings_from(items, 1, 2);
        let skill = if let Some(Item::Choice(_, _, s)) = items.get(3) { *s as u8 } else { 1 };
        self.last_join = None;
        self.prefs.team = team;
        self.prefs.end_by_time = by_time;
        self.prefs.kills_target = kills;
        self.prefs.minutes = minutes;
        self.prefs.bot_skill = skill;
        self.save_prefs();
        let settings = Settings {
            end: if by_time { EndRule::Time { minutes } } else { EndRule::Kills { target: kills } },
            bots: true,
            bot_skill: skill,
        };
        let cfg = ServerConfig {
            participants: 12,
            auto_start_seconds: 1,
            countdown_seconds: 3,
            results_seconds: 15,
            ..Default::default()
        };
        match LocalServer::start("127.0.0.1:0", cfg, settings) {
            Ok(server) => {
                let addr = server.addr.to_string();
                self.connect(&addr, "", team, Some(server), true, None);
            }
            Err(e) => self.screen = Screen::Error(e),
        }
    }

    fn start_host(&mut self, items: &[Item]) {
        let team = if let Some(Item::Choice(_, _, t)) = items.get(2) { *t as u8 } else { 0 };
        let (by_time, kills, minutes) = self.settings_from(items, 3, 4);
        let bots = matches!(items.get(5), Some(Item::Toggle(_, true)));
        let skill = if let Some(Item::Choice(_, _, s)) = items.get(6) { *s as u8 } else { 1 };
        let port: u16 = self.host_port.trim().parse().unwrap_or(PORT);
        self.last_join = None;
        self.prefs.team = team;
        self.prefs.end_by_time = by_time;
        self.prefs.kills_target = kills;
        self.prefs.minutes = minutes;
        self.prefs.bots = bots;
        self.prefs.bot_skill = skill;
        self.save_prefs();
        let key = self.host_key.trim().to_string();
        let settings = Settings {
            end: if by_time { EndRule::Time { minutes } } else { EndRule::Kills { target: kills } },
            bots,
            bot_skill: skill,
        };
        let cfg = ServerConfig {
            participants: 12,
            auto_start_seconds: 0,
            countdown_seconds: 5,
            results_seconds: 20,
            join_key: (!key.is_empty()).then(|| key.clone()),
            ..Default::default()
        };
        match LocalServer::start(&format!("0.0.0.0:{port}"), cfg, settings) {
            Ok(server) => {
                let hint = format!("Same Wi-Fi/LAN only: {}:{}", local_ip(), port);
                self.connect(&format!("127.0.0.1:{port}"), &key, team, Some(server), false, Some(hint));
            }
            Err(e) => self.screen = Screen::Error(format!("{e} (is port {port} already in use?)")),
        }
    }

    // ---- the frame ---------------------------------------------------------------------------------------------

    pub async fn run(&mut self) {
        loop {
            if self.frame(get_time() as f64).await {
                break;
            }
            next_frame().await;
        }
        self.leave();
        self.save_prefs();
    }

    /// One frame; true when the game should close.
    async fn frame(&mut self, now: f64) -> bool {
        let playing = self.screen == Screen::Playing && self.session.is_some();
        // The cursor stays captured for the whole match (killcam included) and is released for the results.
        let alive = playing
            && self
                .session
                .as_ref()
                .is_some_and(|s| !s.over_handled && s.client.view().latest().is_some_and(|l| !l.snap.over));
        self.input.begin_frame(&mut self.shell, alive, super::platform::focused());
        // Explicit scripted captures must keep playing when launched in a hidden Windows window.
        if self.capture_dir.is_some() && (self.controls.script.is_some() || self.autopilot) {
            self.shell.paused = false;
        }
        let dt = self.input.frame_seconds();
        self.time += dt;
        self.now = now;
        self.audio.poll().await;
        let mut nav =
            Nav::gather(self.input.menu_step(), self.input.menu_select(), self.input.menu_back(), &mut self.last_mouse);
        if let Some(sc) = self.controls.script.as_mut() {
            sc.frame += 1;
            nav.accept |= sc.pressed_now("accept");
            nav.down |= sc.pressed_now("down");
            nav.up |= sc.pressed_now("up");
            nav.back |= sc.pressed_now("back");
        }

        if let Some(screen) = self.autostart.take() {
            match screen {
                Screen::Solo => {
                    let items = self.solo_items.clone();
                    self.start_solo(&items);
                }
                Screen::Join => {
                    let target = JoinTarget {
                        addr: self.join_addr.clone(),
                        key: self.join_key.clone(),
                        team: self.prefs.team,
                        room: None,
                    };
                    self.join(target);
                }
                _ => {}
            }
        }
        if self.screen == Screen::Online {
            if let Some(Action::Join { addr, room }) = self.online.as_mut().and_then(|o| o.update(now)) {
                self.online = None;
                let team = self.prefs.team;
                let room = RoomTag { name: room.name, public: room.public };
                self.join(JoinTarget { addr: addr.to_string(), key: String::new(), team, room: Some(room) });
            }
        }
        if self.screen == Screen::Waiting && self.session.is_none() {
            self.retry_waiting(now);
        }
        if self.session.is_some() {
            self.pump_session(now, dt);
        }
        match self.screen.clone() {
            Screen::Playing => self.frame_match(now, dt, &nav),
            Screen::Lobby => self.frame_lobby(&nav, dt),
            Screen::Connecting => self.frame_connecting(&nav, dt),
            Screen::Waiting => self.frame_waiting(&nav, dt),
            other => self.frame_menu(other, &nav, dt),
        }
        // Screenshots for runs nobody can watch.
        self.frame += 1;
        if let Some(dir) = self.capture_dir.clone() {
            if self.capture_frames.contains(&self.frame) {
                let path = dir.join(format!("frame-{:04}.png", self.frame));
                match capture::save_frame(&path) {
                    Ok((w, h)) => println!("captured {} ({w}x{h})", path.display()),
                    Err(e) => eprintln!("capture failed: {e}"),
                }
            }
            if self.capture_frames.iter().all(|f| *f <= self.frame) {
                self.quit = true;
            }
        }
        self.quit || game_client::exit_requested()
    }

    // ---- menus -------------------------------------------------------------------------------------------------

    fn backdrop(&mut self, dt: f32) {
        // A slow flight over the map behind the menus.
        let t = self.time * 0.05;
        let level = crate::level();
        let (r, y) = (level.half_x.min(level.half_z) * 0.55, 18.);
        let eye = vec3((t).cos() * r, y, (t).sin() * r * 0.7);
        let target = vec3(0., 2., 0.);
        let d = (target - eye).normalize();
        let view = View {
            eye,
            yaw: d.x.atan2(-d.z),
            pitch: d.y.asin(),
            roll: 0.,
            fov: render::vfov(80., screen_width() / screen_height()),
        };
        self.renderer.update(dt);
        self.renderer.draw_world(&view, &[], None, u64::MAX, &[], &[], &[], &[0; 16], dt);
        draw_rectangle(0., 0., screen_width(), screen_height(), Color::new(0., 0., 0., 0.45));
    }

    fn title(&self, sub: &str) {
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        hud::text_centered("D E A D F A L L", cx, 120. * ui, 76. * ui, TEXT);
        hud::text_centered(sub, cx, 158. * ui, 22. * ui, DIM);
    }

    fn frame_menu(&mut self, screen: Screen, nav: &Nav, dt: f32) {
        self.backdrop(dt);
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        let back = |app: &mut App| {
            app.audio.ui(Sfx::MenuBack, 0.5);
            app.screen = Screen::Main;
        };
        match screen {
            Screen::Main => {
                self.title("team deathmatch");
                let mut items = vec![
                    Item::Button("Play Online".into()),
                    Item::Button("Solo".into()),
                    Item::Button("Host on this PC (advanced)".into()),
                    Item::Button("Join by address (advanced)".into()),
                    Item::Button("Stats".into()),
                    Item::Button("Settings".into()),
                    Item::Button("Quit".into()),
                ];
                let hit = self.menu.run(nav, &mut items, cx, 210., 440., dt);
                self.draw_build_label();
                if let Hit::Item(i) = hit {
                    self.audio.ui(Sfx::MenuConfirm, 0.5);
                    match i {
                        0 => self.open_online(),
                        1 => {
                            self.rebuild_items();
                            self.screen = Screen::Solo
                        }
                        2 => {
                            self.rebuild_items();
                            self.screen = Screen::Host
                        }
                        3 => {
                            self.rebuild_items();
                            self.screen = Screen::Join
                        }
                        4 => self.screen = Screen::Stats,
                        5 => {
                            self.rebuild_items();
                            self.screen = Screen::Settings
                        }
                        _ => self.quit = true,
                    }
                }
            }
            Screen::Solo => {
                self.title("solo: you and bots against bots");
                let mut items = std::mem::take(&mut self.solo_items);
                let hit = self.menu.run(nav, &mut items, cx, 200., 520., dt);
                self.sync_target_row(&mut items, 1, 2);
                match hit {
                    Hit::Item(5) => {
                        self.audio.ui(Sfx::MenuConfirm, 0.5);
                        self.start_solo(&items);
                    }
                    Hit::Item(6) | Hit::Back => back(self),
                    _ => {}
                }
                if self.screen == Screen::Solo {
                    self.solo_items = items;
                }
            }
            Screen::Host => {
                self.title("host a game for your friends");
                let mut items = std::mem::take(&mut self.host_items);
                let hit = self.menu.run(nav, &mut items, cx, 180., 560., dt);
                self.sync_target_row(&mut items, 3, 4);
                if let Some(Item::Text(_, v, _, _)) = items.first() {
                    self.host_port = v.clone();
                }
                if let Some(Item::Text(_, v, _, _)) = items.get(1) {
                    self.host_key = v.clone();
                }
                hud::text_centered("Friends far away? Use Play Online: no router setup. A hosted game needs this UDP port forwarded on your router.", cx, screen_height() - 14. * ui, 17. * ui, DIM);
                match hit {
                    Hit::Item(8) => {
                        self.audio.ui(Sfx::MenuConfirm, 0.5);
                        self.start_host(&items);
                    }
                    Hit::Item(9) | Hit::Back => back(self),
                    _ => {}
                }
                if self.screen == Screen::Host {
                    self.host_items = items;
                }
            }
            Screen::Join => {
                self.title("join a game");
                let mut items = std::mem::take(&mut self.join_items);
                let hit = self.menu.run(nav, &mut items, cx, 210., 560., dt);
                if let Some(Item::Text(_, v, _, _)) = items.first() {
                    self.join_addr = v.clone();
                }
                if let Some(Item::Text(_, v, _, _)) = items.get(1) {
                    self.join_key = v.clone();
                }
                let team = if let Some(Item::Choice(_, _, t)) = items.get(2) { *t as u8 } else { 0 };
                match hit {
                    Hit::Item(4) => {
                        self.prefs.team = team;
                        self.save_prefs();
                        self.audio.ui(Sfx::MenuConfirm, 0.5);
                        let target =
                            JoinTarget { addr: self.join_addr.clone(), key: self.join_key.clone(), team, room: None };
                        self.join(target);
                    }
                    Hit::Item(5) | Hit::Back => back(self),
                    _ => {}
                }
                if self.screen == Screen::Join {
                    self.join_items = items;
                }
            }
            Screen::Stats => {
                self.title("your record");
                self.draw_stats();
                if nav.back || nav.accept || nav.click {
                    back(self);
                }
            }
            Screen::Settings => {
                self.title("settings");
                let mut items = std::mem::take(&mut self.settings_items);
                let hit = self.menu.run(nav, &mut items, cx, 200., 560., dt);
                if let Some(Item::Text(_, v, _, _)) = items.first() {
                    self.prefs.name = v.chars().filter(|c| !c.is_control()).take(16).collect();
                }
                if let Some(Item::Slider(_, v, _, _)) = items.get(1) {
                    self.prefs.sensitivity = *v;
                }
                if let Some(Item::Slider(_, v, _, _)) = items.get(2) {
                    self.prefs.stick_sensitivity = *v;
                }
                if let Some(Item::Toggle(_, v)) = items.get(3) {
                    self.prefs.invert_y = *v;
                }
                if let Some(Item::Slider(_, v, _, _)) = items.get(4) {
                    self.prefs.volume = *v;
                    self.audio.set_volume(*v);
                }
                if let Some(Item::Toggle(_, v)) = items.get(5) {
                    if *v != self.prefs.fullscreen {
                        self.prefs.fullscreen = *v;
                        set_fullscreen(*v);
                    }
                }
                if let Some(Item::Choice(_, _, k)) = items.get(6) {
                    let q = ShadowQuality::ALL[(*k).min(2)];
                    if q != self.shadow_choice {
                        self.shadow_choice = q;
                        self.prefs.shadows = q;
                        self.renderer.set_shadows(q);
                    }
                }
                hud::text_centered("Keyboard: WASD move, mouse look, LMB fire, RMB aim, R reload, E use, Ctrl crouch, 1-4 weapons, Tab scores", cx, screen_height() - 60. * ui, 17. * ui, DIM);
                hud::text_centered("Controller: sticks, RT fire, LT aim, A jump, B crouch, X reload, RB use, D-pad weapons, Back scores", cx, screen_height() - 36. * ui, 17. * ui, DIM);
                if matches!(hit, Hit::Item(8) | Hit::Back) {
                    self.prefs.sanitize();
                    self.prefs.store();
                    back(self);
                } else {
                    self.settings_items = items;
                }
            }
            Screen::Error(message) => {
                self.title("");
                let top = self.draw_failure(&message);
                let mut items = vec![Item::Button("Back".into())];
                if matches!(self.menu.run(nav, &mut items, cx, top, 300., dt), Hit::Item(_) | Hit::Back) {
                    back(self);
                }
            }
            Screen::ConnectFailed(message) => {
                self.title("");
                let top = self.draw_failure(&message);
                let mut items = vec![Item::Button("Retry".into()), Item::Button("Back".into())];
                match self.menu.run(nav, &mut items, cx, top, 300., dt) {
                    Hit::Item(0) => {
                        self.audio.ui(Sfx::MenuConfirm, 0.5);
                        if let Some(target) = self.last_join.clone() {
                            self.join(target);
                        } else {
                            back(self);
                        }
                    }
                    Hit::Item(_) | Hit::Back => {
                        // A failed room goes back to the room list, an address back to the main menu.
                        let from_list = self.last_join.take().is_some_and(|t| t.room.is_some());
                        if from_list {
                            self.audio.ui(Sfx::MenuBack, 0.5);
                            self.open_online();
                        } else {
                            back(self);
                        }
                    }
                    _ => {}
                }
            }
            Screen::Online => self.frame_online(nav, dt),
            Screen::Connecting | Screen::Waiting | Screen::Lobby | Screen::Playing => {}
        }
    }

    /// Keep the kill/time target row in step with "Ends".
    fn sync_target_row(&mut self, items: &mut [Item], ends: usize, target: usize) {
        let time = matches!(items.get(ends), Some(Item::Choice(_, _, 1)));
        let showing_time = matches!(items.get(target), Some(Item::Choice(l, _, _)) if l == "Length");
        if time != showing_time {
            items[target] = Item::Choice(
                if time { "Length".into() } else { "Kills to win".into() },
                target_labels(time),
                if time { pick(&MINUTES, self.prefs.minutes) } else { pick(&KILL_TARGETS, self.prefs.kills_target) },
            );
        }
    }

    fn draw_stats(&self) {
        let ui = hud::ui_scale();
        let s = &self.stats;
        let cx = screen_width() * 0.5;
        let w = 720. * ui;
        ui::panel(cx - w * 0.5, 190. * ui, w, 460. * ui, "LIFETIME");
        let fav = s.favourite_weapon().map_or("-".to_string(), |(k, n)| {
            format!("{} ({n} kills)", weapons::WEAPONS.iter().find(|w| w.key == k).map_or(k, |w| w.name))
        });
        let rows = [
            ("Kills", format!("{}", s.kills)),
            ("Deaths", format!("{}", s.deaths)),
            ("Kill / death", format!("{:.2}", s.kd())),
            ("Headshots", format!("{}  ({:.0}% of kills)", s.headshots, s.headshot_rate() * 100.)),
            ("Accuracy", format!("{:.0}%", s.accuracy() * 100.)),
            ("Damage dealt", format!("{}", s.damage)),
            ("Matches played", format!("{}", s.matches)),
            ("Won / lost / drawn", format!("{} / {} / {}   (left early: {})", s.wins, s.losses, s.draws, s.left_early)),
            ("Time played", ui::duration(s.seconds_played)),
            ("Best kill streak", format!("{}", s.best_streak)),
            ("Most kills in a match", format!("{}", s.best_match_kills)),
            ("Favourite weapon", fav),
        ];
        for (i, (k, v)) in rows.iter().enumerate() {
            let y = 250. * ui + i as f32 * 30. * ui;
            hud::text_outlined(k, cx - w * 0.5 + 24. * ui, y, 22. * ui, DIM);
            hud::text_right(v, cx + w * 0.5 - 24. * ui, y, 22. * ui, TEXT);
        }
        hud::text_centered("press any key to go back", cx, 650. * ui, 18. * ui, DIM);
    }

    /// The build id in the corner, so two players can see at a glance whether they run the same version.
    fn draw_build_label(&self) {
        let ui = hud::ui_scale();
        let mut c = DIM;
        c.a = 0.7;
        hud::text_right(
            &online::build_label(online::local_build()),
            screen_width() - 14. * ui,
            screen_height() - 12. * ui,
            15. * ui,
            c,
        );
    }

    /// A failure message wrapped in the middle of the screen; returns where the buttons under it go (in 720p units).
    fn draw_failure(&self, message: &str) -> f32 {
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        let size = 26. * ui;
        let lines = ui::wrap(message, (screen_width() - 120. * ui).min(900. * ui), size);
        let y0 = screen_height() * 0.38;
        for (i, line) in lines.iter().enumerate() {
            hud::text_centered(line, cx, y0 + i as f32 * 36. * ui, size, Color::new(1., 0.55, 0.5, 1.));
        }
        (y0 + lines.len() as f32 * 36. * ui + 24. * ui) / ui
    }

    /// Play Online: the room list, or what stands in its way.
    fn frame_online(&mut self, nav: &Nav, dt: f32) {
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        let warn = Color::new(1., 0.55, 0.5, 1.);
        self.title("play online");
        self.draw_build_label();
        let Some(mut online) = self.online.take() else {
            self.screen = Screen::Main;
            return;
        };
        if online.dialog.is_some() {
            self.frame_create_dialog(&mut online, nav, dt);
            self.online = Some(online);
            return;
        }
        let visible = online::visible_rows(screen_height(), ui);
        let mismatch = online.mismatch();
        let selectable = online.can_join();
        let total = online.rooms.len();
        let showing_rooms = online.view == Pane::Rooms && total > 0;
        // Rows (a window onto the list) first, then the buttons.
        let mut nav = nav.clone();
        let mut sel_idx =
            online.selected.as_ref().and_then(|n| online.rooms.iter().position(|r| &r.name == n)).unwrap_or(0);
        let mut offset = online::scroll_to(sel_idx, online.offset, visible, total);
        let mut nrows = if showing_rooms { visible.min(total) } else { 0 };
        if showing_rooms && selectable && self.online_on_row {
            // The highlight rides the room, not the screen position, across refreshes and scrolling.
            let local = sel_idx - offset;
            if nav.down && local + 1 == nrows && offset + nrows < total {
                sel_idx += 1;
                nav.down = false;
            } else if nav.up && local == 0 && offset > 0 {
                sel_idx -= 1;
                nav.up = false;
            }
            offset = online::scroll_to(sel_idx, offset, visible, total);
            self.online_menu.sel = sel_idx - offset;
        }
        nrows = nrows.min(total.saturating_sub(offset));
        let mut items: Vec<Item> = Vec::new();
        if showing_rooms {
            for r in &online.rooms[offset..offset + nrows] {
                let status = online::room_status(r);
                items.push(if selectable {
                    Item::Row(r.name.clone(), status)
                } else {
                    Item::Label(format!("{}    {status}", r.name))
                });
            }
        }
        items.push(Item::Gap);
        #[derive(Clone, Copy, PartialEq)]
        enum Btn {
            Join,
            Create,
            Refresh,
            Retry,
            Address,
            Back,
        }
        let buttons: Vec<(Btn, &str)> = match &online.view {
            Pane::Looking => vec![(Btn::Back, "Back")],
            Pane::Rooms if mismatch => vec![(Btn::Refresh, "Refresh"), (Btn::Back, "Back")],
            Pane::Rooms => {
                vec![(Btn::Join, "Join"), (Btn::Create, "Create Room"), (Btn::Refresh, "Refresh"), (Btn::Back, "Back")]
            }
            Pane::Offline | Pane::Problem(_) => {
                vec![(Btn::Retry, "Retry"), (Btn::Address, "Join by address"), (Btn::Back, "Back")]
            }
        };
        items.extend(buttons.iter().map(|(_, t)| Item::Button((*t).to_string())));
        // The message above the list.
        let mut top = 215.;
        let say = |text: &str, size: f32, c: Color, top: &mut f32| {
            for line in ui::wrap(text, (screen_width() - 120. * ui).min(900. * ui), size * ui) {
                hud::text_centered(&line, cx, (*top - 10.) * ui, size * ui, c);
                *top += size * 1.4;
            }
        };
        match &online.view {
            Pane::Looking => say("Looking for games...", 28., TEXT, &mut top),
            Pane::Rooms if mismatch => say(online::UPDATE_TEXT, 26., warn, &mut top),
            Pane::Rooms => {}
            Pane::Offline => {
                say(
                    "Can't reach the Deadfall servers right now. Check your internet connection, then Retry.",
                    26.,
                    warn,
                    &mut top,
                );
                if let Some(d) = &online.detail {
                    say(d, 18., DIM, &mut top);
                } else {
                    say(&format!("({})", online.hub_spec()), 18., DIM, &mut top);
                }
                top += 14.;
            }
            Pane::Problem(text) => {
                say(text, 26., warn, &mut top);
                top += 14.;
            }
        }
        if mismatch {
            top += 6.;
        }
        let list_top = top;
        let hit = self.online_menu.run(&nav, &mut items, cx, list_top, 680., dt);
        // Remember the highlighted room by name.
        if self.online_menu.sel < nrows && selectable {
            sel_idx = offset + self.online_menu.sel;
            online.selected = online.rooms.get(sel_idx).map(|r| r.name.clone());
            self.online_on_row = true;
        } else {
            self.online_on_row = false;
        }
        online.offset = offset;
        if showing_rooms && total > nrows {
            let below = total - offset - nrows;
            let text = format!("{} more above, {} more below", offset, below);
            hud::text_centered(&text, cx, (list_top + nrows as f32 * 46. + 14.) * ui, 15. * ui, DIM);
        }
        let mut action = None;
        if let Hit::Item(i) = hit {
            self.audio.ui(Sfx::MenuConfirm, 0.5);
            if i < nrows {
                action = online.join_index(offset + i);
            } else if let Some((btn, _)) = i.checked_sub(nrows + 1).and_then(|k| buttons.get(k)) {
                match btn {
                    Btn::Join => action = online.join_selected(),
                    Btn::Create => {
                        online.open_dialog(&self.prefs.display_name());
                        self.dialog_menu = Menu::new();
                    }
                    Btn::Refresh | Btn::Retry => online.refresh(self.now),
                    Btn::Address => {
                        self.online = None;
                        self.rebuild_items();
                        self.screen = Screen::Join;
                        return;
                    }
                    Btn::Back => {
                        self.audio.ui(Sfx::MenuBack, 0.5);
                        self.screen = Screen::Main;
                        return;
                    }
                }
            }
        } else if hit == Hit::Back {
            self.audio.ui(Sfx::MenuBack, 0.5);
            self.screen = Screen::Main;
            return;
        }
        if let Some(Action::Join { addr, room }) = action {
            let team = self.prefs.team;
            let room = RoomTag { name: room.name, public: room.public };
            self.join(JoinTarget { addr: addr.to_string(), key: String::new(), team, room: Some(room) });
            return;
        }
        self.online = Some(online);
    }

    /// The Create Room box, drawn over the dimmed menu background.
    fn frame_create_dialog(&mut self, online: &mut Online, nav: &Nav, dt: f32) {
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        let w = 700. * ui;
        let y = 215. * ui;
        draw_rectangle(0., 0., screen_width(), screen_height(), Color::new(0., 0., 0., 0.35));
        ui::panel(cx - w * 0.5, y, w, 330. * ui, "CREATE A ROOM");
        hud::text_centered(
            "Friends will see this name under Play Online and join with one click.",
            cx,
            y + 62. * ui,
            19. * ui,
            DIM,
        );
        let Some(dialog) = online.dialog.as_mut() else { return };
        let mut close = false;
        match dialog.phase {
            DialogPhase::Editing => {
                let mut items = vec![
                    Item::Text("Room name".into(), dialog.name.clone(), "name", TextRules::ROOM),
                    Item::Gap,
                    Item::Button("Create".into()),
                    Item::Button("Cancel".into()),
                ];
                let enter_in_field = nav.accept && self.dialog_menu.sel == 0;
                let hit = self.dialog_menu.run(nav, &mut items, cx, y / ui + 84., 640., dt);
                if let Some(Item::Text(_, v, _, _)) = items.first() {
                    if *v != dialog.name {
                        dialog.name = v.clone();
                        dialog.error = None;
                    }
                }
                if let Some(e) = &dialog.error {
                    for (i, line) in ui::wrap(e, w - 60. * ui, 19. * ui).iter().enumerate() {
                        hud::text_centered(
                            line,
                            cx,
                            y + 292. * ui + i as f32 * 24. * ui,
                            19. * ui,
                            Color::new(1., 0.55, 0.5, 1.),
                        );
                    }
                }
                match hit {
                    Hit::Item(2) => {
                        self.audio.ui(Sfx::MenuConfirm, 0.5);
                        online.create();
                    }
                    Hit::Item(3) | Hit::Back => close = true,
                    _ if enter_in_field => online.create(),
                    _ => {}
                }
            }
            DialogPhase::Creating => {
                let dots = ".".repeat(1 + (self.time * 2.) as usize % 3);
                hud::text_centered(&format!("Creating your room{dots}"), cx, y + 120. * ui, 28. * ui, TEXT);
                let mut items = vec![Item::Button("Cancel".into())];
                if matches!(
                    self.dialog_menu.run(nav, &mut items, cx, y / ui + 150., 300., dt),
                    Hit::Item(_) | Hit::Back
                ) {
                    close = true;
                }
            }
        }
        if close {
            self.audio.ui(Sfx::MenuBack, 0.5);
            online.close_dialog();
        }
    }

    /// "A match is running": ask the lobby again on schedule.
    fn retry_waiting(&mut self, now: f64) {
        let Some(wait) = self.wait.as_mut() else { return };
        match wait.step(now) {
            RetryStep::Wait => {}
            RetryStep::TryNow => {
                wait.attempted(now);
                match self.last_join.clone() {
                    Some(target) => self.join(target),
                    None => self.wait = None,
                }
            }
            RetryStep::GiveUp => {
                self.show_failure("The match is still running. Try again in a little while.".into());
            }
        }
    }

    fn frame_waiting(&mut self, nav: &Nav, dt: f32) {
        self.backdrop(dt);
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        self.title("");
        hud::text_centered(
            "A match is running. You'll be in the next round.",
            cx,
            screen_height() * 0.43,
            30. * ui,
            TEXT,
        );
        let dots = ".".repeat(1 + (self.time * 2.) as usize % 3);
        hud::text_centered(
            &format!("Trying again every few seconds{dots}"),
            cx,
            screen_height() * 0.43 + 40. * ui,
            20. * ui,
            DIM,
        );
        let mut items = vec![Item::Button("Cancel".into())];
        if matches!(self.menu.run(nav, &mut items, cx, screen_height() / ui * 0.55, 300., dt), Hit::Item(_) | Hit::Back)
        {
            self.leave();
        }
    }

    fn frame_connecting(&mut self, nav: &Nav, dt: f32) {
        self.backdrop(dt);
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        self.title("");
        let dots = ".".repeat(1 + (self.time * 2.) as usize % 3);
        let what = match self.session.as_ref().and_then(|s| s.room.as_ref()) {
            Some(room) => format!("Connecting to {}{dots}", room.name),
            None => format!("Connecting{dots}"),
        };
        hud::text_centered(&what, cx, screen_height() * 0.45, 30. * ui, TEXT);
        let mut items = vec![Item::Button("Cancel".into())];
        if matches!(self.menu.run(nav, &mut items, cx, screen_height() / ui * 0.55, 300., dt), Hit::Item(_) | Hit::Back)
        {
            self.leave();
        }
    }

    fn frame_lobby(&mut self, nav: &Nav, dt: f32) {
        self.backdrop(dt);
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        let Some(session) = self.session.as_mut() else { return };
        let room = session.room.clone();
        match &room {
            Some(r) if !r.public => {
                self.title(&format!("Room: {}", r.name));
                hud::text_centered(
                    &format!("Friends: open Deadfall > Play Online > pick '{}'", r.name),
                    cx,
                    184. * ui,
                    19. * ui,
                    ACCENT,
                );
            }
            Some(_) => {
                self.title("Public room");
                hud::text_centered(
                    "Public room: anyone who clicks Play Online lands here",
                    cx,
                    184. * ui,
                    19. * ui,
                    ACCENT,
                );
            }
            None => self.title("lobby"),
        }
        let Some(session) = self.session.as_mut() else { return };
        let lobby = session.client.lobby().cloned();
        let me = session.client.seat();
        let mine = lobby.as_ref().and_then(|l| l.entries.iter().find(|e| e.slot == me)).cloned();
        let my_team = mine.as_ref().map_or(session.my_team as u8, |e| e.choice);
        session.my_team = my_team as usize;
        let ready = mine.as_ref().is_some_and(|e| e.ready);
        // Team columns, sized to the window.
        let w = 340. * ui;
        let py = if room.is_some() { 200. * ui } else { 190. * ui };
        let ph = (screen_height() - py - 250. * ui).max(150. * ui);
        let mut counts = [0usize; 2];
        for t in 0..2 {
            let x = cx - w - 16. * ui + t as f32 * (w + 32. * ui);
            ui::panel(x, py, w, ph, crate::Team::from_index(t).name());
            let mut n = 0;
            if let Some(l) = &lobby {
                for e in l.entries.iter().filter(|e| e.choice as usize == t) {
                    let mut c = if e.slot == me { ACCENT } else { TEXT };
                    if !e.ready {
                        c.a = 0.7;
                    }
                    hud::text_outlined(
                        &format!("{}{}", e.name, if e.ready { "  (ready)" } else { "" }),
                        x + 18. * ui,
                        py + 70. * ui + n as f32 * 30. * ui,
                        22. * ui,
                        c,
                    );
                    n += 1;
                }
            }
            counts[t] = n;
        }
        if session.solo && session.want_again && !ready {
            session.client.ready(true);
        }
        let waiting = lobby.as_ref().map_or(0, |l| l.entries.len());
        let status = match &lobby {
            Some(l) if l.seconds_left > 0 => format!("Match starts in {}", l.seconds_left),
            _ => format!("{waiting} in the lobby. The match starts when everyone is ready."),
        };
        hud::text_centered(&status, cx, py + ph + 30. * ui, 22. * ui, TEXT);
        if let Some(h) = &session.hosting {
            hud::text_centered(h, cx, py + ph + 60. * ui, 22. * ui, ACCENT);
        }
        let mut items = vec![
            Item::Choice("Team".into(), vec!["Ironclad".into(), "Nightwatch".into()], my_team as usize),
            Item::Button(if ready { "Not ready".into() } else { "Ready".into() }),
            Item::Button("Leave".into()),
        ];
        let hit = self.menu.run(nav, &mut items, cx, (py + ph + 80. * ui) / ui, 420., dt);
        let session = self.session.as_mut().expect("the session exists");
        match hit {
            Hit::Item(0) => {
                if let Item::Choice(_, _, t) = &items[0] {
                    // A full team (six) cannot be joined.
                    if counts[*t] >= crate::sim::TEAM_SIZE && *t as u8 != my_team {
                        session.notice = ("That team is full".into(), 2.);
                    } else {
                        session.client.select(*t as u8);
                        self.prefs.team = *t as u8;
                    }
                }
                self.audio.ui(Sfx::MenuClick, 0.5);
            }
            Hit::Item(1) => {
                session.client.ready(!ready);
                self.audio.ui(Sfx::MenuConfirm, 0.5);
            }
            Hit::Item(2) | Hit::Back => self.leave(),
            _ => {}
        }
        if let Some(s) = self.session.as_mut() {
            if s.notice.1 > 0. {
                overlay::notice(&s.notice.0, s.notice.1);
                s.notice.1 -= dt;
            }
        }
    }

    // ---- the network -------------------------------------------------------------------------------------------

    fn pump_session(&mut self, now: f64, dt: f32) {
        let Some(s) = self.session.as_mut() else { return };
        s.client.poll(now);
        match s.client.state().clone() {
            ClientState::Connecting => {}
            ClientState::Lobby => {
                if matches!(self.screen, Screen::Connecting | Screen::Waiting | Screen::Playing) {
                    self.wait = None;
                    if self.screen == Screen::Playing {
                        // Back from the results.
                        s.tracker = MatchTracker::new(None);
                        s.over_handled = false;
                        s.killcam = None;
                        self.renderer.clear_match();
                        self.audio.clear_match_state();
                        if s.want_again {
                            s.client.ready(true);
                        }
                    }
                    // A room joined on a hub the player named with --hub: use that hub again next time.
                    if matches!(self.screen, Screen::Connecting | Screen::Waiting)
                        && self.hub_origin == ServerOrigin::CliArg
                        && self.last_join.as_ref().is_some_and(|t| t.room.is_some())
                        && self.prefs.last_hub.as_deref() != Some(self.hub_spec.as_str())
                    {
                        self.prefs.last_hub = Some(self.hub_spec.clone());
                        self.prefs.store();
                    }
                    self.screen = Screen::Lobby;
                    if s.solo {
                        s.client.ready(true);
                    }
                }
            }
            ClientState::Playing => {
                if self.screen != Screen::Playing {
                    self.screen = Screen::Playing;
                    s.tracker = MatchTracker::new(s.client.participant().map(|p| p as u8));
                    s.over_handled = false;
                    s.killcam = None;
                    s.feed.clear();
                    s.last_alive = false;
                    self.renderer.clear_match();
                    self.controls.face(0., 0.);
                    self.audio.ui(Sfx::MatchStart, 0.7);
                }
            }
            ClientState::Rejected(why) => {
                let from_list = self.last_join.as_ref().is_some_and(|t| t.room.is_some());
                // The engine client says why (nobody answered, or the server's reason sorted into a case).
                let failure = s.client.failure().unwrap_or_else(|| ConnectFailure::classify(&why));
                self.session.take();
                match failure {
                    // The lobby takes us after this round: keep asking, quietly.
                    ConnectFailure::MatchInProgress if self.last_join.is_some() => {
                        if self.wait.is_none() {
                            self.wait = Some(JoinWait::new(now));
                        }
                        self.screen = Screen::Waiting;
                    }
                    other => self.show_failure(online::failure_message(&other, from_list)),
                }
                return;
            }
            ClientState::Disconnected(why) => {
                let msg = format!("Disconnected: {why}");
                if let Some(mut sess) = self.session.take() {
                    self.flush_match(&mut sess, None);
                }
                self.show_failure(msg);
                return;
            }
        }
        let Some(s) = self.session.as_mut() else { return };
        s.client.frame(now, dt);
        let events = s.client.drain_events();
        if !events.is_empty() {
            self.handle_events(events);
        }
    }

    fn handle_events(&mut self, events: Vec<Event>) {
        let Some(s) = self.session.as_mut() else { return };
        s.client.view_mut().remember_events(&events);
        let me = s.client.participant().map(|p| p as u8);
        s.tracker.me = me;
        for e in &events {
            if let Event::Roster(entries) = e {
                s.client.view_mut().roster = entries.clone();
                // The server may have moved us to the other team to keep them at six a side.
                if let Some(me) = s.client.participant() {
                    if let Some(r) = entries.iter().find(|r| r.slot as usize == me) {
                        s.my_team = r.team as usize;
                    }
                }
            }
        }
        let view = s.client.view();
        let level = crate::level();
        let listener = match view.eye() {
            Some(eye) => Listener { pos: eye, yaw: self.controls.yaw },
            None => Listener { pos: V::ZERO, yaw: 0. },
        };
        let render_tick = view.render_tick();
        let positions = view.players_at(render_tick);
        let pos_of = |slot: u8| positions.iter().find(|p| p.slot == slot).map(|p| p.eye);
        let names: Vec<(u8, String, usize)> =
            view.roster.iter().map(|r| (r.slot, r.name.clone(), r.team as usize)).collect();
        let name_of = |slot: u8| names.iter().find(|n| n.0 == slot).map_or("?".to_string(), |n| n.1.clone());
        let team_of = |slot: u8| names.iter().find(|n| n.0 == slot).map_or(0, |n| n.2);
        let my_yaw = self.controls.yaw;
        for e in &events {
            s.tracker.on_event(e);
            // During a replay, combat effects come from history, not the ongoing match.
            if s.client.view().own.as_ref().is_some_and(|o| !o.alive)
                && matches!(
                    e,
                    Event::Shot { .. }
                        | Event::Hurt { .. }
                        | Event::Blast { .. }
                        | Event::Strike { .. }
                        | Event::Launch { .. }
                        | Event::Throw { .. }
                )
            {
                continue;
            }
            match e {
                Event::Roster(entries) => {
                    // Kept by the view from here on.
                    let _ = entries;
                }
                Event::Shot { shooter, weapon, from, to, hit: kind, material } => {
                    let mine = Some(*shooter) == me;
                    let heavy = weapons::get(*weapon)
                        .is_some_and(|d| matches!(d.class, weapons::Class::Sniper | weapons::Class::Dmr));
                    self.renderer.tracer(*from, *to, heavy);
                    let key = weapons::get(*weapon).map_or("", |d| d.key);
                    if !mine {
                        if let Some(sfx) = audio::shot_for_key(key) {
                            let reach = weapons::get(*weapon).map_or(120., |d| (d.range_m * 2.5).max(90.));
                            self.audio.at(sfx, *from, &listener, 1., reach);
                        }
                        self.renderer.flash(*from);
                        // A bullet going past your head.
                        if let Some(eye) = Some(listener.pos) {
                            let d = *to - *from;
                            let len = d.length().max(0.01);
                            let t = ((eye - *from).dot(d) / (len * len)).clamp(0., 1.);
                            let closest = *from + d * t;
                            if (closest - eye).length() < 1.6 {
                                self.audio.at(Sfx::BulletWhiz, closest, &listener, 0.8, 10.);
                            }
                        }
                    }
                    let c = to_v3(*to);
                    match *kind {
                        hit::WORLD => {
                            let surface = match material {
                                1 => audio::Surface::Metal,
                                2 => audio::Surface::Wood,
                                3 => audio::Surface::Glass,
                                4 => audio::Surface::Dirt,
                                _ => audio::Surface::Concrete,
                            };
                            self.audio.at(audio::impact(surface), *to, &listener, 0.7, 40.);
                            let col = match material {
                                1 => [1., 0.85, 0.5],
                                2 => [0.8, 0.6, 0.35],
                                4 => [0.55, 0.45, 0.3],
                                _ => [0.75, 0.75, 0.72],
                            };
                            self.renderer.fx.sparks(c, 5, 3.5, [1., 0.9, 0.6]);
                            self.renderer.fx.dust(c, 4, 0.4, col);
                        }
                        hit::BODY | hit::HEAD => {
                            let sfx = if *kind == hit::HEAD { Sfx::HelmetClank } else { Sfx::ImpactFlesh };
                            self.audio.at(sfx, *to, &listener, 0.8, 40.);
                            self.renderer.fx.dust(c, 4, 0.25, [0.7, 0.7, 0.7]);
                        }
                        _ => {}
                    }
                }
                Event::Hurt { victim, attacker, damage, head, from, .. } => {
                    if Some(*victim) == me {
                        self.audio.ui(Sfx::HurtThud, 0.8);
                        let d = *from - listener.pos;
                        let angle = d.0.atan2(-d.2) - my_yaw;
                        s.damage.push((angle, 1.));
                        s.shake = s.shake.max((*damage as f32 / 60.).min(1.) * 0.5);
                    }
                    if Some(*attacker) == me && *victim != *attacker {
                        s.hit_marker = (1., *head);
                        self.audio.ui(if *head { Sfx::HitConfirmHead } else { Sfx::HitConfirm }, 0.6);
                    }
                }
                Event::Kill { killer, victim, weapon, head } => {
                    let key = weapons::get(*weapon).map_or("world", |d| d.name);
                    let kname = if *killer == 255 { "a fall".to_string() } else { name_of(*killer) };
                    let line =
                        format!("{} [{}{}] {}", kname, key, if *head { ", headshot" } else { "" }, name_of(*victim));
                    s.feed.push(Feed {
                        text: line,
                        age: 0.,
                        mine: Some(*killer) == me || Some(*victim) == me,
                        team: if *killer == 255 { team_of(*victim) } else { team_of(*killer) },
                    });
                    if Some(*victim) == me {
                        s.died_by_headshot = *head;
                    }
                    if Some(*killer) == me && *victim != *killer {
                        s.notice = (format!("eliminated {}", name_of(*victim)), 2.2);
                        s.kills_by_me.push((*victim, 0.));
                    }
                    if let Some(p) = pos_of(*victim) {
                        self.audio.at(Sfx::BodyFall, p - V(0., 1.2, 0.), &listener, 0.7, 30.);
                    }
                }
                Event::Blast { pos, kind, radius } => {
                    let c = to_v3(*pos);
                    let d = (*pos - listener.pos).length();
                    match kind {
                        0 => {
                            self.audio.at(
                                if d < 50. { Sfx::Explosion } else { Sfx::ExplosionDistant },
                                *pos,
                                &listener,
                                1.,
                                160.,
                            );
                            self.renderer.fx.fireball(c + vec3(0., 0.6, 0.), *radius * 0.6, 0.6, [1., 0.65, 0.25]);
                            self.renderer.fx.ring(c, Vec3::Y, 0.3, *radius, 0.45, [1., 0.8, 0.5]);
                            self.renderer.fx.sparks(c + vec3(0., 0.4, 0.), 40, 9., [1., 0.7, 0.3]);
                            self.renderer.fx.dust(c, 18, *radius * 0.5, [0.45, 0.42, 0.4]);
                            self.renderer.flash(*pos);
                            s.shake = s.shake.max((1. - d / 40.).clamp(0., 1.));
                        }
                        1 => {
                            self.audio.at(Sfx::FlashBang, *pos, &listener, 1., 80.);
                            self.renderer.flash(*pos);
                        }
                        2 => self.audio.at(Sfx::SmokePop, *pos, &listener, 0.8, 50.),
                        _ => self.audio.at(Sfx::IncenIgnite, *pos, &listener, 0.9, 50.),
                    }
                }
                Event::Strike { attacker, weapon, heavy, hit: landed } => {
                    if let Some(p) = pos_of(*attacker) {
                        let key = weapons::get(*weapon).map_or("knife", |d| d.key);
                        let swing = if *heavy { Sfx::HeavySwing } else { Sfx::KnifeSwing };
                        if Some(*attacker) != me {
                            self.audio.at(swing, p, &listener, 0.7, 25.);
                        }
                        if *landed {
                            self.audio.at(
                                audio::melee_hit_for_key(key).unwrap_or(Sfx::KnifeHit),
                                p,
                                &listener,
                                0.9,
                                30.,
                            );
                        }
                    }
                }
                Event::Launch { shooter, weapon, from } => {
                    if Some(*shooter) != me {
                        let key = weapons::get(*weapon).map_or("rpg", |d| d.key);
                        if let Some(sfx) = audio::shot_for_key(key) {
                            self.audio.at(sfx, *from, &listener, 1., 120.);
                        }
                    }
                }
                Event::Throw { thrower, .. } => {
                    if Some(*thrower) != me {
                        if let Some(p) = pos_of(*thrower) {
                            self.audio.at(Sfx::Throw, p, &listener, 0.7, 25.);
                        }
                    }
                }
                Event::Pickup { player, .. } => {
                    if Some(*player) == me {
                        self.audio.ui(Sfx::PickupWeapon, 0.7);
                    } else if let Some(p) = pos_of(*player) {
                        self.audio.at(Sfx::PickupWeapon, p, &listener, 0.5, 20.);
                    }
                }
                Event::Dropped { .. } => {}
                Event::Spawned { player } => {
                    if Some(*player) == me {
                        self.audio.ui(Sfx::Respawn, 0.6);
                    }
                }
                Event::Over { .. } => self.audio.ui(Sfx::MatchEnd, 0.8),
            }
        }
        let _ = level;
    }

    // ---- the match ---------------------------------------------------------------------------------------------

    fn frame_match(&mut self, now: f64, dt: f32, nav: &Nav) {
        let Some(mut s) = self.session.take() else { return };
        let view_state = s.client.view();
        let own = view_state.own.clone();
        let latest = view_state.latest().map(|l| l.snap.clone());
        let Some(snap) = latest else {
            self.backdrop(dt);
            hud::text_centered(
                "Waiting for the first snapshot...",
                screen_width() * 0.5,
                screen_height() * 0.5,
                28. * hud::ui_scale(),
                TEXT,
            );
            self.session = Some(s);
            return;
        };
        let me_slot = s.client.participant();
        let alive = own.as_ref().is_some_and(|o| o.alive);
        let over = snap.over;
        let prefs = self.prefs.clone();
        let ads_ratio = {
            let v = s.client.view();
            weapons::get(v.hands.weapon(&v.inv)).map_or(1., |d| 1. + (d.ads_fov / 90. - 1.) * v.hands.ads)
        };
        let has = {
            let v = s.client.view();
            [v.inv.primary.is_some(), v.inv.secondary.is_some(), true, v.inv.grenades[0] != 0]
        };

        // Input: read the devices, then send one input per fixed tick (neutral while a menu is open or we are dead).
        let live = self.shell.accepting_input() && alive && !over;
        if live {
            self.controls.frame(&self.input, &self.shell, dt, &prefs, ads_ratio, has);
        } else {
            self.controls.frame(&self.input, &self.shell, dt, &prefs, 1., has);
        }
        // Start facing where the server put us.
        if alive && !s.last_alive {
            s.killcam = None;
            s.motion = render::ViewMotion::default();
            if let Some(o) = &own {
                self.controls.face(o.ctrl.yaw, 0.);
                s.last_angles = (self.controls.yaw, self.controls.pitch);
                self.controls.sync_counters(
                    o.hands.seen.reload,
                    o.hands.seen.use_,
                    o.hands.seen.melee,
                    o.hands.seen.drop,
                    o.hands.seen.switch,
                );
            }
        }
        s.last_alive = alive;
        s.acc = (s.acc + dt).min(0.25);
        let render_tick = s.client.view().render_tick().max(0.);
        while s.acc >= TICK {
            s.acc -= TICK;
            let mut input = self.controls.tick(render_tick as u16);
            if !live {
                input.buttons = 0;
                input.right = 0;
                input.forward = 0;
            }
            s.client.tick(input);
            // The local player's own immediate effects (muzzle flash, sound, recoil).
            let locals: Vec<Local> = std::mem::take(&mut s.client.view_mut().local);
            for l in locals {
                self.local_effect(&mut s, l);
            }
        }

        // Stats.
        if !over {
            s.tracker.tick(dt);
            self.play_seconds_unsaved += dt;
        }
        if over && !s.over_handled {
            s.over_handled = true;
            let winner = Some(snap.winner);
            self.flush_match(&mut s, winner);
        }

        // Timers.
        s.hit_marker.0 = (s.hit_marker.0 - dt * 4.).max(0.);
        s.damage.iter_mut().for_each(|d| d.1 -= dt * 1.2);
        s.damage.retain(|d| d.1 > 0.);
        s.notice.1 -= dt;
        s.feed.iter_mut().for_each(|f| f.age += dt);
        s.feed.retain(|f| f.age < 7.);
        s.shake = (s.shake - dt * 2.5).max(0.);
        self.renderer.update(dt);

        // ---- the camera ----
        let view_state = s.client.view();
        let roster = view_state.roster.clone();
        let mut scene_tick = render_tick;
        let mut replay_progress = 0.;
        let mut killcam_info: Option<(String, String, f32, bool)> = None;
        let cam_world: (View, Option<usize>, Hands, u8, f32, f32) = if alive {
            let own = own.as_ref().expect("alive implies own");
            let v = s.client.view();
            let eye = v.eye_at(s.acc / TICK).unwrap_or(own.ctrl.position);
            let def = weapons::get(v.hands.weapon(&v.inv));
            let punch = def.map_or((0., 0.), |d| crate::hands::punch(d, v.hands.recoil));
            let mut yaw = self.controls.yaw + punch.1;
            let mut pitch = self.controls.pitch + punch.0;
            let shake = s.shake * 0.02;
            yaw += (self.time * 47.).sin() * shake;
            pitch += (self.time * 53.).cos() * shake;
            let aspect = screen_width() / screen_height();
            let base = render::vfov(render::HFOV, aspect);
            let fov = base * ads_ratio;
            let speed = v.body.as_ref().map_or(0., |b| {
                if b.is_grounded() {
                    let vel = b.velocity();
                    vel.0.hypot(vel.2)
                } else {
                    0.
                }
            });
            s.motion.update(speed, (self.controls.yaw, self.controls.pitch), s.last_angles, dt);
            s.last_angles = (self.controls.yaw, self.controls.pitch);
            (View { eye: to_v3(eye), yaw, pitch, roll: 0., fov }, me_slot, v.hands, v.hands.weapon(&v.inv), 0., 0.)
        } else {
            // The killcam: the killer's eyes, replayed from a few seconds before the death.
            let o = own.as_ref();
            let (killer, weapon, died, left) =
                o.map_or((255, 0, snap.tick, 0), |o| (o.killer, o.killer_weapon, o.died_tick, o.respawn_ticks));
            if s.killcam.is_none() && o.is_some() {
                let first = s.client.view().history.front().map_or(died, |s| s.snap.tick);
                let from = died.saturating_sub(240).max(first.min(died)) as f32;
                s.killcam = Some(Killcam {
                    started: now,
                    elapsed0: (sim::RESPAWN_TICKS as f32 - left as f32) / 60.,
                    killer,
                    from,
                    last_tick: from - 1.,
                });
                self.renderer.clear_match();
                self.renderer.muzzle_flash = 0.;
                self.audio.ui(Sfx::KillcamWhoosh, 0.7);
            }
            let kc = s.killcam.as_ref();
            let elapsed = kc.map_or(0., |k| k.elapsed0 + (now - k.started) as f32);
            let from = kc.map_or(died as f32, |k| k.from);
            let end = died as f32 + 45.;
            let replay = (from + elapsed * 60.).min(end);
            let v = s.client.view();
            let newest = v.latest().map_or(snap.tick as f32, |l| l.snap.tick as f32) - 3.;
            let at = replay.min(newest);
            scene_tick = at;
            replay_progress = ((at - from) / (end - from).max(1.)).clamp(0., 1.);
            let ps = v.players_at(at);
            let killer_view = ps.iter().find(|p| p.slot == killer).copied();
            let (eye, yaw, pitch, hands, wid) = match killer_view {
                Some(kv) => {
                    let mut h = Hands::default();
                    h.ads = if kv.has(flag::ADS) { 1. } else { 0. };
                    h.recoil = if kv.has(flag::FIRING) { 1.5 } else { 0. };
                    if kv.has(flag::RELOAD) {
                        h.busy = Busy::Reload;
                        h.total = 100;
                        h.left = 50;
                    }
                    if let Some((tick, Event::Strike { heavy, weapon, .. })) =
                        v.replay_events.iter().rev().find(|(tick, event)| {
                            *tick as f32 <= at && matches!(event, Event::Strike { attacker, .. } if *attacker == killer)
                        })
                    {
                        if let Some(melee) = weapons::get(*weapon).and_then(|d| d.melee) {
                            let duration = if *heavy { melee.heavy_s } else { melee.light_s };
                            let progress = 0.4 + (at - *tick as f32) / (duration * 60.);
                            if progress < 1. && kv.weapon == *weapon {
                                h.busy = Busy::Swing;
                                h.heavy = *heavy;
                                h.total = 1000;
                                h.left = ((1. - progress) * 1000.) as u16;
                            }
                        }
                    }
                    (kv.eye, kv.yaw, kv.pitch, h, kv.weapon)
                }
                None => {
                    let e = ps
                        .iter()
                        .find(|p| Some(p.slot as usize) == me_slot)
                        .map_or_else(|| own.as_ref().map_or(V::ZERO, |o| o.ctrl.position), |p| p.eye);
                    (e, self.controls.yaw, 0.3, Hands::default(), 0)
                }
            };
            let name = if killer == 255 {
                "the fall".to_string()
            } else {
                roster.iter().find(|r| r.slot == killer).map_or("?".into(), |r| r.name.clone())
            };
            let wname = weapons::get(weapon).map_or("", |d| d.name).to_string();
            killcam_info = Some((name, wname, (left as f32 / 60.).max(0.), s.died_by_headshot));
            let aspect = screen_width() / screen_height();
            let ads_ratio = weapons::get(wid).map_or(1., |d| 1. + (d.ads_fov / 90. - 1.) * hands.ads);
            (
                View { eye: to_v3(eye), yaw, pitch, roll: 0., fov: render::vfov(render::HFOV, aspect) * ads_ratio },
                Some(killer as usize),
                hands,
                wid,
                0.,
                0.,
            )
        };
        let (view, skip, vm_hands, vm_weapon, _, _) = cam_world;
        let skip = if alive { skip } else { skip.filter(|s| *s < 16) };

        // The camera, soldiers, weapons on the floor, projectiles and zones all share this timeline.
        let v = s.client.view();
        let players_now = v.players_at(scene_tick);
        let scene_snap = if alive { &snap } else { v.snapshot_at(scene_tick).unwrap_or(&snap) };
        if let Some(kc) = s.killcam.as_mut() {
            if !alive {
                let listener = Listener { pos: to_v(view.eye), yaw: view.yaw };
                for (_, event) in
                    v.replay_events.iter().filter(|(tick, _)| *tick as f32 > kc.last_tick && *tick as f32 <= scene_tick)
                {
                    match event {
                        Event::Shot { shooter, weapon, from, to, .. } => {
                            let heavy = weapons::get(*weapon)
                                .is_some_and(|d| matches!(d.class, weapons::Class::Sniper | weapons::Class::Dmr));
                            self.renderer.tracer(*from, *to, heavy);
                            self.renderer.flash(*from);
                            if *shooter == kc.killer {
                                self.renderer.muzzle_flash = 0.06;
                            }
                            if let Some(sfx) = weapons::get(*weapon).and_then(|d| audio::shot_for_key(d.key)) {
                                self.audio.at(sfx, *from, &listener, 0.8, 120.);
                            }
                        }
                        Event::Blast { pos, radius, kind: 0 } => {
                            self.renderer.fx.fireball(to_v3(*pos), *radius * 0.6, 0.6, [1., 0.65, 0.25]);
                            self.audio.at(Sfx::Explosion, *pos, &listener, 0.8, 160.);
                        }
                        Event::Strike { attacker, heavy, .. } => {
                            if *attacker == kc.killer {
                                self.audio.ui(if *heavy { Sfx::HeavySwing } else { Sfx::KnifeSwing }, 0.6);
                            }
                        }
                        _ => {}
                    }
                }
                kc.last_tick = scene_tick;
            }
        }

        // Figures.
        let roster_team = |slot: u8| {
            roster
                .iter()
                .find(|r| r.slot == slot)
                .map_or(crate::Team::Ironclad, |r| crate::Team::from_index(r.team as usize))
        };
        let figures: Vec<Figure> =
            players_now.iter().map(|p| Figure { slot: p.slot as usize, team: roster_team(p.slot), view: *p }).collect();
        let v = s.client.view();
        let dropped = latest_dropped(scene_snap);
        self.renderer.draw_world(
            &view,
            &figures,
            skip,
            scene_snap.loot,
            &dropped.0,
            &scene_snap.projectiles,
            &scene_snap.zones,
            &s.skins,
            dt,
        );

        // Names above teammates.
        if alive {
            let ui = hud::ui_scale();
            for f in &figures {
                if f.team.index() == s.my_team && Some(f.slot) != me_slot && f.view.has(flag::ALIVE) {
                    let head = vec3(f.view.eye.0, f.view.eye.1 + 0.35, f.view.eye.2);
                    if (head - view.eye).length() < 45. && crate::level().line_of_sight(to_v(view.eye), to_v(head)) {
                        if let Some(p) = view.project(head, screen_width(), screen_height()) {
                            let name =
                                roster.iter().find(|r| r.slot as usize == f.slot).map_or("", |r| r.name.as_str());
                            hud::text_centered(name, p.x, p.y, 16. * ui, Color::new(0.6, 0.85, 0.6, 0.85));
                        }
                    }
                }
            }
        } else if vm_hands.ads > 0.92 {
            if let Some(weapons::WeaponDef { sight: weapons::Sight::Scope { zoom }, .. }) = weapons::get(vm_weapon) {
                overlay::scope(*zoom, 1.);
            }
        }

        // Footsteps and ambience.
        let level = crate::level();
        let listener = Listener { pos: view.eye.into_v(), yaw: view.yaw };
        let walkers: Vec<(usize, V, bool, bool)> = players_now
            .iter()
            .map(|p| (p.slot as usize, V(p.eye.0, p.feet, p.eye.2), p.has(flag::CROUCH), p.has(flag::ALIVE)))
            .collect();
        self.audio.footsteps(level, &walkers, &listener, if alive { me_slot } else { None });
        self.audio.update(dt, &listener, level);

        // Weapon in hand.
        if alive {
            let team = crate::Team::from_index(s.my_team);
            let skin = s.skins[me_slot.unwrap_or(0).min(15)];
            self.renderer.draw_viewmodel(
                &view,
                &vm_hands,
                vm_weapon,
                team,
                skin,
                s.motion.speed,
                s.motion.phase,
                s.motion.sway,
                0.,
                s.acc / TICK,
            );
        } else if let Some(k) = s.killcam.as_ref() {
            let team = roster_team(k.killer);
            let skin = s.skins[(k.killer as usize).min(15)];
            self.renderer.draw_viewmodel(&view, &vm_hands, vm_weapon, team, skin, 0., 0., (0., 0.), 0., 0.);
        }

        // ---- overlay ----
        let def = weapons::get(v.hands.weapon(&v.inv));
        let scoped =
            def.is_some_and(|d| matches!(d.sight, weapons::Sight::Scope { .. })) && v.hands.ads > 0.92 && alive;
        if alive {
            if let Some(d) = def {
                if let weapons::Sight::Scope { zoom } = d.sight {
                    overlay::scope(zoom, ((v.hands.ads - 0.85) / 0.15).clamp(0., 1.));
                }
            }
        }
        if self.controls.scoreboard || over {
            // The scoreboard is drawn below.
        } else if alive {
            overlay::score_strip(&snap, s.my_team);
        }
        if let Some(o) = own.as_ref() {
            if alive && !over {
                overlay::crosshair(v.hands.recoil * 1.2 + (1. - v.hands.ads) * 2., scoped || v.hands.ads > 0.6);
                let gun = v.inv.gun(v.hands.sel).copied();
                let reloading = matches!(v.hands.busy, Busy::Reload | Busy::ShellLoad);
                overlay::health_and_ammo(
                    o.health,
                    o.armor,
                    def,
                    gun.map_or(0, |g| g.mag),
                    gun.map_or(0, |g| g.reserve),
                    reloading,
                    v.inv.grenades.iter().filter(|g| **g != 0).count(),
                    def.map_or("", |d| d.name),
                );
                overlay::hit_marker(s.hit_marker.0, s.hit_marker.1);
                for (a, t) in &s.damage {
                    overlay::damage_arc(*a, *t);
                }
                overlay::flash(
                    (o.flash_left / o.flash_total.max(0.01)).clamp(0., 1.).min(1.)
                        * if o.flash_left > 0.5 { 1. } else { o.flash_left / 0.5 },
                );
                if let Some(p) = pickup_prompt(&s, &snap, &v.inv, to_v(view.eye)) {
                    overlay::prompt(&p);
                }
            }
        }
        if s.notice.1 > 0. && alive {
            overlay::notice(&s.notice.0, s.notice.1);
        }
        if alive {
            overlay::killfeed(&s.feed);
        }
        if let Some((name, weapon, left, head)) = &killcam_info {
            if !over {
                overlay::killcam(name, weapon, *left, *head, replay_progress);
            }
        }
        if self.controls.scoreboard || over {
            self.draw_scoreboard(&s, &snap, &roster);
        }
        if over {
            self.results_overlay(&mut s, &snap, nav, dt);
        }
        // The pause menu (Esc): the match goes on without you.
        if self.shell.paused && !over {
            let quit = self.shell.menu(
                "DEADFALL",
                &[
                    "Move: WASD   Look: mouse   Fire: left mouse   Aim: right mouse",
                    "Reload: R   Use: E   Crouch: Ctrl   Weapons: 1-4 / wheel   Scores: Tab",
                    "Controller: sticks, RT fire, LT aim, A jump, B crouch, X reload",
                ],
            );
            if quit {
                self.session = Some(s);
                self.leave();
                return;
            }
        }
        self.session = Some(s);
        if self.go_home {
            self.go_home = false;
            self.leave();
        }
        let _ = (&self.vignette,);
    }

    fn local_effect(&mut self, s: &mut Session, l: Local) {
        match l {
            Local::Shot { weapon, punch } => {
                s.tracker.shot_fired();
                self.renderer.muzzle_flash = 0.05;
                let key = weapons::get(weapon).map_or("", |d| d.key);
                if let Some(sfx) = audio::shot_for_key(key) {
                    self.audio.ui(sfx, 1.);
                }
                let _ = punch;
            }
            Local::Launch { weapon } => {
                self.renderer.muzzle_flash = 0.08;
                let key = weapons::get(weapon).map_or("rpg", |d| d.key);
                if let Some(sfx) = audio::shot_for_key(key) {
                    self.audio.ui(sfx, 1.);
                }
                s.shake = s.shake.max(0.35);
            }
            Local::Throw { .. } => self.audio.ui(Sfx::Throw, 0.7),
            Local::Strike { heavy, .. } => self.audio.ui(if heavy { Sfx::HeavySwing } else { Sfx::KnifeSwing }, 0.7),
            Local::Dry { .. } => self.audio.ui(Sfx::EmptyClick, 0.6),
            Local::ReloadStarted { .. } => {
                let (key, secs) = {
                    let v = s.client.view();
                    let d = weapons::get(v.hands.weapon(&v.inv));
                    (d.map_or("", |d| d.key), d.map_or(2., |d| d.reload_s))
                };
                let key = key.to_string();
                self.audio.reload(&key, secs, None, 0.7);
            }
            Local::Switched { .. } => self.audio.ui(Sfx::WeaponDraw, 0.5),
        }
    }

    fn draw_scoreboard(&self, s: &Session, snap: &Snapshot, roster: &[crate::sim::RosterEntry]) {
        let ui = hud::ui_scale();
        let w = (980. * ui).min(screen_width() - 40.);
        let x = (screen_width() - w) * 0.5;
        let rows = overlay::rows(roster, &snap.players);
        overlay::scoreboard(&rows, snap.scores, s.client.participant().map(|p| p as u8), x, 110. * ui, w);
    }

    fn results_overlay(&mut self, s: &mut Session, snap: &Snapshot, nav: &Nav, dt: f32) {
        let ui = hud::ui_scale();
        let cx = screen_width() * 0.5;
        draw_rectangle(0., 0., screen_width(), screen_height(), Color::new(0., 0., 0., 0.6));
        let (text, colour) = match snap.winner {
            0 => ("IRONCLAD WIN", ui::IRONCLAD),
            1 => ("NIGHTWATCH WIN", ui::NIGHTWATCH),
            _ => ("DRAW", TEXT),
        };
        hud::text_centered(text, cx, 90. * ui, 56. * ui, colour);
        let roster = s.client.view().roster.clone();
        let rows = overlay::rows(&roster, &snap.players);
        let w = (980. * ui).min(screen_width() - 40.);
        overlay::scoreboard(
            &rows,
            snap.scores,
            s.client.participant().map(|p| p as u8),
            (screen_width() - w) * 0.5,
            130. * ui,
            w,
        );
        let mut items = vec![Item::Button("Play again".into()), Item::Button("Home".into())];
        let top = (130. + 110. + 6. * 32. + 40.) * 1.0;
        let hit = self.results_menu.run(nav, &mut items, cx, top, 360., dt);
        match hit {
            Hit::Item(0) => {
                s.want_again = true;
                s.client.ready(true);
                self.audio.ui(Sfx::MenuConfirm, 0.5);
            }
            Hit::Item(1) => {
                self.audio.ui(Sfx::MenuBack, 0.5);
                s.want_again = false;
                s.notice = (String::new(), 0.);
                // Leaving ends the game; done after the frame by the caller via the flag below.
                self.go_home = true;
            }
            _ => {}
        }
        if s.want_again {
            hud::text_centered("Waiting for the next round...", cx, top + 150. * ui, 20. * ui, DIM);
        }
    }
}

fn to_v3(v: V) -> Vec3 {
    vec3(v.0, v.1, v.2)
}

fn to_v(v: Vec3) -> V {
    V(v.x, v.y, v.z)
}

trait IntoV {
    fn into_v(self) -> V;
}
impl IntoV for Vec3 {
    fn into_v(self) -> V {
        V(self.x, self.y, self.z)
    }
}

fn latest_dropped(snap: &Snapshot) -> (Vec<crate::netgame::DroppedView>,) {
    (snap.dropped.clone(),)
}

/// "E  Pick up K-47" when something wanted is within reach.
fn pickup_prompt(_s: &Session, snap: &Snapshot, inv: &crate::hands::Inventory, eye: V) -> Option<String> {
    let level = crate::level();
    let feet = V(eye.0, eye.1 - 1.68, eye.2);
    let near = |p: V| {
        let d = p - feet;
        d.0 * d.0 + d.2 * d.2 < 1.6 * 1.6 && d.1.abs() < 1.4
    };
    let mut found: Option<u8> = None;
    for (i, l) in level.loot.iter().enumerate() {
        if i < 64 && snap.loot & (1 << i) != 0 && near(l.pos) {
            found = Some(l.weapon);
        }
    }
    for d in &snap.dropped {
        if near(d.pos) {
            found = Some(d.weapon);
        }
    }
    let w = weapons::get(found?)?;
    let have = match w.slot {
        weapons::Slot::Primary => inv.primary.is_some(),
        weapons::Slot::Secondary => inv.secondary.is_some(),
        weapons::Slot::Grenade => inv.grenades.iter().all(|g| *g != 0),
        weapons::Slot::Melee => true,
    };
    Some(if have { format!("E   swap for {}", w.name) } else { format!("{}", w.name) })
}
