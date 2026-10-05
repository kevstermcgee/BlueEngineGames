//! Slapstick: the window, renderer, sound and input around the match in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "aim:0.5,-0.8@0-120"                       drive the human input path from a cue script
//!   --connect HOST:PORT   join a Slapstick server ([--transport development|production] [--name N] [--join-key K] [--auto-ready])
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved match / where F5 saves (default: next to the exe)
//! With no server, Enter from the title screen starts a local solo preview: you drive paddle 0, the bot
//! AI drives paddle 1. Mouse or left stick moves your paddle, confined to your own half.
//! Keyboard: mouse moves your paddle, F5/F9 quick save and load (solo preview only), Esc menu.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod models;
mod platform;

use macroquad::prelude::*;
use slapstick::{AirHockeyGame, Event, Input, Phase, Sim, SERVE_TICKS, TABLE_HALF_LENGTH, WIN_GOALS};
use std::net::ToSocketAddrs;
use std::sync::{Arc, OnceLock};
use vesper3d::math::V;
use vesper3d::viewer::{
    devkit::{
        beside_exe, downloads_dir, flag_value, has_flag, parse_size, sanitize_filename, synth, unique_path, Juice,
        Lifecycle, Notice, Settings,
    },
    game_client::{self, AudioMenu, GameShell},
    game_input::{self, ClientInput},
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, SoundBank, Template, Tint, View},
    net::{client_transport, AnyTransport, TransportProfile},
    netplay::{ClientConfig, ClientState, NetClient},
};

const IDENTITY: &str = include_str!("../assets/identity.json");

fn identity() -> Identity {
    Identity::parse(IDENTITY).expect("assets/identity.json is invalid; run: python scripts/ship.py verify")
}

fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let mut conf = game_client::window_config_with_icon(
        &identity().title,
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    );
    let args: Vec<String> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        conf.miniquad_conf.window_width = w.clamp(320, 7680) as i32;
        conf.miniquad_conf.window_height = h.clamp(240, 4320) as i32;
    }
    if has_flag(&args, "--novsync") {
        conf.miniquad_conf.platform.swap_interval = Some(0);
    }
    conf
}

/// This frame's paddle cursor, fed to `Lifecycle` as the "held" state so scripted and live input share
/// one path.
#[derive(Clone, Copy, Default)]
struct Held {
    x: f32,
    z: f32,
}
/// The cue a `--script` may use for the paddle target: two values, `[x, z]`, both `[-1, 1]`.
const CUES: [&str; 1] = ["aim"];
/// How fast the mouse moves the persistent paddle cursor; pixels to normalized units per pixel.
const MOUSE_SENSITIVITY: f32 = 0.0035;
/// Below this, the left stick reads as centred (no override of the mouse cursor).
const STICK_DEADZONE: f32 = 0.15;

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 5] =
    [synth::Preset::Hit, synth::Preset::Thump, synth::Preset::Chime, synth::Preset::Success, synth::Preset::GameOver];
fn sound(preset: synth::Preset) -> usize {
    SOUNDS.iter().position(|p| *p == preset).unwrap_or(0)
}
/// This game uses the full persisted-settings ambient-music path (unlike Prop Hunt, which turns music
/// off for a reason specific to that game): see `AGENTS.md`.
const HAS_MUSIC: bool = true;

/// Renders on the worker thread; `music_wav` is filled once so the Settings-screen "Save music" button
/// can hand the player the exact bytes the stem below plays, without regenerating or hitching. Copied
/// faithfully from `custom-sim`'s own audio wiring (`~/BlueEngine/templates/custom-sim/src/main.rs`).
fn render_audio(music_wav: Arc<OnceLock<Vec<u8>>>, title: &str, tagline: &str) -> Rendered {
    let stems = if HAS_MUSIC {
        let spec = synth::ambient_spec_for(title, tagline);
        let ambient = synth::wav_bytes(&synth::ambient_loop(&spec), synth::RATE);
        let _ = music_wav.set(ambient.clone());
        vec![ambient]
    } else {
        Vec::new()
    };
    Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 7), synth::RATE)).collect())
            .collect(),
        stems,
    }
}

struct Scene {
    sky: Vec<Mesh>,
    table: Vec<Mesh>,
    puck: Template,
    paddles: Vec<Template>,
    goal_glow: Template,
}

