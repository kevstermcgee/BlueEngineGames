//! Spooky Kart: the window, renderer, sound and input around the race in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "fwd:0-400,left:60-120,drift:60-120,perk@200"   drive the human input path from a cue script
//!   --autopilot   the bots' driving logic steers your kart (for captures and load tests)
//!   --connect HOST:PORT   join a Spooky Kart server ([--transport development|production] [--name N] [--join-key K] [--auto-ready])
//!   --hub HOST:PORT   the hub that Play Online asks for rooms (else hub.txt beside the exe, else the last hub used,
//!                     else the public one); P (or Y on a controller) on the select screen opens it
//!   --screen online|create   open on Play Online (create: with the Create Room box open) [--join-room NAME|public]
//!                     [--create-room NAME]: join or make a room as soon as the list arrives (for captures and tests)
//!                     [--online-script "30:down,60:enter,90:type=Hi_there"]: press keys on given frames (cues: up, down,
//!                     left, right, tab, enter, space, backspace, back, type=TEXT, erase=N)
//!   --debug-input   print the input gate and held state once a second (for "my keys do nothing" reports)
//!   --select   open on the character select screen even when capturing
//!   --character NAME   start a race at once as this driver (any part of the name: ghost, frank, ...)
//!   --difficulty easy|medium|hard   how hard the offline bots drive (default medium; also pickable on the select screen)
//!   --shadows off|simple|full   shadows for this run: Simple (soft contact shadows) is the default, Full casts real ones
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved race / where F5 saves (default: next to the exe)
//! Music, sound effects and shadows can be switched in the Esc menu's Settings (remembered in settings.json next to the exe).
//! Keyboard: W/S gas and brake, A/D steer, Shift drift, Space perk, F5/F9 quick save and load, Esc menu.
//! Controller: right trigger (or A) gas, left trigger brake, left stick steer, bumpers drift, X perk.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod models;
mod platform;

use macroquad::prelude::*;
use spooky_kart::online::{self, Btn, RoomTag};
use spooky_kart::{
    controls::{self, Raw},
    music,
    track::{forward, wrap_angle, yaw_of},
    Character, Difficulty, Driver, Event, HazardKind, Inputs, Kart, KartGame, KartInput, Perk, Phase, Sim, ALL, LAPS,
    MAX_RACERS,
};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use vesper3d::viewer::{
    devkit::{
        beside_exe, downloads_dir, flag_value, has_flag, parse_size, sanitize_filename, synth, unique_path, CharFilter,
        Juice, Lifecycle, MenuStep, Notice, ServerOrigin, Settings, ShadowQuality, TextField,
    },
    game_client::{self, AudioMenu, GameShell},
    game_input::ClientInput,
    gamepad::Button,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, Shadows, SoundBank, Template, Tint, View},
    net::{client_transport, AnyTransport, TransportProfile},
    netplay::{
        hub::{
            self,
            client::{scroll_to, Action, DialogPhase, JoinWait, Online, Pane, RetryStep},
            wire::{RoomInfo, MAX_NAME_CHARS},
        },
        ClientConfig, ClientState, ConnectFailure, NetClient,
    },
};

/// Title, tagline and controls live in one file, shared with the build script and `scripts/ship.py`.
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

/// Held device state; the perk press travels separately as an edge bit.
#[derive(Clone, Copy, Default)]
struct Held {
    throttle: f32,
    steer: f32,
    drift: bool,
}
const PERK: u32 = 1;
/// The cue names a `--script` may use (`save` and `load` are always understood).
const CUES: [&str; 6] = ["fwd", "back", "left", "right", "drift", "perk"];

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 9] = [
    synth::Preset::Blip,
    synth::Preset::PowerUp,
    synth::Preset::Hit,
    synth::Preset::Jump,
    synth::Preset::Coin,
    synth::Preset::Pickup,
    synth::Preset::GameOver,
    synth::Preset::Select,
    synth::Preset::Back,
];
fn sound(preset: synth::Preset) -> usize {
    SOUNDS.iter().position(|p| *p == preset).unwrap_or(0)
}
/// The race music (spooky_kart::music): three synced stems, base / melodic / lead, faded with the race.
const HAS_MUSIC: bool = true;

