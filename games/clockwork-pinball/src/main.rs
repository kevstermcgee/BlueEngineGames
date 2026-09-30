//! Clockwork Pinball: the window, renderer, sound and input around the simulation in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "plunger:0-60,left:200-230,right@300"     drive the human input path from a cue script
//!   --autoplay   a simple built-in player plays (for captures and demos)
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved game / where F5 saves (default: next to the exe)
//! Keyboard: A/Left and D/Right flip, Space (hold, release) plunges, R restarts, F5/F9 quick save and load.
//! Controller: LB/LT and RB/RT flip, A (hold, release) plunges.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use clockwork_pinball::{
    autoplay, physics::Body, Event, Input, Phase, Sim, BALL_R, BUMPERS, BUMPER_R, FLIPPER_HALF_THICK, FLIPPER_LEN,
    GEAR_HALF, LANES, LANE_R, LANE_X, PLUNGER_Z, POSTS, POST_R, RIGHT,
};
use macroquad::prelude::*;
use vesper3d::viewer::{
    devkit::{flag_value, has_flag, parse_size, synth, Juice, Lifecycle, Notice},
    game_client::{self, GameShell},
    game_input::ClientInput,
    gamepad::Button,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, SoundBank, Template, Tint, View},
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

/// Held device state; nothing here is an edge (the plunger's release is what launches).
#[derive(Clone, Copy, Default)]
struct Held {
    left: bool,
    right: bool,
    plunger: bool,
}
/// The cue names a `--script` may use (`save` and `load` are always understood).
const CUES: [&str; 3] = ["left", "right", "plunger"];

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 9] = [
    synth::Preset::Shoot,
    synth::Preset::Click,
    synth::Preset::Zap,
    synth::Preset::Blip,
    synth::Preset::Thump,
    synth::Preset::Pickup,
    synth::Preset::PowerUp,
    synth::Preset::Error,
    synth::Preset::GameOver,
];
const S_LAUNCH: usize = 0;
const S_FLIP: usize = 1;
const S_BUMPER: usize = 2;
const S_POST: usize = 3;
const S_GEAR: usize = 4;
const S_LANE: usize = 5;
const S_MULT: usize = 6;
const S_LOST: usize = 7;
const S_OVER: usize = 8;

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

/// The meshes built once at startup.
struct Scene {
    sky: Vec<Mesh>,
    table: Vec<Mesh>,
    ball: Template,
    flipper: Template,
    gear: Template,
    lane_on: Template,
    lane_off: Template,
    plunger: Template,
}

