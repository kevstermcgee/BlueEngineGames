//! Tumble Maze: the window, renderer, sound and input around the simulation in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "right:0-90,down:90-150"   drive the human input path from a cue script (left right up down)
//!   --autoplay   a simple built-in player plays (for captures and demos)   --level N   start on level N (0-2)
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved game / where F5 saves (default: next to the exe)
//! Keyboard: WASD or the arrows tilt the board, R restarts, F5/F9 quick save and load.
//! Controller: the left stick tilts (analog), A continues after the last level.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use macroquad::prelude::*;
use tumble_maze::{autoplay, Event, Grid, Input, Phase, Sim, BALL_R, CLEAR_PAUSE, GOAL_R, HOLE_R, LEVELS, WALL_HEIGHT};
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

/// Held device state: how far each way the player is tilting (analog from a stick, -1/0/1 from keys).
#[derive(Clone, Copy, Default)]
struct Held {
    x: f32,
    z: f32,
}
/// The cue names a `--script` may use (`save` and `load` are always understood).
const CUES: [&str; 4] = ["left", "right", "up", "down"];

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 5] = [
    synth::Preset::Thump,
    synth::Preset::Pickup,
    synth::Preset::Error,
    synth::Preset::PowerUp,
    synth::Preset::GameOver,
];
const S_BUMP: usize = 0;
const S_STAR: usize = 1;
const S_FELL: usize = 2;
const S_CLEAR: usize = 3;
const S_DONE: usize = 4;

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
    /// One static board (floor, walls, holes, pads) per level, in the flat board frame.
    boards: Vec<Template>,
    marble: Template,
    star: Template,
    bars: Vec<Vec<Template>>,
}

fn build_board(level: usize) -> Template {
    let grid = Grid::parse(LEVELS[level].map);
    let (hw, hh) = (grid.w as f32 / 2., grid.h as f32 / 2.);
    let mut t = Template::new();
    // A slab of polished wood under everything, with a faint cell grid on it.
    t.box_(vec3(0., -0.3, 0.), vec3(hw + 0.6, 0.3, hh + 0.6), [0.36, 0.22, 0.12], 0.02);
    t.box_(vec3(0., -0.004, 0.), vec3(hw, 0.004, hh), [0.23, 0.17, 0.12], 0.02);
    for x in 0..=grid.w {
        t.box_(vec3(x as f32 - hw, 0.002, 0.), vec3(0.008, 0.001, hh), [0.17, 0.12, 0.08], 0.);
    }
    for z in 0..=grid.h {
        t.box_(vec3(0., 0.002, z as f32 - hh), vec3(hw, 0.001, 0.008), [0.17, 0.12, 0.08], 0.);
    }
    for y in 0..grid.h {
        let mut x = 0;
        while x < grid.w {
            if !grid.wall((x, y)) {
                x += 1;
                continue;
            }
            let start = x;
            while x < grid.w && grid.wall((x, y)) {
                x += 1;
            }
            let len = (x - start) as f32;
            let c = vec3(start as f32 + len / 2. - hw, WALL_HEIGHT / 2., y as f32 + 0.5 - hh);
            t.box_top(c, vec3(len / 2., WALL_HEIGHT / 2., 0.5), [0.30, 0.45, 0.62], [0.55, 0.72, 0.85], 0.25);
        }
    }
    for h in &grid.holes {
        let c = grid.centre(*h);
        t.cylinder(vec3(c.0, 0.006, c.2), HOLE_R + 0.06, 0.004, [0.02, 0.01, 0.02], 0., 24);
        t.ring(vec3(c.0, 0.012, c.2), HOLE_R + 0.06, HOLE_R + 0.14, [1., 0.25, 0.2], 0.8, 24);
    }
    let s = grid.centre(grid.start);
    t.cylinder(vec3(s.0, 0.004, s.2), 0.42, 0.004, [0.3, 0.7, 0.4], 0.35, 24);
    let g = grid.centre(grid.goal);
    t.cylinder(vec3(g.0, 0.004, g.2), GOAL_R + 0.08, 0.006, [1., 0.85, 0.3], 0.9, 24);
    t.ring(vec3(g.0, 0.012, g.2), GOAL_R + 0.1, GOAL_R + 0.2, [1., 0.95, 0.6], 1., 24);
    t
}

