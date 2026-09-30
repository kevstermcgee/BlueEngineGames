#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod platform;
use macroquad::prelude::*;
use physics_lab::*;
use vesper3d::viewer::{
    devkit::Lifecycle,
    game_client::{self, GameShell},
    game_input::ClientInput,
    identity::Identity,
    kit::{self, hud, Batch, Look, Materials, MirrorPlane, PlanarMirror, PointLight, Template, Tint, View},
};
fn identity() -> Identity {
    Identity::parse(include_str!("../assets/identity.json")).unwrap()
}
fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    game_client::window_config_with_icon(
        &identity().title,
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    )
}
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
}
fn point(v: vesper3d::math::V) -> Vec3 {
    vec3(v.0, v.1, v.2)
}
fn scene() -> Template {
    let mut t = Template::new();
    t.box_(vec3(0., -0.2, 1.5), vec3(12., 0.2, 10.5), [0.12, 0.17, 0.22], 0.);
    for x in -12..=12 {
        t.box_(vec3(x as f32, 0.005, 1.5), vec3(0.012, 0.005, 10.5), [0.24, 0.32, 0.39], 0.);
    }
    for z in -8..=12 {
        t.box_(vec3(0., 0.005, z as f32), vec3(12., 0.005, 0.012), [0.24, 0.32, 0.39], 0.);
    }
    // Station strips, fire tray, waterfall pipe, tank rim, mirror frame.
    for (x, z, color) in [(-5., 1., [0.95, 0.6, 0.2]), (-2., -3., [1., 0.3, 0.08]), (6.5, -2.5, [0.1, 0.65, 0.95])] {
        t.box_(vec3(x, 0.03, z), vec3(3., 0.02, 0.06), color, 0.8);
    }
    t.box_(vec3(-2., 0.12, -3.), vec3(1., 0.12, 1.), [0.2, 0.21, 0.24], 0.);
    t.cylinder(vec3(-4., 0., -3.), 0.12, 4.5, [0.3, 0.4, 0.45], 0., 12);
    t.box_(vec3(-3.8, 4.5, -3.), vec3(0.35, 0.12, 0.12), [0.35, 0.45, 0.5], 0.);
    for x in [4., 9.] {
        t.box_(vec3(x, 0.65, -2.5), vec3(0.05, 0.65, 2.5), [0.25, 0.6, 0.7], 0.);
    }
    for z in [-5., 0.] {
        t.box_(vec3(6.5, 0.65, z), vec3(2.5, 0.65, 0.05), [0.25, 0.6, 0.7], 0.);
    }
    for x in [-5.1, 5.1] {
        t.box_(vec3(x, 2.5, -8.), vec3(0.1, 2.6, 0.12), [0.6, 0.75, 0.8], 0.2);
    }
    for y in [0., 5.] {
        t.box_(vec3(0., y, -8.), vec3(5., 0.08, 0.12), [0.6, 0.75, 0.8], 0.2);
    }
    for i in 0..6 {
        let m = MATERIALS[i];
        t.box_(vec3(-8. + i as f32 * 1.25, 0.15, 3.), vec3(0.4, 0.15, 0.4), m.color, 0.);
    }
    t
}
struct Draw {
    room: Template,
    balls: Vec<Template>,
    drop: Template,
    flame: Template,
    water: Template,
    person: Template,
    world: Batch,
    alpha: Batch,
    add: Batch,
}
impl Draw {
    fn new() -> Self {
        let balls = MATERIALS
            .iter()
            .map(|m| {
                let mut t = Template::new();
                t.ball(Vec3::ZERO, Vec3::splat(0.3), m.color, 0., 12, 8);
                t
            })
            .collect();
        let mut drop = Template::new();
        drop.ball(Vec3::ZERO, vec3(0.035, 0.09, 0.035), [0.25, 0.7, 1.], 0.6, 6, 4);
        let mut flame = Template::new();
        flame.ball(Vec3::ZERO, Vec3::ONE, [1., 0.25, 0.02], 1., 8, 6);
        let mut water = Template::new();
        water.box_(vec3(6.5, 1.15, -2.5), vec3(2.45, 0.03, 2.45), [0.1, 0.55, 0.8], 0.3);
        let mut person = Template::new();
        person.box_(vec3(0., -0.7, 0.), vec3(0.22, 0.55, 0.18), [0.9, 0.8, 0.3], 0.);
        person.ball(vec3(0., 0., 0.), Vec3::splat(0.2), [0.9, 0.7, 0.5], 0., 10, 6);
        Self {
            room: scene(),
            balls,
            drop,
            flame,
            water,
            person,
            world: Batch::new(),
            alpha: Batch::new(),
            add: Batch::new(),
        }
    }
    fn render(&mut self, s: &Sim, mat: &Materials, look: &Look, eye: Vec3, reflection: bool) {
        let time = s.tick as f32 / 60.;
        mat.set_scene(look, eye, time, 0.5);
        let mut lights = vec![];
        if s.fire {
            lights.push(
                PointLight::new(vec3(-2., 0.9, -3.), 6., [1., 0.22, 0.03], 3. + 0.5 * (time * 13.).sin()).unwrap(),
            );
        }
        if s.light == 3 {
            lights.push(
                PointLight::new(vec3(time.cos() * 5., 2.5, 1. + time.sin() * 4.), 7., [0.15, 0.35, 1.], 4.).unwrap(),
            );
        }
        for b in
            s.balls.iter().filter(|b| MATERIALS[b.material].burn && b.heat > 100. && b.fuel > 0.).take(4 - lights.len())
        {
            lights.push(PointLight::new(point(b.p) + Vec3::Y * 0.4, 2., [1., 0.25, 0.02], 1.5).unwrap());
        }
        mat.set_point_lights(&lights).unwrap();

        self.world.clear();
        self.alpha.clear();
        self.add.clear();
        self.world.add(&self.room, Mat4::IDENTITY, Tint::NONE);
        for b in &s.balls {
            self.world.add(
                &self.balls[b.material],
                Mat4::from_translation(point(b.p)),
                Tint::flash(((b.heat - 80.) / 200.).clamp(0., 0.8)),
            );
            if MATERIALS[b.material].burn && b.heat > 100. && b.fuel > 0. {
                self.add.add(
                    &self.flame,
                    Mat4::from_scale_rotation_translation(
                        vec3(0.16, 0.4, 0.16),
                        Quat::IDENTITY,
                        point(b.p) + vec3(0., 0.3, 0.),
                    ),
                    Tint::NONE,
                );
            }
        }
        for d in &s.drops {
            self.world.add(&self.drop, Mat4::from_translation(point(d.p)), Tint::NONE);
        }
        if s.fire {
            for i in 0..9 {
                let phase = time * 5. + i as f32 * 2.;
                let p = vec3(-2. + phase.sin() * 0.65, 0.5 + (phase * 1.3).sin().abs() * 0.4, -3. + phase.cos() * 0.6);
                self.add.add(
                    &self.flame,
                    Mat4::from_scale_rotation_translation(vec3(0.15, 0.4 + 0.2 * phase.sin(), 0.15), Quat::IDENTITY, p),
                    Tint::NONE,
                );
            }
        }
        if reflection {
            self.world.add(&self.person, Mat4::from_translation(point(s.player.position)), Tint::NONE);
        }
        self.alpha.add(&self.water, Mat4::IDENTITY, Tint::alpha(0.55));
        // Expanding rings visualize the stream's impact and surface motion.
        for i in 0..5 {
            let phase = (time * 0.8 + i as f32 * 0.2).fract();
            let mut ring = Template::new();
            ring.ring(vec3(-1.7, 0.045, -3.), 0.08 + phase * 0.7, 0.11 + phase * 0.7, [0.2, 0.65, 0.95], 0.3, 20);
            if s.water {
                self.alpha.add(&ring, Mat4::IDENTITY, Tint::alpha((1. - phase) * 0.5));
            }
        }
        gl_use_material(&mat.world);
        self.world.draw();
        gl_use_material(&mat.fx_alpha);
        self.alpha.draw();
        gl_use_material(&mat.fx_add);
        self.add.draw();
        gl_use_default_material();
    }
}
fn lighting(s: &Sim) -> Look {
    let mut l = match s.light {
        0 => Look::daylight(),
        1 => Look::dusk(),
        _ => Look::night(),
    };
    l.fog_density = 0.002;
    if s.light == 3 {
        let t = s.tick as f32 / 60.;
        l.key_direction = [t.cos(), 0.7, t.sin()];
        l.key_color = [0.5 + 0.5 * t.sin(), 0.4, 0.8];
        l.exposure = 0.85 + 0.15 * (t * 6.).sin();
    }
    l
}
#[macroquad::main(window)]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut life = Lifecycle::<Held>::start_or_exit(
        &args,
        &[
            "fwd", "back", "left", "right", "jump", "look", "water", "fire", "drop", "light", "gravity", "wind",
            "material",
        ],
    );
    let mut sim = Sim::new(life.seed());
    life.load_flag_or_exit(&mut sim);
    let unattended = life.options.unattended();
    let mat = Materials::load().unwrap();
    let mut draw = Draw::new();
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let mut notice = String::from("Welcome. Launch a sample into the fire or buoyancy tank.");
    let mirror =
        PlanarMirror::new(MirrorPlane::new(vec3(0., 2.5, -8.), Vec3::X, Vec3::Y, vec2(10., 5.)).unwrap(), (768, 384))
            .unwrap();
    loop {
        input.begin_frame_with_keyboard(&mut shell, true, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(input.frame_seconds());
        let (held, edges, look) = if let Some(s) = life.script() {
            let mut e = if s.starts("jump") { 128 } else { 0 };
            for (name, bit) in [
                ("water", WATER),
                ("fire", FIRE),
                ("drop", DROP),
                ("light", LIGHT),
                ("gravity", GRAVITY),
                ("wind", WIND),
                ("material", MATERIAL),
            ] {
                if s.starts(name) {
                    e |= bit;
                }
            }
            (
                Held { forward: s.axis("fwd", "back"), right: s.axis("right", "left") },
                e,
                [s.value("look", 0), s.value("look", 1)],
            )
        } else {
            let m = input.movement(&shell);
            let mut e = if m.jump { 128 } else { 0 };
            for (key, bit) in [
                (KeyCode::Key1, WATER),
                (KeyCode::Key2, FIRE),
                (KeyCode::E, DROP),
                (KeyCode::L, LIGHT),
                (KeyCode::G, GRAVITY),
                (KeyCode::B, WIND),
                (KeyCode::M, MATERIAL),
            ] {
                if input.pressed(key) {
                    e |= bit;
                }
            }
            (Held { forward: m.forward, right: m.right }, e, input.look_delta_with(&shell, dt))
        };
        life.feed(held, edges, look);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            notice = life.quick_save(&sim, "Physics lab").detail;
        }
        if load {
            notice = life.quick_load(&mut sim).detail;
        }
        if !shell.paused && input.pressed(KeyCode::R) {
            sim = Sim::new(life.restart_seed());
            life.reset_input();
        }
        for _ in 0..life.ticks(dt, 1., !shell.paused) {
            let t = life.take_tick();
            sim.step(&Input {
                forward: t.held.forward,
                right: t.held.right,
                look: t.look,
                jump: t.pressed(128),
                action: t.edges & 127,
            });
        }
        let eye = point(sim.player.position);
        let pending = life.pending_look();
        let view = View::first_person(eye, sim.player.yaw + pending[0], sim.player.pitch - pending[1]);
        let look = lighting(&sim);
        let reflected = mirror.camera(eye, 100.);
        if let Some(camera) = &reflected {
            set_camera(camera);
            clear_background(look.clear_color());
            draw.render(&sim, &mat, &look, camera.eye, true);
        }
        set_camera(&view.camera(0.05, 100.));
        clear_background(look.clear_color());
        draw.render(&sim, &mat, &look, eye, false);
        if reflected.is_some() {
            mirror.draw_surface();
        }
        set_default_camera();
        for (p, label) in [
            (vec3(-5., 0.5, 3.), "MATERIAL DROP TEST"),
            (vec3(-2., 2., -3.), "FIRE / QUENCH"),
            (vec3(6.5, 1.8, -2.5), "BUOYANCY TANK"),
            (vec3(0., 5.3, -8.), "PLANAR MIRROR"),
        ] {
            if let Some(at) = view.project(p, screen_width(), screen_height()) {
                hud::text_centered(label, at.x, at.y, 16., WHITE);
            }
        }
        hud::panel(16., 16., screen_width() - 32., 94., 8., Color::new(0.02, 0.04, 0.07, 0.9));
        hud::text_outlined("PHYSICS LAB  /  environmental playground", 30., 42., 24., WHITE);
        hud::text_outlined(
            &format!(
                "Sample: {}   Bodies: {}   Water: {}   Fire: {}   Gravity: {}   Wind: {}   Light: {}",
                MATERIALS[sim.selected].name,
                sim.balls.len(),
                sim.water,
                sim.fire,
                if sim.low_gravity { "moon" } else { "earth" },
                sim.wind,
                ["daylight", "sunset", "night", "rotating pulse"][sim.light]
            ),
            30.,
            70.,
            17.,
            SKYBLUE,
        );
        let sample = MATERIALS[sim.selected];
        hud::text_outlined(
            &format!(
                "{} | density {:.0} kg/m3, restitution {:.2}, friction {:.2}",
                notice, sample.density, sample.bounce, sample.friction
            ),
            30.,
            94.,
            14.,
            WHITE,
        );
        hud::text_outlined(
            "E launch | M material | 1 water | 2 fire | L light | G gravity | B wind | R reset | F5/F9 save/load",
            20.,
            screen_height() - 42.,
            17.,
            WHITE,
        );
        hud::text_outlined(
            "Sphere contacts + approximate buoyancy/heat; glass and ice are samples, without fracture or melting.",
            20.,
            screen_height() - 20.,
            14.,
            GRAY,
        );
        hud::crosshair(1., 0., WHITE);
        if shell.local_menu(
            "Physics Lab",
            &[
                "WASD move; mouse look; Space jump",
                "E launch; M next material",
                "1 water; 2 fire; L lighting; G gravity; B wind",
                "R reset; F5/F9 save/load",
            ],
        ) {
            break;
        }
        if let Some(p) = life.capture_path() {
            life.captured(&p, kit::capture::save_frame(&p).map_err(|e| e.to_string()));
        }
        if life.end_frame(dt) {
            break;
        }
        next_frame().await;
    }
    if let Some(r) = life.report() {
        println!("{r}");
    }
}
