//! Looks at Slagworks from screenshots:
//!
//! ```text
//! preview_level --what top   --out DIR [--pitch 88 --dist 115 --angles 0 --cut 5.5]   # the whole map from above
//! preview_level --what tiles --out DIR                                                 # four close top-down tiles
//! preview_level --what walk  --out DIR --at x,y,z --angles 0,90 [--pitch 0]            # eye-height view, FOV 90
//! preview_level --what lanes --out DIR                                                 # the view from each base's spawns
//! ```
//! `--shadows off|simple|full` draws through the game's own renderer instead (shadows, blobs and soldiers as in a match),
//! with `--men x:z:yaw,x:z:yaw[:y]` standing soldiers on the map (slots 0.., alternating teams, `y` is the floor height)
//! and `--repeat N` frames per view (default 3; the average time of the later ones is printed).
//! `--cut H` leaves out every block that starts above H metres (roofs), to see into buildings from above.
use deadfall::client::level_view::{self, LevelScene};
use deadfall::client::previewkit::{self, Args};
use deadfall::client::render::{Figure, Renderer};
use deadfall::maps::MapId;
use deadfall::netgame::{flag, PlayerView};
use deadfall::team::Team;
use macroquad::prelude::*;
use vesper3d::math::V;
use vesper3d::viewer::{
    devkit::{flag_value, ShadowQuality},
    kit::{Materials, View},
};

fn window() -> macroquad::conf::Conf {
    let mut conf = previewkit::window_conf("Slagworks preview");
    conf.draw_call_vertex_capacity = 60000;
    conf.draw_call_index_capacity = 90000;
    conf
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
    let mut level = MapId::from_id(flag_value(&args.raw, "--map").and_then(|v| v.parse().ok()).unwrap_or(0)).build();
    if let Some(cut) = flag_value(&args.raw, "--cut").and_then(|v| v.parse::<f32>().ok()) {
        level.blocks.retain(|b| b.min.1 < cut);
    }
    eprintln!("{}: {} blocks, {} decor, {} loot", level.name, level.blocks.len(), level.decor.len(), level.loot.len());
    let mut scene = level_view::build(&level);
    // Thin the haze for far views (`--fog 0.0008`); the default for the top views is almost none.
    let default_fog = if matches!(args.what.as_str(), "top" | "tiles" | "") { 0.0008 } else { scene.look.fog_density };
    scene.look.fog_density = flag_value(&args.raw, "--fog").and_then(|v| v.parse().ok()).unwrap_or(default_fog);
    if let Some(quality) = ShadowQuality::from_flag(&args.raw).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    }) {
        return game_path(&args, &level, quality).await;
    }
    let mats = Materials::load().expect("the materials failed to compile");
    let meshes = Meshes {
        sky: scene.sky.to_meshes(),
        solid: scene.ground.iter().chain(&scene.solid).flat_map(|t| t.to_meshes()).collect(),
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

/// The game's own `Renderer`: what a match draws, with soldiers placed by `--men`.
async fn game_path(args: &Args, level: &deadfall::level::Level, quality: ShadowQuality) {
    let mut renderer = Renderer::new(level);
    renderer.set_shadows(quality);
    let figures: Vec<Figure> = flag_value(&args.raw, "--men")
        .map(|v| {
            v.split(',')
                .enumerate()
                .filter_map(|(i, m)| {
                    let n: Vec<f32> = m.split(':').filter_map(|p| p.parse().ok()).collect();
                    (n.len() >= 3).then(|| {
                        let y = n.get(3).copied().unwrap_or(0.);
                        Figure {
                            slot: i,
                            team: Team::from_index(i % 2),
                            view: PlayerView {
                                slot: i as u8,
                                flags: flag::ALIVE,
                                eye: V(n[0], y + 1.68, n[1]),
                                yaw: n[2].to_radians(),
                                health: 100,
                                weapon: 3 + i as u8,
                                feet: y,
                                ..Default::default()
                            },
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let repeat: u32 = flag_value(&args.raw, "--repeat").and_then(|v| v.parse().ok()).unwrap_or(3).max(2);
    let views = plan(args, level);
    for (name, view) in &views {
        let mut later = std::time::Duration::ZERO;
        for f in 0..repeat {
            let started = std::time::Instant::now();
            renderer.update(1. / 60.);
            renderer.draw_world(view, &figures, None, 0, &[], &[], &[], &[0; 16], 1. / 60.);
            if f + 1 == repeat {
                if let Some(out) = &args.out {
                    previewkit::shot(out, name);
                }
            }
            if f > 0 {
                later += started.elapsed();
            }
            next_frame().await;
        }
        eprintln!(
            "{name}: {:.1} ms per frame (software GL, {} {})",
            later.as_secs_f64() * 1000. / (repeat - 1) as f64,
            quality.label(),
            figures.len()
        );
        if args.out.is_none() {
            break;
        }
    }
}
