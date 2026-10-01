//! Dead Air: the window, renderer, sound and input around the simulation in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "fwd:0-200,look:0.01@0-100"               drive the human input path from a cue script
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved game / where F5 saves (default: next to the exe)
//! F5 saves the run to the `quick` slot and F9 loads it. Esc pauses; the Settings screen there has a
//! music/sound toggle and a "Save music" button.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use dead_air::{
    CallerState, Event, Input, Outcome, Sim, AMMO, BATTERIES, CONTROL_ROOM, FUEL_CANS, GENERATOR, GENERATOR_ROOM,
    NOTES, NOTE_COUNT, RADIO_DESK,
};
use macroquad::prelude::*;
use std::sync::{Arc, OnceLock};
use vesper3d::viewer::{
    controller::Collider,
    devkit::{
        beside_exe, downloads_dir, flag_value, has_flag, parse_size, sanitize_filename, synth, unique_path, Lifecycle,
        Notice, Rng, Settings,
    },
    game_client::{self, AudioMenu, GameShell},
    game_input::ClientInput,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, PointLight, Rendered, SoundBank, Template, Tint, View},
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

/// Held device state; press edges travel separately as bits.
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
    sneak: bool,
}
const FLASHLIGHT: u32 = 1;
const FIRE: u32 = 2;
const INTERACT: u32 = 4;
const CUES: [&str; 5] = ["fwd", "back", "left", "right", "look"];

/// The station log pages, in reading order (the station does not enforce finding them in order).
const NOTE_TEXT: [&str; NOTE_COUNT] = [
    "STATION LOG - NIGHT 3\n\nSame signal again, 02:14 sharp, same dead frequency nothing is assigned \
     to. It is not words yet. It is closer to words every night.",
    "STATION LOG - NIGHT 7\n\nIt said a name tonight. My brother's name, in his voice. He has been gone \
     eleven years. I am writing down what it said because someone should, not because I believe it.",
    "STATION LOG - NIGHT 12\n\nFound a gap behind the generator housing that is not on any blueprint. \
     Welded a plate over it myself. The humming under the floor started that same night and has not \
     stopped.",
    "STATION LOG - NIGHT 16\n\nSomething outside called for help in my own voice. Writing this down so \
     whoever reads it understands: it does not matter whose voice it borrows. Do not go outside.",
    "STATION LOG - NIGHT 19\n\nAll clear. It knows the schedule now. If you can hear this being read \
     back to you, it has already answered for m\u{2014}",
];

// Sound: generated, never recorded. Presets cover the ordinary cues; three are hand-built because the
// feel of a gunshot, a startled shriek and a closing-distance pulse matters more here. See
// devkit::synth and docs/AUDIO.md.
const SFX_GUNSHOT: usize = 0;
const SFX_SHRIEK: usize = 1;
const SFX_HEARTBEAT: usize = 2;
const SFX_PICKUP: usize = 3;
const SFX_NOTE: usize = 4;
const SFX_BROADCAST: usize = 5;
const SFX_REFUSED: usize = 6;
const SFX_WARNING: usize = 7;
const SFX_GAMEOVER: usize = 8;
const SFX_CLICK: usize = 9;

fn gunshot_sound(seed: u64) -> Vec<f32> {
    use synth::*;
    let mut rng = Rng::new(seed);
    let crack = shape(highpass(&noise(0.05, &mut rng), 2200., 0.8), |t| decay(t, 0.012));
    let thump = shape(osc(0.12, |_| 90., sine), |t| ad(t, 0.0005, 0.05));
    let mut m = Mix::new(0.15);
    m.add(0., &crack, 1.0);
    m.add(0., &thump, 0.6);
    finish(m.into_vec(), 0.9, 15.)
}

/// A harsh, low, startled sound: not a scream, a warning. Plays when the Caller is staggered.
fn caller_shriek(seed: u64) -> Vec<f32> {
    use synth::*;
    let mut rng = Rng::new(seed);
    let dur = 0.9;
    let voice = osc_bl(dur, glide(220., 70., 0.7), Wave::Saw);
    let voice = filter(&voice, FilterKind::Low, |t| 1800. - 1500. * (t / dur).min(1.), 1.3);
    let rasp = bandpass(&noise(dur, &mut rng), 900., 0.6);
    let mut m = Mix::new(dur);
    m.add(0., &shape(voice, |t| ad(t, 0.02, 0.4)), 0.8);
    m.add(0., &shape(rasp, |t| ad(t, 0.02, 0.3)), 0.4);
    let mut v = m.into_vec();
    soft_clip(&mut v, 1.6);
    finish(v, 0.85, 40.)
}

