//! Look at the soldiers and the first-person arms.
//!
//! ```text
//! preview_chars --what team0|team1 [--pose stand|walk|run|crouch|aim|dead|reload|throw|melee] [--hold rifle] [--skin 0]
//! preview_chars --what lineup [--dist 40]        both teams side by side
//! preview_chars --what arms0|arms1 [--hold rifle|pistol|..] [--t 0.4] [--ads 1] [--fp]
//! ```
//! Angles go through `--angles`; `--fp` frames the arms as the player sees them (eye at the origin, FOV 70).
use deadfall::client::arms::{first_person_arms, ArmPose};
use deadfall::client::character::{Hold, Pose, Rig};
use deadfall::client::previewkit::{self, Args, Stage};
use deadfall::client::WeaponAnchors;
use deadfall::Team;
use macroquad::prelude::*;
use vesper3d::viewer::{devkit::flag_value, kit::Template};

fn window() -> macroquad::conf::Conf {
    // A soldier is thousands of vertices: raise the draw-call capacities like the game's own window does.
    macroquad::conf::Conf {
        miniquad_conf: previewkit::window_conf("Deadfall characters"),
        draw_call_vertex_capacity: 30000,
        draw_call_index_capacity: 30000,
        ..Default::default()
    }
}

fn num(args: &Args, name: &str, d: f32) -> f32 {
    flag_value(&args.raw, name).and_then(|v| v.parse().ok()).unwrap_or(d)
}

/// A 1.80 m marker: a thin pole with ticks every 0.3 m and a cap at the top.
fn marker() -> Template {
    let mut t = Template::new();
    t.box_(vec3(0., 0.9, 0.), vec3(0.008, 0.9, 0.008), [0.95, 0.2, 0.2], 0.3);
    for k in 0..7 {
        let y = k as f32 * 0.3;
        t.box_(vec3(0., y, 0.), vec3(if k == 6 { 0.22 } else { 0.06 }, 0.006, 0.006), [0.95, 0.2, 0.2], 0.3);
    }
    t
}

fn pose_named(name: &str, hold: Hold, t: f32) -> Pose {
    let mut p = Pose { hold, ..Pose::default() };
    match name {
        "walk" => {
            p.speed = 1.8;
            p.walk_phase = 0.9;
        }
        "run" => {
            p.speed = 5.5;
            p.walk_phase = 0.7;
        }
        "crouch" => p.crouch = 1.,
        "aim" => p.aim = 1.,
        "dead" => p.dead = t.max(0.0),
        "reload" => p.reload = t,
        "throw" => {
            p.hold = Hold::Grenade;
            p.throwing = t;
        }
        "melee" => {
            p.hold = Hold::Melee;
            p.swing = t;
        }
        "air" => p.airborne = true,
        _ => {}
    }
    p
}

struct Shot {
    name: String,
    yaw: f32,
    pitch: f32,
    dist: f32,
    at: Vec3,
    fov: f32,
    items: Vec<(Template, Mat4)>,
}

fn stand_in_weapon(stock: bool) -> Template {
    let mut w = Template::new();
    w.box_(vec3(0., 0.03, -0.32), vec3(0.03, 0.04, 0.42), [0.12, 0.12, 0.13], 0.); // receiver + barrel
    w.box_(vec3(0., -0.06, 0.0), vec3(0.018, 0.06, 0.03), [0.10, 0.10, 0.11], 0.); // pistol grip
    w.box_(vec3(0., -0.10, -0.10), vec3(0.02, 0.06, 0.03), [0.25, 0.22, 0.12], 0.); // magazine
    w.box_(vec3(0., 0.085, 0.05), vec3(0.008, 0.012, 0.008), [0.05, 0.05, 0.05], 0.); // rear sight
    w.box_(vec3(0., 0.085, -0.6), vec3(0.004, 0.014, 0.004), [0.9, 0.2, 0.1], 0.); // front sight post
    if stock {
        w.box_(vec3(0., 0.02, 0.22), vec3(0.025, 0.04, 0.12), [0.2, 0.15, 0.1], 0.);
        // stock
    }
    w
}

