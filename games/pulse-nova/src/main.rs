//! Pulse Nova: the window, renderer, sound and input around the simulation in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window):
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "fwd:0-200,look:0.01@0-100,jump@60"      drive the human input path from a cue script
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved game / where F5 saves (default: next to the exe)
//! F5 saves the run to the `quick` slot and F9 loads it. `--script` accepts `save@N` and `load@N` cues too.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use macroquad::prelude::*;
use std::time::Instant;
use vesper3d::math::V;
use vesper3d::viewer::{
    devkit::{
        flag_value, has_flag, parse_size, snapshot, synth, CapturePlan, FixedStepper, InputAccumulator, Juice,
        PerfReport, SaveSlots, Source, Timeline, QUICK_SLOT, TICK,
    },
    game_client::{self, GameShell},
    game_input::ClientInput,
    gamepad::Button,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, SoundBank, Template, Tint, View},
};
use pulse_nova::{EnemyKind, Event, Input, Sim, ARENA_HALF, MAX_HEALTH, TOTAL_WAVES};

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

/// Held device state; press edges travel separately as bits.
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
}
const JUMP: u32 = 1;
const FIRE: u32 = 2;
const DASH: u32 = 4;

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 11] = [
    synth::Preset::Shoot,
    synth::Preset::Hit,
    synth::Preset::Explosion,
    synth::Preset::Zap,
    synth::Preset::Whoosh,
    synth::Preset::PowerUp,
    synth::Preset::Coin,
    synth::Preset::Hurt,
    synth::Preset::Jump,
    synth::Preset::Success,
    synth::Preset::GameOver,
];

fn sound(preset: synth::Preset) -> usize {
    SOUNDS.iter().position(|p| *p == preset).unwrap_or(0)
}

fn render_audio() -> Rendered {
    Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 7), synth::RATE)).collect())
            .collect(),
        stems: Vec::new(),
    }
}

/// The static meshes and dynamic entity templates built once at startup.
struct Scene {
    sky: Vec<Mesh>,
    arena: Vec<Mesh>,
    scout: Template,
    sentinel: Template,
    goliath: Template,
    player_proj: Template,
    enemy_proj: Template,
    shard: Template,
    pad: Template,
}