/// A low double-thud, like a pulse. Played at a rate that climbs as the Caller closes in on a hunt.
fn heartbeat_sound() -> Vec<f32> {
    use synth::*;
    let thump = shape(osc(0.16, |_| 55., sine), |t| ad(t, 0.005, 0.06));
    let mut m = Mix::new(0.42);
    m.add(0., &thump, 0.9);
    m.add(0.13, &thump, 0.55);
    finish(m.into_vec(), 0.55, 40.)
}

fn render_audio(music_wav: Arc<OnceLock<Vec<u8>>>, title: String, tagline: String) -> Rendered {
    use synth::{wav_bytes, Preset, RATE};
    let presets = [
        (Preset::Pickup, SFX_PICKUP),
        (Preset::Chime, SFX_NOTE),
        (Preset::Success, SFX_BROADCAST),
        (Preset::Error, SFX_REFUSED),
        (Preset::Warning, SFX_WARNING),
        (Preset::GameOver, SFX_GAMEOVER),
        (Preset::Click, SFX_CLICK),
    ];
    let mut sfx: Vec<Vec<Vec<u8>>> = vec![Vec::new(); 10];
    sfx[SFX_GUNSHOT] = vec![wav_bytes(&gunshot_sound(11), RATE)];
    sfx[SFX_SHRIEK] = vec![wav_bytes(&caller_shriek(13), RATE)];
    sfx[SFX_HEARTBEAT] = vec![wav_bytes(&heartbeat_sound(), RATE)];
    for (preset, slot) in presets {
        sfx[slot] = (0..preset.variants()).map(|v| wav_bytes(&synth::render(preset, v, 7), RATE)).collect();
    }
    let spec = synth::ambient_spec_for(&title, &tagline);
    let ambient = wav_bytes(&synth::ambient_loop(&spec), RATE);
    let _ = music_wav.set(ambient.clone());
    Rendered { sfx, stems: vec![ambient] }
}

/// Static geometry: walls (matching `dead_air`'s colliders exactly), floors, ceilings and fixed decor.
/// Pickups and the Caller are drawn separately each frame since they move or disappear.
struct Scene {
    station: Vec<Mesh>,
    caller_body: Template,
    marker_fuel: Template,
    marker_battery: Template,
    marker_ammo: Template,
    marker_note: Template,
}

fn wall_boxes(t: &mut Template, walls: &[Collider], color: [f32; 3]) {
    for wall in walls {
        let center = (wall.min + wall.max) * 0.5;
        let half = (wall.max - wall.min) * 0.5;
        t.box_(vec3(center.0, center.1, center.2), vec3(half.0, half.1, half.2), color, 0.);
    }
}