fn build_scene() -> Scene {
    let mut sky = Template::new();
    sky.sky_dome(
        120.,
        |e| {
            let t = e.clamp(0., 1.).sqrt();
            [0.03 + 0.05 * (1. - t), 0.03 + 0.05 * (1. - t), 0.06 + 0.10 * (1. - t)]
        },
        24,
        12,
    );
    Scene {
        sky: sky.to_meshes(),
        table: models::table(),
        puck: models::puck(),
        paddles: (0..4).map(|c| models::paddle(models::paddle_color(c))).collect(),
        goal_glow: models::goal_glow(),
    }
}

fn rink_look() -> Look {
    let mut look = Look::night();
    look.ambient_sky = [0.18, 0.2, 0.3];
    look.ambient_ground = [0.08, 0.09, 0.13];
    look.key_color = [0.85, 0.9, 1.0];
    look.rim_strength = 0.2;
    look.fog_color = [0.03, 0.03, 0.05];
    look.fog_density = 0.01;
    look
}

fn v3(p: V) -> Vec3 {
    vec3(p.0, p.1, p.2)
}

/// A fixed, steep overhead-angled shot from behind the player's own goal looking down the table: each
/// client renders from behind their own goal, so their own paddle reads near the bottom of their screen.
fn table_camera(my_slot: usize) -> View {
    let side = if my_slot == 0 { -1. } else { 1. };
    View {
        eye: vec3(0., 3.0, side * (TABLE_HALF_LENGTH + 1.6)),
        yaw: if my_slot == 0 { std::f32::consts::PI } else { 0. },
        pitch: -62f32.to_radians(),
        roll: 0.,
        fov: 65f32.to_radians(),
    }
}

/// A local solo preview: paddle 0 is the human, paddle 1 is the bot AI (there is no second human offline).
fn new_local(seed: u64) -> Sim {
    let mut sim = Sim::new(seed);
    sim.paddles[1].ai = true;
    sim
}

fn announce(fx: &mut Fx, notice: &Notice) {
    fx.banners.clear();
    fx.banner(notice.title, notice.detail.clone(), notice.color);
}

/// Turn one simulation event into sound and HUD feedback, from `me`'s point of view (`None` while
/// spectating, which cannot happen in v1 with only two seats and no spectators, but handled anyway). The
/// `match` is exhaustive on purpose: a new `Event` variant must be handled here.
fn react(event: &Event, me: Option<usize>, sounds: &mut SoundBank, fx: &mut Fx, juice: &mut Juice) {
    match event {
        Event::Serve { .. } => {
            // `draw_serve_banner` already draws "YOUR SERVE"/"OPPONENT SERVES" live from the current
            // phase every frame while serving; this reaction is sound only, so it never doubles up with
            // (or outlives) that banner, or with the goal banner a serve immediately follows.
            sounds.play(sound(synth::Preset::Chime), 0.6);
        }
        Event::Goal { scorer, score } => {
            sounds.play(sound(synth::Preset::Success), 0.9);
            let mine = me == Some(*scorer);
            let (title, colour) = if mine { ("GOAL!", [1., 0.85, 0.3]) } else { ("OPPONENT SCORES", [0.9, 0.4, 0.3]) };
            fx.banners.clear(); // a fresh goal always wins over whatever banner was still fading
            fx.banner(title, format!("{} : {}", score[0], score[1]), colour);
            juice.kick(3.);
            juice.flash(colour, 0.3);
        }
        Event::PaddleHit { paddle, speed } => {
            sounds.play(sound(synth::Preset::Hit), (speed / 10.).clamp(0.25, 1.));
            if me == Some(*paddle) {
                juice.shake((speed / 20.).clamp(0.1, 0.5));
            }
        }
        Event::WallHit { speed } => {
            sounds.play(sound(synth::Preset::Thump), (speed / 12.).clamp(0.2, 0.8));
        }
        Event::GameOver { winner } => {
            let won = me == Some(*winner);
            sounds.play(sound(synth::Preset::GameOver), 1.);
            let (title, colour) = if won { ("YOU WIN", [0.4, 0.95, 0.5]) } else { ("YOU LOSE", [0.9, 0.4, 0.3]) };
            fx.banner(title, "", colour);
            juice.shake(0.8);
        }
    }
}