fn build_scene() -> Scene {
    let mut sky = Template::new();
    sky.sky_dome(
        300.,
        |e| {
            let t = e.clamp(0., 1.).sqrt();
            [0.10 + 0.05 * (1. - t), 0.14 + 0.06 * (1. - t), 0.22 + 0.12 * (1. - t)]
        },
        24,
        12,
    );
    let mut marble = Template::new();
    marble.ball(Vec3::ZERO, Vec3::splat(BALL_R), [0.80, 0.84, 0.92], 0.25, 18, 12);
    marble.ball(vec3(0., BALL_R * 0.45, -BALL_R * 0.2), Vec3::splat(BALL_R * 0.5), [1., 1., 1.], 0.85, 10, 6);
    let mut star = Template::new();
    star.ball(Vec3::ZERO, vec3(0.16, 0.16, 0.16), [1., 0.85, 0.25], 1., 10, 6);
    star.box_(Vec3::ZERO, vec3(0.26, 0.03, 0.03), [1., 0.95, 0.5], 1.);
    star.box_(Vec3::ZERO, vec3(0.03, 0.03, 0.26), [1., 0.95, 0.5], 1.);
    let bars = LEVELS
        .iter()
        .map(|l| {
            l.movers
                .iter()
                .map(|m| {
                    let mut t = Template::new();
                    let half = vec3(m.half.0, WALL_HEIGHT * 0.5, m.half.1);
                    t.box_top(Vec3::ZERO, half, [0.75, 0.25, 0.2], [1., 0.5, 0.35], 0.5);
                    t
                })
                .collect()
        })
        .collect();
    Scene { sky: sky.to_meshes(), boards: (0..LEVELS.len()).map(build_board).collect(), marble, star, bars }
}

fn look() -> Look {
    let mut look = Look::night();
    look.ambient_sky = [0.55, 0.55, 0.65];
    look.ambient_ground = [0.30, 0.22, 0.18];
    look.key_color = [1.0, 0.92, 0.80];
    look.rim_color = [0.5, 0.7, 1.0];
    look.fog_color = [0.10, 0.14, 0.22];
    look.fog_density = 0.002;
    look
}

/// The board's tilt as a transform of the flat frame the physics lives in.
fn tilt_matrix(tilt: (f32, f32)) -> Mat4 {
    Mat4::from_rotation_x(tilt.1) * Mat4::from_rotation_z(-tilt.0)
}

