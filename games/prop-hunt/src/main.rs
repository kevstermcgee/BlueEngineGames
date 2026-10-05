//! Prop Hunt: the window, renderer, sound and input around the round in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "fwd:0-400,left:60-120"                   drive the human input path from a cue script
//!   --connect HOST:PORT   join a Prop Hunt server ([--transport development|production] [--name N] [--join-key K] [--auto-ready])
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved round / where F5 saves (default: next to the exe)
//! With no server, Enter from the title screen starts a local solo preview: you drive hider seat 0. There
//! is no bot AI, so a solo preview's seeker never moves — play online (`--connect`, or run
//! `prop-hunt-server`) for an actual hunt.
//! Keyboard: WASD move, mouse look, 1-0 and the scroll wheel cycle disguise (hide phase), E confirm
//! placement, Space hold to inspect (seek phase), F5/F9 quick save and load (solo preview only), Esc menu.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod models;
mod platform;

use macroquad::prelude::*;
use prop_hunt::{
    netgame::HiderView, DisguiseKind, Event, Input, Inputs, Outcome, Phase, PropHuntGame, Sim, CONFIRM_RADIUS,
    INSPECT_HOLD_TICKS, MAX_HIDERS, SEEKER_SLOT,
};
use std::net::ToSocketAddrs;
use vesper3d::viewer::{
    devkit::{flag_value, has_flag, parse_size, synth, Lifecycle, Notice},
    game_client::{self, AudioMenu, GameShell},
    game_input::ClientInput,
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

/// Held device state for one real frame; `confirm_placement` travels separately as a press edge.
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
    /// Seek phase: held to inspect the nearest untagged hider in range.
    inspect: bool,
}
const CONFIRM: u32 = 1;
const CUES: [&str; 7] = ["fwd", "back", "left", "right", "look", "confirm", "inspect"];

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 7] = [
    synth::Preset::Blip,
    synth::Preset::Chime,
    synth::Preset::Thump,
    synth::Preset::Hurt,
    synth::Preset::Warning,
    synth::Preset::Success,
    synth::Preset::GameOver,
];
fn sound(preset: synth::Preset) -> usize {
    SOUNDS.iter().position(|p| *p == preset).unwrap_or(0)
}
/// No ambient music: the core tension mechanic is silence punctuated by footsteps, and a generated
/// ambient bed would mask that cue (see `AGENTS.md`). Sound effects only.
const HAS_MUSIC: bool = false;
fn render_audio() -> Rendered {
    Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 7), synth::RATE)).collect())
            .collect(),
        stems: Vec::new(),
    }
}

struct Scene {
    sky: Vec<Mesh>,
    house: Vec<Mesh>,
    disguises: Vec<Template>,
    humanoid_hider: Template,
    humanoid_seeker: Template,
}

fn build_scene() -> Scene {
    Scene {
        sky: models::sky(),
        house: models::house(),
        disguises: DisguiseKind::ALL.iter().map(|k| models::disguise_mesh(*k)).collect(),
        humanoid_hider: models::humanoid([0.55, 0.55, 0.65]),
        humanoid_seeker: models::humanoid([0.65, 0.15, 0.15]),
    }
}

// A lived-in house, not a horror game: visibility is never the obstacle (the hiding is in plain sight,
// not in the dark), so this starts from `Look::daylight()` and only dims it a little for an indoor mood.
fn house_look() -> Look {
    let mut look = Look::daylight();
    look.ambient_sky = [0.32, 0.31, 0.34];
    look.ambient_ground = [0.22, 0.21, 0.22];
    look.key_color = [0.92, 0.88, 0.80];
    look.rim_strength = 0.15;
    look.fog_color = [0.11, 0.11, 0.12];
    look.fog_density = 0.008;
    look
}

fn v3(p: vesper3d::math::V) -> Vec3 {
    vec3(p.0, p.1, p.2)
}

/// Eye height above a locked feet position, for drawing a confirmed (frozen) hider's own camera.
const EYE_HEIGHT: f32 = 1.68;

fn announce(fx: &mut Fx, notice: &Notice) {
    fx.banners.clear();
    fx.banner(notice.title, notice.detail.clone(), notice.color);
}