/// Write the currently playing ambient track to the player's Downloads folder as a `.wav`.
fn save_music(music_wav: &OnceLock<Vec<u8>>, title: &str) -> Notice {
    let Some(bytes) = music_wav.get() else {
        return Notice {
            title: "NOT READY",
            detail: "the music is still rendering; try again in a moment".into(),
            color: [1., 0.7, 0.3],
            ok: false,
        };
    };
    let Some(dir) = downloads_dir() else {
        return Notice {
            title: "SAVE FAILED",
            detail: "could not find your Downloads folder".into(),
            color: [1., 0.5, 0.3],
            ok: false,
        };
    };
    let path = unique_path(&dir, &format!("{} - Ambient Music", sanitize_filename(title)), "wav");
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, bytes)) {
        Ok(()) => Notice { title: "MUSIC SAVED", detail: path.display().to_string(), color: [0.4, 0.9, 0.6], ok: true },
        Err(error) => Notice { title: "SAVE FAILED", detail: error.to_string(), color: [1., 0.5, 0.3], ok: false },
    }
}

type Client = NetClient<AirHockeyGame, AnyTransport>;

enum Screen {
    Select,
    Lobby,
    Playing,
}

/// Where to play online, if anywhere: `--connect` on the command line, else a `server.txt` next to the
/// program. `--offline` skips it.
fn server_settings(args: &[String]) -> Option<(String, String, TransportProfile)> {
    if has_flag(args, "--offline") {
        return None;
    }
    let flag = |name: &str| flag_value(args, name).map(|v| v.to_string());
    if let Some(target) = flag("--connect") {
        let profile = flag("--transport").and_then(|t| t.parse().ok()).unwrap_or(TransportProfile::Development);
        return Some((target, flag("--join-key").unwrap_or_default(), profile));
    }
    let text = std::fs::read_to_string(std::env::current_exe().ok()?.parent()?.join("server.txt")).ok()?;
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    let address = lines.next()?.to_string();
    let key = lines.next().filter(|k| *k != "-").unwrap_or("").to_string();
    let profile = lines.next().and_then(|t| t.parse().ok()).unwrap_or(TransportProfile::Production);
    Some((address, key, profile))
}

fn draw_select(choice: u8, time: f32, ui: f32, prompt: &str) {
    let (w, h) = (screen_width(), screen_height());
    hud::text_centered("SLAPSTICK", w * 0.5, h * 0.35, 56. * ui, hud::col([0.3, 0.75, 1.], 1.));
    hud::text_centered(
        "Two-player online air hockey. First to seven wins.",
        w * 0.5,
        h * 0.35 + 40. * ui,
        18. * ui,
        hud::col([0.8, 0.85, 0.9], 1.),
    );
    let colour = models::paddle_color(choice);
    hud::text_centered(
        "Your paddle colour (Left/Right to change)",
        w * 0.5,
        h * 0.6,
        18. * ui,
        hud::col([0.8, 0.8, 0.85], 1.),
    );
    draw_circle(w * 0.5, h * 0.6 + 50. * ui, 24. * ui, hud::col(colour, 1.));
    let blink = 0.6 + 0.4 * (time * 3.).sin();
    hud::text_centered(prompt, w * 0.5, h - 40. * ui, 18. * ui, Color::new(1., 1., 1., blink));
}

