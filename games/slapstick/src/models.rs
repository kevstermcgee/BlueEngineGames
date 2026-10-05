//! Every mesh drawn, built once from `kit::Template` primitives: the table surface, centre line and
//! rails (with a real gap at each goal mouth matching the physics in `sim.rs` exactly — no wall is drawn
//! across a goal opening), the puck, a paddle per cosmetic colour, and a thin goal-mouth glow marker.
//! Presentation only; nothing here touches the simulation.
use macroquad::prelude::*;
use slapstick::{GOAL_HALF_WIDTH, PADDLE_RADIUS, PUCK_RADIUS, TABLE_HALF_LENGTH, TABLE_HALF_WIDTH};
use vesper3d::viewer::kit::{Rgb, Template};

const RAIL_HEIGHT: f32 = 0.06;
const RAIL_WIDTH: f32 = 0.035;

/// The playing surface, centre line/circle and rail walls, baked once (static geometry).
pub fn table() -> Vec<Mesh> {
    let mut t = Template::new();
    t.box_top(
        vec3(0., -0.02, 0.),
        vec3(TABLE_HALF_WIDTH, 0.02, TABLE_HALF_LENGTH),
        [0.05, 0.05, 0.07],
        [0.07, 0.30, 0.48],
        0.1,
    );
    // Centre line and circle.
    t.box_top(
        vec3(0., 0.001, 0.),
        vec3(TABLE_HALF_WIDTH * 0.97, 0.0015, 0.008),
        [0.85, 0.85, 0.9],
        [0.85, 0.85, 0.9],
        0.25,
    );
    t.ring(vec3(0., 0.0015, 0.), 0.16, 0.18, [0.85, 0.85, 0.9], 0.25, 32);
    // Rails along the long edges, full length.
    for side in [-1f32, 1.] {
        t.box_(
            vec3(side * (TABLE_HALF_WIDTH + RAIL_WIDTH), RAIL_HEIGHT * 0.5, 0.),
            vec3(RAIL_WIDTH, RAIL_HEIGHT * 0.5, TABLE_HALF_LENGTH + RAIL_WIDTH),
            [0.55, 0.08, 0.12],
            0.05,
        );
    }
    // End rails, split either side of each goal mouth: an actual gap matching `sim.rs`'s physics exactly.
    let seg_half = (TABLE_HALF_WIDTH - GOAL_HALF_WIDTH) * 0.5;
    for end in [-1f32, 1.] {
        let z = end * (TABLE_HALF_LENGTH + RAIL_WIDTH);
        for side in [-1f32, 1.] {
            let x = side * (GOAL_HALF_WIDTH + seg_half);
            t.box_(
                vec3(x, RAIL_HEIGHT * 0.5, z),
                vec3(seg_half, RAIL_HEIGHT * 0.5, RAIL_WIDTH),
                [0.55, 0.08, 0.12],
                0.05,
            );
        }
    }
    t.to_meshes()
}

/// A short glowing cylinder: the puck.
pub fn puck() -> Template {
    let mut t = Template::new();
    t.cylinder(vec3(0., 0., 0.), PUCK_RADIUS, 0.03, [0.06, 0.06, 0.08], 0.08, 20);
    t.disc(vec3(0., 0.03, 0.), PUCK_RADIUS, [0.12, 0.12, 0.15], 0.05, 20);
    t
}

/// A squat paddle with a domed cap, tinted by cosmetic colour.
pub fn paddle(color: Rgb) -> Template {
    let mut t = Template::new();
    t.cylinder(vec3(0., 0., 0.), PADDLE_RADIUS * 0.85, 0.045, color, 0.08, 24);
    t.ball(
        vec3(0., 0.045, 0.),
        vec3(PADDLE_RADIUS * 0.68, PADDLE_RADIUS * 0.4, PADDLE_RADIUS * 0.68),
        color,
        0.2,
        20,
        10,
    );
    t
}

/// The four cosmetic paddle colours (`AirHockeyGame::CHOICES`).
pub fn paddle_color(choice: u8) -> Rgb {
    match choice % 4 {
        0 => [0.85, 0.2, 0.2],
        1 => [0.2, 0.5, 0.95],
        2 => [0.25, 0.8, 0.35],
        _ => [0.95, 0.78, 0.15],
    }
}

/// A thin glowing strip across one goal mouth, placed and tinted by the caller (brighter on a goal).
pub fn goal_glow() -> Template {
    let mut t = Template::new();
    t.box_top(vec3(0., 0.005, 0.), vec3(GOAL_HALF_WIDTH, 0.003, 0.012), [0.3, 0.3, 0.3], [1., 0.95, 0.6], 0.8);
    t
}