/// Turn one simulation event into sound and HUD feedback. The `match` is exhaustive on purpose: a new
/// `Event` variant must be handled (or explicitly ignored) here.
fn react(event: &Event, me_tagged: bool, sounds: &mut SoundBank, fx: &mut Fx) {
    match event {
        Event::HidePhaseEnded => {}
        Event::SeekPhaseBegan => {
            sounds.play(sound(synth::Preset::Warning), 0.9);
            fx.banner("THE HUNT BEGINS", "", [0.9, 0.3, 0.3]);
        }
        Event::InspectStarted => {
            if me_tagged {
                sounds.play(sound(synth::Preset::Thump), 0.5);
            }
        }
        Event::Tagged { .. } => {
            if me_tagged {
                sounds.play(sound(synth::Preset::Hurt), 1.0);
                fx.banner("FOUND", "You've been tagged", [0.9, 0.3, 0.2]);
            } else {
                sounds.play(sound(synth::Preset::Thump), 0.6);
            }
        }
        Event::RoundOver(Outcome::HidersWin) => {
            sounds.play(sound(synth::Preset::Success), 1.0);
            fx.banner("HIDERS WIN", "Time ran out on the seeker", [0.4, 0.9, 0.5]);
        }
        Event::RoundOver(Outcome::SeekerWins) => {
            sounds.play(sound(synth::Preset::GameOver), 1.0);
            fx.banner("SEEKER WINS", "Every hider was found", [0.9, 0.5, 0.2]);
        }
    }
}

type Client = NetClient<PropHuntGame, AnyTransport>;

enum Screen {
    Select,
    Lobby,
    Hiding,
    Seeking,
    Results,
}

/// Where to play online, if anywhere: `--connect` on the command line, else a `server.txt` next to the
/// program (line 1 `host:port`, optional line 2 join key, optional line 3 `development` or `production`,
/// default production; `#` starts a comment). `--offline` skips it.
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

fn draw_select(time: f32, ui: f32, prompt: &str) {
    let (w, h) = (screen_width(), screen_height());
    hud::text_centered("PROP HUNT", w * 0.5, h * 0.4, 56. * ui, hud::col([0.9, 0.5, 0.2], 1.));
    hud::text_centered(
        "Seven hide as furniture. One seeker has three minutes.",
        w * 0.5,
        h * 0.4 + 40. * ui,
        18. * ui,
        hud::col([0.8, 0.8, 0.85], 1.),
    );
    let blink = 0.6 + 0.4 * (time * 3.).sin();
    hud::text_centered(prompt, w * 0.5, h - 40. * ui, 18. * ui, Color::new(1., 1., 1., blink));
}

fn draw_lobby(client: &Client, prefers_seeker: bool, my_ready: bool, time: f32, ui: f32) {
    let (w, h) = (screen_width(), screen_height());
    let prompt = if my_ready {
        "Ready! Enter to un-ready    Left/Right: hide or seek preference"
    } else {
        "Left/Right: hide or seek preference    Enter when ready"
    };
    draw_select(time, ui, prompt);
    let (pw, px, py) = (340. * ui, w - 380. * ui, h * 0.5 - 120. * ui);
    hud::panel(px, py, pw, 280. * ui, 14. * ui, Color::new(0.05, 0.02, 0.08, 0.8));
    hud::text_outlined(
        if prefers_seeker { "Preference: SEEKER" } else { "Preference: HIDER" },
        px + 20. * ui,
        py + 36. * ui,
        20. * ui,
        hud::col(if prefers_seeker { [0.9, 0.4, 0.3] } else { [0.4, 0.8, 0.5] }, 1.),
    );
    match client.state() {
        ClientState::Connecting => {
            hud::text_outlined("Connecting...", px + 20. * ui, py + 76. * ui, 20. * ui, hud::col([1., 0.8, 0.3], 1.))
        }
        ClientState::Rejected(why) | ClientState::Disconnected(why) => {
            hud::text_outlined("Cannot play", px + 20. * ui, py + 76. * ui, 22. * ui, hud::col([1., 0.4, 0.4], 1.));
            for (i, line) in hud::wrap(why, pw - 40. * ui, 16. * ui).iter().enumerate() {
                hud::text_outlined(line, px + 20. * ui, py + 104. * ui + i as f32 * 20. * ui, 16. * ui, WHITE);
            }
        }
        _ => {
            let lobby = client.lobby();
            let n = lobby.map_or(0, |l| l.entries.len());
            hud::text_outlined(
                &format!("Lobby  {n}"),
                px + 20. * ui,
                py + 76. * ui,
                22. * ui,
                hud::col([1., 0.8, 0.3], 1.),
            );
            if let Some(l) = lobby {
                for (i, e) in l.entries.iter().enumerate() {
                    let y = py + 108. * ui + i as f32 * 24. * ui;
                    let role = if e.choice == 1 { "seeker" } else { "hider" };
                    hud::text_outlined(&format!("{}  ({role})", e.name), px + 20. * ui, y, 15. * ui, WHITE);
                    if e.ready {
                        hud::text_right("READY", px + pw - 16. * ui, y, 14. * ui, hud::col([0.4, 1., 0.5], 1.));
                    }
                }
                if l.seconds_left > 0 {
                    hud::text_outlined(
                        &format!("Round starts in {}", l.seconds_left),
                        px + 20. * ui,
                        py + 256. * ui,
                        16. * ui,
                        hud::col([1., 0.9, 0.5], 1.),
                    );
                }
            }
        }
    }
}