fn build_scene() -> Scene {
    let mut sky = Template::new();
    sky.sky_dome(
        220.,
        |e| {
            let t = e.clamp(0., 1.).sqrt();
            [
                0.04 + (0.18 - 0.04) * (1. - t),
                0.03 + (0.10 - 0.03) * (1. - t),
                0.12 + (0.35 - 0.12) * (1. - t),
            ]
        },
        32,
        16,
    );

    let mut arena = Template::new();
    // Main arena floor
    arena.box_top(
        vec3(0., -0.5, 0.),
        vec3(ARENA_HALF, 0.5, ARENA_HALF),
        [0.08, 0.09, 0.15],
        [0.12, 0.15, 0.24],
        0.05,
    );
    // Center dais
    arena.box_top(
        vec3(0., 0.5, 0.),
        vec3(3.5, 0.5, 3.5),
        [0.10, 0.13, 0.22],
        [0.18, 0.24, 0.38],
        0.2,
    );
    // Dais neon trim
    arena.box_(vec3(0., 1.02, 0.), vec3(3.55, 0.02, 3.55), [0.2, 0.8, 1.0], 0.7);

    // 4 tactical pillars
    for (cx, cz) in [(-7.0, -7.0), (7.0, -7.0), (-7.0, 7.0), (7.0, 7.0)] {
        arena.box_top(
            vec3(cx, 1.75, cz),
            vec3(0.8, 1.75, 0.8),
            [0.11, 0.12, 0.18],
            [0.16, 0.18, 0.26],
            0.1,
        );
        // Emissive vertical core strip
        arena.box_(vec3(cx, 1.75, cz), vec3(0.85, 1.4, 0.15), [0.8, 0.2, 1.0], 0.8);
        arena.box_(vec3(cx, 1.75, cz), vec3(0.15, 1.4, 0.85), [0.8, 0.2, 1.0], 0.8);
    }

    // Perimeter curbs & boundary energy rails
    for (center, half) in [
        (vec3(0., 0.35, ARENA_HALF - 0.25), vec3(ARENA_HALF, 0.35, 0.25)),
        (vec3(0., 0.35, -ARENA_HALF + 0.25), vec3(ARENA_HALF, 0.35, 0.25)),
        (vec3(ARENA_HALF - 0.25, 0.35, 0.), vec3(0.25, 0.35, ARENA_HALF)),
        (vec3(-ARENA_HALF + 0.25, 0.35, 0.), vec3(0.25, 0.35, ARENA_HALF)),
    ] {
        arena.box_top(center, half, [0.09, 0.11, 0.18], [0.15, 0.22, 0.35], 0.2);
        arena.box_(center + vec3(0., half.y + 0.02, 0.), vec3(half.x, 0.02, half.z), [0.1, 0.7, 1.0], 0.8);
    }

    // Entity templates
    let mut scout = Template::new();
    scout.ball(Vec3::ZERO, vec3(0.35, 0.22, 0.45), [0.2, 0.85, 1.0], 0.9, 10, 6);
    scout.box_(vec3(0., 0., -0.2), vec3(0.55, 0.05, 0.25), [0.1, 0.5, 0.9], 0.5);

    let mut sentinel = Template::new();
    sentinel.ball(Vec3::ZERO, Vec3::splat(0.48), [1.0, 0.25, 0.45], 0.95, 12, 8);
    sentinel.box_(vec3(0., 0., 0.), vec3(0.7, 0.08, 0.7), [0.8, 0.15, 0.3], 0.6);

    let mut goliath = Template::new();
    goliath.ball(Vec3::ZERO, Vec3::splat(1.15), [0.95, 0.7, 0.2], 0.85, 16, 12);
    goliath.box_(vec3(0., 0., 0.), vec3(1.3, 0.25, 1.3), [0.6, 0.4, 0.1], 0.4);

    let mut player_proj = Template::new();
    player_proj.ball(Vec3::ZERO, Vec3::splat(0.2), [0.4, 0.9, 1.0], 1.0, 8, 6);

    let mut enemy_proj = Template::new();
    enemy_proj.ball(Vec3::ZERO, Vec3::splat(0.24), [1.0, 0.2, 0.3], 1.0, 8, 6);

    let mut shard = Template::new();
    shard.ball(Vec3::ZERO, vec3(0.22, 0.38, 0.22), [0.3, 1.0, 0.6], 0.95, 8, 6);

    let mut pad = Template::new();
    pad.box_(vec3(0., 0.03, 0.), vec3(0.8, 0.03, 0.8), [0.15, 0.85, 0.9], 0.9);

    Scene {
        sky: sky.to_meshes(),
        arena: arena.to_meshes(),
        scout,
        sentinel,
        goliath,
        player_proj,
        enemy_proj,
        shard,
        pad,
    }
}