/// Turn one simulation event into sound, particles, shake and text. Exhaustive on purpose: a new `Event` must be
/// handled (or explicitly ignored) here.
fn react(event: &Event, m: Mat4, sounds: &mut SoundBank, fx: &mut Fx, juice: &mut Juice) {
    match event {
        Event::Bump { speed } => sounds.play(S_BUMP, (0.15 + 0.12 * speed).min(0.7)),
        Event::Star { at, total } => {
            sounds.play(S_STAR, 0.8);
            let p = m.transform_point3(v3(*at) + vec3(0., 0.3, 0.));
            fx.sparks(p, 18, 3., [1., 0.85, 0.3]);
            fx.popup(p + vec3(0., 0.5, 0.), format!("{total}/3"), [1., 0.9, 0.4], 26.);
        }
        Event::Fell { at } => {
            sounds.play(S_FELL, 0.8);
            let p = m.transform_point3(v3(*at));
            fx.sparks(p, 24, 2.5, [1., 0.3, 0.25]);
            juice.flash([1., 0.2, 0.2], 0.3);
            juice.shake(0.3);
            fx.popup(p + vec3(0., 0.6, 0.), "+5s", [1., 0.4, 0.4], 30.);
        }
        Event::Cleared { level, time, points } => {
            sounds.play(S_CLEAR, 0.9);
            juice.flash([1., 0.9, 0.4], 0.35);
            fx.banner(
                format!("{} CLEARED", LEVELS[*level].name.to_uppercase()),
                format!("{time:.1}s  +{points}"),
                [1., 0.9, 0.4],
            );
        }
        Event::AllDone { score } => {
            sounds.play(S_DONE, 0.9);
            fx.banner("ALL MAZES SOLVED", format!("{score} points: press R to play again"), [0.6, 1., 0.7]);
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
    let first_level: usize = flag_value(&args, "--level").and_then(|v| v.parse().ok()).unwrap_or(0);

    let materials = Materials::load().expect("the materials failed to compile");
    let look = look();
    let scene = build_scene();
    let mut sounds = SoundBank::start(life.options.silent(), 0.9, 0.6, render_audio).await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let (mut fx, mut juice) = (Fx::new(seed), Juice::default());
    let mut sim = Sim::at(first_level);
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
        let axis = |neg: bool, pos: bool| pos as i32 as f32 - neg as i32 as f32;
        let held = match life.script() {
            Some(s) => Held { x: axis(s.held("left"), s.held("right")), z: axis(s.held("up"), s.held("down")) },
            None if accepting => {
                let key = |a: KeyCode, b: KeyCode| input.down(a) || input.down(b);
                let x = axis(key(KeyCode::A, KeyCode::Left), key(KeyCode::D, KeyCode::Right)) + pad.left_stick[0];
                // The stick's Y is positive up, and tilting "up" raises the far edge: the marble rolls toward -z.
                let z = axis(key(KeyCode::W, KeyCode::Up), key(KeyCode::S, KeyCode::Down)) - pad.left_stick[1];
                Held { x: x.clamp(-1., 1.), z: z.clamp(-1., 1.) }
            }
            None => Held::default(),
        };
        life.feed(held, 0, [0., 0.]);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let notice = if sim.phase == Phase::Done {
                Notice::refused("the game is over")
            } else {
                life.quick_save(&sim, &format!("Quick save, {}", LEVELS[sim.level].name))
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
        if input.pressed(KeyCode::R) || (sim.phase == Phase::Done && pad.pressed(Button::South)) {
            best = best.max(sim.score);
            sim = Sim::at(first_level);
            fx.clear();
            life.reset_input();
        }

        // 2. Simulation: whole fixed ticks, each with exactly one Input.
        let playing = !shell.paused;
        let m_before = tilt_matrix(sim.tilt);
        for _ in 0..life.ticks(dt, juice.time_scale(None), playing) {
            let tick = life.take_tick();
            let step_input = if autoplayer { autoplay(&sim) } else { Input { x: tick.held.x, z: tick.held.z } };
            sim.step(&step_input);
            let m = tilt_matrix(sim.tilt);
            for event in sim.drain_events() {
                react(
                    &event,
                    if matches!(event, Event::Cleared { .. }) { m_before } else { m },
                    &mut sounds,
                    &mut fx,
                    &mut juice,
                );
            }
        }
        if playing {
            juice.update(dt);
            fx.update(dt);
        }
        best = best.max(sim.score);

        // 3. Camera: above the board, looking at its centre from the near side, framed to the board's size.
        let (shake, roll) = juice.camera_shake();
        let span = sim.grid.w.max(sim.grid.h * 2) as f32 * 0.72 + 4.;
        let eye = vec3(0., span * 0.95, span * 0.28) + vec3(shake.0, shake.1, shake.2);
        let mut view = View::first_person(eye, 0., -(eye.y / eye.z).atan());
        view.fov = 45f32.to_radians();
        view.roll = roll * 0.3;

        // 4. Draw: sky, board, marble and bars (all carried by the tilt), effects, then the 2D layer.
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
        let m = tilt_matrix(sim.tilt);
        world.add(&scene.boards[sim.level], m, Tint::NONE);
        for (i, bar) in scene.bars[sim.level].iter().enumerate() {
            world.add(bar, m * Mat4::from_translation(v3(sim.mover_pose(i))), Tint::NONE);
        }
        for (i, got) in sim.stars_got.iter().enumerate() {
            if !got {
                let c = sim.grid.centre(sim.grid.stars[i]);
                let bob = 0.3 + 0.05 * (time * 3. + i as f32).sin();
                world.add(
                    &scene.star,
                    m * Mat4::from_translation(vec3(c.0, bob, c.2)) * Mat4::from_rotation_y(time * 1.5),
                    Tint::NONE,
                );
            }
        }
        let bp = sim.ball_pos();
        let roll_q = {
            let r = sim.phys.rotation(sim.ball);
            Quat::from_xyzw(r[0], r[1], r[2], r[3])
        };
        if bp.1 > -0.5 {
            world.add(&scene.marble, m * Mat4::from_translation(v3(bp)) * Mat4::from_quat(roll_q), Tint::NONE);
        }
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        gl_use_material(&materials.world);
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_default_material();
        set_default_camera();

        let ui = hud::ui_scale();
        hud::overlay(&vignette, Color::new(0.02, 0.04, 0.1, 0.4));
        if juice.flash > 0.01 {
            draw_rectangle(0., 0., screen_width(), screen_height(), hud::col(juice.flash_color, juice.flash * 0.4));
        }
        hud::draw_popups(&fx.popups, &view, ui);
        hud::panel(20. * ui, 16. * ui, 290. * ui, 118. * ui, 12. * ui, Color::new(0.02, 0.05, 0.12, 0.7));
        hud::text_outlined(
            &format!("LEVEL {}  {}", sim.level + 1, LEVELS[sim.level].name.to_uppercase()),
            34. * ui,
            40. * ui,
            16. * ui,
            hud::col([0.6, 0.85, 1.], 1.),
        );
        let over_par = sim.seconds() > LEVELS[sim.level].par_secs;
        let clock_color = if over_par { [1., 0.5, 0.4] } else { [1., 1., 1.] };
        hud::text_outlined(&format!("{:.1}s", sim.seconds()), 34. * ui, 78. * ui, 36. * ui, hud::col(clock_color, 1.));
        hud::text_outlined(
            &format!(
                "par {:.0}s   stars {}/{}   falls {}",
                LEVELS[sim.level].par_secs,
                sim.stars(),
                sim.stars_got.len(),
                sim.falls
            ),
            34. * ui,
            112. * ui,
            15. * ui,
            hud::col([0.8, 0.85, 0.95], 1.),
        );
        hud::text_right(&hud::commas(sim.score), screen_width() - 28. * ui, 48. * ui, 36. * ui, WHITE);
        hud::text_right(
            &format!("BEST {}", hud::commas(best)),
            screen_width() - 28. * ui,
            76. * ui,
            15. * ui,
            hud::col([0.8, 0.85, 0.95], 1.),
        );
        if let Phase::Cleared(n) = sim.phase {
            hud::bar(
                screen_width() / 2. - 100. * ui,
                screen_height() - 50. * ui,
                200. * ui,
                10. * ui,
                n as f32 / CLEAR_PAUSE as f32,
                hud::col([1., 0.9, 0.4], 1.),
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