fn build_scene() -> Scene {
    let mut sky = Template::new();
    sky.sky_dome(
        300.,
        |e| {
            let t = e.clamp(0., 1.).sqrt();
            [0.05 + 0.03 * (1. - t), 0.02, 0.12 + 0.10 * (1. - t)]
        },
        24,
        12,
    );
    let mut table = Template::new();
    // The playfield: a dark slab with a faint grid, walls with glowing rims, the glass's shadow is left out.
    table.quad_facing(
        [vec3(-3.2, 0., -5.2), vec3(3.2, 0., -5.2), vec3(3.2, 0., 5.3), vec3(-3.2, 0., 5.3)],
        Vec3::Y,
        [0.06, 0.05, 0.14],
        0.,
    );
    for i in -5..=6 {
        let x = i as f32 * 0.5;
        table.box_(vec3(x, 0.005, 0.), vec3(0.008, 0.002, 5.2), [0.25, 0.15, 0.5], 0.4);
    }
    for i in -10..=10 {
        let z = i as f32 * 0.5;
        table.box_(vec3(0., 0.005, z), vec3(3.2, 0.002, 0.008), [0.25, 0.15, 0.5], 0.4);
    }
    let rim = [0.2, 0.9, 1.0];
    let mut wall = |c: Vec3, half: Vec3, yaw: f32| {
        let mut t = Template::new();
        t.box_(Vec3::ZERO, half, [0.12, 0.10, 0.30], 0.05);
        t.box_(vec3(0., half.y + 0.01, 0.), vec3(half.x, 0.02, half.z * 0.6), rim, 0.9);
        table.append(&t.transformed(Mat4::from_translation(c) * Mat4::from_rotation_y(yaw)));
    };
    let h = 0.45;
    wall(vec3(-3.1, h, 0.), vec3(0.1, h, 6.), 0.);
    wall(vec3(3.05, h, 0.), vec3(0.1, h, 6.), 0.);
    wall(vec3(0., h, -5.1), vec3(3.2, h, 0.1), 0.);
    wall(vec3(RIGHT + 0.05, h, 0.35), vec3(0.05, h, 4.0), 0.);
    wall(vec3(2.38, h, -4.45), vec3(0.78, h, 0.08), -0.85);
    wall(vec3(-2.45, h, -4.55), vec3(0.75, h, 0.08), 0.78);
    wall(vec3(-2.25, h, 3.3), vec3(0.85, h, 0.08), -0.62);
    wall(vec3(1.7, h, 3.3), vec3(0.85, h, 0.08), 0.62);
    wall(vec3(LANE_X, h, PLUNGER_Z + 0.25), vec3(0.3, h, 0.1), 0.);
    // Bumpers: a dark post, a glowing cap and a ring.
    for (x, z) in BUMPERS {
        table.cylinder(vec3(x, 0., z), BUMPER_R, 0.6, [0.9, 0.15, 0.5], 0.15, 20);
        table.cylinder(vec3(x, 0.6, z), BUMPER_R * 0.75, 0.05, [1., 0.6, 0.9], 0.9, 20);
        table.ring(vec3(x, 0.02, z), BUMPER_R + 0.05, BUMPER_R + 0.12, [1., 0.3, 0.7], 0.9, 24);
    }
    for (x, z) in POSTS {
        table.cylinder(vec3(x, 0., z), POST_R, 0.5, [0.3, 0.9, 1.], 0.5, 12);
    }
    // The drain and the flipper pivots.
    table.box_(vec3(-0.25, 0.01, 5.35), vec3(0.6, 0.01, 0.04), [1., 0.2, 0.2], 0.8);
    for pivot in clockwork_pinball::PIVOTS {
        table.cylinder(vec3(pivot.0, 0., pivot.2), 0.16, 0.45, [0.9, 0.8, 0.2], 0.3, 12);
    }
    let mut ball = Template::new();
    ball.ball(Vec3::ZERO, Vec3::splat(BALL_R), [0.85, 0.88, 0.95], 0.35, 16, 10);
    ball.ball(vec3(0., BALL_R * 0.4, 0.), Vec3::splat(BALL_R * 0.55), [1., 1., 1.], 0.9, 10, 6);
    let mut flipper = Template::new();
    flipper.box_(Vec3::ZERO, vec3(FLIPPER_LEN * 0.5, 0.2, FLIPPER_HALF_THICK), [1., 0.55, 0.15], 0.35);
    flipper.box_(vec3(0., 0.21, 0.), vec3(FLIPPER_LEN * 0.5, 0.01, FLIPPER_HALF_THICK * 0.5), [1., 0.9, 0.5], 0.9);
    let mut gear = Template::new();
    gear.box_(Vec3::ZERO, vec3(GEAR_HALF, 0.2, 0.08), [0.9, 0.85, 0.3], 0.3);
    gear.cylinder(vec3(0., -0.2, 0.), 0.14, 0.4, [0.6, 0.5, 0.15], 0.2, 12);
    let disc = |lit: bool| {
        let mut t = Template::new();
        let (c, e) = if lit { ([0.4, 1., 0.5], 1.0) } else { ([0.1, 0.25, 0.15], 0.1) };
        t.cylinder(Vec3::ZERO, LANE_R * 0.7, 0.02, c, e, 16);
        t
    };
    let mut plunger = Template::new();
    plunger.box_(vec3(0., 0.15, 0.4), vec3(0.06, 0.05, 0.4), [0.8, 0.8, 0.85], 0.2);
    plunger.cylinder(vec3(0., 0., 0.8), 0.12, 0.3, [1., 0.3, 0.3], 0.5, 12);
    Scene {
        sky: sky.to_meshes(),
        table: table.to_meshes(),
        ball,
        flipper,
        gear,
        lane_on: disc(true),
        lane_off: disc(false),
        plunger,
    }
}

fn neon_look() -> Look {
    let mut look = Look::night();
    look.ambient_sky = [0.30, 0.25, 0.48];
    look.ambient_ground = [0.12, 0.08, 0.20];
    look.key_color = [1.0, 0.85, 1.0];
    look.rim_color = [0.3, 0.9, 1.0];
    look.fog_color = [0.04, 0.02, 0.10];
    look.fog_density = 0.004;
    look
}