fn draw_disguise_picker(choice: u8, near_spot: bool, ui: f32) {
    let (w, h) = (screen_width(), screen_height());
    hud::panel(w * 0.5 - 180. * ui, h - 110. * ui, 360. * ui, 90. * ui, 10. * ui, Color::new(0.03, 0.01, 0.06, 0.7));
    let kind = DisguiseKind::from_index(choice).unwrap_or(DisguiseKind::ALL[0]);
    hud::text_centered(kind.label(), w * 0.5, h - 76. * ui, 22. * ui, hud::col([0.9, 0.8, 0.5], 1.));
    let hint = if near_spot { "E: confirm this disguise here" } else { "Get closer to a legal spot to confirm" };
    hud::text_centered(
        hint,
        w * 0.5,
        h - 44. * ui,
        16. * ui,
        hud::col(if near_spot { [0.5, 1., 0.6] } else { [0.8, 0.7, 0.5] }, 1.),
    );
    hud::text_centered("Scroll or 1-0 to cycle", w * 0.5, h - 20. * ui, 13. * ui, hud::col([0.7, 0.7, 0.75], 1.));
}

fn draw_inspect_bar(progress: u32, ui: f32) {
    if progress == 0 {
        return;
    }
    let (w, h) = (screen_width(), screen_height());
    let frac = (progress as f32 / INSPECT_HOLD_TICKS as f32).clamp(0., 1.);
    hud::bar(w * 0.5 - 100. * ui, h * 0.5 + 40. * ui, 200. * ui, 14. * ui, frac, hud::col([0.9, 0.3, 0.2], 1.));
    hud::text_centered("INSPECTING", w * 0.5, h * 0.5 + 30. * ui, 14. * ui, hud::col([0.9, 0.6, 0.5], 1.));
}

fn draw_results(outcome: Option<Outcome>, ui: f32) {
    let (w, h) = (screen_width(), screen_height());
    let (pw, ph) = (480. * ui, 260. * ui);
    let (x, y) = ((w - pw) * 0.5, (h - ph) * 0.5);
    hud::panel(x, y, pw, ph, 14. * ui, Color::new(0.03, 0.01, 0.06, 0.88));
    let (title, colour) = match outcome {
        Some(Outcome::HidersWin) => ("HIDERS WIN", [0.4, 0.9, 0.5]),
        Some(Outcome::SeekerWins) => ("SEEKER WINS", [0.9, 0.5, 0.2]),
        None => ("ROUND OVER", [0.8, 0.8, 0.8]),
    };
    hud::text_centered(title, w * 0.5, y + 60. * ui, 38. * ui, hud::col(colour, 1.));
    hud::text_centered(
        "Back to the lobby shortly...",
        w * 0.5,
        y + ph - 30. * ui,
        16. * ui,
        hud::col([0.8, 0.8, 0.85], 1.),
    );
}