fn draw_lobby(client: &Client, choice: u8, my_ready: bool, time: f32, ui: f32) {
    let (w, h) = (screen_width(), screen_height());
    let prompt = if my_ready {
        "Ready! Enter to un-ready    O: practice offline"
    } else {
        "Left/Right: paddle colour    Enter when ready    O: practice offline"
    };
    draw_select(choice, time, ui, prompt);
    let (pw, px, py) = (340. * ui, w - 380. * ui, h * 0.5 - 100. * ui);
    hud::panel(px, py, pw, 220. * ui, 14. * ui, Color::new(0.02, 0.03, 0.06, 0.8));
    match client.state() {
        ClientState::Connecting => {
            hud::text_outlined("Connecting...", px + 20. * ui, py + 36. * ui, 20. * ui, hud::col([1., 0.8, 0.3], 1.))
        }
        ClientState::Rejected(why) | ClientState::Disconnected(why) => {
            hud::text_outlined("Cannot play", px + 20. * ui, py + 36. * ui, 22. * ui, hud::col([1., 0.4, 0.4], 1.));
            for (i, line) in hud::wrap(why, pw - 40. * ui, 16. * ui).iter().enumerate() {
                hud::text_outlined(line, px + 20. * ui, py + 64. * ui + i as f32 * 20. * ui, 16. * ui, WHITE);
            }
        }
        _ => {
            let lobby = client.lobby();
            let n = lobby.map_or(0, |l| l.entries.len());
            hud::text_outlined(
                &format!("Lobby  {n}/2"),
                px + 20. * ui,
                py + 36. * ui,
                22. * ui,
                hud::col([1., 0.8, 0.3], 1.),
            );
            if let Some(l) = lobby {
                for (i, e) in l.entries.iter().enumerate() {
                    let y = py + 68. * ui + i as f32 * 24. * ui;
                    draw_rectangle(
                        px + 20. * ui,
                        y - 14. * ui,
                        8. * ui,
                        18. * ui,
                        hud::col(models::paddle_color(e.choice), 1.),
                    );
                    hud::text_outlined(&e.name, px + 38. * ui, y, 15. * ui, WHITE);
                    if e.ready {
                        hud::text_right("READY", px + pw - 16. * ui, y, 14. * ui, hud::col([0.4, 1., 0.5], 1.));
                    }
                }
                if l.seconds_left > 0 {
                    hud::text_outlined(
                        &format!("Match starts in {}", l.seconds_left),
                        px + 20. * ui,
                        py + 190. * ui,
                        16. * ui,
                        hud::col([1., 0.9, 0.5], 1.),
                    );
                }
            }
        }
    }
}

fn draw_serve_banner(phase: Phase, me: Option<usize>, ui: f32) {
    let Phase::Serve { ticks_left, server } = phase else { return };
    let (w, h) = (screen_width(), screen_height());
    let mine = me == Some(server);
    let (text, colour) = if mine { ("YOUR SERVE", [0.4, 0.9, 0.6]) } else { ("OPPONENT SERVES", [0.9, 0.6, 0.3]) };
    hud::text_centered(text, w * 0.5, h * 0.28, 30. * ui, hud::col(colour, 1.));
    let frac = 1. - (ticks_left as f32 / SERVE_TICKS as f32).clamp(0., 1.);
    hud::bar(w * 0.5 - 90. * ui, h * 0.28 + 18. * ui, 180. * ui, 10. * ui, frac, hud::col(colour, 1.));
}

fn draw_score(score: [u32; 2], me: Option<usize>, ui: f32) {
    let (mine, theirs) = match me {
        Some(1) => (score[1], score[0]),
        _ => (score[0], score[1]),
    };
    let (w, _h) = (screen_width(), screen_height());
    hud::panel(w * 0.5 - 110. * ui, 16. * ui, 220. * ui, 64. * ui, 12. * ui, Color::new(0.02, 0.03, 0.06, 0.7));
    hud::text_centered(&format!("{mine}"), w * 0.5 - 50. * ui, 56. * ui, 40. * ui, hud::col([0.4, 0.9, 1.], 1.));
    hud::text_centered("-", w * 0.5, 50. * ui, 28. * ui, WHITE);
    hud::text_centered(&format!("{theirs}"), w * 0.5 + 50. * ui, 56. * ui, 40. * ui, hud::col([1., 0.6, 0.5], 1.));
    hud::text_centered("YOU", w * 0.5 - 50. * ui, 26. * ui, 12. * ui, hud::col([0.7, 0.8, 0.85], 1.));
    hud::text_centered("OPPONENT", w * 0.5 + 50. * ui, 26. * ui, 12. * ui, hud::col([0.7, 0.8, 0.85], 1.));
    hud::text_centered(&format!("first to {WIN_GOALS}"), w * 0.5, 72. * ui, 11. * ui, hud::col([0.6, 0.65, 0.7], 1.));
}