/// Turn one simulation event into sound, particles, shake and text. Exhaustive on purpose: a new `Event` must be
/// handled (or explicitly ignored) here.
fn react(event: &Event, sounds: &mut SoundBank, fx: &mut Fx, juice: &mut Juice) {
    match event {
        Event::Launch { power } => {
            sounds.play(S_LAUNCH, 0.5 + 0.4 * power);
            juice.kick(1. + 2. * power);
        }
        Event::Flip { .. } => sounds.play(S_FLIP, 0.6),
        Event::Bumper { at, points } => {
            sounds.play(S_BUMPER, 0.8);
            fx.sparks(v3(*at), 22, 5., [1., 0.4, 0.8]);
            fx.ring(v3(*at) - vec3(0., 0.25, 0.), Vec3::Y, 0.3, 1.4, 0.35, [1., 0.5, 0.9]);
            fx.popup(v3(*at) + vec3(0., 0.6, 0.), format!("+{points}"), [1., 0.8, 0.95], 30.);
            juice.shake(0.25);
        }
        Event::Post { at, points } => {
            sounds.play(S_POST, 0.6);
            fx.sparks(v3(*at), 10, 3., [0.4, 0.9, 1.]);
            fx.popup(v3(*at) + vec3(0., 0.5, 0.), format!("+{points}"), [0.6, 0.95, 1.], 24.);
        }
        Event::Gear { at, points } => {
            sounds.play(S_GEAR, 0.5);
            fx.sparks(v3(*at), 8, 3., [1., 0.9, 0.4]);
            fx.popup(v3(*at) + vec3(0., 0.5, 0.), format!("+{points}"), [1., 0.95, 0.5], 22.);
        }
        Event::Lane { index, completed } => {
            sounds.play(S_LANE, 0.7);
            let (x, z) = LANES[*index];
            fx.ring(vec3(x, 0.05, z), Vec3::Y, 0.2, 1., 0.5, [0.4, 1., 0.5]);
            if *completed {
                fx.banner("LANES COMPLETE", "multiplier up", [0.5, 1., 0.6]);
            }
        }
        Event::Multiplier(m) => {
            sounds.play(S_MULT, 0.8);
            juice.flash([0.4, 1., 0.6], 0.3);
            fx.banner(format!("x{m} MULTIPLIER"), "", [1., 0.85, 0.3]);
        }
        Event::BallLost { balls_left } => {
            sounds.play(S_LOST, 0.7);
            juice.flash([1., 0.2, 0.2], 0.3);
            fx.banner("BALL LOST", format!("{balls_left} left"), [1., 0.4, 0.4]);
        }
        Event::GameOver { score } => {
            sounds.play(S_OVER, 0.9);
            fx.banner("GAME OVER", format!("{score} points: press R to play again"), [1., 0.85, 0.3]);
        }
    }
}

/// Show the outcome of a quick save or load.
fn announce(fx: &mut Fx, notice: &Notice) {
    fx.banners.clear();
    fx.banner(notice.title, notice.detail.clone(), notice.color);
}

