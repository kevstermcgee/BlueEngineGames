//! Wobble Tower: the window, renderer, sound and input around the simulation in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "drop@60,drop@400"                        drive the human input path from a cue script
//!   --autoplay   a simple built-in player plays (for captures and demos)
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved game / where F5 saves (default: next to the exe)
//! Keyboard: Space / Down / Enter drops the crate, R restarts, F5/F9 quick save and load, Esc menu.
//! Controller: A or B drops.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use macroquad::prelude::*;
use std::collections::HashMap;
use vesper3d::viewer::{
    devkit::{flag_value, has_flag, parse_size, synth, Juice, Lifecycle, Notice},
    game_client::{self, GameShell},
    game_input::ClientInput,
    gamepad::Button,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, SoundBank, Template, Tint, View},
};
use wobble_tower::{
    autoplay, physics::Body, CrateState, Event, Input, Kind, Phase, Sim, CRANE_RANGE, DEPTH, LIVES, PLATFORM_HALF,
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

/// Held device state: just the drop button.
#[derive(Clone, Copy, Default)]
struct Held {
    drop: bool,
}
/// The cue names a `--script` may use (`save` and `load` are always understood).
const CUES: [&str; 1] = ["drop"];

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 7] = [
    synth::Preset::Whoosh,
    synth::Preset::Thump,
    synth::Preset::Blip,
    synth::Preset::Coin,
    synth::Preset::Error,
    synth::Preset::GameOver,
    synth::Preset::PowerDown,
];
const S_DROP: usize = 0;
const S_LAND: usize = 1;
const S_SETTLE: usize = 2;
const S_PERFECT: usize = 3;
const S_FELL: usize = 4;
const S_OVER: usize = 5;
const S_GUST: usize = 6;

fn render_audio() -> Rendered {
    Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 7), synth::RATE)).collect())
            .collect(),
        stems: Vec::new(),
    }
}

fn v3(p: vesper3d::math::V) -> Vec3 {
    vec3(p.0, p.1, p.2)
}

fn palette(kind: Kind) -> ([f32; 3], [f32; 3]) {
    match kind {
        Kind::Crate => ([0.62, 0.40, 0.20], [0.35, 0.22, 0.10]),
        Kind::Heavy => ([0.30, 0.33, 0.40], [0.14, 0.15, 0.20]),
        Kind::Light => ([0.88, 0.80, 0.50], [0.60, 0.52, 0.28]),
    }
}

/// A crate of half extents (hx, hy) and kind: planks, a frame and corner studs.
fn crate_template(hx: f32, hy: f32, kind: Kind) -> Template {
    let (wood, dark) = palette(kind);
    let mut t = Template::new();
    t.box_(Vec3::ZERO, vec3(hx, hy, DEPTH), wood, 0.);
    // Frame on the front and back faces, and a diagonal brace.
    for z in [DEPTH + 0.005, -DEPTH - 0.005] {
        t.box_(vec3(0., hy - 0.06, z), vec3(hx, 0.06, 0.012), dark, 0.);
        t.box_(vec3(0., -hy + 0.06, z), vec3(hx, 0.06, 0.012), dark, 0.);
        t.box_(vec3(hx - 0.06, 0., z), vec3(0.06, hy, 0.012), dark, 0.);
        t.box_(vec3(-hx + 0.06, 0., z), vec3(0.06, hy, 0.012), dark, 0.);
        let diag = (hx * hx + hy * hy).sqrt();
        let mut brace = Template::new();
        brace.box_(Vec3::ZERO, vec3(diag * 0.92, 0.04, 0.012), dark, 0.);
        t.append(&brace.transformed(Mat4::from_translation(vec3(0., 0., z)) * Mat4::from_rotation_z(hy.atan2(hx))));
    }
    if kind == Kind::Heavy {
        for (sx, sy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            t.ball(
                vec3(sx * (hx - 0.12), sy * (hy - 0.12), DEPTH + 0.02),
                Vec3::splat(0.05),
                [0.8, 0.8, 0.85],
                0.2,
                6,
                4,
            );
        }
    }
    t
}

struct Scene {
    sky: Vec<Mesh>,
    platform: Vec<Mesh>,
    crane: Template,
    carriage: Template,
    crates: HashMap<(Kind, i32, i32), Template>,
}

fn key(kind: Kind, half: vesper3d::math::V) -> (Kind, i32, i32) {
    (kind, (half.0 * 100.).round() as i32, (half.1 * 100.).round() as i32)
}