/// Turn one simulation event into sound, particles, shake and text.
fn react(event: &Event, sounds: &mut SoundBank, fx: &mut Fx, juice: &mut Juice) {
    let v = |p: V| vec3(p.0, p.1, p.2);
    match event {
        Event::Fired { at, dir } => {
            sounds.play(sound(synth::Preset::Shoot), 0.75);
            fx.sparks(v(*at), 8, 5., [0.4, 0.9, 1.0]);
            let _ = dir;
            juice.kick(1.4);
        }
        Event::Dashed { at } => {
            sounds.play(sound(synth::Preset::Whoosh), 0.9);
            fx.dust(v(*at), 18, 6., [0.2, 0.8, 1.0]);
            fx.ring(v(*at), Vec3::Y, 0.4, 3.2, 0.35, [0.3, 0.85, 1.0]);
            juice.flash([0.15, 0.55, 1.0], 0.16);
        }
        Event::EnemyHit { at, fatal } => {
            sounds.play(sound(synth::Preset::Hit), 0.85);
            fx.sparks(v(*at), if *fatal { 22 } else { 8 }, 7., [1.0, 0.85, 0.3]);
        }
        Event::EnemyExploded { at, radius } => {
            sounds.play(sound(synth::Preset::Explosion), 1.0);
            fx.sparks(v(*at), 40, 13., [1.0, 0.55, 0.2]);
            fx.ring(v(*at), Vec3::Y, 0.3, *radius * 1.5, 0.45, [1.0, 0.6, 0.2]);
            juice.shake(0.65);
            juice.stop(0.04);
        }
        Event::PlayerHit { health } => {
            sounds.play(sound(synth::Preset::Hurt), 1.0);
            juice.shake(0.85);
            juice.flash([1.0, 0.1, 0.1], 0.35);
            let _ = health;
        }
        Event::ShardCollected { at, score } => {
            sounds.play(sound(synth::Preset::Coin), 0.85);
            fx.sparks(v(*at), 18, 5., [0.3, 1.0, 0.6]);
            fx.ring(v(*at), Vec3::Y, 0.2, 1.8, 0.4, [0.4, 1.0, 0.7]);
            fx.popup(v(*at) + vec3(0., 0.5, 0.), format!("+SHARD ({score})"), [0.4, 1.0, 0.7], 28.);
            juice.kick(1.5);
        }
        Event::JumpPadUsed { at } => {
            sounds.play(sound(synth::Preset::PowerUp), 0.95);
            fx.ring(v(*at), Vec3::Y, 0.3, 2.5, 0.45, [0.2, 0.9, 1.0]);
            fx.sparks(v(*at), 22, 9., [0.2, 0.9, 1.0]);
            juice.kick(2.5);
        }
        Event::Jumped => sounds.play(sound(synth::Preset::Jump), 0.5),
        Event::Landed => {
            sounds.play(sound(synth::Preset::Land), 0.6);
            juice.land(0.4);
        }
        Event::WaveCleared { wave } => {
            sounds.play(sound(synth::Preset::Success), 1.0);
            fx.banners.clear();
            fx.banner(&format!("WAVE {wave} CLEARED"), "PREPARE FOR INCOMING SURGE", [0.3, 1.0, 0.8]);
            juice.flash([0.2, 1.0, 0.6], 0.45);
        }
        Event::WaveStarted { wave } => {
            fx.banners.clear();
            fx.banner(&format!("WAVE {wave} / {TOTAL_WAVES}"), "HOSTILE SWARMS INBOUND", [1.0, 0.85, 0.25]);
        }
        Event::Won => {
            sounds.play(sound(synth::Preset::Success), 1.0);
            fx.banners.clear();
            fx.banner("MISSION COMPLETE", "SURVIVED THE NOVA SURGE! Press R to play again", [0.3, 1.0, 0.9]);
            juice.flash([0.3, 1.0, 0.8], 0.7);
        }
        Event::Lost => {
            sounds.play(sound(synth::Preset::GameOver), 1.0);
            fx.banners.clear();
            fx.banner("CRITICAL FAILURE", "SHIELDS OVERWHELMED. Press R to restart", [1.0, 0.3, 0.4]);
            juice.shake(1.2);
            juice.flash([1.0, 0.1, 0.1], 0.6);
        }
    }
}

/// F5: write the run to the quick slot. Returns the banner to show.
fn quick_save(sim: &Sim, slots: &SaveSlots) -> (&'static str, String, [f32; 3]) {
    if sim.over {
        return ("NOT SAVED", "the run is over".into(), [1., 0.6, 0.3]);
    }
    match snapshot::save_to_slot(sim, slots, QUICK_SLOT, &format!("Quick save, score {}", sim.score)) {
        Ok(()) => ("GAME SAVED", "F9 loads it".into(), [0.4, 1., 0.6]),
        Err(e) => ("SAVE FAILED", e.to_string(), [1., 0.4, 0.4]),
    }
}

/// F9: resume the quick slot. A damaged save falls back to the previous good one.
fn quick_load(sim: &mut Sim, slots: &SaveSlots) -> (&'static str, String, [f32; 3]) {
    match snapshot::load_from_slot(sim, slots, QUICK_SLOT) {
        Ok((header, Source::Primary)) => ("GAME LOADED", header.label, [0.4, 0.9, 1.]),
        Ok((header, Source::Backup(_))) => ("LOADED PREVIOUS SAVE", header.label, [1., 0.8, 0.4]),
        Err(e) => ("LOAD FAILED", e.to_string(), [1., 0.4, 0.4]),
    }
}

