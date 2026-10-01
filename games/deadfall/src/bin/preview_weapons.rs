//! `preview_weapons --what k47 --out DIR --angles 270,0,300` renders one weapon (a metre ruler under it);
//! `--what all` renders contact sheets of all 33 (12 per sheet, camera facing each weapon's left side, labelled);
//! `--eye` looks through the sight anchor down the bore (try `--fov 20`); `--person` adds a 1.80 m mannequin for scale, `--anchors` marks grip/support/sight/muzzle/eject, `--mag` and
//! `--slide` draw the mag/slide displaced (reload / recoil pose) instead of at rest.
//!
//! ```text
//! xvfb-run -a -s "-screen 0 1280x720x24" target/debug/preview_weapons --what all --out /tmp/sheets
//! ```
use deadfall::client::previewkit::{self, Args, Stage};
use deadfall::client::weapon_models::{self, WeaponModel};
use macroquad::prelude::*;
use vesper3d::viewer::kit::{Template, View};

/// The stage's window, with draw-call buffers large enough for whole weapons (macroquad's default 5000
/// indices silently drops anything bigger).
fn window() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: previewkit::window_conf("Deadfall weapon preview"),
        draw_call_vertex_capacity: 60000,
        draw_call_index_capacity: 90000,
        ..Default::default()
    }
}

struct Shot {
    name: String,
    items: Vec<(Template, Mat4)>,
    yaw: f32,
    at: Vec3,
    dist: f32,
    pitch: f32,
    labels: Vec<(String, Vec3)>,
}

fn bounds(t: &Template) -> (Vec3, Vec3) {
    t.verts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |a, v| (a.0.min(v.p), a.1.max(v.p)))
}

/// Small glowing markers at the anchor points.
fn anchor_marks(m: &WeaponModel) -> Template {
    let mut t = Template::new();
    let a = &m.anchors;
    let mut dot = |p: Vec3, c: [f32; 3]| t.ball(p, Vec3::splat(0.007), c, 1., 8, 6);
    dot(a.grip, [0.2, 1., 0.2]);
    if let Some(s) = a.support {
        dot(s, [0.3, 0.5, 1.]);
    }
    dot(a.sight, [1., 1., 0.1]);
    dot(a.muzzle, [1., 0.2, 0.2]);
    dot(a.eject, [1., 0.3, 1.]);
    t
}

/// Metre ruler along Z with alternating 10 cm blocks, centred at z = 0.
fn ruler() -> Template {
    let mut t = Template::new();
    for i in 0..20 {
        let z = -1. + i as f32 * 0.1 + 0.05;
        let c = if i % 2 == 0 { [0.9, 0.85, 0.2] } else { [0.15, 0.15, 0.15] };
        t.box_(vec3(0., 0., z), vec3(0.008, 0.004, 0.05), c, 0.);
    }
    t
}

/// A blocky 1.80 m person facing -Z, feet on y = 0.
fn mannequin() -> Template {
    let mut t = Template::new();
    let cloth = [0.30, 0.36, 0.22];
    t.box_(vec3(0.10, 0.42, 0.), vec3(0.075, 0.42, 0.09), [0.22, 0.26, 0.17], 0.);
    t.box_(vec3(-0.10, 0.42, 0.), vec3(0.075, 0.42, 0.09), [0.22, 0.26, 0.17], 0.);
    t.box_(vec3(0., 1.15, 0.), vec3(0.22, 0.30, 0.12), cloth, 0.);
    t.box_(vec3(0.30, 1.15, 0.), vec3(0.06, 0.28, 0.07), cloth, 0.);
    t.box_(vec3(-0.30, 1.15, 0.), vec3(0.06, 0.28, 0.07), cloth, 0.);
    t.ball(vec3(0., 1.62, 0.), vec3(0.085, 0.11, 0.10), [0.85, 0.65, 0.5], 0., 10, 8);
    t.ball(vec3(0., 1.68, 0.), vec3(0.12, 0.12, 0.13), [0.25, 0.3, 0.17], 0., 10, 8);
    t
}

fn single(key: &str, args: &Args) -> Vec<Shot> {
    let Some(m) = weapon_models::build(key) else {
        eprintln!("unknown weapon {key}");
        return vec![];
    };
    let mut parts = m.body.clone();
    if let Some((t, off)) = &m.mag {
        let d = if args.has("--mag") { vec3(0., -0.12, 0.) } else { Vec3::ZERO };
        parts.append(&t.transformed(Mat4::from_translation(*off + d)));
    }
    if let Some((t, travel)) = &m.slide {
        let k = if args.has("--slide") { 1. } else { 0. };
        parts.append(&t.transformed(Mat4::from_translation(*travel * k)));
    }
    let (lo, hi) = bounds(&parts);
    let grip_world = if args.has("--person") { vec3(0.30, 1.25, -0.30) } else { vec3(0., 1.0, 0.) };
    let mut items = vec![(previewkit::floor(6.), Mat4::IDENTITY), (parts, Mat4::from_translation(grip_world))];
    if args.has("--anchors") {
        items.push((anchor_marks(&m), Mat4::from_translation(grip_world)));
    }
    items.push((ruler(), Mat4::from_translation(vec3(grip_world.x, lo.y + grip_world.y - 0.05, grip_world.z))));
    let at = if args.has("--person") {
        vec3(0., 1.0, -0.3)
    } else if args.has("--at") {
        args.at
    } else {
        grip_world + (lo + hi) * 0.5
    };
    if args.has("--person") {
        items.push((mannequin(), Mat4::IDENTITY));
    }
    if args.has("--eye") {
        // Look through the sight anchor straight down the bore: a check of `anchors.sight`.
        let eye = grip_world + m.anchors.sight;
        return vec![Shot {
            name: format!("{key}-eye"),
            items,
            yaw: 0.,
            at: eye - vec3(0., 0., 0.5),
            dist: 0.5,
            pitch: 0.,
            labels: vec![],
        }];
    }
    let dist = if args.has("--dist") {
        args.dist
    } else if args.has("--person") {
        3.4
    } else {
        (hi - lo).max_element() * 1.15 + 0.15
    };
    args.angles
        .iter()
        .map(|&yaw| Shot {
            name: format!("{key}-{:03}", yaw as i32),
            items: items.clone(),
            yaw,
            at,
            dist,
            pitch: args.pitch,
            labels: vec![],
        })
        .collect()
}