fn build_scene() -> Scene {
    let mut sky = Template::new();
    sky.sky_dome(
        300.,
        |e| {
            let t = e.clamp(0., 1.).powf(0.6);
            [0.95 - 0.80 * t, 0.55 - 0.42 * t, 0.42 - 0.12 * t]
        },
        24,
        12,
    );
    let mut platform = Template::new();
    // A chunk of floating rock with a grassy, slightly wider cap.
    platform.box_(vec3(0., -0.6, 0.), vec3(PLATFORM_HALF, 0.6, DEPTH + 0.1), [0.45, 0.42, 0.40], 0.);
    platform.box_(vec3(0., 0.02, 0.), vec3(PLATFORM_HALF + 0.05, 0.05, DEPTH + 0.15), [0.30, 0.55, 0.25], 0.);
    platform.cone(vec3(0., -4.2, 0.), 0.3, PLATFORM_HALF * 0.9, 3.5, [0.35, 0.32, 0.30], 0., 10);
    platform.box_(vec3(0., -1.4, 0.), vec3(PLATFORM_HALF * 0.85, 0.9, DEPTH * 0.7), [0.40, 0.37, 0.35], 0.);
    let mut crane = Template::new();
    // The gantry: two towers and a beam (drawn relative to the carriage height).
    crane.box_(vec3(0., 0., 0.), vec3(CRANE_RANGE + 1.2, 0.18, 0.25), [0.85, 0.70, 0.15], 0.1);
    for x in [-(CRANE_RANGE + 1.0), CRANE_RANGE + 1.0] {
        crane.box_(vec3(x, -30., 0.), vec3(0.12, 30., 0.12), [0.55, 0.45, 0.12], 0.);
    }
    let mut carriage = Template::new();
    carriage.box_(Vec3::ZERO, vec3(0.45, 0.28, 0.32), [0.9, 0.3, 0.2], 0.2);
    let mut crates = HashMap::new();
    for kind in [Kind::Crate, Kind::Heavy, Kind::Light] {
        for hx in [0.5, 0.7, 0.9] {
            for hy in [0.4, 0.5] {
                crates.insert(key(kind, vesper3d::math::V(hx, hy, 0.)), crate_template(hx, hy, kind));
            }
        }
    }
    Scene { sky: sky.to_meshes(), platform: platform.to_meshes(), crane, carriage, crates }
}

fn dusk_look() -> Look {
    let mut look = Look::daylight();
    look.ambient_sky = [0.55, 0.45, 0.50];
    look.ambient_ground = [0.25, 0.20, 0.25];
    look.key_color = [1.0, 0.75, 0.55];
    look.key_direction = [-0.5, 0.6, 0.7];
    look.fog_color = [0.75, 0.45, 0.40];
    look.fog_density = 0.0035;
    look
}

/// Turn one simulation event into sound, particles, shake and text. Exhaustive on purpose: a new `Event` must be
/// handled (or explicitly ignored) here.
fn react(event: &Event, sounds: &mut SoundBank, fx: &mut Fx, juice: &mut Juice) {
    match event {
        Event::Drop { .. } => sounds.play(S_DROP, 0.5),
        Event::Land { at, kind } => {
            sounds.play(S_LAND, if *kind == Kind::Heavy { 1. } else { 0.6 });
            fx.dust(v3(*at) - vec3(0., 0.4, 0.), 10, 1.4, [0.8, 0.7, 0.6]);
            juice.shake(if *kind == Kind::Heavy { 0.5 } else { 0.25 });
        }
        Event::Settled { at, perfect, points } => {
            if *perfect {
                sounds.play(S_PERFECT, 0.8);
                fx.sparks(v3(*at) + vec3(0., 0.5, 0.), 26, 5., [1., 0.9, 0.3]);
                fx.popup(v3(*at) + vec3(0., 1., 0.), format!("PERFECT +{points}"), [1., 0.9, 0.3], 34.);
            } else {
                sounds.play(S_SETTLE, 0.5);
                fx.popup(v3(*at) + vec3(0., 1., 0.), format!("+{points}"), [1., 1., 1.], 26.);
            }
        }
        Event::Fell { at, lives_left } => {
            sounds.play(S_FELL, 0.8);
            juice.flash([1., 0.2, 0.2], 0.3);
            fx.dust(v3(*at), 16, 2., [0.9, 0.6, 0.5]);
            fx.banner("CRATE LOST", format!("{lives_left} left"), [1., 0.45, 0.4]);
        }
        Event::Gust { strength } => {
            sounds.play(S_GUST, 0.25 + 0.15 * strength.abs());
        }
        Event::GameOver { score, height } => {
            sounds.play(S_OVER, 0.9);
            fx.banner("TOWER DOWN", format!("{score} points, {height:.1} m: press R to try again"), [1., 0.85, 0.4]);
        }
    }
}

