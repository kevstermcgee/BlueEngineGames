//! Looks at Slagworks from screenshots:
//!
//! ```text
//! preview_level --what top   --out DIR [--pitch 88 --dist 115 --angles 0 --cut 5.5]   # the whole map from above
//! preview_level --what tiles --out DIR                                                 # four close top-down tiles
//! preview_level --what walk  --out DIR --at x,y,z --angles 0,90 [--pitch 0]            # eye-height view, FOV 90
//! preview_level --what lanes --out DIR                                                 # the view from each base's spawns
//! ```
//! `--cut H` leaves out every block that starts above H metres (roofs), to see into buildings from above.
use deadfall::client::level_view::{self, LevelScene};
use deadfall::client::previewkit::{self, Args};
use deadfall::slagworks;
use macroquad::prelude::*;
use vesper3d::viewer::{
    devkit::flag_value,
    kit::{Materials, View},
};

fn window() -> macroquad::conf::Conf {
    // The kit's meshes hold up to 9000 vertices and 27000 indices: raise macroquad's draw-call capacity.
    macroquad::conf::Conf {
        miniquad_conf: previewkit::window_conf("Slagworks preview"),
        draw_call_vertex_capacity: 30_000,
        draw_call_index_capacity: 30_000,
        ..Default::default()
    }
}

struct Meshes {
    sky: Vec<Mesh>,
    solid: Vec<Mesh>,
    decor: Vec<Mesh>,
    glass: Vec<Mesh>,
}

fn orbit(at: Vec3, yaw_deg: f32, pitch_deg: f32, dist: f32, fov_deg: f32) -> View {
    let (yaw, pitch) = (yaw_deg.to_radians(), pitch_deg.to_radians());
    let eye = at + vec3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()) * dist;
    let to = (at - eye).normalize();
    View { eye, yaw: to.x.atan2(-to.z), pitch: to.y.asin(), roll: 0., fov: fov_deg.to_radians() }
}

fn eye_view(eye: Vec3, yaw_deg: f32, pitch_deg: f32, fov_deg: f32) -> View {
    View { eye, yaw: yaw_deg.to_radians(), pitch: pitch_deg.to_radians(), roll: 0., fov: fov_deg.to_radians() }
}

fn plan(args: &Args, level: &deadfall::level::Level) -> Vec<(String, View)> {
    let mut out = Vec::new();
    let num = |name: &str, default: f32| flag_value(&args.raw, name).and_then(|v| v.parse().ok()).unwrap_or(default);
    match args.what.as_str() {
        "walk" => {
            let pitch = num("--pitch", 0.);
            let fov = if args.has("--fov") { args.fov } else { 90. };
            for yaw in &args.angles {
                let at = args.at;
                out.push((
                    format!("walk-{:.0}_{:.0}_{:.0}-yaw{:03}", at.x, at.y, at.z, *yaw as i32),
                    eye_view(at, *yaw, pitch, fov),
                ));
            }
        }
        "lanes" => {
            for team in 0..2 {
                let sp = &level.spawns[team];
                for i in [0, sp.len() / 2, sp.len() - 1] {
                    let s = sp[i];
                    let eye = vec3(s.pos.0, s.pos.1 + 1.68, s.pos.2);
                    out.push((format!("lane-team{team}-spawn{i}"), eye_view(eye, s.yaw.to_degrees(), 0., 90.)));
                }
            }
        }
        "tiles" => {
            for (i, (x, z)) in [(-30., -22.), (30., -22.), (-30., 22.), (30., 22.)].into_iter().enumerate() {
                out.push((
                    format!("tile-{i}"),
                    orbit(vec3(x, 0., z), 0., num("--pitch", 72.), num("--dist", 62.), 45.),
                ));
            }
        }
        _ => {
            for yaw in &args.angles {
                let pitch = num("--pitch", 88.);
                let dist = num("--dist", 118.);
                out.push((
                    format!("top-yaw{:03}-pitch{:.0}", *yaw as i32, pitch),
                    orbit(args.at, *yaw, pitch, dist, 45.),
                ));
            }
        }
    }
    out
}

fn draw(mats: &Materials, scene: &LevelScene, m: &Meshes, view: &View) {
    clear_background(scene.look.clear_color());
    set_camera(&view.sky_camera());
    gl_use_material(&mats.sky);
    for mesh in &m.sky {
        draw_mesh(mesh);
    }
    set_camera(&view.camera(0.05, 800.));
    mats.set_scene(&scene.look, view.eye, 0., 0.);
    let _ = mats.set_point_lights(&scene.nearest_lights(view.eye));
    gl_use_material(&mats.world);
    for mesh in m.solid.iter().chain(&m.decor).chain(&m.glass) {
        draw_mesh(mesh);
    }
    gl_use_default_material();
    set_default_camera();
}

#[macroquad::main(window)]
async fn main() {
    let args = Args::parse();
    let mut level = slagworks::build();
    if let Some(cut) = flag_value(&args.raw, "--cut").and_then(|v| v.parse::<f32>().ok()) {
        level.blocks.retain(|b| b.min.1 < cut);
    }
    eprintln!("{}: {} blocks, {} decor, {} loot", level.name, level.blocks.len(), level.decor.len(), level.loot.len());
    let mut scene = level_view::build(&level);
    // Thin the haze for far views (`--fog 0.0008`); the default for the top views is almost none.
    let default_fog = if matches!(args.what.as_str(), "top" | "tiles" | "") { 0.0008 } else { scene.look.fog_density };
    scene.look.fog_density = flag_value(&args.raw, "--fog").and_then(|v| v.parse().ok()).unwrap_or(default_fog);
    let mats = Materials::load().expect("the materials failed to compile");
    let meshes = Meshes {
        sky: scene.sky.to_meshes(),
        solid: scene.solid.iter().flat_map(|t| t.to_meshes()).collect(),
        decor: scene.decor.iter().flat_map(|t| t.to_meshes()).collect(),
        glass: scene.glass.to_meshes(),
    };
    let views = plan(&args, &level);
    let mut i = 0;
    let mut frame = 0;
    while let Some((name, view)) = views.get(i) {
        draw(&mats, &scene, &meshes, view);
        frame += 1;
        if let Some(out) = &args.out {
            if frame == 3 {
                previewkit::shot(out, name);
                frame = 0;
                i += 1;
            }
        }
        next_frame().await;
    }
}