/// Held device state and the press edges for this frame from a `--script`.
fn scripted(script: &Timeline, frame: u32) -> (Held, bool, bool, bool, [f32; 2]) {
    let mut held = Held::default();
    let mut look = [0.; 2];
    for cue in script.active(frame) {
        match cue.name.as_str() {
            "fwd" => held.forward += 1.,
            "back" => held.forward -= 1.,
            "right" => held.right += 1.,
            "left" => held.right -= 1.,
            "look" => {
                look[0] += cue.value(0);
                look[1] += cue.value(1);
            }
            _ => {}
        }
    }
    let jump = script.starting(frame).any(|c| c.name == "jump");
    let fire = script.starting(frame).any(|c| c.name == "fire") || script.active(frame).any(|c| c.name == "fire");
    let dash = script.starting(frame).any(|c| c.name == "dash");
    (held, jump, fire, dash, look)
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identity = identity();
    let plan = CapturePlan::from_args(&args).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    if let Some(plan) = &plan {
        if let Err(e) = plan.create_dir() {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    let script = flag_value(&args, "--script").map(|text| {
        Timeline::parse(text, &["fwd", "back", "left", "right", "jump", "fire", "dash", "look", "save", "load"]).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        })
    });
    let seed = flag_value(&args, "--seed").and_then(|s| s.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
    });
    let unattended = plan.is_some() || script.is_some();

    let materials = Materials::load().expect("the materials failed to compile");
    let look = Look::dusk();
    let scene = build_scene();
    let mut sounds = SoundBank::start(has_flag(&args, "--mute") || unattended, 0.9, 0.6, render_audio).await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let (mut acc, mut stepper) = (InputAccumulator::<Held>::new(), FixedStepper::new());
    let (mut fx, mut juice) = (Fx::new(seed), Juice::default());
    let mut sim = Sim::new(seed);
    let slots = flag_value(&args, "--save-dir").map_or_else(SaveSlots::beside_exe, SaveSlots::new);
    if let Some(target) = flag_value(&args, "--load") {
        match snapshot::load_target(&mut sim, &slots, target) {
            Ok((header, _)) => println!("loaded: {}", header.label),
            Err(e) => {
                eprintln!("--load {target}: {e}");
                std::process::exit(2);
            }
        }
    }
    let mut perf = PerfReport::new(30);
    let vignette = hud::make_vignette();
    let (mut world, mut alpha, mut add) = (Batch::new(), Batch::new(), Batch::new());
    let mut frame: u32 = 0;
    let mut time = 0.;

    loop {
        let began = Instant::now();
        input.begin_frame_with_keyboard(&mut shell, !sim.over, unattended || platform::focused(), platform::keyboard());
        let dt = if unattended { TICK } else { input.frame_seconds() };
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        time += dt;

        // 1. Devices in
        let (held, jump, fire, dash, look_delta) = match &script {
            Some(script) => scripted(script, frame),
            None => {
                let m = input.movement(&shell);
                let is_fire = is_mouse_button_pressed(MouseButton::Left)
                    || is_mouse_button_down(MouseButton::Left)
                    || input.down(KeyCode::E)
                    || input.pressed(KeyCode::E)
                    || input.gamepad().pressed(Button::RightTrigger2)
                    || input.gamepad().pressed(Button::West);

                let is_dash = is_mouse_button_pressed(MouseButton::Right)
                    || input.pressed(KeyCode::LeftShift)
                    || input.down(KeyCode::LeftShift)
                    || input.gamepad().pressed(Button::LeftTrigger2)
                    || input.gamepad().pressed(Button::East);

                (Held { forward: m.forward, right: m.right }, m.jump, is_fire, is_dash, input.look_delta_with(&shell, dt))
            }
        };

        let mut edges = 0;
        if jump { edges |= JUMP; }
        if fire { edges |= FIRE; }
        if dash { edges |= DASH; }
        acc.feed(held, edges, look_delta);

        let (save, load) = match &script {
            Some(script) => (
                script.starting(frame).any(|c| c.name == "save"),
                script.starting(frame).any(|c| c.name == "load"),
            ),
            None => (input.pressed(KeyCode::F5), input.pressed(KeyCode::F9)),
        };
        if save {
            let (title, detail, color) = quick_save(&sim, &slots);
            fx.banners.clear();
            fx.banner(title, &detail, color);
        }
        if load {
            let (title, detail, color) = quick_load(&mut sim, &slots);
            fx.banners.clear();
            fx.banner(title, &detail, color);
            acc.clear();
            stepper = FixedStepper::new();
            juice = Juice::default();
        }
        if sim.over && (input.pressed(KeyCode::R) || input.pressed(KeyCode::Enter)) {
            sim = Sim::new(seed.wrapping_add(u64::from(frame)));
            fx.clear();
            acc.clear();
        }

        // 2. Simulation ticks
        let playing = !shell.paused;
        let scale = juice.time_scale(None);
        if playing {
            for _ in 0..stepper.advance(dt * scale) {
                let tick = acc.take_tick();
                sim.step(&Input {
                    forward: tick.held.forward,
                    right: tick.held.right,
                    look: tick.look,
                    jump: tick.pressed(JUMP),
                    fire: tick.pressed(FIRE),
                    dash: tick.pressed(DASH),
                });
                for event in sim.drain_events() {
                    react(&event, &mut sounds, &mut fx, &mut juice);
                }
            }
            juice.update(dt);
            fx.update(dt);
        } else {
            acc.clear();
        }

        // 3. Camera
        let pending = acc.pending_look();
        let (shake, roll) = juice.camera_shake();
        let eye = vec3(sim.player.position.0, sim.player.position.1 + juice.dip + 0.8, sim.player.position.2);
        let mut view = View::first_person(
            eye + vec3(shake.0, shake.1, shake.2),
            sim.player.yaw + pending[0],
            (sim.player.pitch - pending[1]).clamp(-1.5, 1.5),
        );
        view.roll = roll;
        view.fov = (72. + juice.fov_kick).to_radians();

        // 4. Draw
        clear_background(look.clear_color());
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for mesh in &scene.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera(0.05, 400.));
        materials.set_scene(&look, view.eye, time, 0.5 + 0.5 * (time * 3.).sin());
        world.clear();
        alpha.clear();
        add.clear();

        // Jump pads
        for pad in &sim.jump_pads {
            world.add(&scene.pad, Mat4::from_translation(vec3(pad.pos.0, pad.pos.1, pad.pos.2)), Tint::NONE);
        }

        // Shards
        let shard_bob = (time * 3.).sin() * 0.12;
        for shard in &sim.shards {
            let at = vec3(shard.pos.0, shard.pos.1 + shard_bob, shard.pos.2);
            world.add(&scene.shard, Mat4::from_translation(at) * Mat4::from_rotation_y(time * 2.), Tint::NONE);
        }

        // Enemies
        for enemy in &sim.enemies {
            let rot = Mat4::from_rotation_y(enemy.vel.0.atan2(enemy.vel.2));
            let at = vec3(enemy.pos.0, enemy.pos.1, enemy.pos.2);
            let template = match enemy.kind {
                EnemyKind::Scout => &scene.scout,
                EnemyKind::Sentinel => &scene.sentinel,
                EnemyKind::Goliath => &scene.goliath,
            };
            world.add(template, Mat4::from_translation(at) * rot, Tint::NONE);
        }

        // Projectiles
        for proj in &sim.projectiles {
            let at = vec3(proj.pos.0, proj.pos.1, proj.pos.2);
            let template = if proj.from_player { &scene.player_proj } else { &scene.enemy_proj };
            world.add(template, Mat4::from_translation(at), Tint::NONE);
        }

        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        gl_use_material(&materials.world);
        for mesh in &scene.arena {
            draw_mesh(mesh);
        }
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_default_material();
        set_default_camera();

        // 2D HUD
        let ui = hud::ui_scale();
        let w = screen_width();
        let h = screen_height();

        hud::overlay(&vignette, Color::new(0.04, 0.02, 0.08, 0.45));
        if juice.flash > 0.01 {
            draw_rectangle(0., 0., w, h, hud::col(juice.flash_color, juice.flash * 0.4));
        }

        // Crosshair
        let recoil = (juice.fov_kick * 3.0).clamp(0., 15.);
        hud::crosshair(ui, recoil, Color::new(0.4, 0.9, 1.0, 0.85));

        // Shield / Health bar
        let hp_ratio = (sim.health / MAX_HEALTH).clamp(0., 1.);
        let hp_col = if hp_ratio > 0.5 {
            Color::new(0.2, 0.9, 0.5, 0.9)
        } else if hp_ratio > 0.25 {
            Color::new(1.0, 0.8, 0.2, 0.9)
        } else {
            Color::new(1.0, 0.2, 0.2, 0.9)
        };
        hud::panel(24. * ui, h - 80. * ui, 260. * ui, 50. * ui, 8. * ui, Color::new(0.04, 0.06, 0.12, 0.8));
        hud::text_outlined("SHIELD INTEGRITY", 36. * ui, h - 56. * ui, 14. * ui, hud::col([0.6, 0.85, 1.0], 1.));
        hud::bar(36. * ui, h - 48. * ui, 236. * ui, 10. * ui, hp_ratio, hp_col);

        // Dash ready prompt
        let dash_ready = sim.dash_cooldown == 0;
        let dash_col = if dash_ready { Color::new(0.3, 0.9, 1.0, 0.95) } else { Color::new(0.4, 0.4, 0.5, 0.5) };
        hud::text_outlined(if dash_ready { "DASH READY [SHIFT / R-CLICK]" } else { "RECHARGING DASH..." }, 36. * ui, h - 18. * ui, 13. * ui, dash_col);

        // Score & Wave panel
        hud::panel(w - 280. * ui, 20. * ui, 256. * ui, 72. * ui, -8. * ui, Color::new(0.04, 0.06, 0.12, 0.8));
        hud::text_right(&format!("SCORE: {}", hud::commas(u64::from(sim.score))), w - 40. * ui, 48. * ui, 22. * ui, Color::new(1.0, 0.9, 0.4, 1.0));
        hud::text_right(&format!("WAVE {} / {}", sim.wave, TOTAL_WAVES), w - 40. * ui, 76. * ui, 18. * ui, hud::col([0.3, 0.9, 1.0], 1.));

        // Combo counter
        if sim.combo > 1 {
            let combo_col = Color::new(1.0, 0.6, 0.2, 0.95);
            hud::text_outlined(&format!("COMBO x{} (+{}%)", sim.combo, (sim.combo / 2) * 50), 32. * ui, 52. * ui, 24. * ui, combo_col);
        }

        // Radar / Enemy distance indicator
        if let Some(closest) = sim.enemies.iter().min_by(|a, b| {
            let da = (vec3(a.pos.0, a.pos.1, a.pos.2) - eye).length_squared();
            let db = (vec3(b.pos.0, b.pos.1, b.pos.2) - eye).length_squared();
            da.partial_cmp(&db).unwrap()
        }) {
            let dist = (vec3(closest.pos.0, closest.pos.1, closest.pos.2) - eye).length();
            hud::text_centered(&format!("THREAT: {:.1}m", dist), w * 0.5, h * 0.5 + 40. * ui, 13. * ui, Color::new(1.0, 0.35, 0.35, 0.85));
        }

        hud::draw_popups(&fx.popups, &view, ui);
        hud::draw_banners(&fx.banners, ui);

        let controls: Vec<&str> = identity.controls.split(", ").collect();
        if shell.local_menu(&identity.title, &controls) {
            break;
        }

        // 5. Evidence for automated checkers
        if let Some(plan) = &plan {
            if plan.wants(frame) {
                match kit::capture::save_frame(&plan.path_for(frame)) {
                    Ok((w, h)) => println!(
                        "{{\"frame\":{frame},\"width\":{w},\"height\":{h},\"path\":{:?}}}",
                        plan.path_for(frame).display().to_string()
                    ),
                    Err(e) => eprintln!("capture failed: {e}"),
                }
            }
            if plan.finished(frame + 1) {
                break;
            }
        }
        if has_flag(&args, "--perf") {
            perf.frame(dt);
            perf.work(began.elapsed().as_secs_f32());
        }
        frame += 1;
        next_frame().await;
    }

    if has_flag(&args, "--perf") {
        println!("{}", perf.text(25.));
    }
}
