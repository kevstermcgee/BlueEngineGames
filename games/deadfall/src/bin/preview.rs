//! `preview --what floor --out DIR`: the smallest stage, to prove the harness renders.
use deadfall::client::previewkit::{self, Args, Stage};
use macroquad::prelude::*;
use vesper3d::viewer::kit::Template;

fn window() -> Conf {
    previewkit::window_conf("Deadfall preview")
}

#[macroquad::main(window)]
async fn main() {
    let args = Args::parse();
    let mut stage = Stage::new();
    let floor = previewkit::floor(4.);
    let mut cube = Template::new();
    cube.box_(vec3(0., 0.5, 0.), vec3(0.5, 0.5, 0.5), [0.8, 0.3, 0.2], 0.);
    let mut frame = 0;
    let mut shots = args.angles.iter();
    loop {
        let yaw = args.angles.get(frame / 4).copied().unwrap_or(0.);
        stage.draw(&args, yaw, &[(&floor, Mat4::IDENTITY), (&cube, Mat4::IDENTITY)]);
        frame += 1;
        if let Some(out) = &args.out {
            if frame % 4 == 3 {
                previewkit::shot(out, &format!("angle-{:03}", yaw as i32));
                if shots.next().is_none() || frame / 4 + 1 >= args.angles.len() {
                    break;
                }
            }
        }
        next_frame().await;
    }
}