/// Show the outcome of a quick save or load.
fn announce(fx: &mut Fx, notice: &Notice) {
    fx.banners.clear();
    fx.banner(notice.title, notice.detail.clone(), notice.color);
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identity = identity();
    // The run flags, the fixed-step loop, quick save/load and evidence for a caller that cannot watch.
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let seed = life.seed();
    let unattended = life.options.unattended();
    let autoplayer = has_flag(&args, "--autoplay");

    let materials = Materials::load().expect("the materials failed to compile");
    let look = dusk_look();
    let scene = build_scene();
    let mut sounds = SoundBank::start(life.options.silent(), 0.9, 0.6, render_audio).await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let (mut fx, mut juice) = (Fx::new(seed), Juice::default());
    let mut sim = Sim::new(seed);
    life.load_flag_or_exit(&mut sim);
    let vignette = hud::make_vignette();
    let (mut world, mut alpha, mut add) = (Batch::new(), Batch::new(), Batch::new());
    let mut best = (0u64, 0.0f32);
    let mut camera_y = 4.0f32;

    loop {
        // No mouse look here, so the mouse is never captured.
        input.begin_frame_with_keyboard(&mut shell, false, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        let pad = input.gamepad().clone();

        // 1. Devices in: one frame of held state (from the script when there is one).
        let accepting = shell.accepting_input() && (unattended || platform::focused());
        let held = match life.script() {
            Some(s) => Held { drop: s.held("drop") },
            None if accepting => Held {
                drop: input.down(KeyCode::Space)
                    || input.down(KeyCode::Down)
                    || input.down(KeyCode::Enter)
                    || input.down(KeyCode::S)
                    || pad.down(Button::South)
                    || pad.down(Button::East),
            },
            None => Held::default(),
        };
        life.feed(held, 0, [0., 0.]);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let notice = if sim.phase == Phase::Over {
                Notice::refused("the tower is down")
            } else {
                life.quick_save(&sim, &format!("Quick save, {:.1} m", sim.height))
            };
            announce(&mut fx, &notice);
        }
        if load {
            let notice = life.quick_load(&mut sim);
            announce(&mut fx, &notice);
            if notice.ok {
                juice = Juice::default();
            }
        }
        if input.pressed(KeyCode::R) {
            best = (best.0.max(sim.score), best.1.max(sim.height));
            sim = Sim::new(life.restart_seed());
            fx.clear();
            life.reset_input();
        }

        // 2. Simulation: whole fixed ticks, each with exactly one Input.
        let playing = !shell.paused;
        for _ in 0..life.ticks(dt, juice.time_scale(None), playing) {
            let tick = life.take_tick();
            let step_input = if autoplayer { autoplay(&sim) } else { Input { drop: tick.held.drop } };
            sim.step(&step_input);
            for event in sim.drain_events() {
                react(&event, &mut sounds, &mut fx, &mut juice);
            }
        }
        if playing {
            juice.update(dt);
            fx.update(dt);
        }
        best = (best.0.max(sim.score), best.1.max(sim.height));

        // 3. Camera: side-on, climbing with the tower.
        let want = (sim.height * 0.8 + 3.4).max(4.0);
        camera_y += (want - camera_y) * (1. - (-2.5 * dt.min(0.1)).exp());
        let (shake, roll) = juice.camera_shake();
        let eye = vec3(0., camera_y, 17.0) + vec3(shake.0, shake.1, 0.);
        let mut view = View::first_person(eye, 0., -0.06);
        view.fov = 48f32.to_radians();
        view.roll = roll * 0.4;

        // 4. Draw: sky, platform, crates, crane, wind, translucent and additive effects, then the 2D layer.
        clear_background(look.clear_color());
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for mesh in &scene.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera(0.1, 300.));
        materials.set_scene(&look, view.eye, time, 0.5 + 0.5 * (time * 3.).sin());
        world.clear();
        alpha.clear();
        add.clear();
        for c in sim.crates.iter().filter(|c| c.state != CrateState::Fell) {
            let b = Body(c.body);
            let p = sim.phys.position(b);
            if let Some(t) = scene.crates.get(&key(c.kind, c.half)) {
                let m = Mat4::from_translation(vec3(p.0, p.1, 0.)) * Mat4::from_rotation_z(sim.phys.roll(b));
                world.add(t, m, Tint::NONE);
            }
        }
        // The crane and the crate on its hook.
        let crane = sim.crane_at();
        world.add(&scene.crane, Mat4::from_translation(vec3(0., crane.1 + 0.6, 0.)), Tint::NONE);
        world.add(&scene.carriage, Mat4::from_translation(vec3(crane.0, crane.1 + 0.3, 0.)), Tint::NONE);
        if let Some((half, kind)) = sim.carried {
            if let Some(t) = scene.crates.get(&key(kind, half)) {
                let hang = crane.1 - 1.0;
                world.add(t, Mat4::from_translation(vec3(crane.0, hang, 0.)), Tint::NONE);
                let mut rope = Template::new();
                rope.box_(Vec3::ZERO, vec3(0.03, (crane.1 + 0.3 - (hang + half.1)) * 0.5, 0.03), [0.2, 0.2, 0.2], 0.);
                world.add(
                    &rope,
                    Mat4::from_translation(vec3(crane.0, (crane.1 + 0.3 + hang + half.1) * 0.5, 0.)),
                    Tint::NONE,
                );
            }
        }
        // Wind streaks, brighter and faster the harder it blows.
        if sim.wind.abs() > 0.05 {
            for i in 0..18 {
                let f = i as f32;
                let speed = sim.wind * 3.0;
                let x = ((f * 5.3 + time * speed * 2.0).rem_euclid(24.)) - 12.;
                let y = camera_y + ((f * 2.7).rem_euclid(9.)) - 3.;
                let len = 0.8 + 0.3 * (f % 3.);
                let dir = sim.wind.signum();
                let a = vec3(x, y, 1.5);
                let b = vec3(x + dir * len, y, 1.5);
                add.beam(a, b, view.eye, 0.04, [0.9, 0.95, 1.], 0.0, (0.15 * sim.wind.abs()).min(0.5), 0.6);
            }
        }
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        gl_use_material(&materials.world);
        for mesh in &scene.platform {
            draw_mesh(mesh);
        }
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_default_material();
        set_default_camera();

        let ui = hud::ui_scale();
        hud::overlay(&vignette, Color::new(0.1, 0.02, 0.05, 0.35));
        if juice.flash > 0.01 {
            draw_rectangle(0., 0., screen_width(), screen_height(), hud::col(juice.flash_color, juice.flash * 0.4));
        }
        hud::draw_popups(&fx.popups, &view, ui);
        hud::panel(20. * ui, 16. * ui, 240. * ui, 100. * ui, 12. * ui, Color::new(0.08, 0.03, 0.05, 0.6));
        hud::text_outlined("HEIGHT", 34. * ui, 38. * ui, 16. * ui, hud::col([1., 0.8, 0.4], 1.));
        hud::text_outlined(&format!("{:.1} m", sim.height), 34. * ui, 74. * ui, 36. * ui, WHITE);
        hud::text_outlined(
            &format!("{} pts   best {:.1} m", hud::commas(sim.score), best.1),
            34. * ui,
            102. * ui,
            15. * ui,
            hud::col([0.95, 0.85, 0.8], 1.),
        );
        for i in 0..LIVES {
            let on = i < sim.lives;
            draw_rectangle(
                screen_width() - (50. + 34. * i as f32) * ui,
                20. * ui,
                26. * ui,
                22. * ui,
                if on { hud::col([0.7, 0.45, 0.2], 1.) } else { Color::new(0.3, 0.2, 0.2, 0.4) },
            );
        }
        if let Some((_, kind)) = sim.carried {
            let (label, tint) = match kind {
                Kind::Crate => ("TIMBER CRATE", [1., 1., 1.]),
                Kind::Heavy => ("HEAVY: dense and grippy, hits hard", [0.7, 0.8, 1.]),
                Kind::Light => ("LIGHT: slippery, the wind loves it", [1., 0.95, 0.6]),
            };
            hud::text_centered(label, screen_width() * 0.5, screen_height() - 60. * ui, 18. * ui, hud::col(tint, 1.));
        }
        hud::text_centered(
            "Space / A to drop the crate",
            screen_width() * 0.5,
            screen_height() - 34. * ui,
            15. * ui,
            hud::col([1., 1., 1.], 0.7),
        );
        // The wind gauge.
        if sim.stacked >= 3 {
            let x = screen_width() - 130. * ui;
            let y = 86. * ui;
            hud::text_centered("WIND", x, y - 14. * ui, 14. * ui, hud::col([0.8, 0.9, 1.], 1.));
            draw_rectangle(x - 60. * ui, y, 120. * ui, 8. * ui, Color::new(0.1, 0.1, 0.2, 0.6));
            let w = (sim.wind / 2.4).clamp(-1., 1.);
            draw_rectangle(
                if w >= 0. { x } else { x + w * 60. * ui },
                y,
                w.abs() * 60. * ui,
                8. * ui,
                hud::col([0.6, 0.85, 1.], 1.),
            );
        }
        hud::draw_banners(&fx.banners, ui);
        let controls: Vec<&str> = identity.controls.split(", ").collect();
        if shell.local_menu(&identity.title, &controls) {
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