fn body_matrix(sim: &Sim, b: Body, lift: f32) -> Mat4 {
    let p = sim.phys.position(b);
    Mat4::from_translation(vec3(p.0, lift, p.2)) * Mat4::from_rotation_y(sim.phys.yaw(b))
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
    let look = neon_look();
    let scene = build_scene();
    let mut sounds = SoundBank::start(life.options.silent(), 0.9, 0.6, render_audio).await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let (mut fx, mut juice) = (Fx::new(seed), Juice::default());
    let mut sim = Sim::new(seed);
    life.load_flag_or_exit(&mut sim);
    let vignette = hud::make_vignette();
    let (mut world, mut alpha, mut add) = (Batch::new(), Batch::new(), Batch::new());
    let mut best = 0u64;

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
            Some(s) => Held { left: s.held("left"), right: s.held("right"), plunger: s.held("plunger") },
            None if accepting => Held {
                left: input.down(KeyCode::A)
                    || input.down(KeyCode::Left)
                    || pad.down(Button::LeftTrigger)
                    || pad.down(Button::LeftTrigger2),
                right: input.down(KeyCode::D)
                    || input.down(KeyCode::Right)
                    || pad.down(Button::RightTrigger)
                    || pad.down(Button::RightTrigger2),
                plunger: input.down(KeyCode::Space) || input.down(KeyCode::Down) || pad.down(Button::South),
            },
            None => Held::default(),
        };
        life.feed(held, 0, [0., 0.]);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let notice = if sim.phase == Phase::GameOver {
                Notice::refused("the game is over")
            } else {
                life.quick_save(&sim, &format!("Quick save, {} points", sim.score))
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
        if input.pressed(KeyCode::R) || (sim.phase == Phase::GameOver && pad.pressed(Button::South)) {
            best = best.max(sim.score);
            sim = Sim::new(life.restart_seed());
            fx.clear();
            life.reset_input();
        }

        // 2. Simulation: whole fixed ticks, each with exactly one Input.
        let playing = !shell.paused;
        for _ in 0..life.ticks(dt, juice.time_scale(None), playing) {
            let tick = life.take_tick();
            let step_input = if autoplayer {
                autoplay(&sim)
            } else {
                Input { left: tick.held.left, right: tick.held.right, plunger: tick.held.plunger }
            };
            sim.step(&step_input);
            for event in sim.drain_events() {
                react(&event, &mut sounds, &mut fx, &mut juice);
            }
        }
        if playing {
            juice.update(dt);
            fx.update(dt);
        }
        best = best.max(sim.score);

        // 3. Camera: a fixed view down the table from the flipper end, with a little shake.
        let (shake, roll) = juice.camera_shake();
        let eye = vec3(-0.3, 11.6, 9.9) + vec3(shake.0, shake.1, shake.2);
        let mut view = View::first_person(eye, 0., -0.93);
        view.fov = 41f32.to_radians();
        view.roll = roll * 0.3;

        // 4. Draw: sky, table, bodies, translucent effects, additive effects, then the 2D layer.
        clear_background(look.clear_color());
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for mesh in &scene.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera(0.1, 200.));
        materials.set_scene(&look, view.eye, time, 0.5 + 0.5 * (time * 3.).sin());
        world.clear();
        alpha.clear();
        add.clear();
        let bp = sim.ball_pos();
        if sim.phase != Phase::GameOver || sim.ball_pos().1 > -0.5 {
            world.add(&scene.ball, Mat4::from_translation(vec3(bp.0, bp.1, bp.2)), Tint::NONE);
        }
        for (body, _) in &sim.flippers {
            world.add(&scene.flipper, body_matrix(&sim, *body, 0.2), Tint::NONE);
        }
        world.add(&scene.gear, body_matrix(&sim, sim.gear, 0.2), Tint::NONE);
        for (i, (x, z)) in LANES.iter().enumerate() {
            let t = if sim.lanes[i] { &scene.lane_on } else { &scene.lane_off };
            world.add(t, Mat4::from_translation(vec3(*x, 0.01, *z)), Tint::NONE);
        }
        // The plunger slides back as it is pulled.
        let pull = if sim.phase == Phase::Waiting { sim.plunger * 0.7 } else { 0. };
        world.add(&scene.plunger, Mat4::from_translation(vec3(LANE_X, 0., PLUNGER_Z + 0.2 + pull)), Tint::NONE);
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        gl_use_material(&materials.world);
        for mesh in &scene.table {
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
        hud::overlay(&vignette, Color::new(0.04, 0., 0.1, 0.45));
        if juice.flash > 0.01 {
            draw_rectangle(0., 0., screen_width(), screen_height(), hud::col(juice.flash_color, juice.flash * 0.4));
        }
        hud::draw_popups(&fx.popups, &view, ui);
        hud::panel(20. * ui, 16. * ui, 240. * ui, 100. * ui, 12. * ui, Color::new(0.03, 0.01, 0.1, 0.65));
        hud::text_outlined("SCORE", 34. * ui, 38. * ui, 16. * ui, hud::col([0.3, 0.9, 1.], 1.));
        hud::text_outlined(&hud::commas(sim.score), 34. * ui, 74. * ui, 36. * ui, WHITE);
        hud::text_outlined(
            &format!("BEST {}", hud::commas(best)),
            34. * ui,
            102. * ui,
            15. * ui,
            hud::col([0.8, 0.75, 0.95], 1.),
        );
        for i in 0..sim.balls_left {
            draw_circle(
                screen_width() - (40. + 30. * i as f32) * ui,
                36. * ui,
                10. * ui,
                hud::col([0.85, 0.88, 0.95], 1.),
            );
        }
        if sim.multiplier > 1 {
            hud::text_right(
                &format!("x{}", sim.multiplier),
                screen_width() - 28. * ui,
                92. * ui,
                40. * ui,
                hud::col([1., 0.85, 0.3], 1.),
            );
        }
        if sim.phase == Phase::Waiting {
            hud::bar(
                screen_width() - 230. * ui,
                screen_height() - 50. * ui,
                200. * ui,
                14. * ui,
                sim.plunger,
                hud::col([1., 0.4, 0.4], 1.),
            );
            hud::text_right(
                "hold SPACE, release to plunge",
                screen_width() - 28. * ui,
                screen_height() - 62. * ui,
                16. * ui,
                WHITE,
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