/// Renders on the worker thread. `music_wav` is filled once with the whole loop (all three layers mixed) so
/// the Settings screen's "Save music" button can hand over the exact track without regenerating it.
fn render_audio(music_wav: Arc<OnceLock<Vec<u8>>>) -> Rendered {
    let stems = if HAS_MUSIC {
        let loop_ = synth::music_loop(&music::SPEC);
        let mix: Vec<f32> =
            loop_.base.iter().zip(&loop_.melodic).zip(&loop_.lead).map(|((a, b), c)| (a + b + c) * 0.5).collect();
        let _ = music_wav.set(synth::wav_bytes(&mix, synth::RATE));
        [&loop_.base, &loop_.melodic, &loop_.lead].map(|stem| synth::wav_bytes(stem, synth::RATE)).to_vec()
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

/// Meshes that stand up from the ground and so cast shadows, with the ground rectangle
/// `[min_x, min_z, max_x, max_z]` they lie within (the shadow pass skips the ones far from its focus).
struct Casters {
    bounds: [f32; 4],
    meshes: Vec<Mesh>,
}

/// The meshes built once at startup.
struct Scene {
    sky: Vec<Mesh>,
    /// Ground slab, road, verges and kerbs: they only receive shadows and never go into the shadow pass
    /// (the slab is 1.4 km wide).
    ground: Vec<Mesh>,
    /// Walls, the start arch and the scenery: drawn in the normal pass and cast in the shadow pass.
    casters: Vec<Casters>,
    karts: Vec<Template>,
    bandage: Template,
    bone: Template,
    podium: Vec<Mesh>,
}

fn build_scene(sim: &Sim) -> Scene {
    let mut sky = Template::new();
    sky.sky_dome(
        400.,
        |e| {
            let t = e.clamp(0., 1.).sqrt();
            let (low, high) = ([0.30, 0.11, 0.26], [0.02, 0.01, 0.09]);
            [low[0] + (high[0] - low[0]) * t, low[1] + (high[1] - low[1]) * t, low[2] + (high[2] - low[2]) * t]
        },
        32,
        16,
    );
    let mut podium = Template::new();
    podium.cylinder(vec3(-300., -0.05, -300.), 3.4, 0.22, [0.18, 0.12, 0.25], 0.1, 32);
    podium.ring(vec3(-300., 0.2, -300.), 3.0, 3.3, [1., 0.5, 0.05], 0.9, 32);
    let world = models::world(sim.track());
    Scene {
        sky: sky.to_meshes(),
        ground: world.receivers.iter().flat_map(|t| t.to_meshes()).collect(),
        casters: world
            .casters
            .iter()
            .map(|c| Casters { bounds: c.bounds, meshes: c.templates.iter().flat_map(|t| t.to_meshes()).collect() })
            .collect(),
        karts: ALL.iter().map(|c| models::kart(*c)).collect(),
        bandage: models::bandage(),
        bone: models::bone(),
        podium: podium.to_meshes(),
    }
}

fn halloween_look() -> Look {
    let mut look = Look::night();
    look.ambient_sky = [0.26, 0.20, 0.42];
    look.ambient_ground = [0.10, 0.06, 0.14];
    // The moon, lower and more from the side than the stock night look (almost overhead, which hid a kart's
    // shadow under the kart): shadows now fall to the right of a driver heading east and are long enough to
    // read. The colour is scaled up to keep flat ground as bright as it was.
    look.key_direction = [-0.1, 0.62, -0.6];
    look.key_color = [0.99, 0.81, 1.15];
    look.rim_color = [1.0, 0.5, 0.15];
    look.fog_color = [0.07, 0.04, 0.11];
    look.fog_density = 0.0085;
    look
}

/// Shadow box: half-width and depth along the light, in metres. 2048 texels over 2 x 32 m is 3 cm a texel,
/// which keeps a kart's and a gravestone's shadow crisp; the box is centred a little ahead of the human's kart
/// (where the chase camera looks) and fades out over its outer edge.
const SHADOW_HALF: f32 = 32.;
const SHADOW_DEPTH: f32 = 80.;
const SHADOW_RESOLUTION: u32 = 2048;
/// How far ahead of the kart the shadow box is centred, metres.
const SHADOW_AHEAD: f32 = 12.;
/// The podium the select screen shows the kart on: where it is, how big, and how high its top is.
const PODIUM: Vec3 = Vec3::new(-300., 0., -300.);
const PODIUM_RADIUS: f32 = 3.4;
const PODIUM_TOP: f32 = 0.17;
/// Radius of the soft contact shadow under a kart, metres. A kart is about 1.8 wide and 3.6 long and the blob is a
/// disc whose opacity is already down to a quarter at 70% of its radius, so it is drawn wider than the kart.
const KART_BLOB: f32 = 2.0;

/// Whether a caster cell (ground rectangle `[min_x, min_z, max_x, max_z]`) can shadow anything the shadow box
/// around `focus` covers: the box's corner distance plus a margin for tall things beside it.
fn shadow_reaches(bounds: &[f32; 4], focus: Vec3) -> bool {
    let reach = SHADOW_HALF * std::f32::consts::SQRT_2 + 12.;
    let nearest = vec2(focus.x.clamp(bounds[0], bounds[2]), focus.z.clamp(bounds[1], bounds[3]));
    (nearest - vec2(focus.x, focus.z)).length() <= reach
}

fn v3(p: vesper3d::math::V) -> Vec3 {
    vec3(p.0, p.1, p.2)
}

/// Where `human` starts: the fifth grid slot, so there is always someone to chase.
fn new_race(seed: u64, human: Character, difficulty: Difficulty) -> (Sim, usize) {
    let mut grid: Vec<(Character, Driver)> = ALL.iter().filter(|c| **c != human).map(|c| (*c, Driver::Bot)).collect();
    let slot = 5.min(grid.len());
    grid.insert(slot, (human, Driver::Human));
    (Sim::with_difficulty(seed, &grid, difficulty), slot)
}

fn character_from(name: &str) -> Option<Character> {
    if let Ok(n) = name.parse::<usize>() {
        return Some(Character::from_index(n));
    }
    let name = name.to_lowercase();
    ALL.iter().copied().find(|c| c.name().to_lowercase().contains(&name))
}

fn short_name(c: Character) -> String {
    c.name().replace("Frankenstein's Monster", "Monster")
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn clock(seconds: f32) -> String {
    let total = (seconds * 100.).round() as u32;
    format!("{}:{:02}.{:02}", total / 6000, (total / 100) % 60, total % 100)
}

/// Turn one simulation event into sound, particles, shake and text. The `match` is exhaustive on
/// purpose: a new `Event` variant must be handled (or explicitly ignored) here.
fn react(event: &Event, sim: &Sim, me: usize, sounds: &mut SoundBank, fx: &mut Fx, juice: &mut Juice) {
    let at = |k: usize| v3(sim.karts[k].pos) + vec3(0., 0.8, 0.);
    match event {
        Event::Count(_) => sounds.play(sound(synth::Preset::Blip), 0.7),
        Event::Go => {
            sounds.play(sound(synth::Preset::Jump), 0.9);
            fx.banner("GO!", "", [1., 0.6, 0.1]);
        }
        Event::LapDone { kart, lap, ticks } => {
            if *kart == me {
                sounds.play(sound(synth::Preset::Coin), 0.8);
                if *lap < LAPS {
                    fx.banner(format!("LAP {} / {LAPS}", lap + 1), clock(*ticks as f32 / 60.), [1., 0.8, 0.3]);
                }
            }
        }
        Event::Finished { kart, place, .. } => {
            if *kart == me {
                sounds.play(sound(synth::Preset::GameOver), 0.9);
                fx.banner(format!("{} PLACE", ordinal(*place)), sim.karts[me].character.name(), [1., 0.9, 0.4]);
                fx.confetti_fountain(at(me), 60, 9.);
            }
        }
        Event::DriftBoost { kart, tier } => {
            fx.sparks(at(*kart), 10 + 8 * *tier as usize, 7., [1., 0.6, 0.15]);
            if *kart == me {
                sounds.play(sound(synth::Preset::PowerUp), 0.6);
                juice.kick(2. + *tier as f32);
            }
        }
        Event::Bump { a, b, speed } => {
            let mid = (at(*a) + at(*b)) * 0.5;
            fx.sparks(mid, 10, 6., [1., 0.9, 0.6]);
            if *a == me || *b == me {
                sounds.play(sound(synth::Preset::Hit), (speed / 12.).clamp(0.3, 1.));
                juice.shake((speed / 14.).clamp(0.2, 0.8));
            }
        }
        Event::WallHit { kart, speed } => {
            fx.sparks(at(*kart), 14, 8., [0.8, 0.5, 1.]);
            if *kart == me {
                sounds.play(sound(synth::Preset::Hit), 0.7);
                juice.shake((speed / 18.).clamp(0.3, 1.));
            }
        }
        Event::PerkUsed { kart, perk } => {
            let colour = models::colour(sim.karts[*kart].character);
            match perk {
                Perk::Honk => fx.ring(at(*kart), Vec3::Y, 0.5, 9., 0.5, [1., 0.9, 0.2]),
                Perk::Phase => fx.ring(at(*kart), Vec3::Y, 0.5, 3., 0.7, [0.6, 0.9, 1.]),
                Perk::CrowSwarm => fx.sparks(at(*kart) + vec3(0., 1.5, 0.), 24, 10., [0.1, 0.05, 0.15]),
                _ => fx.ring(at(*kart), Vec3::Y, 0.4, 2.5, 0.5, colour),
            }
            if *kart == me {
                sounds.play(sound(synth::Preset::Pickup), 0.7);
            }
        }
        Event::HazardHit { kart, kind } => {
            let colour = if *kind == HazardKind::Bone { [0.9, 0.9, 0.8] } else { [0.9, 0.85, 0.7] };
            fx.sparks(at(*kart), 12, 5., colour);
            if *kart == me {
                sounds.play(sound(synth::Preset::Hit), 0.6);
                juice.flash([0.8, 0.3, 1.], 0.3);
            }
        }
        Event::RaceOver => {}
    }
}

/// Write the race music to the player's Downloads folder as a `.wav`, never overwriting an earlier save.
fn save_music(music_wav: &OnceLock<Vec<u8>>, title: &str) -> Notice {
    let fail = |detail: String, ok_colour: bool| Notice {
        title: "SAVE FAILED",
        detail,
        color: if ok_colour { [1., 0.7, 0.3] } else { [1., 0.5, 0.3] },
        ok: false,
    };
    let Some(bytes) = music_wav.get() else {
        return fail("the music is still rendering; try again in a moment".into(), true);
    };
    let Some(dir) = downloads_dir() else {
        return fail("could not find your Downloads folder".into(), false);
    };
    let path = unique_path(&dir, &format!("{} - Race Music", sanitize_filename(title)), "wav");
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, bytes)) {
        Ok(()) => Notice { title: "MUSIC SAVED", detail: path.display().to_string(), color: [0.4, 0.9, 0.6], ok: true },
        Err(error) => fail(error.to_string(), false),
    }
}

/// Show the outcome of a quick save or load.
fn announce(fx: &mut Fx, notice: &Notice) {
    fx.banners.clear();
    fx.banner(notice.title, notice.detail.clone(), notice.color);
}

type Client = NetClient<KartGame, AnyTransport>;

enum Screen {
    Select,
    /// Play Online: the hub's room list and the Create Room box.
    Online,
    Lobby,
    Race,
}

fn stat_bar(label: &str, value: f32, x: f32, y: f32, w: f32, ui: f32, colour: Color) {
    hud::text_outlined(label, x, y + 14. * ui, 15. * ui, WHITE);
    hud::bar(x + 120. * ui, y, w, 18. * ui, value.clamp(0., 1.), colour);
}

/// The Easy / Medium / Hard chips and a line about the chosen one (offline only).
fn draw_difficulty(current: Difficulty, ui: f32) {
    let w = screen_width();
    let (cw, ch, gap, y) = (130. * ui, 34. * ui, 12. * ui, 118. * ui);
    let total = cw * 3. + gap * 2.;
    hud::text_outlined(
        "Rivals",
        w * 0.5 - total * 0.5 - 90. * ui,
        y + 24. * ui,
        18. * ui,
        hud::col([0.8, 0.7, 1.], 1.),
    );
    for (i, d) in Difficulty::ALL.iter().enumerate() {
        let x = w * 0.5 - total * 0.5 + i as f32 * (cw + gap);
        let on = *d == current;
        hud::panel(
            x,
            y,
            cw,
            ch,
            8. * ui,
            if on { Color::new(0.35, 0.15, 0.05, 0.92) } else { Color::new(0.05, 0.02, 0.12, 0.7) },
        );
        let tint = match d {
            Difficulty::Easy => [0.4, 0.9, 0.5],
            Difficulty::Medium => [1., 0.8, 0.3],
            Difficulty::Hard => [1., 0.4, 0.35],
        };
        if on {
            draw_rectangle(x + 10. * ui, y + ch - 7. * ui, cw - 20. * ui, 3. * ui, hud::col(tint, 1.));
        }
        hud::text_centered(
            &d.name().to_uppercase(),
            x + cw * 0.5,
            y + 23. * ui,
            18. * ui,
            if on { WHITE } else { hud::col([0.7, 0.65, 0.85], 1.) },
        );
    }
    hud::text_centered(current.blurb(), w * 0.5, y + ch + 24. * ui, 16. * ui, hud::col([1., 0.9, 0.6], 1.));
}

fn draw_select(choice: usize, time: f32, ui: f32, prompt: &str, difficulty: Option<Difficulty>) {
    let c = Character::from_index(choice);
    let (w, h) = (screen_width(), screen_height());
    hud::text_centered("SPOOKY KART", w * 0.5, 60. * ui, 54. * ui, hud::col([1., 0.55, 0.1], 1.));
    hud::text_centered("Choose your driver", w * 0.5, 96. * ui, 20. * ui, hud::col([0.8, 0.7, 1.], 1.));
    if let Some(d) = difficulty {
        draw_difficulty(d, ui);
    }
    // The driver card.
    let (px, py, pw) = (40. * ui, h * 0.5 - 130. * ui, 360. * ui);
    hud::panel(px, py, pw, 300. * ui, 14. * ui, Color::new(0.05, 0.02, 0.12, 0.75));
    hud::text_outlined(c.name(), px + 20. * ui, py + 44. * ui, 30. * ui, hud::col(models::colour(c), 1.));
    hud::text_outlined(c.kart_name(), px + 20. * ui, py + 72. * ui, 18. * ui, hud::col([0.75, 0.7, 0.9], 1.));
    let s = c.stats();
    let bar_w = 150. * ui;
    stat_bar(
        "Top speed",
        (s.top_speed - 22.) / 9.,
        px + 20. * ui,
        py + 96. * ui,
        bar_w,
        ui,
        hud::col([1., 0.5, 0.2], 1.),
    );
    stat_bar(
        "Acceleration",
        (s.accel - 8.) / 9.,
        px + 20. * ui,
        py + 124. * ui,
        bar_w,
        ui,
        hud::col([1., 0.85, 0.2], 1.),
    );
    stat_bar(
        "Handling",
        (s.handling - 1.2) / 1.5,
        px + 20. * ui,
        py + 152. * ui,
        bar_w,
        ui,
        hud::col([0.3, 0.9, 0.5], 1.),
    );
    stat_bar("Weight", (s.mass - 0.5) / 1.4, px + 20. * ui, py + 180. * ui, bar_w, ui, hud::col([0.6, 0.6, 1.], 1.));
    stat_bar(
        "Off-road",
        (s.offroad - 0.4) / 0.6,
        px + 20. * ui,
        py + 208. * ui,
        bar_w,
        ui,
        hud::col([0.5, 0.8, 0.3], 1.),
    );
    for (i, line) in hud::wrap(c.perk_description(), pw - 40. * ui, 16. * ui).iter().enumerate() {
        hud::text_outlined(
            line,
            px + 20. * ui,
            py + 248. * ui + i as f32 * 20. * ui,
            16. * ui,
            hud::col([1., 0.9, 0.5], 1.),
        );
    }
    // The row of drivers.
    let card = (w - 80. * ui) / MAX_RACERS as f32;
    for (i, d) in ALL.iter().enumerate() {
        let x = 40. * ui + i as f32 * card;
        let on = i == choice;
        let lift = if on { -10. * ui } else { 0. };
        hud::panel(
            x + 4. * ui,
            h - 110. * ui + lift,
            card - 8. * ui,
            70. * ui,
            8. * ui,
            if on { Color::new(0.35, 0.15, 0.05, 0.9) } else { Color::new(0.05, 0.02, 0.12, 0.7) },
        );
        draw_rectangle(x + 12. * ui, h - 102. * ui + lift, card - 24. * ui, 8. * ui, hud::col(models::colour(*d), 1.));
        hud::text_centered(
            &short_name(*d),
            x + card * 0.5,
            h - 66. * ui + lift,
            15. * ui,
            if on { WHITE } else { hud::col([0.7, 0.65, 0.85], 1.) },
        );
    }
    let blink = 0.6 + 0.4 * (time * 3.).sin();
    hud::text_centered(prompt, w * 0.5, h - 20. * ui, 18. * ui, Color::new(1., 1., 1., blink));
}

fn draw_results(sim: &Sim, me: usize, ui: f32) {
    let (w, h) = (screen_width(), screen_height());
    let (pw, ph) = (520. * ui, 440. * ui);
    let (x, y) = ((w - pw) * 0.5, (h - ph) * 0.5);
    hud::panel(x, y, pw, ph, 14. * ui, Color::new(0.04, 0.02, 0.10, 0.88));
    let place = sim.karts[me].place;
    hud::text_centered(
        &format!("{} PLACE", ordinal(place)),
        w * 0.5,
        y + 52. * ui,
        42. * ui,
        hud::col(if place == 1 { [1., 0.85, 0.2] } else { [0.85, 0.8, 1.] }, 1.),
    );
    for (row, kart) in sim.standings().into_iter().enumerate() {
        let k = &sim.karts[kart];
        let ty = y + 96. * ui + row as f32 * 36. * ui;
        if kart == me {
            draw_rectangle(x + 14. * ui, ty - 22. * ui, pw - 28. * ui, 30. * ui, Color::new(1., 0.5, 0.1, 0.25));
        }
        draw_rectangle(x + 24. * ui, ty - 16. * ui, 8. * ui, 20. * ui, hud::col(models::colour(k.character), 1.));
        hud::text_outlined(&ordinal(row as u32 + 1), x + 44. * ui, ty, 20. * ui, WHITE);
        hud::text_outlined(k.character.name(), x + 100. * ui, ty, 20. * ui, WHITE);
        let time = k.finished_tick.map_or("DNF".to_string(), |t| clock(t as f32 / 60.));
        hud::text_right(&time, x + pw - 28. * ui, ty, 20. * ui, hud::col([0.85, 0.85, 1.], 1.));
    }
    hud::text_centered(
        "Enter or A: choose a driver     R: race again",
        w * 0.5,
        y + ph - 20. * ui,
        17. * ui,
        hud::col([1., 0.9, 0.6], 1.),
    );
}

fn draw_race_hud(sim: &Sim, me: usize, view: &View, ui: f32) {
    let (w, h) = (screen_width(), screen_height());
    let kart = &sim.karts[me];
    let standings = sim.standings();
    let rank = standings.iter().position(|k| *k == me).unwrap_or(0) as u32 + 1;
    // Position and lap.
    hud::panel(20. * ui, 16. * ui, 200. * ui, 96. * ui, 12. * ui, Color::new(0.04, 0.01, 0.10, 0.65));
    hud::text_outlined(&ordinal(rank), 34. * ui, 70. * ui, 50. * ui, hud::col([1., 0.8, 0.25], 1.));
    hud::text_outlined(
        &format!("/ {}", sim.karts.len()),
        128. * ui,
        70. * ui,
        24. * ui,
        hud::col([0.8, 0.75, 0.95], 1.),
    );
    hud::text_outlined(&format!("LAP {} / {LAPS}", (kart.lap + 1).min(LAPS)), 34. * ui, 100. * ui, 18. * ui, WHITE);
    if sim.racing() {
        hud::text_right(&clock(sim.race_tick as f32 / 60.), w - 20. * ui, 40. * ui, 26. * ui, WHITE);
    }
    // Standings down the left edge.
    for (row, k) in standings.iter().enumerate() {
        let y = 140. * ui + row as f32 * 22. * ui;
        draw_rectangle(
            20. * ui,
            y - 13. * ui,
            6. * ui,
            16. * ui,
            hud::col(models::colour(sim.karts[*k].character), 1.),
        );
        let tint = if *k == me { hud::col([1., 0.85, 0.3], 1.) } else { hud::col([0.85, 0.82, 0.95], 0.9) };
        hud::text_outlined(
            &format!("{}  {}", row + 1, short_name(sim.karts[*k].character)),
            32. * ui,
            y,
            15. * ui,
            tint,
        );
    }
    // Speed and perk.
    let kmh = (kart.speed() * 3.6).round() as u32;
    hud::text_right(&kmh.to_string(), w - 60. * ui, h - 60. * ui, 54. * ui, WHITE);
    hud::text_outlined("km/h", w - 52. * ui, h - 60. * ui, 18. * ui, hud::col([0.8, 0.75, 0.95], 1.));
    let c = kart.character;
    hud::panel(w * 0.5 - 150. * ui, h - 70. * ui, 300. * ui, 54. * ui, 10. * ui, Color::new(0.04, 0.01, 0.10, 0.65));
    let title = c.perk_description().split(':').next().unwrap_or("").to_string();
    let ready = kart.cooldown == 0;
    if c.perk().is_active() {
        hud::text_centered(
            &title,
            w * 0.5,
            h - 48. * ui,
            18. * ui,
            if ready { hud::col([1., 0.9, 0.4], 1.) } else { hud::col([0.6, 0.55, 0.7], 1.) },
        );
        let charge = 1. - kart.cooldown as f32 / c.cooldown_ticks().max(1) as f32;
        hud::bar(
            w * 0.5 - 130. * ui,
            h - 36. * ui,
            260. * ui,
            10. * ui,
            charge,
            if ready { hud::col([1., 0.7, 0.1], 1.) } else { hud::col([0.5, 0.4, 0.8], 1.) },
        );
    } else {
        hud::text_centered(
            &format!("{title} (always on)"),
            w * 0.5,
            h - 42. * ui,
            18. * ui,
            hud::col([1., 0.9, 0.4], 1.),
        );
    }
    if kart.drifting {
        let tier = (kart.drift_charge / 2.).clamp(0., 1.);
        hud::bar(w * 0.5 - 60. * ui, h - 92. * ui, 120. * ui, 8. * ui, tier, hud::col([1., 0.55, 0.1], 1.));
    }
    // The minimap.
    let pts = sim.track().samples();
    let (mut lo, mut hi) = (vec2(f32::MAX, f32::MAX), vec2(f32::MIN, f32::MIN));
    for p in pts {
        lo = lo.min(vec2(p.0, p.2));
        hi = hi.max(vec2(p.0, p.2));
    }
    let size = 170. * ui;
    let scale = (size / (hi.x - lo.x)).min(size / (hi.y - lo.y));
    let origin = vec2(w - size - 40. * ui, 70. * ui);
    let map = |p: vesper3d::math::V| origin + (vec2(p.0, p.2) - lo) * scale;
    draw_rectangle(
        origin.x - 10. * ui,
        origin.y - 10. * ui,
        size + 20. * ui,
        (hi.y - lo.y) * scale + 20. * ui,
        Color::new(0.04, 0.01, 0.10, 0.55),
    );
    for i in 0..pts.len() {
        let (a, b) = (map(pts[i]), map(pts[(i + 1) % pts.len()]));
        draw_line(a.x, a.y, b.x, b.y, 3. * ui, Color::new(0.6, 0.55, 0.8, 0.9));
    }
    for (i, k) in sim.karts.iter().enumerate() {
        let p = map(k.pos);
        let r = if i == me { 6. * ui } else { 4. * ui };
        if i == me {
            draw_circle(p.x, p.y, r + 2. * ui, WHITE);
        }
        draw_circle(p.x, p.y, r, hud::col(models::colour(k.character), 1.));
    }
    // Names over nearby rivals.
    for (i, k) in sim.karts.iter().enumerate() {
        if i == me || (k.pos - kart.pos).length() > 28. {
            continue;
        }
        if let Some(p) = view.project(v3(k.pos) + vec3(0., 3.2, 0.), w, h) {
            hud::text_centered(
                &short_name(k.character),
                p.x,
                p.y,
                15. * ui,
                hud::col(models::colour(k.character), 0.95),
            );
        }
    }
    // The start lights.
    if let Phase::Countdown(left) = sim.phase {
        let n = left.div_ceil(60).clamp(1, 3);
        let pulse = 1. + 0.25 * ((left % 60) as f32 / 60.);
        hud::text_centered(&n.to_string(), w * 0.5, h * 0.35, 150. * ui * pulse, hud::col([1., 0.55, 0.1], 1.));
    }
}

/// The networked lobby: the driver picker plus who is here and what the server says.
fn draw_lobby(client: &Client, choice: usize, my_ready: bool, time: f32, ui: f32, room: Option<&RoomCtx>) {
    let (w, h) = (screen_width(), screen_height());
    let prompt = match (my_ready, room.is_some()) {
        (true, false) => "Ready! Enter or A to un-ready    O: practice offline",
        (false, false) => "A / D or left stick to choose    Enter or A when ready    O: practice offline",
        (true, true) => "Ready! Enter or A to un-ready    Backspace or B: other rooms    O: practice offline",
        (false, true) => "A / D: driver    Enter or A: ready    Backspace or B: other rooms    O: practice offline",
    };
    draw_select(choice, time, ui, prompt, None);
    if let Some(ctx) = room {
        let (name, friends) = ctx.tag.lobby_lines();
        hud::text_centered(&name, w * 0.5, 134. * ui, 24. * ui, hud::col([1., 0.85, 0.4], 1.));
        if let Some(friends) = friends {
            hud::text_centered(&friends, w * 0.5, 160. * ui, 17. * ui, hud::col([0.8, 0.7, 1.], 1.));
        }
    }
    let (pw, px, py) = (330. * ui, w - 370. * ui, h * 0.5 - 130. * ui);
    hud::panel(px, py, pw, 300. * ui, 14. * ui, Color::new(0.05, 0.02, 0.12, 0.75));
    match client.state() {
        ClientState::Connecting => {
            hud::text_outlined("Connecting...", px + 20. * ui, py + 44. * ui, 26. * ui, hud::col([1., 0.8, 0.3], 1.))
        }
        ClientState::Rejected(why) | ClientState::Disconnected(why) => {
            let failure = client.failure();
            let waiting = room.is_some_and(|r| r.wait.is_some());
            let view = online::failure_view(failure.as_ref(), why, room.is_some(), waiting);
            let title_tint = if view.waiting { [1., 0.8, 0.3] } else { [1., 0.4, 0.4] };
            hud::text_outlined(view.title, px + 20. * ui, py + 44. * ui, 26. * ui, hud::col(title_tint, 1.));
            for (i, line) in hud::wrap(&view.text, pw - 40. * ui, 18. * ui).iter().enumerate() {
                hud::text_outlined(line, px + 20. * ui, py + 76. * ui + i as f32 * 22. * ui, 18. * ui, WHITE);
            }
            let (footer, footer2) = match (room.is_some(), view.waiting) {
                (false, _) => ("Enter or O: practice offline", None),
                (true, true) => ("Trying again every few seconds.", Some("Backspace or B: other rooms")),
                (true, false) => ("Enter or A: try again", Some("Backspace or B: other rooms")),
            };
            let footer_y = if footer2.is_some() { 236. } else { 260. };
            hud::text_outlined(footer, px + 20. * ui, py + footer_y * ui, 16. * ui, hud::col([1., 0.9, 0.5], 1.));
            if let Some(f) = footer2 {
                hud::text_outlined(f, px + 20. * ui, py + 260. * ui, 16. * ui, hud::col([1., 0.9, 0.5], 1.));
            }
        }
        _ => {
            let lobby = client.lobby();
            let n = lobby.map_or(0, |l| l.entries.len());
            hud::text_outlined(
                &format!("Lobby  {n}/8"),
                px + 20. * ui,
                py + 40. * ui,
                26. * ui,
                hud::col([1., 0.8, 0.3], 1.),
            );
            if let Some(l) = lobby {
                for (i, e) in l.entries.iter().enumerate() {
                    let y = py + 76. * ui + i as f32 * 26. * ui;
                    let c = Character::from_wire(e.choice).unwrap_or(Character::Vampire);
                    draw_rectangle(px + 20. * ui, y - 16. * ui, 8. * ui, 20. * ui, hud::col(models::colour(c), 1.));
                    let mine = e.slot == client.seat();
                    hud::text_outlined(
                        &e.name,
                        px + 38. * ui,
                        y,
                        17. * ui,
                        if mine { hud::col([1., 0.85, 0.3], 1.) } else { WHITE },
                    );
                    hud::text_right(&short_name(c), px + pw - 84. * ui, y, 15. * ui, hud::col([0.8, 0.75, 0.95], 1.));
                    if e.ready {
                        hud::text_right("READY", px + pw - 16. * ui, y, 15. * ui, hud::col([0.4, 1., 0.5], 1.));
                    }
                }
                let bots = (l.participants as usize).saturating_sub(l.entries.len());
                let footer = if l.seconds_left > 0 {
                    format!("Race starts in {}", l.seconds_left)
                } else if bots > 0 {
                    format!("{bots} bot{} will fill the grid", if bots == 1 { "" } else { "s" })
                } else {
                    "Waiting for everyone to be ready".to_string()
                };
                hud::text_outlined(&footer, px + 20. * ui, py + 280. * ui, 16. * ui, hud::col([1., 0.9, 0.5], 1.));
            }
            if client.stats().rtt_ms > 0. {
                hud::text_right(
                    &format!("{:.0} ms", client.stats().rtt_ms),
                    w - 20. * ui,
                    30. * ui,
                    16. * ui,
                    hud::col([0.8, 0.75, 0.95], 1.),
                );
            }
        }
    }
}

// ---- Play Online --------------------------------------------------------------------------------------------------
// The state machine (hub conversation, room order, retry schedule, name rules) is the engine's `hub::Online` and
// `spooky_kart::online`; this is only the window half: focus, input, and drawing in the game's own style.

/// The room a lobby belongs to, and how the join is going. `None` for a direct `--connect` / `server.txt` game.
struct RoomCtx {
    tag: RoomTag,
    addr: SocketAddr,
    /// The hub this room was found on (remembered once a join works, when it was chosen with `--hub`).
    hub: String,
    /// Waiting for a running race to end: the retry schedule.
    wait: Option<JoinWait>,
    remembered: bool,
}

/// The Play Online screen: the engine's `Online` plus which row or button has focus and the Create Room box.
struct OnlineUi {
    online: Online,
    /// Focus is on a room row (else on `button`).
    on_rows: bool,
    button: usize,
    /// The Create Room box: the name being typed, which of its four items has focus, the rivals chosen.
    field: TextField,
    dialog_focus: usize,
    rivals: Difficulty,
    rivals_touched: bool,
    /// A join that could not even start (shown above the list).
    notice: Option<String>,
    /// Automation for captures and tests (`--screen create`, `--create-room`, `--join-room`).
    auto_box: bool,
    auto_create: Option<String>,
    auto_join: Option<String>,
}

/// Where a frame of the Play Online screen sent the player.
enum OnlineOutcome {
    Stay,
    Back,
    Join(SocketAddr, RoomInfo),
}

const DIALOG_ITEMS: usize = 4;

impl OnlineUi {
    fn new(hub: &str, local_build: u32, now: f64) -> Self {
        OnlineUi {
            online: Online::new(hub, "spooky-kart", local_build, now),
            on_rows: true,
            button: 0,
            field: TextField::new(MAX_NAME_CHARS, CharFilter::Name),
            dialog_focus: 0,
            rivals: Difficulty::Medium,
            rivals_touched: false,
            notice: None,
            auto_box: false,
            auto_create: None,
            auto_join: None,
        }
    }

    /// The name box has the keyboard: typed letters must not reach anything else (not even the shell's F key).
    fn typing(&self) -> bool {
        self.online.dialog.as_ref().is_some_and(|d| d.phase == DialogPhase::Editing) && self.dialog_focus == 0
    }

    fn open_create(&mut self, player: &str) {
        self.online.open_dialog(player);
        if let Some(d) = self.online.dialog.as_mut() {
            d.name = online::default_room_name(player);
            self.field = TextField::with_text(MAX_NAME_CHARS, CharFilter::Name, &d.name);
            self.dialog_focus = 0;
            self.rivals = Difficulty::Medium;
            self.rivals_touched = false;
        }
    }

    /// Send the Create (rivals only when the player chose them: otherwise the hub's own default applies).
    fn create(&mut self) {
        let settings = [(spooky_kart::netgame::SETTING_DIFFICULTY, self.rivals.index() as u32)];
        self.online.set_create_settings(if self.rivals_touched { &settings } else { &[] });
        self.online.create();
    }

    fn join_wanted(&mut self) -> Option<OnlineOutcome> {
        let want = self.auto_join.clone()?;
        let room = self
            .online
            .rooms
            .iter()
            .find(|r| (want.eq_ignore_ascii_case("public") && r.public) || r.name.eq_ignore_ascii_case(&want))?
            .name
            .clone();
        self.auto_join = None;
        self.online.selected = Some(room);
        match self.online.join_selected() {
            Some(Action::Join { addr, room }) => Some(OnlineOutcome::Join(addr, room)),
            None => None,
        }
    }
}

/// One frame of keys for the Play Online screen: the keyboard, the D-pad and stick (as menu steps) and the controller
/// buttons, all as plain booleans, so a script can stand in for a person (`--online-script`).
#[derive(Default, Clone)]
struct OnlineKeys {
    /// Arrow keys or a D-pad / stick step.
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    /// Letter keys that also move in the list (but type into the name box).
    w: bool,
    s: bool,
    a: bool,
    d: bool,
    tab: bool,
    enter: bool,
    space: bool,
    backspace: bool,
    /// Controller A and B.
    select: bool,
    back: bool,
    /// A script's typing and erasing in the name box (a real keyboard goes through `ClientInput::feed_text`).
    typed: String,
    erase: usize,
}

impl OnlineKeys {
    fn read(input: &ClientInput, step: MenuStep) -> Self {
        OnlineKeys {
            up: input.pressed(KeyCode::Up) || step.up,
            down: input.pressed(KeyCode::Down) || step.down,
            left: input.pressed(KeyCode::Left) || step.left,
            right: input.pressed(KeyCode::Right) || step.right,
            w: input.pressed(KeyCode::W),
            s: input.pressed(KeyCode::S),
            a: input.pressed(KeyCode::A),
            d: input.pressed(KeyCode::D),
            tab: input.pressed(KeyCode::Tab),
            enter: input.pressed(KeyCode::Enter),
            space: input.pressed(KeyCode::Space),
            backspace: input.pressed(KeyCode::Backspace),
            select: input.menu_select(),
            back: input.menu_back(),
            ..Default::default()
        }
    }

    /// Add what a script says for this frame: names like `down`, `enter`, `tab`, `back`, `space`, `left`, `right`,
    /// `up`, `type=TEXT`, `erase=N`.
    fn script(&mut self, cues: &[&str]) {
        for cue in cues {
            match cue.split_once('=') {
                Some(("type", text)) => self.typed.push_str(&text.replace('_', " ")),
                Some(("erase", n)) => self.erase += n.parse::<usize>().unwrap_or(1),
                _ => match *cue {
                    "up" => self.up = true,
                    "down" => self.down = true,
                    "left" => self.left = true,
                    "right" => self.right = true,
                    "tab" => self.tab = true,
                    "enter" => self.enter = true,
                    "space" => self.space = true,
                    "backspace" => self.backspace = true,
                    "back" => self.back = true,
                    other => eprintln!("--online-script: unknown cue {other}"),
                },
            }
        }
    }
}

/// `--online-script "30:down,60:enter,90:type=Hi_there"`: each `FRAME:CUE` is pressed on that frame of the run.
fn online_script(args: &[String]) -> Vec<(u32, String)> {
    flag_value(args, "--online-script")
        .map(|text| {
            text.split(',')
                .filter_map(|c| c.split_once(':'))
                .filter_map(|(f, cue)| Some((f.trim().parse().ok()?, cue.trim().to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// One frame of Play Online: the hub's answers, then the keys (keyboard and controller alike).
fn online_input(
    ou: &mut OnlineUi,
    k: &OnlineKeys,
    input: &ClientInput,
    player: &str,
    now: f64,
    ui: f32,
    sounds: &mut SoundBank,
    live: bool,
) -> OnlineOutcome {
    if let Some(Action::Join { addr, room }) = ou.online.update(now) {
        return OnlineOutcome::Join(addr, room);
    }
    if ou.online.can_join() && ou.online.dialog.is_none() {
        if let Some(name) = ou.auto_create.take() {
            ou.open_create(player);
            if let Some(d) = ou.online.dialog.as_mut() {
                d.name = name;
            }
            ou.create();
        } else if std::mem::take(&mut ou.auto_box) {
            ou.open_create(player);
        } else if let Some(join) = ou.join_wanted() {
            return join;
        }
    }
    if !live {
        return OnlineOutcome::Stay;
    }
    let blip = |sounds: &mut SoundBank| sounds.play(sound(synth::Preset::Blip), 0.5);
    let accept_key = k.enter || k.select;
    if let Some(phase) = ou.online.dialog.as_ref().map(|d| d.phase.clone()) {
        match phase {
            DialogPhase::Creating => {
                if accept_key || k.backspace || k.back {
                    sounds.play(sound(synth::Preset::Back), 0.6);
                    ou.online.close_dialog();
                }
            }
            DialogPhase::Editing => {
                let before = ou.dialog_focus;
                let down = k.down || k.tab;
                let up = k.up;
                ou.dialog_focus = online::step_focus(ou.dialog_focus, DIALOG_ITEMS, up, down);
                if ou.dialog_focus != before {
                    blip(sounds);
                }
                if ou.dialog_focus == 0 {
                    input.feed_text(&mut ou.field);
                    for _ in 0..k.erase {
                        ou.field.backspace();
                    }
                    ou.field.insert_str(&k.typed);
                    if let Some(d) = ou.online.dialog.as_mut() {
                        if d.name != ou.field.text {
                            d.name = ou.field.text.clone();
                            d.error = None;
                        }
                    }
                } else if ou.dialog_focus == 1 {
                    let left = k.left || k.a;
                    let right = k.right || k.d;
                    let next = ou.rivals.step(i32::from(right) - i32::from(left));
                    if next != ou.rivals {
                        ou.rivals = next;
                        ou.rivals_touched = true;
                        blip(sounds);
                    }
                }
                let accept = accept_key || (ou.dialog_focus != 0 && k.space);
                if k.back || (accept && ou.dialog_focus == 3) {
                    sounds.play(sound(synth::Preset::Back), 0.6);
                    ou.online.close_dialog();
                } else if accept && matches!(ou.dialog_focus, 0 | 2) {
                    sounds.play(sound(synth::Preset::Select), 0.8);
                    ou.create();
                }
            }
        }
        return OnlineOutcome::Stay;
    }
    // The list.
    let view = ou.online.view.clone();
    let mismatch = ou.online.mismatch();
    let rows = if view == Pane::Rooms { ou.online.rooms.len() } else { 0 };
    let buttons = online::buttons(&view, mismatch, rows);
    let selectable = ou.online.can_join() && rows > 0;
    ou.button = ou.button.min(buttons.len().saturating_sub(1));
    let (up, down, left, right) = (k.up || k.w, k.down || k.s, k.left || k.a, k.right || k.d);
    if ou.on_rows && selectable {
        let current = ou.online.selected.as_ref().and_then(|n| ou.online.rooms.iter().position(|r| &r.name == n));
        let at = current.unwrap_or(0);
        if down && at + 1 == rows {
            ou.on_rows = false;
            ou.button = 0;
            blip(sounds);
        } else {
            let to = online::step_focus(at, rows, up, down);
            if to != at || current.is_none() {
                ou.online.selected = ou.online.rooms.get(to).map(|r| r.name.clone());
                if to != at {
                    blip(sounds);
                }
            }
            ou.online.offset = scroll_to(to, ou.online.offset, online_rows(ui), rows);
        }
    } else if up && selectable {
        ou.on_rows = true;
        blip(sounds);
    } else {
        let to = online::step_focus(ou.button, buttons.len(), left, right);
        if to != ou.button {
            ou.button = to;
            blip(sounds);
        }
    }
    if k.backspace || k.back {
        sounds.play(sound(synth::Preset::Back), 0.6);
        return OnlineOutcome::Back;
    }
    if accept_key || k.space {
        let action = if ou.on_rows && selectable {
            ou.online.join_selected()
        } else {
            match buttons.get(ou.button) {
                Some(Btn::Join) => ou.online.join_selected(),
                Some(Btn::Create) => {
                    ou.open_create(player);
                    None
                }
                Some(Btn::Refresh | Btn::Retry) => {
                    ou.notice = None;
                    ou.online.refresh(now);
                    None
                }
                Some(Btn::Back) => {
                    sounds.play(sound(synth::Preset::Back), 0.6);
                    return OnlineOutcome::Back;
                }
                None => None,
            }
        };
        sounds.play(sound(synth::Preset::Select), 0.8);
        if let Some(Action::Join { addr, room }) = action {
            return OnlineOutcome::Join(addr, room);
        }
    }
    OnlineOutcome::Stay
}

/// How many room rows fit between the heading and the buttons.
fn online_rows(ui: f32) -> usize {
    (((screen_height() - 335. * ui) / (40. * ui)).floor() as usize).clamp(3, 8)
}

fn chip(x: f32, y: f32, w: f32, h: f32, ui: f32, label: &str, on: bool, tint: [f32; 3]) {
    hud::panel(
        x,
        y,
        w,
        h,
        8. * ui,
        if on { Color::new(0.35, 0.15, 0.05, 0.92) } else { Color::new(0.05, 0.02, 0.12, 0.82) },
    );
    if on {
        draw_rectangle(x + 10. * ui, y + h - 7. * ui, w - 20. * ui, 3. * ui, hud::col(tint, 1.));
    }
    hud::text_centered(
        label,
        x + w * 0.5,
        y + h * 0.5 + 6. * ui,
        18. * ui,
        if on { WHITE } else { hud::col([0.7, 0.65, 0.85], 1.) },
    );
}

const WARN: [f32; 3] = [1., 0.55, 0.45];
const DIM: [f32; 3] = [0.7, 0.65, 0.85];

fn draw_online(ou: &OnlineUi, time: f32, ui: f32, local_build: u32) {
    let (w, h) = (screen_width(), screen_height());
    let online = &ou.online;
    hud::text_centered("SPOOKY KART", w * 0.5, 60. * ui, 54. * ui, hud::col([1., 0.55, 0.1], 1.));
    hud::text_centered("Play Online", w * 0.5, 96. * ui, 20. * ui, hud::col([0.8, 0.7, 1.], 1.));
    let (pw, py) = (700. * ui, 118. * ui);
    let px = (w - pw) * 0.5;
    let mismatch = online.mismatch();
    let rows = if online.view == Pane::Rooms { online.rooms.len() } else { 0 };
    // The panel fits its rooms (or the message): the buttons sit right under it.
    let visible = online_rows(ui);
    let top = if mismatch || ou.notice.is_some() { 212. } else { 160. };
    let panel_h = if rows > 0 {
        top + rows.min(visible) as f32 * 40. + if rows > visible { 24. } else { 4. } - 118.
    } else {
        200.
    } * ui;
    hud::panel(px, py, pw, panel_h, 14. * ui, Color::new(0.05, 0.02, 0.12, 0.93));
    let rooms_selectable = online.can_join();
    let on_rows = ou.on_rows && rooms_selectable && rows > 0;
    let say = |text: &str, y: f32, size: f32, tint: [f32; 3]| {
        for (i, line) in hud::wrap(text, pw - 60. * ui, size * ui).iter().enumerate() {
            hud::text_centered(line, w * 0.5, (y + i as f32 * size * 1.35) * ui, size * ui, hud::col(tint, 1.));
        }
    };
    // What stands in the way, above or instead of the list.
    match &online.view {
        Pane::Looking => {
            let dots = ".".repeat(1 + (time * 2.) as usize % 3);
            say(&format!("Looking for races{dots}"), 205., 26., [1., 0.9, 0.6]);
        }
        Pane::Offline => {
            say(
                "Can't reach the Spooky Kart servers right now. Check your internet connection, then Retry.",
                176.,
                24.,
                WARN,
            );
            let detail = online.detail.clone().unwrap_or_else(|| format!("({})", online.hub_spec()));
            say(&detail, 252., 17., DIM);
        }
        Pane::Problem(text) => say(text, 186., 24., WARN),
        Pane::Rooms => {
            if mismatch {
                say(&hub_update_message(), 148., 20., WARN);
            } else if let Some(n) = &ou.notice {
                say(n, 148., 20., WARN);
            }
            if rows == 0 {
                say("No races are open right now. Create a room and invite your friends.", 200., 22., [1., 0.9, 0.6]);
            }
        }
    }
    // The rooms.
    if rows > 0 {
        let offset = ou.online.offset.min(rows.saturating_sub(visible));
        hud::text_outlined("ROOM", px + 34. * ui, (top - 6.) * ui, 14. * ui, hud::col(DIM, 1.));
        hud::text_right("RACERS", px + pw - 34. * ui, (top - 6.) * ui, 14. * ui, hud::col(DIM, 1.));
        let selected = online.selected_room().map(|r| r.name.clone());
        let can = online.can_join();
        for (i, r) in online.rooms.iter().enumerate().skip(offset).take(visible) {
            let y = (top + (i - offset) as f32 * 40.) * ui;
            let on = can && selected.as_deref() == Some(r.name.as_str());
            let strong = on && on_rows;
            let fill = if strong {
                Color::new(0.35, 0.15, 0.05, 0.92)
            } else if on {
                Color::new(0.2, 0.09, 0.1, 0.85)
            } else {
                Color::new(0.09, 0.05, 0.18, 0.85)
            };
            hud::panel(px + 20. * ui, y, pw - 40. * ui, 34. * ui, 8. * ui, fill);
            if strong {
                draw_rectangle(px + 34. * ui, y + 28. * ui, pw - 68. * ui, 3. * ui, hud::col([1., 0.6, 0.15], 1.));
            }
            let dim = if can { 1. } else { 0.55 };
            let name_tint = if r.public { [1., 0.7, 0.2] } else { [1., 1., 1.] };
            hud::text_outlined(&r.name, px + 34. * ui, y + 24. * ui, 20. * ui, hud::col(name_tint, dim));
            let status = online::room_label(r);
            let tint = if status.ends_with("Full") {
                [1., 0.45, 0.4]
            } else if status.ends_with("In race") {
                [1., 0.8, 0.3]
            } else {
                [0.45, 1., 0.55]
            };
            hud::text_right(&status, px + pw - 34. * ui, y + 24. * ui, 18. * ui, hud::col(tint, dim));
        }
        if rows > visible {
            let below = rows - offset - visible.min(rows - offset);
            hud::text_centered(
                &format!("{offset} more above, {below} more below"),
                w * 0.5,
                (top + visible as f32 * 40. + 14.) * ui,
                14. * ui,
                hud::col(DIM, 1.),
            );
        }
    }
    // The buttons.
    let buttons = online::buttons(&online.view, mismatch, rows);
    let (bw, bh, gap) = (160. * ui, 40. * ui, 12. * ui);
    let total = buttons.len() as f32 * bw + (buttons.len() as f32 - 1.) * gap;
    for (i, b) in buttons.iter().enumerate() {
        let x = w * 0.5 - total * 0.5 + i as f32 * (bw + gap);
        let on = !on_rows && ou.button.min(buttons.len() - 1) == i && online.dialog.is_none();
        chip(x, py + panel_h + 22. * ui, bw, bh, ui, b.label(), on, [1., 0.6, 0.15]);
    }
    let blink = 0.6 + 0.4 * (time * 3.).sin();
    let hint = if online.dialog.is_some() {
        "Type a name     Up / Down or stick: move     Left / Right: rivals     Enter or A: select"
    } else if rows > 0 && rooms_selectable {
        "Up / Down or stick: rooms     Left / Right: buttons     Enter or A: select     Backspace or B: back"
    } else {
        "Left / Right: buttons     Enter or A: select     Backspace or B: back"
    };
    hud::text_centered(hint, w * 0.5, h - 20. * ui, 17. * ui, Color::new(1., 1., 1., blink));
    hud::text_outlined(&hub::client::build_label(local_build), 16. * ui, h - 14. * ui, 13. * ui, hud::col(DIM, 0.8));
    hud::text_right(online.hub_spec(), w - 16. * ui, h - 14. * ui, 13. * ui, hud::col(DIM, 0.8));
    if online.dialog.is_some() {
        draw_create_dialog(ou, time, ui);
    }
}

fn hub_update_message() -> String {
    hub::client::update_message(online::TITLE)
}

/// The Create Room box over the dimmed list.
fn draw_create_dialog(ou: &OnlineUi, time: f32, ui: f32) {
    let (w, h) = (screen_width(), screen_height());
    let Some(d) = &ou.online.dialog else { return };
    draw_rectangle(0., 0., w, h, Color::new(0.02, 0., 0.06, 0.62));
    let (pw, ph) = (640. * ui, 350. * ui);
    let (px, py) = ((w - pw) * 0.5, (h - ph) * 0.5 - 20. * ui);
    hud::panel(px, py, pw, ph, 14. * ui, Color::new(0.06, 0.025, 0.14, 0.97));
    hud::text_centered("CREATE A ROOM", w * 0.5, py + 46. * ui, 32. * ui, hud::col([1., 0.55, 0.1], 1.));
    for (i, line) in
        hud::wrap("Friends will see this name under Play Online and join with one click.", pw - 60. * ui, 17. * ui)
            .iter()
            .enumerate()
    {
        hud::text_centered(line, w * 0.5, py + 76. * ui + i as f32 * 22. * ui, 17. * ui, hud::col(DIM, 1.));
    }
    if d.phase == DialogPhase::Creating {
        let dots = ".".repeat(1 + (time * 2.) as usize % 3);
        hud::text_centered(
            &format!("Creating your room{dots}"),
            w * 0.5,
            py + 170. * ui,
            28. * ui,
            hud::col([1., 0.9, 0.6], 1.),
        );
        chip(w * 0.5 - 80. * ui, py + 270. * ui, 160. * ui, 40. * ui, ui, "CANCEL", true, [1., 0.6, 0.15]);
        return;
    }
    // The name box.
    let (fx, fy, fw, fh) = (px + 40. * ui, py + 124. * ui, pw - 80. * ui, 44. * ui);
    let focus = ou.dialog_focus;
    hud::text_outlined("Room name", fx, fy - 8. * ui, 15. * ui, hud::col(DIM, 1.));
    hud::panel(
        fx,
        fy,
        fw,
        fh,
        6. * ui,
        if focus == 0 { Color::new(0.2, 0.09, 0.05, 0.95) } else { Color::new(0.04, 0.02, 0.1, 0.9) },
    );
    if focus == 0 {
        draw_rectangle(fx + 8. * ui, fy + fh - 5. * ui, fw - 16. * ui, 3. * ui, hud::col([1., 0.6, 0.15], 1.));
    }
    let size = 24. * ui;
    hud::text_outlined(&ou.field.text, fx + 14. * ui, fy + 31. * ui, size, WHITE);
    if focus == 0 && (time * 2.).fract() < 0.6 {
        let before = &ou.field.text[..ou.field.caret_byte()];
        let cx = fx + 14. * ui + hud::width_of(before, size);
        draw_rectangle(cx + 1. * ui, fy + 9. * ui, 2. * ui, 28. * ui, hud::col([1., 0.85, 0.4], 1.));
    }
    // The rivals.
    let ry = py + 196. * ui;
    hud::text_outlined(
        "Rivals",
        fx,
        ry + 27. * ui,
        18. * ui,
        if focus == 1 { hud::col([1., 0.85, 0.4], 1.) } else { hud::col(DIM, 1.) },
    );
    let (cw, gap) = (110. * ui, 10. * ui);
    for (i, dif) in Difficulty::ALL.iter().enumerate() {
        let x = px + pw - 40. * ui - 3. * cw - 2. * gap + i as f32 * (cw + gap);
        let tint = match dif {
            Difficulty::Easy => [0.4, 0.9, 0.5],
            Difficulty::Medium => [1., 0.8, 0.3],
            Difficulty::Hard => [1., 0.4, 0.35],
        };
        chip(x, ry, cw, 36. * ui, ui, &dif.name().to_uppercase(), *dif == ou.rivals, tint);
        if focus == 1 && *dif == ou.rivals {
            draw_rectangle(x + 4. * ui, ry - 5. * ui, cw - 8. * ui, 2. * ui, hud::col([1., 0.85, 0.4], 1.));
        }
    }
    // Create and Cancel.
    let by = py + 256. * ui;
    chip(w * 0.5 - 170. * ui, by, 160. * ui, 40. * ui, ui, "CREATE", focus == 2, [0.4, 0.9, 0.5]);
    chip(w * 0.5 + 10. * ui, by, 160. * ui, 40. * ui, ui, "CANCEL", focus == 3, [1., 0.6, 0.15]);
    if let Some(e) = &d.error {
        for (i, line) in hud::wrap(e, pw - 60. * ui, 17. * ui).iter().enumerate() {
            hud::text_centered(line, w * 0.5, py + 322. * ui + i as f32 * 21. * ui, 17. * ui, hud::col(WARN, 1.));
        }
    }
}

/// The Play Online chip on the select screen.
fn draw_online_chip(ui: f32, time: f32) {
    let w = screen_width();
    let (cw, ch) = (250. * ui, 40. * ui);
    let pulse = 0.85 + 0.15 * (time * 2.5).sin();
    hud::panel(w - cw - 30. * ui, 28. * ui, cw, ch, 8. * ui, Color::new(0.3, 0.12, 0.06, 0.9 * pulse));
    hud::text_centered("P or Y: PLAY ONLINE", w - cw * 0.5 - 30. * ui, 28. * ui + 27. * ui, 18. * ui, WHITE);
}

/// Dust behind sliding karts, sparks behind boosting ones.
fn trails(sim: &Sim, fx: &mut Fx) {
    for k in &sim.karts {
        let tail = v3(k.pos) - v3(forward(k.yaw)) * 1.4 + vec3(0., 0.3, 0.);
        if k.drifting {
            fx.dust(tail, 1, 0.6, [0.55, 0.5, 0.6]);
        }
        if k.boost_ticks > 0 {
            fx.sparks(tail, 1, 4., [1., 0.6, 0.1]);
        }
    }
}

/// Which way the chase camera looks: the kart's heading, eased toward where it is really going.
fn chase_target(k: &Kart) -> f32 {
    let speed = k.vel.length();
    if speed > 4. {
        let blend = ((speed - 4.) / 12.).clamp(0., 0.5);
        k.yaw + wrap_angle(yaw_of(k.vel) - k.yaw) * blend
    } else {
        k.yaw
    }
}

/// The name this player races under: `--name`, else the account name, else "Racer".
fn player_name(args: &[String]) -> String {
    flag_value(args, "--name")
        .map(|n| n.to_string())
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("USERNAME").ok())
        .unwrap_or_else(|| "Racer".into())
}

/// A client for the server at `address` (a direct connection, or a hub's room over raw UDP).
fn make_client(
    address: SocketAddr,
    profile: TransportProfile,
    name: String,
    key: String,
    choice: usize,
) -> Result<Client, String> {
    client_transport(profile, address)
        .and_then(|t| Client::new(t, address, ClientConfig { name, key, choice: choice as u8 }))
        .map_err(|e| e.to_string())
}

/// Open Play Online: pick the hub (`--hub`, `hub.txt`, the last one used, the public one) and start asking it.
fn open_online(args: &[String], settings: &Settings, local_build: u32, now: f64) -> (OnlineUi, ServerOrigin, String) {
    let dir =
        std::env::current_exe().ok().and_then(|p| p.parent().map(std::path::Path::to_path_buf)).unwrap_or_default();
    let hub = online::hub_choice(&dir, flag_value(args, "--hub"), settings.last_server.as_deref());
    (OnlineUi::new(&hub.address, local_build, now), hub.origin, hub.address)
}

/// Where to play online, if anywhere: `--connect` on the command line, else a `server.txt` next to the program
/// (line 1 `host:port`, optional line 2 join key, optional line 3 `development` or `production`, default
/// production; `#` starts a comment). `--offline` skips it. Returns (address, join key, transport).
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

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identity = identity();
    // The run flags, the fixed-step loop, quick save/load and evidence for a caller that cannot watch.
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let seed = life.seed();
    let unattended = life.options.unattended();
    let direct = flag_value(&args, "--character").and_then(character_from);
    let mut difficulty = match flag_value(&args, "--difficulty").map(|d| d.parse::<Difficulty>()) {
        Some(Ok(d)) => d,
        Some(Err(e)) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
        None => Difficulty::default(),
    };
    let autopilot = has_flag(&args, "--autopilot");
    let debug_input = has_flag(&args, "--debug-input");
    let mut last_debug = u32::MAX;

    let materials = Materials::load().expect("the materials failed to compile");
    let look = halloween_look();
    let (mut sim, mut me) = new_race(seed, direct.unwrap_or(Character::Vampire), difficulty);
    let scene = build_scene(&sim);
    // Audio settings survive a relaunch next to the exe; `music_wav` is filled by the render thread.
    let settings_path = beside_exe("settings.json");
    let mut settings = Settings::load(&settings_path);
    let music_wav: Arc<OnceLock<Vec<u8>>> = Arc::new(OnceLock::new());
    let mut sounds = SoundBank::start(life.options.silent(), settings.sfx_level(), settings.music_level(), {
        let music_wav = music_wav.clone();
        move || render_audio(music_wav)
    })
    .await;
    // Shadows: Off, Simple (soft contact shadows, the default) or Full (a real shadow map). `--shadows` picks the
    // tier for this run only; Esc > Settings changes it and remembers it in settings.json.
    let quality = match ShadowQuality::from_flag(&args) {
        Ok(flag) => flag.unwrap_or(settings.shadow_quality),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let mut shadows = Shadows::new(quality).with_range(SHADOW_HALF, SHADOW_DEPTH).with_resolution(SHADOW_RESOLUTION);
    // The ground is flat at 0 everywhere but on the select screen's podium.
    shadows.set_ground(|x, z| if (x - PODIUM.x).hypot(z - PODIUM.z) < PODIUM_RADIUS { PODIUM_TOP } else { 0. });
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let (mut fx, mut juice) = (Fx::new(seed), Juice::default());
    let mut choice = direct.map_or(0, |c| c.index());
    let auto_ready = has_flag(&args, "--auto-ready");
    let mut client: Option<Client> = None;
    if let Some((target, key, profile)) = server_settings(&args) {
        let address = target.to_socket_addrs().ok().and_then(|mut a| a.next()).unwrap_or_else(|| {
            eprintln!("Cannot find a server at {target}");
            std::process::exit(2);
        });
        match make_client(address, profile, player_name(&args), key, choice) {
            Ok(c) => client = Some(c),
            Err(e) => eprintln!("Cannot start the network ({e}); playing offline"),
        }
    }
    // Play Online: the hub's rooms. `--screen online|create` opens it at once (for captures and tests).
    let fake_build = std::env::var(online::FAKE_BUILD_ENV).ok();
    let local_build = online::local_build(fake_build.as_deref());
    let player = player_name(&args);
    let script = online_script(&args);
    let mut frame_no = 0u32;
    let mut online_ui: Option<OnlineUi> = None;
    let mut hub_origin = ServerOrigin::Builtin;
    let mut hub_spec = String::new();
    let mut room_ctx: Option<RoomCtx> = None;
    let start_online = client.is_none() && matches!(flag_value(&args, "--screen"), Some("online" | "create"));
    if start_online {
        let (mut ou, origin, spec) = open_online(&args, &settings, local_build, 0.);
        ou.auto_box = flag_value(&args, "--screen") == Some("create");
        ou.auto_create = flag_value(&args, "--create-room").map(str::to_string);
        ou.auto_join = flag_value(&args, "--join-room").map(str::to_string);
        (online_ui, hub_origin, hub_spec) = (Some(ou), origin, spec);
    }
    let mut screen = if client.is_some() {
        Screen::Lobby
    } else if start_online {
        Screen::Online
    } else if has_flag(&args, "--select") || (direct.is_none() && !unattended) {
        Screen::Select
    } else {
        Screen::Race
    };
    let mut camera_yaw = sim.karts[me].yaw;
    let mut camera_reset = false;
    let mut choice_changed = f32::NEG_INFINITY;
    if client.is_none() {
        life.load_flag_or_exit(&mut sim);
        difficulty = sim.difficulty;
    }
    let vignette = hud::make_vignette();
    let (mut world, mut alpha, mut add) = (Batch::new(), Batch::new(), Batch::new());

    loop {
        frame_no += 1;
        let cues_now: Vec<&str> = script.iter().filter(|(f, _)| *f == frame_no).map(|(_, c)| c.as_str()).collect();
        // The shell turns F into fullscreen; while a name is being typed that is just a letter. The engine has no way to
        // mask it, so undo the toggle (a frame in which the box did not have the keyboard is unaffected).
        let typing = matches!(screen, Screen::Online) && online_ui.as_ref().is_some_and(OnlineUi::typing);
        let was_fullscreen = shell.fullscreen;
        input.begin_frame_with_keyboard(&mut shell, false, unattended || platform::focused(), platform::keyboard());
        if typing && shell.fullscreen != was_fullscreen {
            shell.fullscreen = was_fullscreen;
            set_fullscreen(was_fullscreen);
        }
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        // The layers follow the race on screen (the network client's predicted view when online).
        let race_view = match (&screen, &client) {
            (Screen::Race, Some(c)) => c.participant().map(|m| (c.view().sim(), m)),
            (Screen::Race, None) => Some((&sim, me)),
            _ => None,
        };
        sounds.update_music(dt, &music::layers(race_view));
        let ui = hud::ui_scale();
        let pad = input.gamepad().clone();
        let menu_step = input.menu_step();

        // The network first: read it, follow its state into the right screen, react to its events.
        let mut my_ready = false;
        if let Some(c) = client.as_mut() {
            c.poll(time as f64);
            c.frame(time as f64, dt);
            let me_net = c.participant().unwrap_or(0);
            for event in c.drain_events() {
                if c.view().sim().karts.len() > me_net {
                    react(&event, c.view().sim(), me_net, &mut sounds, &mut fx, &mut juice);
                }
            }
            match (c.state().clone(), &screen) {
                (ClientState::Playing, Screen::Lobby) => {
                    screen = Screen::Race;
                    fx.clear();
                    juice = Juice::default();
                    life.reset_input();
                    camera_reset = true;
                }
                (ClientState::Lobby, Screen::Race) => {
                    screen = Screen::Lobby;
                    fx.clear();
                }
                _ => {}
            }
            if let Some(ctx) = room_ctx.as_mut().filter(|r| !r.remembered && *c.state() == ClientState::Lobby) {
                // A hub typed with --hub is remembered once a room on it let us in (not for unattended runs).
                ctx.remembered = true;
                ctx.wait = None;
                if hub_origin == ServerOrigin::CliArg
                    && !unattended
                    && settings.last_server.as_deref() != Some(&ctx.hub)
                {
                    settings.last_server = Some(ctx.hub.clone());
                    settings.store(&settings_path);
                }
            }
            let mine = c.lobby().and_then(|l| l.entries.iter().find(|e| e.slot == c.seat()).cloned());
            my_ready = mine.as_ref().is_some_and(|e| e.ready);
            if let Some(e) = &mine {
                // Follow the server's answer (it may have given us another driver) unless we just chose.
                if time - choice_changed > 1.0 {
                    choice = e.choice as usize % MAX_RACERS;
                }
            }
            if auto_ready && *c.state() == ClientState::Lobby && mine.is_some() && !my_ready {
                c.ready(true);
            }
        }

        match screen {
            Screen::Lobby => {
                let offline_key = input.pressed(KeyCode::O);
                let failed = client
                    .as_ref()
                    .is_some_and(|c| matches!(c.state(), ClientState::Rejected(_) | ClientState::Disconnected(_)));
                let accept = input.pressed(KeyCode::Enter) || input.menu_select();
                if let Some(ctx) = room_ctx.as_mut() {
                    // A room from Play Online. "A race is running" is waited out: ask again every few seconds.
                    let now = time as f64;
                    let mut reconnect = failed && accept && ctx.wait.is_none();
                    if let Some(c) = client.as_ref().filter(|_| failed) {
                        if c.failure() == Some(ConnectFailure::MatchInProgress) {
                            match ctx.wait.as_mut() {
                                None => ctx.wait = Some(JoinWait::new(now)),
                                Some(w) => match w.step(now) {
                                    RetryStep::TryNow => {
                                        w.attempted(now);
                                        reconnect = true;
                                    }
                                    RetryStep::GiveUp => ctx.wait = None,
                                    RetryStep::Wait => {}
                                },
                            }
                        } else {
                            ctx.wait = None;
                        }
                    }
                    if reconnect {
                        if let Ok(c) =
                            make_client(ctx.addr, TransportProfile::Development, player.clone(), String::new(), choice)
                        {
                            client = Some(c);
                        }
                    }
                    if !offline_key
                        && (input.pressed(KeyCode::Backspace) || input.menu_back() || cues_now.contains(&"back"))
                    {
                        if let Some(c) = client.as_mut() {
                            c.leave();
                        }
                        client = None;
                        room_ctx = None;
                        let (mut ou, origin, spec) = open_online(&args, &settings, local_build, time as f64);
                        ou.on_rows = true;
                        (online_ui, hub_origin, hub_spec) = (Some(ou), origin, spec);
                        screen = Screen::Online;
                        sounds.play(sound(synth::Preset::Back), 0.6);
                    }
                }
                let leaves_room_on_accept = room_ctx.is_none();
                if matches!(screen, Screen::Lobby) && (offline_key || (failed && accept && leaves_room_on_accept)) {
                    room_ctx = None;
                    // Practice offline: leave the server (politely, if we are on it) and use the local game.
                    if let Some(c) = client.as_mut() {
                        c.leave();
                    }
                    client = None;
                    screen = Screen::Select;
                    sounds.play(sound(synth::Preset::Back), 0.6);
                }
                if let Some(c) = client.as_mut().filter(|c| *c.state() == ClientState::Lobby) {
                    let left = input.pressed(KeyCode::A) || input.pressed(KeyCode::Left) || menu_step.left;
                    let right = input.pressed(KeyCode::D) || input.pressed(KeyCode::Right) || menu_step.right;
                    if left || right {
                        choice = if left { (choice + MAX_RACERS - 1) % MAX_RACERS } else { (choice + 1) % MAX_RACERS };
                        choice_changed = time;
                        c.select(choice as u8);
                        sounds.play(sound(synth::Preset::Blip), 0.5);
                    }
                    if input.pressed(KeyCode::Enter) || input.pressed(KeyCode::Space) || input.menu_select() {
                        c.ready(!my_ready);
                        sounds.play(sound(synth::Preset::Select), 0.8);
                    }
                }
            }
            Screen::Select => {
                let left = input.pressed(KeyCode::A) || input.pressed(KeyCode::Left) || menu_step.left;
                let right = input.pressed(KeyCode::D) || input.pressed(KeyCode::Right) || menu_step.right;
                if left {
                    choice = (choice + MAX_RACERS - 1) % MAX_RACERS;
                    sounds.play(sound(synth::Preset::Blip), 0.5);
                }
                if right {
                    choice = (choice + 1) % MAX_RACERS;
                    sounds.play(sound(synth::Preset::Blip), 0.5);
                }
                // Up / Down (W / S, or the stick) set how hard the rivals drive; not while the pause menu has the keys.
                if !shell.paused {
                    let harder = input.pressed(KeyCode::S) || input.pressed(KeyCode::Down) || menu_step.down;
                    let easier = input.pressed(KeyCode::W) || input.pressed(KeyCode::Up) || menu_step.up;
                    let next = difficulty.step(i32::from(harder) - i32::from(easier));
                    if next != difficulty {
                        difficulty = next;
                        sounds.play(sound(synth::Preset::Blip), 0.5);
                    }
                }
                if !shell.paused && (input.pressed(KeyCode::P) || pad.pressed(Button::North)) {
                    let (ou, origin, spec) = open_online(&args, &settings, local_build, time as f64);
                    (online_ui, hub_origin, hub_spec) = (Some(ou), origin, spec);
                    screen = Screen::Online;
                    sounds.play(sound(synth::Preset::Select), 0.8);
                } else if input.pressed(KeyCode::Enter) || input.pressed(KeyCode::Space) || input.menu_select() {
                    sounds.play(sound(synth::Preset::Select), 0.8);
                    let (s, slot) = new_race(life.restart_seed(), Character::from_index(choice), difficulty);
                    sim = s;
                    me = slot;
                    fx.clear();
                    juice = Juice::default();
                    life.reset_input();
                    camera_yaw = sim.karts[me].yaw;
                    screen = Screen::Race;
                }
            }
            Screen::Online => {
                let outcome = match online_ui.as_mut() {
                    Some(ou) => {
                        let mut keys = OnlineKeys::read(&input, menu_step);
                        keys.script(&cues_now);
                        online_input(ou, &keys, &input, &player, time as f64, ui, &mut sounds, !shell.paused)
                    }
                    None => OnlineOutcome::Back,
                };
                match outcome {
                    OnlineOutcome::Stay => {}
                    OnlineOutcome::Back => {
                        online_ui = None;
                        screen = Screen::Select;
                    }
                    OnlineOutcome::Join(addr, room) => {
                        // The hub only found the room: the existing online client takes over, over raw UDP (a hub's
                        // rooms are the Development transport, with no join key).
                        match make_client(addr, TransportProfile::Development, player.clone(), String::new(), choice) {
                            Ok(c) => {
                                client = Some(c);
                                room_ctx = Some(RoomCtx {
                                    tag: RoomTag { name: room.name, public: room.public },
                                    addr,
                                    hub: hub_spec.clone(),
                                    wait: None,
                                    remembered: false,
                                });
                                online_ui = None;
                                screen = Screen::Lobby;
                                choice_changed = f32::NEG_INFINITY;
                            }
                            Err(e) => {
                                if let Some(ou) = online_ui.as_mut() {
                                    ou.notice = Some(format!("Could not start the network: {e}"));
                                }
                            }
                        }
                    }
                }
            }
            Screen::Race => {
                // 1. Devices in: one frame of held state and the perk press (from the script when there is one).
                let live = controls::accepts_input(shell.paused, unattended || platform::focused());
                if debug_input && (life.time() as u32) != last_debug {
                    last_debug = life.time() as u32;
                    eprintln!(
                        "[input] paused={} (shell.playing()={} needs a captured mouse: not used) focused={} live={}",
                        shell.paused,
                        shell.playing(),
                        unattended || platform::focused(),
                        live
                    );
                }
                let (held, perk) = match life.script() {
                    Some(s) => (
                        Held {
                            throttle: s.axis("fwd", "back"),
                            steer: s.axis("right", "left"),
                            drift: s.held("drift"),
                        },
                        s.starts("perk"),
                    ),
                    None if live => {
                        let key = |a: KeyCode, b: KeyCode| f32::from(input.down(a) || input.down(b));
                        let raw = Raw {
                            key_throttle: key(KeyCode::W, KeyCode::Up) - key(KeyCode::S, KeyCode::Down),
                            key_steer: key(KeyCode::D, KeyCode::Right) - key(KeyCode::A, KeyCode::Left),
                            key_drift: input.down(KeyCode::LeftShift) || input.down(KeyCode::RightShift),
                            pad_steer: pad.left_stick[0],
                            pad_gas: pad.triggers[1],
                            pad_brake: pad.triggers[0],
                            pad_gas_button: pad.down(Button::South),
                            pad_drift: pad.down(Button::LeftTrigger) || pad.down(Button::RightTrigger),
                        };
                        let k = controls::resolve(&raw);
                        (
                            Held { throttle: k.throttle, steer: k.steer, drift: k.drift },
                            input.pressed(KeyCode::Space) || pad.pressed(Button::West),
                        )
                    }
                    None => (Held::default(), false),
                };
                life.feed(held, if perk { PERK } else { 0 }, [0., 0.]);
                if client.is_none() {
                    let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
                    if save {
                        let notice = if sim.is_over() {
                            Notice::refused("the race is over")
                        } else {
                            life.quick_save(&sim, &format!("Quick save, lap {}", sim.karts[me].lap + 1))
                        };
                        announce(&mut fx, &notice);
                    }
                    if load {
                        let notice = life.quick_load(&mut sim);
                        announce(&mut fx, &notice);
                        if notice.ok {
                            difficulty = sim.difficulty;
                            juice = Juice::default();
                        }
                    }
                    if sim.is_over() && input.pressed(KeyCode::R) {
                        let human = sim.karts[me].character;
                        let (s, slot) = new_race(life.restart_seed(), human, difficulty);
                        sim = s;
                        me = slot;
                        fx.clear();
                        life.reset_input();
                        camera_yaw = sim.karts[me].yaw;
                    } else if sim.is_over() && (input.pressed(KeyCode::Enter) || input.menu_select()) && !unattended {
                        screen = Screen::Select;
                        sounds.play(sound(synth::Preset::Select), 0.8);
                    }
                }
                // 2. Simulation: whole fixed ticks, each with exactly one Inputs.
                let playing = !shell.paused;
                if let Some(c) = client.as_mut() {
                    // Online: each tick is predicted and sent; the server decides the race.
                    for _ in 0..life.ticks(dt, 1., playing) {
                        let tick = life.take_tick();
                        let mine = c.participant().filter(|m| c.view().sim().karts.len() > *m);
                        let kart_input = match (autopilot, mine) {
                            (true, Some(m)) => spooky_kart::bot::drive(c.view().sim(), m),
                            (true, None) => KartInput::default(),
                            (false, _) => KartInput {
                                throttle: tick.held.throttle,
                                steer: tick.held.steer,
                                drift: tick.held.drift,
                                perk: tick.pressed(PERK),
                            },
                        };
                        c.tick(kart_input);
                    }
                    if playing {
                        trails(c.view().sim(), &mut fx);
                        juice.update(dt);
                        fx.update(dt);
                    }
                } else {
                    for _ in 0..life.ticks(dt, juice.time_scale(None), playing) {
                        let tick = life.take_tick();
                        let mut inputs = Inputs::default();
                        inputs.0[me] = if autopilot {
                            // A steady Medium-level driver whatever the rivals are set to.
                            spooky_kart::bot::drive_as(&sim, me, Difficulty::Medium)
                        } else {
                            KartInput {
                                throttle: tick.held.throttle,
                                steer: tick.held.steer,
                                drift: tick.held.drift,
                                perk: tick.pressed(PERK),
                            }
                        };
                        sim.step(&inputs);
                        for event in sim.drain_events() {
                            react(&event, &sim, me, &mut sounds, &mut fx, &mut juice);
                        }
                    }
                    if playing {
                        trails(&sim, &mut fx);
                        juice.update(dt);
                        fx.update(dt);
                    }
                }
            }
        }

        // What is on screen: the local race, or the network client's predicted view of the server's.
        let (view_sim, view_me): (&Sim, usize) = match &client {
            Some(c) => (c.view().sim(), c.participant().unwrap_or(0)),
            None => (&sim, me),
        };
        let have_karts = view_me < view_sim.karts.len();
        if camera_reset && have_karts {
            camera_yaw = view_sim.karts[view_me].yaw;
            camera_reset = false;
        }
        let racing_view = matches!(screen, Screen::Race) && have_karts;

        // 3. Camera: a chase view behind the kart, or an orbit around the podium on the select screen.
        let (shake, roll) = juice.camera_shake();
        let podium = PODIUM;
        let mut view = match screen {
            Screen::Race if have_karts => {
                let kart = &view_sim.karts[view_me];
                let target = chase_target(kart);
                let ease = 1. - (-6. * dt.min(0.1)).exp();
                camera_yaw = wrap_angle(camera_yaw + wrap_angle(target - camera_yaw) * ease);
                let eye = v3(kart.pos) - v3(forward(camera_yaw)) * 8.2
                    + vec3(0., 3.7 + juice.dip, 0.)
                    + vec3(shake.0, shake.1, shake.2);
                let mut v = View::first_person(eye, camera_yaw, -0.21);
                v.fov = (68. + juice.fov_kick + kart.speed() * 0.25 + if kart.boost_ticks > 0 { 6. } else { 0. })
                    .to_radians();
                v
            }
            _ => {
                let angle = time * 0.45;
                let eye = podium + vec3(angle.sin() * 6.5, 2.6, angle.cos() * 6.5);
                let to = podium + vec3(0., 1.1, 0.) - eye;
                View::first_person(eye, to.x.atan2(-to.z), to.y.atan2((to.x * to.x + to.z * to.z).sqrt()))
            }
        };
        view.roll = roll;

        // 4. Draw. The dynamic geometry is filled first (CPU only) so the shadow pass can reuse it; then the shadow
        // pass, sky, world, contact blobs, karts, translucent effects, additive effects, then the 2D layer.
        clear_background(look.clear_color());
        world.clear();
        alpha.clear();
        add.clear();
        // The shadow box follows the human's kart (a little ahead of it, where the camera looks), or the podium.
        let focus = match &screen {
            Screen::Race if racing_view => v3(view_sim.karts[view_me].pos) + v3(forward(camera_yaw)) * SHADOW_AHEAD,
            _ => podium,
        };
        shadows.begin_frame(&look, focus);
        match screen {
            Screen::Race if racing_view => {
                for (i, k) in view_sim.karts.iter().enumerate() {
                    let lean = if k.drifting { -k.drift_dir * 0.12 } else { 0. };
                    let slide = if k.drifting { k.drift_dir * 0.3 } else { 0. };
                    // The kart only ever lifts: a bob that dips would sink the wheels into the road.
                    let bob = 0.025 * (0.5 + 0.5 * (time * 30. + i as f32).sin()) * (k.speed() / 25.).min(1.);
                    let m = Mat4::from_translation(vec3(k.pos.0, bob, k.pos.2))
                        * Mat4::from_rotation_y(-(k.yaw + slide))
                        * Mat4::from_rotation_z(lean);
                    let template = &scene.karts[k.character.index()];
                    // Phasing stays opaque (a translucent kart drawn unsorted without depth writes shows its
                    // insides, and which ones flips as the camera moves): a cold spectral tint and extra glow
                    // say "ghost" without flicker.
                    let tint = if k.phased() {
                        Tint { mul: [0.45, 0.75, 1.3], add: [0.06, 0.2, 0.32], alpha: 1., glow: 1.8 }
                    } else if k.slow_ticks > 0 {
                        Tint { mul: [0.8, 0.7, 1.], ..Tint::NONE }
                    } else {
                        Tint::NONE
                    };
                    world.add(template, m, tint);
                    shadows.blob(vec3(k.pos.0, 0., k.pos.2), KART_BLOB);
                }
                for h in &view_sim.hazards {
                    let m = Mat4::from_translation(vec3(h.pos.0, 0., h.pos.2));
                    world.add(if h.kind == HazardKind::Bone { &scene.bone } else { &scene.bandage }, m, Tint::NONE);
                    shadows.blob(vec3(h.pos.0, 0., h.pos.2), h.radius.clamp(0.4, 1.2));
                }
            }
            _ => {
                let at = podium + vec3(0., 0.2, 0.);
                let m = Mat4::from_translation(at) * Mat4::from_rotation_y(time * 0.6);
                world.add(&scene.karts[choice % MAX_RACERS], m, Tint::NONE);
                shadows.blob(at, KART_BLOB);
            }
        }
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        // Full only: what stands up (walls, scenery, karts, hazards), seen from the sun. The ground and road are
        // receivers and stay out; so do the translucent and additive effects and the sky.
        shadows.cast(|| {
            for cell in scene.casters.iter().filter(|c| shadow_reaches(&c.bounds, focus)) {
                for mesh in &cell.meshes {
                    draw_mesh(mesh);
                }
            }
            world.draw();
        });
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for mesh in &scene.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera(0.3, 700.));
        materials.set_scene(&look, view.eye, time, 0.5 + 0.5 * (time * 3.).sin());
        shadows.apply(&materials);
        materials.draw_static(&scene.ground);
        for cell in &scene.casters {
            for mesh in &cell.meshes {
                draw_mesh(mesh);
            }
        }
        if !racing_view {
            for mesh in &scene.podium {
                draw_mesh(mesh);
            }
        }
        shadows.draw_decals(&materials); // after the static world, before the karts (it leaves `decal` bound)
        gl_use_material(&materials.world);
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_default_material();
        set_default_camera();

        hud::overlay(&vignette, Color::new(0.05, 0., 0.1, 0.45));
        if juice.flash > 0.01 {
            draw_rectangle(0., 0., screen_width(), screen_height(), hud::col(juice.flash_color, juice.flash * 0.4));
        }
        match screen {
            Screen::Select => {
                draw_select(
                    choice,
                    time,
                    ui,
                    "A / D or left stick: driver     W / S or up / down: rivals     Enter or A to race",
                    Some(difficulty),
                );
                draw_online_chip(ui, time);
            }
            Screen::Online => {
                if let Some(ou) = &online_ui {
                    draw_online(ou, time, ui, local_build);
                }
            }
            Screen::Lobby => {
                if let Some(c) = &client {
                    draw_lobby(c, choice, my_ready, time, ui, room_ctx.as_ref());
                }
            }
            Screen::Race if racing_view => {
                hud::draw_popups(&fx.popups, &view, ui);
                draw_race_hud(view_sim, view_me, &view, ui);
                hud::draw_banners(&fx.banners, ui);
                if view_sim.is_over() {
                    draw_results(view_sim, view_me, ui);
                }
            }
            Screen::Race => {
                hud::text_centered(
                    "Waiting for the race...",
                    screen_width() * 0.5,
                    screen_height() * 0.5,
                    30. * ui,
                    WHITE,
                );
            }
        }
        let controls: Vec<&str> = identity.controls.split(", ").collect();
        let audio_menu = AudioMenu { music_on: settings.music_on, sfx_on: settings.sfx_on, has_music: HAS_MUSIC };
        let outcome = shell.local_menu_with_options(&identity.title, &controls, audio_menu, shadows.quality());
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
        if outcome.cycle_shadows {
            shadows.set_quality(shadows.quality().next());
            settings.shadow_quality = shadows.quality();
            settings.store(&settings_path);
        }
        if outcome.download_music {
            announce(&mut fx, &save_music(&music_wav, &identity.title));
        }
        if outcome.quit {
            break;
        }

        // 5. Evidence for a caller that cannot watch: screenshots and frame times, then exit.
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
