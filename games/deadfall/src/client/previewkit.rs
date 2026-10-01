//! A tiny stage for looking at one thing at a time: a model, a character, the map. Used by the `preview`-style
//! binaries so art can be judged from screenshots on a machine with no display (`xvfb-run`, see the engine's
//! docs/HEADLESS_CAPTURE.md).
//!
//! ```text
//! xvfb-run -a -s "-screen 0 1280x720x24" target/debug/preview --what weapon:k47 --out /tmp/shots --angles 0,90,180
//! ```
use macroquad::prelude::*;
use std::path::PathBuf;
use vesper3d::viewer::{
    devkit::{flag_value, has_flag},
    kit::{self, Batch, Look, Materials, Template, Tint, View},
};

/// Command line shared by every preview binary.
pub struct Args {
    pub raw: Vec<String>,
    /// Directory the screenshots go to (`--out`); `None` runs interactively.
    pub out: Option<PathBuf>,
    /// Camera yaw angles in degrees, one screenshot each (`--angles 0,90`).
    pub angles: Vec<f32>,
    /// Camera pitch in degrees (`--pitch`), distance in metres (`--dist`), target point (`--at x,y,z`).
    pub pitch: f32,
    pub dist: f32,
    pub at: Vec3,
    pub fov: f32,
    /// Free text for the binary (`--what weapon:k47`).
    pub what: String,
}

impl Args {
    pub fn parse() -> Self {
        let raw: Vec<String> = std::env::args().collect();
        let num = |name: &str, default: f32| flag_value(&raw, name).and_then(|v| v.parse().ok()).unwrap_or(default);
        let at = flag_value(&raw, "--at")
            .map(|v| v.split(',').filter_map(|p| p.trim().parse::<f32>().ok()).collect::<Vec<_>>())
            .filter(|v| v.len() == 3)
            .map_or(Vec3::ZERO, |v| vec3(v[0], v[1], v[2]));
        let angles = flag_value(&raw, "--angles")
            .map(|v| v.split(',').filter_map(|p| p.trim().parse::<f32>().ok()).collect::<Vec<_>>())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec![0.]);
        Self {
            out: flag_value(&raw, "--out").map(PathBuf::from),
            angles,
            pitch: num("--pitch", 15.),
            dist: num("--dist", 2.),
            at,
            fov: num("--fov", 45.),
            what: flag_value(&raw, "--what").unwrap_or("").to_string(),
            raw,
        }
    }
    pub fn has(&self, flag: &str) -> bool {
        has_flag(&self.raw, flag)
    }
}

/// Window configuration for a preview (size from `--size WxH`, default 1280x720). Built on the engine's own
/// window config so the draw-call vertex/index capacities are the engine's 30,000 (macroquad's defaults are
/// 10,000/5,000 and silently clamp a larger mesh).
pub fn window_conf(title: &str) -> macroquad::conf::Conf {
    let args: Vec<String> = std::env::args().collect();
    let (w, h) = flag_value(&args, "--size").and_then(vesper3d::viewer::devkit::parse_size).unwrap_or((1280, 720));
    let mut conf = vesper3d::viewer::game_client::window_config(title);
    conf.miniquad_conf.window_width = w as i32;
    conf.miniquad_conf.window_height = h as i32;
    conf.miniquad_conf.high_dpi = false;
    conf
}

/// The world material and a neutral daylight to judge colours by.
pub struct Stage {
    pub materials: Materials,
    pub look: Look,
    pub batch: Batch,
}

impl Stage {
    pub fn new() -> Self {
        Self {
            materials: Materials::load().expect("the materials failed to compile"),
            look: Look::daylight(),
            batch: Batch::new(),
        }
    }

    /// Draw `items` from a camera orbiting `args.at` at yaw `yaw_deg`, on a flat grey floor.
    pub fn draw(&mut self, args: &Args, yaw_deg: f32, items: &[(&Template, Mat4)]) {
        let yaw = yaw_deg.to_radians();
        let pitch = args.pitch.to_radians();
        let eye = args.at + vec3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()) * args.dist;
        // Look from the eye towards the target: the view's yaw/pitch face the opposite way to the offset.
        let to = (args.at - eye).normalize();
        let view = View { eye, yaw: to.x.atan2(-to.z), pitch: to.y.asin(), roll: 0., fov: args.fov.to_radians() };
        clear_background(self.look.clear_color());
        set_camera(&view.camera(0.02, 400.));
        self.materials.set_scene(&self.look, eye, 0., 0.);
        self.batch.clear();
        for (template, transform) in items {
            self.batch.add(template, *transform, Tint::NONE);
        }
        gl_use_material(&self.materials.world);
        self.batch.draw();
        gl_use_default_material();
        set_default_camera();
    }
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

/// A floor slab to stand things on, so scale and ground contact are visible.
pub fn floor(size: f32) -> Template {
    let mut t = Template::new();
    t.box_top(vec3(0., -0.05, 0.), vec3(size, 0.05, size), [0.32, 0.33, 0.35], [0.42, 0.43, 0.45], 0.);
    t
}

/// Save the frame to `dir/<name>.png`, creating `dir`; returns the path.
pub fn shot(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join(format!("{name}.png"));
    if let Err(e) = kit::capture::save_frame(&path) {
        eprintln!("cannot save {}: {e}", path.display());
    }
    path
}