fn draw_tagged_banner(ui: f32) {
    let (w, h) = (screen_width(), screen_height());
    hud::text_centered("YOU WERE FOUND", w * 0.5, h * 0.5, 30. * ui, hud::col([0.9, 0.4, 0.3], 1.));
    hud::text_centered(
        "Watching the rest of the hunt",
        w * 0.5,
        h * 0.5 + 36. * ui,
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
    let look = house_look();
    let scene = build_scene();
    let mut sounds = SoundBank::start(life.options.silent(), 0.9, 0.6, render_audio).await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let mut fx = Fx::new(seed);
    let vignette = hud::make_vignette();

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
            .unwrap_or_else(|| "Hider".into());
        let made = client_transport(profile, address)
            .and_then(|t| Client::new(t, address, ClientConfig { name, key, choice: 0 }));
        match made {
            Ok(c) => client = Some(c),
            Err(e) => eprintln!("Cannot start the network ({e}); playing a local solo preview instead"),
        }
    }
    let mut prefers_seeker = false;

    // Offline: a local solo preview, always hider seat 0 (there is no bot AI for a seeker to drive). An
    // unattended run (a capture, a script, a smoke test) cannot press Enter at the title screen, so it
    // starts the preview immediately, exactly as the other BlueEngine games skip their own menus.
    const LOCAL_ME: usize = 0;
    let mut local: Option<Sim> = (client.is_none() && unattended).then(|| Sim::new(seed));
    let mut screen = if client.is_some() {
        Screen::Lobby
    } else if local.is_some() {
        Screen::Hiding
    } else {
        Screen::Select
    };
    let mut disguise_choice: u8 = 0;

    loop {
        input.begin_frame_with_keyboard(&mut shell, false, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music(); // a no-op: HAS_MUSIC is false, so render_audio() ships no stems
        }
        let ui = hud::ui_scale();
        let menu_step = input.menu_step();

        // The network first: read it, follow its state into the right screen, react to its events.
        let mut my_ready = false;
        if let Some(c) = client.as_mut() {
            c.poll(time as f64);
            c.frame(time as f64, dt);
            let me_tagged = c
                .participant()
                .filter(|&p| p < MAX_HIDERS)
                .and_then(|p| c.view().snapshot().map(|s| s.hiders[p].tagged))
                .unwrap_or(false);
            for event in c.drain_events() {
                react(&event, me_tagged, &mut sounds, &mut fx);
            }
            if let Some(s) = c.view().snapshot() {
                screen = match (c.state(), s.phase) {
                    (ClientState::Playing, Phase::Hiding(_)) => Screen::Hiding,
                    (ClientState::Playing, Phase::Seeking(_)) => Screen::Seeking,
                    (ClientState::Playing, Phase::RoundOver) => Screen::Results,
                    (ClientState::Lobby, _) => Screen::Lobby,
                    _ => screen,
                };
            } else if *c.state() == ClientState::Lobby {
                screen = Screen::Lobby;
            }
            let mine = c.lobby().and_then(|l| l.entries.iter().find(|e| e.slot == c.seat()).cloned());
            my_ready = mine.as_ref().is_some_and(|e| e.ready);
            if let Some(e) = &mine {
                prefers_seeker = e.choice == 1;
            }
            if auto_ready && *c.state() == ClientState::Lobby && mine.is_some() && !my_ready {
                c.ready(true);
            }
        } else if let Some(sim) = &local {
            screen = match sim.phase {
                Phase::Hiding(_) => Screen::Hiding,
                Phase::Seeking(_) => Screen::Seeking,
                Phase::RoundOver => Screen::Results,
            };
        }

        match screen {
            Screen::Select => {
                if input.pressed(KeyCode::Enter) || input.menu_select() {
                    sounds.play(sound(synth::Preset::Chime), 0.8);
                    local = Some(Sim::new(life.restart_seed()));
                    fx.clear();
                    life.reset_input();
                }
            }
            Screen::Lobby => {
                if let Some(c) = client.as_mut().filter(|c| *c.state() == ClientState::Lobby) {
                    let left = input.pressed(KeyCode::A) || input.pressed(KeyCode::Left) || menu_step.left;
                    let right = input.pressed(KeyCode::D) || input.pressed(KeyCode::Right) || menu_step.right;
                    if left || right {
                        prefers_seeker = !prefers_seeker;
                        c.select(prefers_seeker as u8);
                        sounds.play(sound(synth::Preset::Blip), 0.5);
                    }
                    if input.pressed(KeyCode::Enter) || input.pressed(KeyCode::Space) || input.menu_select() {
                        c.ready(!my_ready);
                        sounds.play(sound(synth::Preset::Chime), 0.7);
                    }
                }
            }
            Screen::Hiding | Screen::Seeking | Screen::Results => {
                // 1. Devices in: one frame of held state and the confirm press (from the script when there is one).
                let (held, edges, look_delta) = match life.script() {
                    Some(s) => (
                        Held {
                            forward: s.axis("fwd", "back"),
                            right: s.axis("right", "left"),
                            inspect: s.held("inspect"),
                        },
                        if s.starts("confirm") { CONFIRM } else { 0 },
                        [s.value("look", 0), s.value("look", 1)],
                    ),
                    None => {
                        let m = input.movement(&shell);
                        let mut edges = 0u32;
                        if shell.accepting_input() {
                            if input.pressed(KeyCode::E) {
                                edges |= CONFIRM;
                            }
                            let keys = [
                                KeyCode::Key1,
                                KeyCode::Key2,
                                KeyCode::Key3,
                                KeyCode::Key4,
                                KeyCode::Key5,
                                KeyCode::Key6,
                                KeyCode::Key7,
                                KeyCode::Key8,
                                KeyCode::Key9,
                                KeyCode::Key0,
                            ];
                            for (i, k) in keys.iter().enumerate() {
                                if input.pressed(*k) && i < DisguiseKind::ALL.len() {
                                    disguise_choice = i as u8;
                                }
                            }
                            let wheel = mouse_wheel().1;
                            let count = DisguiseKind::ALL.len() as u8;
                            if wheel > 0. {
                                disguise_choice = (disguise_choice + 1) % count;
                            } else if wheel < 0. {
                                disguise_choice = (disguise_choice + count - 1) % count;
                            }
                        }
                        let inspect = shell.accepting_input() && input.down(KeyCode::Space);
                        (Held { forward: m.forward, right: m.right, inspect }, edges, input.look_delta_with(&shell, dt))
                    }
                };
                life.feed(held, edges, look_delta);

                if client.is_none() {
                    if let Some(sim) = local.as_mut() {
                        let (save, load) =
                            life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
                        if save {
                            let notice = if sim.is_over() {
                                Notice::refused("the round is over")
                            } else {
                                life.quick_save(sim, "Quick save")
                            };
                            announce(&mut fx, &notice);
                        }
                        if load {
                            let notice = life.quick_load(sim);
                            announce(&mut fx, &notice);
                        }
                    }
                }

                // 2. Simulation: whole fixed ticks, each with exactly one Input.
                let playing = !shell.paused;
                if let Some(c) = client.as_mut() {
                    for _ in 0..life.ticks(dt, 1., playing) {
                        let tick = life.take_tick();
                        let hiding = matches!(c.view().snapshot().map(|s| s.phase), Some(Phase::Hiding(_)));
                        c.tick(Input {
                            forward: tick.held.forward,
                            right: tick.held.right,
                            sprint: false,
                            look: tick.look,
                            choose_disguise: hiding.then_some(disguise_choice),
                            confirm_placement: tick.pressed(CONFIRM),
                            inspect: tick.held.inspect,
                        });
                    }
                    if playing {
                        fx.update(dt);
                    }
                } else if let Some(sim) = local.as_mut() {
                    for _ in 0..life.ticks(dt, 1., playing) {
                        let tick = life.take_tick();
                        let hiding = matches!(sim.phase, Phase::Hiding(_));
                        let mut inputs = Inputs::default();
                        inputs.0[LOCAL_ME] = Input {
                            forward: tick.held.forward,
                            right: tick.held.right,
                            sprint: false,
                            look: tick.look,
                            choose_disguise: hiding.then_some(disguise_choice),
                            confirm_placement: tick.pressed(CONFIRM),
                            inspect: tick.held.inspect,
                        };
                        sim.step(&inputs);
                        let me_tagged = sim.hiders[LOCAL_ME].tagged;
                        for event in sim.drain_events() {
                            react(&event, me_tagged, &mut sounds, &mut fx);
                        }
                    }
                    if playing {
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

        // 3. Camera: first person, from whichever source of truth is live.
        let pose = camera_pose(&client, &local, LOCAL_ME);

        clear_background(look.clear_color());
        let mut view = View::first_person(pose.eye, pose.yaw, pose.pitch);
        view.fov = 78_f32.to_radians();
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for mesh in &scene.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera(0.05, 400.));
        materials.set_scene(&look, view.eye, time, 0.);
        let mut world = Batch::new();
        let mut alpha = Batch::new();
        let mut add = Batch::new();
        draw_world(&client, &local, LOCAL_ME, &scene, &mut world);
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        gl_use_material(&materials.world);
        for mesh in &scene.house {
            draw_mesh(mesh);
        }
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_default_material();
        set_default_camera();

        hud::overlay(&vignette, Color::new(0., 0., 0., 0.5));
        hud::draw_popups(&fx.popups, &view, ui);
        hud::draw_banners(&fx.banners, ui);
        match screen {
            Screen::Select => {
                draw_select(time, ui, "Enter: solo preview (hider)    --connect HOST:PORT for a real hunt")
            }
            Screen::Lobby => {
                if let Some(c) = &client {
                    draw_lobby(c, prefers_seeker, my_ready, time, ui);
                }
            }
            Screen::Hiding => {
                if pose.am_seeker {
                    hud::text_centered(
                        "The hiders are getting into position...",
                        screen_width() * 0.5,
                        screen_height() * 0.5,
                        24. * ui,
                        WHITE,
                    );
                } else if !pose.am_tagged {
                    hud::crosshair(ui, 0., Color::new(1., 1., 1., 0.4));
                    let near = nearest_spot_distance(&client, &local, LOCAL_ME) < CONFIRM_RADIUS;
                    draw_disguise_picker(disguise_choice, near, ui);
                }
            }
            Screen::Seeking => {
                if pose.am_seeker {
                    hud::crosshair(ui, 0., Color::new(1., 0.6, 0.5, 0.9));
                    draw_inspect_bar(pose.inspect_progress, ui);
                } else if pose.am_tagged {
                    draw_tagged_banner(ui);
                }
            }
            Screen::Results => {
                let outcome = client
                    .as_ref()
                    .and_then(|c| c.view().snapshot().and_then(|s| s.outcome))
                    .or(local.as_ref().and_then(|s| s.outcome));
                draw_results(outcome, ui);
            }
        }

        let controls: Vec<&str> = identity.controls.split(", ").collect();
        let audio_menu = AudioMenu { music_on: false, sfx_on: true, has_music: HAS_MUSIC };
        let outcome = shell.local_menu_with_audio(&identity.title, &controls, audio_menu);
        if outcome.quit {
            break;
        }

        // 4. Evidence for a caller that cannot watch: screenshots and frame times, then exit.
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

/// What the camera should show this frame, and a little HUD state that is cheapest to compute alongside it.
struct Pose {
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    am_seeker: bool,
    am_tagged: bool,
    inspect_progress: u32,
}

fn camera_pose(client: &Option<Client>, local: &Option<Sim>, local_me: usize) -> Pose {
    if let Some(c) = client {
        let participant = c.participant();
        let am_seeker = participant == Some(SEEKER_SLOT);
        if let Some(mine) = c.view().mine() {
            // A moving entity (an unconfirmed hider, or the seeker): predicted locally. Pitch is never
            // sent over the wire (movement never depends on it), so it is tracked client-side separately.
            let inspect_progress = if am_seeker { c.view().snapshot().map_or(0, |s| s.inspect_progress) } else { 0 };
            return Pose {
                eye: v3(mine.position),
                yaw: mine.yaw,
                pitch: c.view().pitch(),
                am_seeker,
                am_tagged: false,
                inspect_progress,
            };
        }
        if let (Some(p), Some(s)) = (participant, c.view().snapshot()) {
            if p < MAX_HIDERS {
                let h = s.hiders[p];
                return Pose {
                    eye: vec3(h.pos.0, EYE_HEIGHT, h.pos.2),
                    yaw: h.yaw,
                    pitch: c.view().pitch(),
                    am_seeker: false,
                    am_tagged: h.tagged,
                    inspect_progress: 0,
                };
            }
        }
        return Pose {
            eye: vec3(0., EYE_HEIGHT, 0.),
            yaw: 0.,
            pitch: 0.,
            am_seeker,
            am_tagged: false,
            inspect_progress: 0,
        };
    }
    if let Some(sim) = local {
        let h = &sim.hiders[local_me];
        let p = h.controller.position;
        return Pose {
            eye: v3(p),
            yaw: h.controller.yaw,
            pitch: h.controller.pitch,
            am_seeker: false,
            am_tagged: h.tagged,
            inspect_progress: 0,
        };
    }
    Pose { eye: vec3(0., EYE_HEIGHT, 0.), yaw: 0., pitch: 0., am_seeker: false, am_tagged: false, inspect_progress: 0 }
}

/// How far (metres, flat ground) the local hider is from the nearest legal disguise spot, for the "get
/// closer" hint; infinite once there is nothing to measure (spectating, or already frozen).
fn nearest_spot_distance(client: &Option<Client>, local: &Option<Sim>, local_me: usize) -> f32 {
    let pos = if let Some(c) = client {
        match c.view().mine() {
            Some(mine) => mine.position,
            None => return f32::INFINITY,
        }
    } else if let Some(sim) = local {
        sim.hiders[local_me].controller.position
    } else {
        return f32::INFINITY;
    };
    prop_hunt::layout::DISGUISE_SPOTS
        .iter()
        .map(|s| vesper3d::math::V(pos.0 - s.pos.0, 0., pos.2 - s.pos.2).length())
        .fold(f32::INFINITY, f32::min)
}

/// Decoy props (always drawn) plus every hider's and the seeker's mesh, drawn from whichever snapshot
/// this client actually has — a blanked (redacted) hider simply has no disguise to look up and is
/// skipped, by construction, never by a special case here.
fn draw_world(client: &Option<Client>, local: &Option<Sim>, local_me: usize, scene: &Scene, world: &mut Batch) {
    for decoy in prop_hunt::layout::decoys() {
        let mesh = &scene.disguises[decoy.kind.index() as usize];
        let m = Mat4::from_translation(vec3(decoy.pos.0, 0., decoy.pos.2)) * Mat4::from_rotation_y(-decoy.yaw);
        world.add(mesh, m, Tint::NONE);
    }
    if let Some(c) = client {
        if let Some(s) = c.view().snapshot() {
            for h in &s.hiders {
                draw_hider_view(h, scene, world);
            }
            let m =
                Mat4::from_translation(vec3(s.seeker.pos.0, 0., s.seeker.pos.2)) * Mat4::from_rotation_y(-s.seeker.yaw);
            world.add(&scene.humanoid_seeker, m, Tint::NONE);
        }
        return;
    }
    if let Some(sim) = local {
        let m = Mat4::from_translation(vec3(sim.seeker.position.0, 0., sim.seeker.position.2))
            * Mat4::from_rotation_y(-sim.seeker.yaw);
        world.add(&scene.humanoid_seeker, m, Tint::NONE);
        for (i, h) in sim.hiders.iter().enumerate() {
            if i == local_me {
                continue; // never draw your own first-person body
            }
            let pos = h.controller.position;
            let m = Mat4::from_translation(vec3(pos.0, 0., pos.2)) * Mat4::from_rotation_y(-h.controller.yaw);
            match h.disguise {
                Some(kind) => world.add(&scene.disguises[kind.index() as usize], m, Tint::NONE),
                None => world.add(&scene.humanoid_hider, m, Tint::NONE),
            }
        }
    }
}

fn draw_hider_view(h: &HiderView, scene: &Scene, world: &mut Batch) {
    let Some(kind) = h.disguise else { return }; // blanked (redacted) or not yet disguised: nothing to draw
    let m = Mat4::from_translation(vec3(h.pos.0, 0., h.pos.2)) * Mat4::from_rotation_y(-h.yaw);
    world.add(&scene.disguises[kind.index() as usize], m, Tint::NONE);
}