/// Contact sheets: 3 columns x 4 rows per sheet, weapon left sides facing the camera.
fn sheets(args: &Args) -> Vec<Shot> {
    let listed: Vec<String> = vesper3d::viewer::devkit::flag_value(&args.raw, "--keys")
        .map(|v| v.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let all: Vec<&str> = weapon_models::keys().to_vec();
    let keys: Vec<&str> = if listed.is_empty() { all } else { listed.iter().map(String::as_str).collect() };
    let num = |name: &str, d: f32| {
        vesper3d::viewer::devkit::flag_value(&args.raw, name).and_then(|v| v.parse().ok()).unwrap_or(d)
    };
    let cols = if listed.is_empty() { 3usize } else { num("--cols", 2.) as usize };
    let rows = if listed.is_empty() { 4usize } else { keys.len().div_ceil(cols) };
    let (dx, dy) = (num("--dx", 1.7), num("--dy", 0.62));
    let mut out = vec![];
    for (si, chunk) in keys.chunks(cols * rows).enumerate() {
        let mut items = vec![];
        let mut labels = vec![];
        for (i, key) in chunk.iter().enumerate() {
            let m = weapon_models::build(key).expect("roster key");
            let parts = m.assembled();
            let (lo, hi) = bounds(&parts);
            let (c, r) = (i % cols, i / cols);
            // Screen right is +Z when looking along +X: the muzzle (-Z) points left.
            let cz = (c as f32 - (cols as f32 - 1.) * 0.5) * dx;
            let cy = ((rows as f32 - 1.) * 0.5 - r as f32) * dy;
            let pos = vec3(0., cy - (lo.y + hi.y) * 0.5, cz - (lo.z + hi.z) * 0.5);
            items.push((parts, Mat4::from_translation(pos)));
            if args.has("--anchors") {
                items.push((anchor_marks(&m), Mat4::from_translation(pos)));
            }
            labels.push((format!("{key} {:.2}m", m.length), vec3(0., cy - dy * 0.46, cz - dx * 0.3)));
        }
        out.push(Shot {
            name: format!("sheet-{}", si + 1),
            items,
            yaw: if args.has("--angles") { args.angles[0] } else { 270. },
            at: vec3(0., 0., 0.),
            dist: if args.has("--dist") { args.dist } else { 3.7 },
            pitch: if args.has("--pitch") { args.pitch } else { 0. },
            labels,
        });
    }
    out
}

#[macroquad::main(window)]
async fn main() {
    let args = Args::parse();
    let shots = if args.what == "all" { sheets(&args) } else { single(&args.what, &args) };
    if shots.is_empty() {
        eprintln!(
            "usage: preview_weapons --what <key|all> [--out DIR --angles 270,0 --person --anchors --mag --slide]"
        );
        return;
    }
    let mut stage = Stage::new();
    let mut frame = 0usize;
    let mut idx = 0usize;
    loop {
        let s = &shots[idx];
        let mut a = Args { ..Args::parse() };
        a.at = s.at;
        a.dist = s.dist;
        a.pitch = s.pitch;
        let refs: Vec<(&Template, Mat4)> = s.items.iter().map(|(t, m)| (t, *m)).collect();
        stage.draw(&a, s.yaw, &refs);
        // Project the labels with the same camera the stage used.
        let yaw = s.yaw.to_radians();
        let pitch = s.pitch.to_radians();
        let eye = s.at + vec3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()) * s.dist;
        let to = (s.at - eye).normalize();
        let view = View { eye, yaw: to.x.atan2(-to.z), pitch: to.y.asin(), roll: 0., fov: a.fov.to_radians() };
        let m = macroquad::camera::Camera::matrix(&view.camera(0.02, 400.));
        for (text, p) in &s.labels {
            let ndc = m.project_point3(*p);
            let (sx, sy) = ((ndc.x * 0.5 + 0.5) * screen_width(), (0.5 - ndc.y * 0.5) * screen_height());
            draw_text(text, sx, sy, 22., WHITE);
        }
        draw_text(&s.name, 10., 22., 22., YELLOW);
        frame += 1;
        if let Some(out) = &args.out {
            if frame % 4 == 3 {
                previewkit::shot(out, &s.name);
                idx += 1;
                if idx >= shots.len() {
                    break;
                }
            }
        }
        next_frame().await;
    }
}