fn room_shell(t: &mut Template, room: (f32, f32, f32, f32), floor: [f32; 3], ceiling: [f32; 3]) {
    let (x0, x1, z0, z1) = room;
    let (cx, cz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
    let (hx, hz) = ((x1 - x0) * 0.5, (z1 - z0) * 0.5);
    t.box_top(vec3(cx, -0.05, cz), vec3(hx, 0.05, hz), [0.06, 0.06, 0.07], floor, 0.);
    t.box_(vec3(cx, 2.6, cz), vec3(hx, 0.05, hz), ceiling, 0.);
}

fn build_scene() -> Scene {
    let mut station = Template::new();
    wall_boxes(&mut station, &dead_air::colliders(), [0.16, 0.15, 0.17]);
    room_shell(&mut station, CONTROL_ROOM, [0.20, 0.17, 0.12], [0.10, 0.10, 0.11]);
    room_shell(&mut station, dead_air::ARCHIVE_ROOM, [0.18, 0.16, 0.14], [0.10, 0.10, 0.11]);
    room_shell(&mut station, dead_air::DORMITORY, [0.14, 0.14, 0.18], [0.10, 0.10, 0.11]);
    room_shell(&mut station, GENERATOR_ROOM, [0.12, 0.12, 0.12], [0.09, 0.09, 0.10]);
    let (cx0, cx1) = dead_air::CORRIDOR_X;
    let (cz0, cz1) = dead_air::CORRIDOR_Z;
    room_shell(&mut station, (cx0, cx1, cz0, cz1), [0.15, 0.15, 0.16], [0.09, 0.09, 0.10]);

    // Control room: desk, chair and a small glowing status light.
    station.box_(vec3(4.6, 0.45, 1.6), vec3(0.6, 0.45, 0.35), [0.30, 0.22, 0.14], 0.);
    station.box_(vec3(4.6, 0.25, 2.5), vec3(0.22, 0.25, 0.22), [0.22, 0.17, 0.12], 0.);
    station.box_(vec3(RADIO_DESK.pos.0, 0.95, RADIO_DESK.pos.2 - 0.3), vec3(0.05, 0.05, 0.05), [1.0, 0.15, 0.1], 0.9);

    // Archive: shelves.
    for z in [0.6, 1.8, 2.8] {
        station.box_(vec3(-5.6, 0.9, z), vec3(0.3, 0.85, 0.12), [0.25, 0.20, 0.14], 0.);
    }

    // Dormitory: bed and locker.
    station.box_(vec3(4.3, 0.3, -8.6), vec3(0.9, 0.3, 0.5), [0.22, 0.24, 0.30], 0.);
    station.box_(vec3(2.4, 0.6, -6.5), vec3(0.3, 0.6, 0.25), [0.18, 0.18, 0.20], 0.);

    // Generator room: the generator block, with its own small glow.
    station.box_(vec3(GENERATOR.pos.0, 0.6, GENERATOR.pos.2), vec3(0.8, 0.6, 0.6), [0.14, 0.14, 0.15], 0.);
    station.box_(vec3(GENERATOR.pos.0, 1.0, GENERATOR.pos.2), vec3(0.05, 0.05, 0.05), [0.9, 0.7, 0.2], 0.6);

    // Yard: two antenna towers with warning lamps, and a cracked concrete apron.
    for x in [-4.0, 4.0] {
        station.cylinder(vec3(x, 0., 6.5), 0.12, 7.0, [0.20, 0.20, 0.22], 0., 8);
        station.ball(vec3(x, 7.0, 6.5), Vec3::splat(0.25), [0.9, 0.1, 0.1], 0.5, 8, 6);
    }
    station.box_top(vec3(0., -0.05, 5.5), vec3(7., 0.05, 3.5), [0.05, 0.05, 0.05], [0.10, 0.12, 0.10], 0.);

    let mut caller_body = Template::new();
    caller_body.box_(vec3(0., 1.05, 0.), vec3(0.22, 0.85, 0.14), [0.72, 0.70, 0.66], 0.);
    caller_body.box_(vec3(0., 2.05, 0.), vec3(0.14, 0.16, 0.13), [0.68, 0.66, 0.62], 0.);
    caller_body.box_(vec3(-0.30, 0.95, 0.08), vec3(0.06, 0.65, 0.06), [0.60, 0.58, 0.55], 0.);
    caller_body.box_(vec3(0.30, 0.95, 0.08), vec3(0.06, 0.65, 0.06), [0.60, 0.58, 0.55], 0.);
    caller_body.box_(vec3(-0.12, 0.35, 0.), vec3(0.09, 0.40, 0.09), [0.55, 0.53, 0.50], 0.);
    caller_body.box_(vec3(0.12, 0.35, 0.), vec3(0.09, 0.40, 0.09), [0.55, 0.53, 0.50], 0.);

    let mut marker_fuel = Template::new();
    marker_fuel.cylinder(vec3(0., 0., 0.), 0.14, 0.3, [0.95, 0.75, 0.15], 0.7, 10);
    let mut marker_battery = Template::new();
    marker_battery.box_(vec3(0., 0.1, 0.), vec3(0.12, 0.1, 0.06), [0.25, 0.75, 0.95], 0.7);
    let mut marker_ammo = Template::new();
    marker_ammo.box_(vec3(0., 0.08, 0.), vec3(0.1, 0.08, 0.16), [0.8, 0.45, 0.15], 0.6);
    let mut marker_note = Template::new();
    marker_note.box_(vec3(0., 0.02, 0.), vec3(0.14, 0.02, 0.1), [0.85, 0.82, 0.70], 0.4);

    Scene { station: station.to_meshes(), caller_body, marker_fuel, marker_battery, marker_ammo, marker_note }
}

/// Turn one simulation event into sound and HUD feedback.
fn react(event: &Event, sounds: &mut SoundBank, fx: &mut Fx) {
    match event {
        Event::PickedUpFuel | Event::PickedUpBattery | Event::PickedUpAmmo => sounds.play(SFX_PICKUP, 0.8),
        Event::ReadNote(_) => sounds.play(SFX_NOTE, 0.9),
        Event::Refuelled => sounds.play(SFX_PICKUP, 0.9),
        Event::FlashlightOn | Event::FlashlightOff => sounds.play(SFX_CLICK, 0.6),
        Event::FlashlightEmpty | Event::GeneratorDied => sounds.play(SFX_WARNING, 0.9),
        Event::GeneratorRestarted => sounds.play(SFX_BROADCAST, 0.5),
        Event::Fired { .. } => sounds.play(SFX_GUNSHOT, 1.0),
        Event::DryFire => sounds.play(SFX_REFUSED, 0.6),
        Event::CallerStaggered => sounds.play(SFX_SHRIEK, 1.0),
        Event::BroadcastSent { .. } => sounds.play(SFX_BROADCAST, 1.0),
        Event::BroadcastRefused => sounds.play(SFX_REFUSED, 0.7),
        Event::Won => {
            sounds.play(SFX_BROADCAST, 1.0);
            fx.banner("DAWN", "You made it to sunrise.", [0.9, 0.85, 0.5]);
        }
        Event::Caught => {
            sounds.play(SFX_GAMEOVER, 1.0);
            fx.banner("DEAD AIR", "It was already standing behind you.", [0.9, 0.2, 0.2]);
        }
        Event::RanOutOfTime => {
            sounds.play(SFX_GAMEOVER, 0.8);
            fx.banner("DEAD AIR", "The last broadcast never went out.", [0.8, 0.7, 0.3]);
        }
    }
}

fn announce(fx: &mut Fx, notice: &Notice) {
    fx.banners.clear();
    fx.banner(notice.title, notice.detail.clone(), notice.color);
}

fn clock(shift_seconds: f32) -> String {
    let total_minutes = (shift_seconds / dead_air::SHIFT_LENGTH * 360.) as u32;
    let (hour, minute) = ((total_minutes / 60) % 24, total_minutes % 60);
    format!("{hour:02}:{minute:02}")
}

fn draw_journal(notes_found: &[bool; NOTE_COUNT], ui: f32) {
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0., 0., w, h, Color::from_rgba(5, 5, 8, 215));
    hud::text_outlined("STATION LOG", 60. * ui, 70. * ui, 30. * ui, hud::col([0.9, 0.85, 0.6], 1.));
    let found = notes_found.iter().filter(|f| **f).count();
    hud::text_outlined(
        &format!("{found}/{NOTE_COUNT} pages found - J to close"),
        60. * ui,
        98. * ui,
        16. * ui,
        hud::col([0.7, 0.7, 0.7], 1.),
    );
    let mut y = 140. * ui;
    for (i, text) in NOTE_TEXT.iter().enumerate() {
        if !notes_found[i] {
            continue;
        }
        for line in hud::wrap(text, w - 140. * ui, 15. * ui) {
            hud::text_outlined(&line, 70. * ui, y, 15. * ui, hud::col([0.88, 0.86, 0.80], 1.));
            y += 20. * ui;
        }
        y += 18. * ui;
    }
    if found == 0 {
        hud::text_outlined("Nothing found yet.", 70. * ui, y, 15. * ui, hud::col([0.5, 0.5, 0.5], 1.));
    }
    let _ = h;
}

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

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identity = identity();
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let seed = life.seed();
    let unattended = life.options.unattended();

    let materials = Materials::load().expect("the materials failed to compile");
    let mut look = Look::night();
    // Dark but navigable: a flashlight should matter, not be the only way to see anything at all.
    look.fog_density = 0.12;
    look.ambient_sky = [0.16, 0.15, 0.20];
    look.ambient_ground = [0.10, 0.10, 0.13];
    look.key_color = [0.14, 0.13, 0.20];
    look.rim_strength = 0.15;
    let scene = build_scene();

    let settings_path = beside_exe("settings.json");
    let mut settings = Settings::load(&settings_path);
    let music_wav: Arc<OnceLock<Vec<u8>>> = Arc::new(OnceLock::new());
    let mut sounds = SoundBank::start(life.options.silent(), settings.sfx_level(), settings.music_level(), {
        let music_wav = music_wav.clone();
        let (title, tagline) = (identity.title.clone(), identity.tagline.clone());
        move || render_audio(music_wav, title, tagline)
    })
    .await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let mut fx = Fx::new(seed);
    let mut sim = Sim::new(seed);
    life.load_flag_or_exit(&mut sim);
    let vignette = hud::make_vignette();
    let (mut world, mut alpha, mut add) = (Batch::new(), Batch::new(), Batch::new());
    let mut show_journal = false;
    let mut heartbeat_timer = 0.;

    loop {
        input.begin_frame_with_keyboard(
            &mut shell,
            sim.outcome.is_none(),
            unattended || platform::focused(),
            platform::keyboard(),
        );
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        sounds.update_music(dt, &[0.6]);

        if shell.accepting_input() && input.pressed(KeyCode::J) {
            show_journal = !show_journal;
        }

        let (held, look_delta) = match life.script() {
            Some(s) => (
                Held { forward: s.axis("fwd", "back"), right: s.axis("right", "left"), sneak: false },
                [s.value("look", 0), s.value("look", 1)],
            ),
            None => {
                let m = input.movement(&shell);
                (Held { forward: m.forward, right: m.right, sneak: m.crouch }, input.look_delta_with(&shell, dt))
            }
        };
        let mut edges = 0u32;
        if shell.accepting_input() && input.pressed(KeyCode::T) {
            edges |= FLASHLIGHT;
        }
        if shell.playing() && input.pressed(KeyCode::Space) {
            edges |= FIRE;
        }
        if shell.accepting_input() && input.pressed(KeyCode::E) {
            edges |= INTERACT;
        }
        life.feed(held, edges, look_delta);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let notice = life.quick_save(&sim, &format!("Quick save, {} broadcasts", sim.broadcasts_done));
            announce(&mut fx, &notice);
        }
        if load {
            let notice = life.quick_load(&mut sim);
            announce(&mut fx, &notice);
        }
        if sim.outcome.is_some() && (input.pressed(KeyCode::R) || input.pressed(KeyCode::Enter)) {
            sim = Sim::new(life.restart_seed());
            fx.clear();
            life.reset_input();
        }

        let playing = !shell.paused && !show_journal;
        for _ in 0..life.ticks(dt, 1.0, playing) {
            let tick = life.take_tick();
            sim.step(&Input {
                forward: tick.held.forward,
                right: tick.held.right,
                sneak: tick.held.sneak,
                look: tick.look,
                flashlight_toggle: tick.pressed(FLASHLIGHT),
                fire: tick.pressed(FIRE),
                interact: tick.pressed(INTERACT),
            });
            for event in sim.drain_events() {
                react(&event, &mut sounds, &mut fx);
            }
        }
        if playing {
            fx.update(dt);
        }

        let distance_to_caller = (sim.caller.pos - sim.player.position).length();
        if sim.caller.state == CallerState::Hunt && playing {
            let interval = (distance_to_caller / 8.0).clamp(0.35, 1.4);
            heartbeat_timer -= dt;
            if heartbeat_timer <= 0. {
                heartbeat_timer = interval;
                sounds.play(SFX_HEARTBEAT, (1.2 - distance_to_caller / 10.).clamp(0.3, 1.0));
            }
        } else {
            heartbeat_timer = 0.2;
        }

        let eye = vec3(sim.player.position.0, sim.player.position.1, sim.player.position.2);
        let mut view = View::first_person(eye, sim.player.yaw, sim.player.pitch);
        view.fov = 78_f32.to_radians();

        clear_background(look.clear_color());
        materials.set_scene(&look, eye, time, 0.);
        let mut lights = Vec::new();
        if sim.flashlight_on {
            let forward = sim.player.direction();
            let at = sim.player.position + forward * 0.3;
            if let Ok(l) = PointLight::new(vec3(at.0, at.1, at.2), 9.0, [1.0, 0.96, 0.85], 2.2) {
                lights.push(l);
            }
        }
        let _ = materials.set_point_lights(&lights);

        set_camera(&view.camera(0.05, 60.));
        world.clear();
        alpha.clear();
        add.clear();
        gl_use_material(&materials.world);
        for mesh in &scene.station {
            draw_mesh(mesh);
        }
        if sim.outcome != Some(Outcome::Won) {
            let to_player = sim.player.position - sim.caller.pos;
            let yaw = to_player.0.atan2(-to_player.2);
            world.add(
                &scene.caller_body,
                Mat4::from_rotation_translation(
                    Quat::from_rotation_y(yaw),
                    vec3(sim.caller.pos.0, 0., sim.caller.pos.2),
                ),
                Tint::NONE,
            );
        }
        for (i, spot) in FUEL_CANS.iter().enumerate() {
            if !sim.fuel_taken(i) {
                world.add(&scene.marker_fuel, Mat4::from_translation(vec3(spot.pos.0, 0.15, spot.pos.2)), Tint::NONE);
            }
        }
        for (i, spot) in BATTERIES.iter().enumerate() {
            if !sim.battery_taken(i) {
                world.add(&scene.marker_battery, Mat4::from_translation(vec3(spot.pos.0, 0.6, spot.pos.2)), Tint::NONE);
            }
        }
        if !sim.ammo_taken() {
            world.add(&scene.marker_ammo, Mat4::from_translation(vec3(AMMO.pos.0, 0.6, AMMO.pos.2)), Tint::NONE);
        }
        for (i, spot) in NOTES.iter().enumerate() {
            if !sim.notes_found[i] {
                world.add(&scene.marker_note, Mat4::from_translation(vec3(spot.pos.0, 0.75, spot.pos.2)), Tint::NONE);
            }
        }
        gl_use_material(&materials.world);
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_default_material();
        set_default_camera();

        let ui = hud::ui_scale();
        hud::overlay(&vignette, Color::new(0., 0., 0., 0.55));
        hud::draw_popups(&fx.popups, &view, ui);
        hud::draw_banners(&fx.banners, ui);
        hud::crosshair(ui, 0., Color::new(1., 1., 1., 0.6));

        hud::panel(20. * ui, 16. * ui, 230. * ui, 118. * ui, 10. * ui, Color::new(0.02, 0.02, 0.03, 0.65));
        hud::text_outlined(&clock(sim.shift_seconds), 34. * ui, 40. * ui, 22. * ui, hud::col([0.85, 0.85, 0.9], 1.));
        hud::text_outlined(
            &format!("Broadcasts {}/{}", sim.broadcasts_done, dead_air::BROADCASTS_NEEDED),
            34. * ui,
            64. * ui,
            14. * ui,
            hud::col([0.8, 0.8, 0.6], 1.),
        );
        hud::text_outlined("BATTERY", 34. * ui, 84. * ui, 12. * ui, hud::col([0.7, 0.7, 0.7], 1.));
        hud::bar(
            100. * ui,
            74. * ui,
            90. * ui,
            10. * ui,
            sim.flashlight_battery / 100.,
            hud::col([0.9, 0.85, 0.3], 1.),
        );
        hud::text_outlined("FUEL", 34. * ui, 102. * ui, 12. * ui, hud::col([0.7, 0.7, 0.7], 1.));
        hud::bar(100. * ui, 92. * ui, 90. * ui, 10. * ui, sim.generator_fuel / 100., hud::col([0.5, 0.8, 0.4], 1.));
        hud::text_outlined(
            &format!("AMMO {}", sim.ammo),
            34. * ui,
            122. * ui,
            14. * ui,
            hud::col([0.85, 0.7, 0.5], 1.),
        );
        if sim.carrying_fuel {
            hud::text_outlined("Carrying a fuel can", 34. * ui, 150. * ui, 13. * ui, hud::col([0.95, 0.8, 0.3], 1.));
        }
        let found = sim.notes_found.iter().filter(|f| **f).count();
        hud::text_right(
            &format!("Pages {found}/{NOTE_COUNT}  (J)"),
            screen_width() - 24. * ui,
            40. * ui,
            15. * ui,
            hud::col([0.8, 0.78, 0.65], 1.),
        );

        if show_journal {
            draw_journal(&sim.notes_found, ui);
        }

        let controls: Vec<&str> = identity.controls.split(", ").collect();
        let audio_menu = AudioMenu { music_on: settings.music_on, sfx_on: settings.sfx_on, has_music: true };
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

        if sim.outcome.is_some() && !shell.paused {
            hud::text_centered(
                "Press R to try again",
                screen_width() / 2.,
                screen_height() - 60. * ui,
                18. * ui,
                WHITE,
            );
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
