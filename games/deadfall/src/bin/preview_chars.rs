//! Look at the soldiers and the first-person arms.
//!
//! ```text
//! preview_chars --what team0|team1 [--pose stand|walk|run|crouch|aim|dead|reload|throw|melee] [--hold rifle] [--skin 0]
//! preview_chars --what lineup [--dist 40]        both teams side by side
//! preview_chars --what arms0|arms1 [--weapon knife] [--swing 0.4] [--ads 1] [--fp]
//! ```
//! Angles go through `--angles`; `--fp` frames the arms as the player sees them (eye at the origin, FOV 70).
use deadfall::client::arms::{first_person_arms, ArmPose};
use deadfall::client::character::{Hold, Pose, Rig};
use deadfall::client::previewkit::{self, Args, Stage};
use deadfall::client::{weapon_models, WeaponAnchors};
use deadfall::weapons::{self, Class};
use deadfall::Team;
use macroquad::prelude::*;
use vesper3d::viewer::{devkit::flag_value, kit::Template};

fn window() -> macroquad::conf::Conf {
    {
        let mut c = previewkit::window_conf("Deadfall characters");
        c.draw_call_vertex_capacity = 60000;
        c.draw_call_index_capacity = 90000;
        c
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

fn hold_for(id: weapons::WeaponId) -> Hold {
    match weapons::get(id).map(|d| d.class) {
        Some(Class::Pistol) => Hold::Pistol,
        Some(Class::Smg) => Hold::Smg,
        Some(Class::AssaultRifle | Class::Dmr | Class::Lmg | Class::Shotgun) => Hold::Rifle,
        Some(Class::Sniper) => Hold::Sniper,
        Some(Class::Launcher) => Hold::Launcher,
        Some(Class::Grenade) => Hold::Grenade,
        Some(Class::Melee) => Hold::Melee,
        None => Hold::Unarmed,
    }
}

fn pose_named(name: &str, hold: Hold, t: f32, phase: f32) -> Pose {
    let mut p = Pose { hold, ..Pose::default() };
    match name {
        "walk" => {
            p.speed = 1.8;
            p.walk_phase = phase;
        }
        "run" => {
            p.speed = 5.5;
            p.walk_phase = phase;
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
        let rig = Rig::variant(team, skin.min(3), num(&args, "--avatar", 0.) as u8);
        let pose = pose_named(&pose_name, hold, t, num(&args, "--phase", 0.8));
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
            Rig::variant(Team::Nightwatch, 2, 1),
            Rig::variant(Team::Ironclad, 3, 2),
            Rig::variant(Team::Nightwatch, 0, 3),
        ];
        let pose = pose_named(if pose_name == "stand" { "walk" } else { &pose_name }, hold, t, 0.);
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
        // `--weapon key` uses the real model (and its hold); otherwise a stand-in with `--hold`.
        let real = flag_value(&args.raw, "--weapon").and_then(weapon_models::build);
        let hold = match flag_value(&args.raw, "--weapon").and_then(weapons::id_of) {
            Some(id) => hold_for(id),
            None => hold,
        };
        let two_hand = !matches!(hold, Hold::Pistol | Hold::Grenade | Hold::Melee | Hold::Unarmed);
        let anchors = match &real {
            Some(m) => m.anchors,
            None => WeaponAnchors {
                grip: Vec3::ZERO,
                support: two_hand.then(|| vec3(0., -0.02, -0.35)),
                sight: vec3(0., 0.08, 0.05),
                muzzle: vec3(0., 0.03, -0.85),
                eject: vec3(0.03, 0.04, -0.1),
            },
        };
        let ads = num(&args, "--ads", 0.);
        let pose = ArmPose {
            reload: num(&args, "--reload", 0.),
            swing: num(&args, "--swing", 0.),
            throwing: num(&args, "--throw", 0.),
            draw: num(&args, "--draw", 1.),
            ads,
            pin: args.has("--pin"),
        };
        let arms = first_person_arms(team, skin.min(3), &anchors, hold, &pose);
        println!("{} arm vertices, hold {:?}", arms.verts.len(), hold);
        let fp = args.has("--fp");
        // Same placement as the game's viewmodel: hip position, or the sight point on the eye.
        let hip = if hold == Hold::Melee { vec3(0.18, -0.17, -0.38) } else { vec3(0.15, -0.14, -0.32) };
        let relief = flag_value(&args.raw, "--weapon")
            .and_then(weapons::id_of)
            .and_then(weapons::get)
            .map_or(0.4, deadfall::client::render::viewmodel_eye_relief);
        let aimed = -anchors.sight + vec3(0., 0., -relief);
        let mut place = if fp { Mat4::from_translation(hip.lerp(aimed, ads)) } else { Mat4::IDENTITY };
        if hold == Hold::Melee {
            let (offset, angles) = deadfall::client::render::melee_motion(pose.swing, args.has("--heavy"));
            place = place
                * Mat4::from_translation(offset)
                * Mat4::from_rotation_z(angles.z)
                * Mat4::from_rotation_y(angles.y)
                * Mat4::from_rotation_x(angles.x)
                * Mat4::from_rotation_x(0.65)
                * Mat4::from_rotation_z(0.85);
        }
        if let (Some(m), Some(sp)) = (&real, anchors.support) {
            let (mut lo, mut hi) = (Vec3::splat(9.), Vec3::splat(-9.));
            for v in m.assembled().verts.iter().filter(|v| (v.p.z - sp.z).abs() < 0.03) {
                lo = lo.min(v.p);
                hi = hi.max(v.p);
            }
            println!("support {:?} section lo {:?} hi {:?}", sp, lo, hi);
        }
        let weapon = match &real {
            Some(m) => m.assembled(),
            None => stand_in_weapon(!(fp && ads > 0.5)),
        };
        for &a in &args.angles {
            let mut items: Vec<(Template, Mat4)> = vec![(arms.clone(), place)];
            if !args.has("--bare") {
                items.push((weapon.clone(), place));
            }
            let (dist, at, fov, pitch) =
                if fp { (0.001, Vec3::ZERO, 58., 0.) } else { (args.dist, args.at, args.fov, args.pitch) };
            if fp {
                items.push((floor.clone(), Mat4::from_translation(vec3(0., -1.68, 0.))));
            }
            shots.push(Shot {
                name: format!(
                    "{what}-{}-{}{:03}",
                    flag_value(&args.raw, "--weapon").or(flag_value(&args.raw, "--hold")).unwrap_or("rifle"),
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