fn draw_results(phase: Phase, me: Option<usize>, ui: f32) {
    let Phase::GameOver { winner } = phase else { return };
    let (w, h) = (screen_width(), screen_height());
    let (pw, ph) = (440. * ui, 220. * ui);
    let (x, y) = ((w - pw) * 0.5, (h - ph) * 0.5);
    hud::panel(x, y, pw, ph, 14. * ui, Color::new(0.02, 0.03, 0.06, 0.88));
    let won = me == Some(winner);
    let (title, colour) = if won { ("YOU WIN", [0.4, 0.95, 0.5]) } else { ("YOU LOSE", [0.9, 0.4, 0.3]) };
    hud::text_centered(title, w * 0.5, y + 70. * ui, 42. * ui, hud::col(colour, 1.));
    hud::text_centered(
        "Back to the lobby shortly...",
        w * 0.5,
        y + ph - 30. * ui,
        16. * ui,
        hud::col([0.8, 0.8, 0.85], 1.),
    );
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identity = identity();
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let seed = life.seed();
    let unattended = life.options.unattended();
    let auto_ready = has_flag(&args, "--auto-ready");

    let materials = Materials::load().expect("the materials failed to compile");
    let look = rink_look();
    let scene = build_scene();
    let settings_path = beside_exe("settings.json");
    let mut settings = Settings::load(&settings_path);
    let music_wav: Arc<OnceLock<Vec<u8>>> = Arc::new(OnceLock::new());
    let mut sounds = SoundBank::start(life.options.silent(), settings.sfx_level(), settings.music_level(), {
        let music_wav = music_wav.clone();
        let (title, tagline) = (identity.title.clone(), identity.tagline.clone());
        move || render_audio(music_wav, &title, &tagline)
    })
    .await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let mut fx = Fx::new(seed);
    let mut juice = Juice::default();
    let vignette = hud::make_vignette();
    let mut cursor = [0f32; 2];
    let mut choice: u8 = 0;

    let mut client: Option<Client> = None;
    if let Some((target, key, profile)) = server_settings(&args) {
        let address = target.to_socket_addrs().ok().and_then(|mut a| a.next()).unwrap_or_else(|| {
            eprintln!("Cannot find a server at {target}");
            std::process::exit(2);
        });
        let name = flag_value(&args, "--name")
            .map(|n| n.to_string())
            .or_else(|| std::env::var("USER").ok())
            .or_else(|| std::env::var("USERNAME").ok())
            .unwrap_or_else(|| "Player".into());
        let made = client_transport(profile, address)
            .and_then(|t| Client::new(t, address, ClientConfig { name, key, choice }));
        match made {
            Ok(c) => client = Some(c),
            Err(e) => eprintln!("Cannot start the network ({e}); playing a local solo preview instead"),
        }
    }

    const LOCAL_ME: usize = 0;
    let mut local: Option<Sim> = (client.is_none() && unattended).then(|| new_local(seed));
    let mut screen = if client.is_some() {
        Screen::Lobby
    } else if local.is_some() {
        Screen::Playing
    } else {
        Screen::Select
    };

    loop {
        let playing_now = matches!(screen, Screen::Playing);
        input.begin_frame_with_keyboard(
            &mut shell,
            playing_now,
            unattended || platform::focused(),
            platform::keyboard(),
        );
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        sounds.update_music(dt, &[1.]);
        let ui = hud::ui_scale();
        let menu_step = input.menu_step();

        // The paddle cursor: a script drives it absolutely; live play accumulates mouse motion into a
        // persistent cursor, overridden by the left stick's absolute position when it is actually pushed.
        let held = match life.script() {
            Some(s) => Held { x: s.value("aim", 0), z: s.value("aim", 1) },
            None => {
                let pad = input.gamepad().clone();
                if pad.left_stick[0].abs() > STICK_DEADZONE || pad.left_stick[1].abs() > STICK_DEADZONE {
                    cursor = [pad.left_stick[0].clamp(-1., 1.), pad.left_stick[1].clamp(-1., 1.)];
                } else if playing_now && shell.accepting_input() {
                    let delta = game_input::mouse_pixels();
                    cursor[0] = (cursor[0] + delta[0] * MOUSE_SENSITIVITY).clamp(-1., 1.);
                    cursor[1] = (cursor[1] - delta[1] * MOUSE_SENSITIVITY).clamp(-1., 1.);
                }
                Held { x: cursor[0], z: cursor[1] }
            }
        };
        life.feed(held, 0, [0., 0.]);

        // The network first: read it, follow its state into the right screen, react to its events.
        let mut my_ready = false;
        if let Some(c) = client.as_mut() {
            c.poll(time as f64);
            c.frame(time as f64, dt);
            let me = c.participant();
            for event in c.drain_events() {
                react(&event, me, &mut sounds, &mut fx, &mut juice);
            }
            match (c.state().clone(), &screen) {
                (ClientState::Playing, Screen::Lobby) => {
                    screen = Screen::Playing;
                    fx.clear();
                    juice = Juice::default();
                    life.reset_input();
                }
                (ClientState::Lobby, Screen::Playing) => {
                    screen = Screen::Lobby;
                    fx.clear();
                }
                _ => {}
            }
            let mine = c.lobby().and_then(|l| l.entries.iter().find(|e| e.slot == c.seat()).cloned());
            my_ready = mine.as_ref().is_some_and(|e| e.ready);
            if let Some(e) = &mine {
                choice = e.choice;
            }
            if auto_ready && *c.state() == ClientState::Lobby && mine.is_some() && !my_ready {
                c.ready(true);
            }
        }

        match screen {
            Screen::Select => {
                let left = input.pressed(KeyCode::A) || input.pressed(KeyCode::Left) || menu_step.left;
                let right = input.pressed(KeyCode::D) || input.pressed(KeyCode::Right) || menu_step.right;
                if left {
                    choice = (choice + 3) % 4;
                    sounds.play(sound(synth::Preset::Thump), 0.4);
                }
                if right {
                    choice = (choice + 1) % 4;
                    sounds.play(sound(synth::Preset::Thump), 0.4);
                }
                if input.pressed(KeyCode::Enter) || input.pressed(KeyCode::Space) || input.menu_select() {
                    sounds.play(sound(synth::Preset::Chime), 0.7);
                    local = Some(new_local(life.restart_seed()));
                    fx.clear();
                    juice = Juice::default();
                    life.reset_input();
                    screen = Screen::Playing;
                }
            }
            Screen::Lobby => {
                let offline_key = input.pressed(KeyCode::O);
                let failed = client
                    .as_ref()
                    .is_some_and(|c| matches!(c.state(), ClientState::Rejected(_) | ClientState::Disconnected(_)));
                if offline_key || (failed && (input.pressed(KeyCode::Enter) || input.menu_select())) {
                    if let Some(c) = client.as_mut() {
                        c.leave();
                    }
                    client = None;
                    screen = Screen::Select;
                    sounds.play(sound(synth::Preset::Thump), 0.5);
                }
                if let Some(c) = client.as_mut().filter(|c| *c.state() == ClientState::Lobby) {
                    let left = input.pressed(KeyCode::A) || input.pressed(KeyCode::Left) || menu_step.left;
                    let right = input.pressed(KeyCode::D) || input.pressed(KeyCode::Right) || menu_step.right;
                    if left || right {
                        choice = if left { (choice + 3) % 4 } else { (choice + 1) % 4 };
                        c.select(choice);
                        sounds.play(sound(synth::Preset::Thump), 0.4);
                    }
                    if input.pressed(KeyCode::Enter) || input.pressed(KeyCode::Space) || input.menu_select() {
                        c.ready(!my_ready);
                        sounds.play(sound(synth::Preset::Chime), 0.7);
                    }
                }
            }
            Screen::Playing => {
                if client.is_none() {
                    if let Some(sim) = local.as_mut() {
                        let (save, load) =
                            life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
                        if save {
                            let notice = if sim.is_over() {
                                Notice::refused("the match is over")
                            } else {
                                life.quick_save(sim, "Quick save")
                            };
                            announce(&mut fx, &notice);
                        }
                        if load {
                            let notice = life.quick_load(sim);
                            announce(&mut fx, &notice);
                            if notice.ok {
                                juice = Juice::default();
                            }
                        }
                    }
                }

                let advancing = !shell.paused;
                if let Some(c) = client.as_mut() {
                    for _ in 0..life.ticks(dt, 1., advancing) {
                        let tick = life.take_tick();
                        c.tick(Input { target_x: tick.held.x, target_z: tick.held.z });
                    }
                    if advancing {
                        juice.update(dt);
                        fx.update(dt);
                    }
                } else if let Some(sim) = local.as_mut() {
                    for _ in 0..life.ticks(dt, juice.time_scale(None), advancing) {
                        let tick = life.take_tick();
                        let mut inputs = slapstick::Inputs::default();
                        inputs.0[LOCAL_ME] = Input { target_x: tick.held.x, target_z: tick.held.z };
                        sim.step(&inputs);
                        for event in sim.drain_events() {
                            react(&event, Some(LOCAL_ME), &mut sounds, &mut fx, &mut juice);
                        }
                    }
                    if advancing {
                        juice.update(dt);
                        fx.update(dt);
                    }
                    if sim.is_over() && (input.pressed(KeyCode::Enter) || input.menu_select()) && !unattended {
                        screen = Screen::Select;
                        local = None;
                        sounds.play(sound(synth::Preset::Chime), 0.7);
                    }
                }
            }
        }

        // What is on screen: the local match, or the network client's predicted view of the server's.
        let (me, phase, score, paddles, puck): (Option<usize>, Phase, [u32; 2], [V; 2], V) = match &client {
            Some(c) => (c.participant(), c.view().phase(), c.view().score(), c.view().paddles(), c.view().puck()),
            None => match &local {
                Some(sim) => {
                    (Some(LOCAL_ME), sim.phase, sim.score, [sim.paddles[0].pos, sim.paddles[1].pos], sim.puck.pos)
                }
                None => (None, Phase::Playing, [0, 0], [V::ZERO, V::ZERO], V::ZERO),
            },
        };
        let my_slot = me.unwrap_or(0);
        let playable = matches!(screen, Screen::Playing);

        // Camera: fixed, steep, overhead, from behind the player's own goal looking down the table.
        let view = table_camera(my_slot);

        clear_background(look.clear_color());
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for mesh in &scene.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera(0.05, 50.));
        materials.set_scene(&look, view.eye, time, 0.5 + 0.5 * (time * 2.).sin());
        let mut world = Batch::new();
        let mut alpha = Batch::new();
        let mut add = Batch::new();
        if playable {
            for (i, pos) in paddles.iter().enumerate() {
                let template = &scene.paddles[(choice as usize + if i == my_slot { 0 } else { 2 }) % 4];
                world.add(template, Mat4::from_translation(v3(*pos)), Tint::NONE);
            }
            world.add(&scene.puck, Mat4::from_translation(v3(puck)), Tint::NONE);
            for end in [-1f32, 1.] {
                let glowing =
                    matches!(phase, Phase::GameOver { .. }) || (end < 0. && my_slot == 0) || (end > 0. && my_slot == 1);
                let tint = if glowing { Tint { mul: [1.3, 1.3, 1.2], ..Tint::NONE } } else { Tint::NONE };
                world.add(&scene.goal_glow, Mat4::from_translation(vec3(0., 0., end * TABLE_HALF_LENGTH)), tint);
            }
        }
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        gl_use_material(&materials.world);
        if playable {
            for mesh in &scene.table {
                draw_mesh(mesh);
            }
        }
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_default_material();
        set_default_camera();

        hud::overlay(&vignette, Color::new(0., 0., 0.05, 0.4));
        match screen {
            Screen::Select => draw_select(
                choice,
                time,
                ui,
                "Left/Right: paddle colour    Enter: solo preview    --connect HOST:PORT online",
            ),
            Screen::Lobby => {
                if let Some(c) = &client {
                    draw_lobby(c, choice, my_ready, time, ui);
                }
            }
            Screen::Playing => {
                hud::draw_popups(&fx.popups, &view, ui);
                draw_score(score, me, ui);
                // Skip the live serve banner while a fresher event banner (a goal, most often) is still
                // fading: the two occupy the same part of the screen and a goal always just preceded a
                // new serve, so the event banner wins until it clears on its own.
                if fx.banners.is_empty() {
                    draw_serve_banner(phase, me, ui);
                }
                hud::draw_banners(&fx.banners, ui);
                draw_results(phase, me, ui);
            }
        }
        let controls: Vec<&str> = identity.controls.split(", ").collect();
        let audio_menu = AudioMenu { music_on: settings.music_on, sfx_on: settings.sfx_on, has_music: HAS_MUSIC };
        let outcome = shell.local_menu_with_audio(&identity.title, &controls, audio_menu);
        if outcome.toggle_music {
            settings.toggle_music();
            sounds.music_volume = settings.music_level();
            settings.store(&settings_path);
        }
        if outcome.toggle_sfx {
            settings.toggle_sfx();
            sounds.sfx_volume = settings.sfx_level();
            settings.store(&settings_path);
        }
        if outcome.download_music {
            announce(&mut fx, &save_music(&music_wav, &identity.title));
        }
        if outcome.quit {
            break;
        }

        if let Some(path) = life.capture_path() {
            life.captured(&path, kit::capture::save_frame(&path).map_err(|e| e.to_string()));
        }
        if life.end_frame(dt) {
            break;
        }
        next_frame().await;
    }
    if let Some(report) = life.report() {
        println!("{report}");
    }
}