#[macroquad::main(window)]
async fn main() {
    let args = Args::parse();
    let what = args.what.clone();
    let hold = flag_value(&args.raw, "--hold").and_then(Hold::from_name).unwrap_or(Hold::Rifle);
    let skin = num(&args, "--skin", 1.) as u8;
    let t = num(&args, "--t", 0.4);
    let pose_name = flag_value(&args.raw, "--pose").unwrap_or("stand").to_string();
    let floor = previewkit::floor(if what == "lineup" { 60. } else { 4. });
    let marker = marker();
    let mut shots: Vec<Shot> = Vec::new();

    if what == "team0" || what == "team1" {
        let team = Team::from_index(if what == "team0" { 0 } else { 1 });
        let rig = Rig::new(team, skin.min(3));
        let pose = pose_named(&pose_name, hold, t);
        println!("{} vertices per soldier: {:?}", rig.vertex_count(), rig.part_sizes());
        for &a in &args.angles {
            let mut items: Vec<(Template, Mat4)> =
                vec![(floor.clone(), Mat4::IDENTITY), (marker.clone(), Mat4::from_translation(vec3(0.9, 0., 0.)))];
            for (t, m) in rig.parts(Vec3::ZERO, 0., &pose) {
                items.push((t.clone(), m));
            }
            if pose.hold != Hold::Unarmed && pose_name != "dead" {
                items.push((stand_in_weapon(true), rig.weapon_mount(Vec3::ZERO, 0., &pose)));
            }
            shots.push(Shot {
                name: format!("{what}-{pose_name}-{:03}", a as i32),
                yaw: a,
                pitch: args.pitch,
                dist: args.dist,
                at: args.at,
                fov: args.fov,
                items,
            });
        }
    } else if what == "lineup" {
        let rigs = [
            Rig::new(Team::Ironclad, 1),
            Rig::new(Team::Nightwatch, 2),
            Rig::new(Team::Ironclad, 3),
            Rig::new(Team::Nightwatch, 0),
        ];
        let pose = pose_named(if pose_name == "stand" { "walk" } else { &pose_name }, hold, t);
        for &a in &args.angles {
            let mut items: Vec<(Template, Mat4)> = vec![(floor.clone(), Mat4::IDENTITY)];
            for k in 0..12 {
                let rig = &rigs[k % 4];
                let x = (k as f32 - 5.5) * 1.2;
                let yaw = (k as f32 * 0.37).sin() * 0.5;
                let feet = vec3(x, 0., 0.);
                for (t, m) in rig.parts(feet, yaw, &Pose { walk_phase: k as f32, ..pose }) {
                    items.push((t.clone(), m));
                }
                if pose.hold != Hold::Unarmed {
                    items.push((
                        stand_in_weapon(true),
                        rig.weapon_mount(feet, yaw, &Pose { walk_phase: k as f32, ..pose }),
                    ));
                }
            }
            shots.push(Shot {
                name: format!("lineup-{}-{:03}", args.dist as i32, a as i32),
                yaw: a,
                pitch: args.pitch,
                dist: args.dist,
                at: args.at,
                fov: args.fov,
                items,
            });
        }
    } else if what == "arms0" || what == "arms1" {
        let team = Team::from_index(if what == "arms0" { 0 } else { 1 });
        let two_hand = !matches!(hold, Hold::Pistol | Hold::Grenade | Hold::Melee | Hold::Unarmed);
        let anchors = WeaponAnchors {
            grip: Vec3::ZERO,
            support: two_hand.then(|| vec3(0., -0.02, -0.35)),
            sight: vec3(0., 0.08, 0.05),
            muzzle: vec3(0., 0.03, -0.85),
            eject: vec3(0.03, 0.04, -0.1),
        };
        let ads = num(&args, "--ads", 0.);
        let pose = ArmPose {
            reload: num(&args, "--reload", 0.),
            swing: num(&args, "--swing", 0.),
            throwing: num(&args, "--throw", 0.),
            draw: num(&args, "--draw", 1.),
            ads,
        };
        let arms = first_person_arms(team, skin.min(3), &anchors, hold, &pose);
        println!("{} arm vertices", arms.verts.len());
        let fp = args.has("--fp");
        let place = if fp {
            if ads > 0.5 {
                Mat4::from_translation(-anchors.sight)
            } else {
                Mat4::from_translation(vec3(0.25, -0.22, -0.4))
            }
        } else {
            Mat4::IDENTITY
        };
        for &a in &args.angles {
            let mut items: Vec<(Template, Mat4)> =
                vec![(arms.clone(), place), (stand_in_weapon(!(fp && ads > 0.5)), place)];
            let (dist, at, fov, pitch) =
                if fp { (0.001, Vec3::ZERO, 70., 0.) } else { (args.dist, args.at, args.fov, args.pitch) };
            if fp {
                items.push((floor.clone(), Mat4::from_translation(vec3(0., -1.68, 0.))));
            }
            shots.push(Shot {
                name: format!(
                    "{what}-{}-{}{:03}",
                    flag_value(&args.raw, "--hold").unwrap_or("rifle"),
                    if fp { "fp-" } else { "" },
                    a as i32
                ),
                yaw: a,
                pitch,
                dist,
                at,
                fov,
                items,
            });
        }
    } else {
        eprintln!("usage: --what team0|team1|lineup|arms0|arms1");
        return;
    }

    let mut stage = Stage::new();
    let mut frame = 0;
    let mut cam = Args::parse();
    loop {
        let i = (frame / 4).min(shots.len() - 1);
        let s = &shots[i];
        cam.pitch = s.pitch;
        cam.dist = s.dist;
        cam.at = s.at;
        cam.fov = s.fov;
        let items: Vec<(&Template, Mat4)> = s.items.iter().map(|(t, m)| (t, *m)).collect();
        stage.draw(&cam, s.yaw, &items);
        frame += 1;
        if let Some(out) = &args.out {
            if frame % 4 == 3 {
                previewkit::shot(out, &s.name);
                if i + 1 >= shots.len() {
                    break;
                }
            }
        }
        next_frame().await;
    }
}
