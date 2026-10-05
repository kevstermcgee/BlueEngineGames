//! Every mesh drawn, built once from primitives: the house's walls and floors, one mesh per
//! [`DisguiseKind`] (used identically for a disguised hider and for every decoy `layout::decoys()`
//! scatters around, so the two are visually indistinguishable by construction — there is no separate
//! "real" shape to accidentally render differently), and a plain humanoid for the seeker and for a hider
//! who has not yet confirmed a disguise. Presentation only; nothing here touches the simulation.
use macroquad::prelude::*;
use prop_hunt::{layout, DisguiseKind};
use vesper3d::viewer::controller::Collider;
use vesper3d::viewer::kit::Template;

/// One mesh per [`DisguiseKind`], sized from its real `half_extents` (the same numbers `disguise.rs`
/// pulls from the engine's own prop catalog). Simple primitives, not the engine's authored prop art: a
/// custom-sim game draws its own meshes (see `AGENTS.md`); what matters for the game is that this
/// function is the *only* place a disguise's shape comes from, for a hider and a decoy alike.
pub fn disguise_mesh(kind: DisguiseKind) -> Template {
    let h = kind.half_extents();
    let (hx, hy, hz) = (h.0, h.1, h.2);
    let mut t = Template::new();
    match kind {
        DisguiseKind::Chair => {
            t.box_(vec3(0., hy, 0.), vec3(hx, hy * 0.15, hz), [0.45, 0.28, 0.14], 0.);
            for (x, z) in [(-hx * 0.8, -hz * 0.8), (hx * 0.8, -hz * 0.8), (-hx * 0.8, hz * 0.8), (hx * 0.8, hz * 0.8)] {
                t.box_(vec3(x, hy * 0.5, z), vec3(hx * 0.1, hy * 0.5, hz * 0.1), [0.3, 0.18, 0.1], 0.);
            }
            t.box_(vec3(0., hy * 1.6, hz * 0.8), vec3(hx, hy * 0.6, hz * 0.12), [0.45, 0.28, 0.14], 0.);
        }
        DisguiseKind::TableLamp => {
            t.cylinder(vec3(0., hy * 0.1, 0.), hx * 0.7, hy * 0.2, [0.2, 0.2, 0.22], 0., 10);
            t.cylinder(vec3(0., hy * 0.6, 0.), hx * 0.12, hy * 0.8, [0.3, 0.3, 0.32], 0., 8);
            t.cylinder(vec3(0., hy * 1.5, 0.), hx, hy * 0.7, [0.92, 0.85, 0.65], 0.4, 12);
        }
        DisguiseKind::BookStack => {
            for (i, c) in [[0.6, 0.15, 0.15], [0.15, 0.4, 0.2], [0.2, 0.25, 0.55]].into_iter().enumerate() {
                t.box_(vec3(0., hy * (0.33 + i as f32 * 0.66), 0.), vec3(hx, hy * 0.33, hz), c, 0.);
            }
        }
        DisguiseKind::CandleTrio => {
            t.box_(vec3(0., hy * 0.15, 0.), vec3(hx, hy * 0.15, hz), [0.5, 0.42, 0.3], 0.);
            for (x, z) in [(-hx * 0.5, 0.), (hx * 0.5, -hz * 0.3), (0., hz * 0.5)] {
                t.cylinder(vec3(x, hy * 0.6, z), hx * 0.18, hy * 0.7, [0.92, 0.88, 0.75], 0.5, 8);
            }
        }
        DisguiseKind::PottedCactus => {
            t.cylinder(vec3(0., hy * 0.25, 0.), hx * 0.8, hy * 0.5, [0.55, 0.3, 0.2], 0., 10);
            t.ball(vec3(0., hy * 1.1, 0.), vec3(hx * 0.5, hy * 0.8, hz * 0.5), [0.2, 0.5, 0.25], 0., 10, 8);
        }
        DisguiseKind::FlowerVase => {
            t.cylinder(vec3(0., hy * 0.4, 0.), hx * 0.6, hy * 0.8, [0.85, 0.85, 0.9], 0.2, 10);
            t.ball(vec3(0., hy * 1.5, 0.), vec3(hx * 0.8, hy * 0.6, hz * 0.8), [0.9, 0.5, 0.6], 0., 10, 8);
        }
        DisguiseKind::TallVase => {
            t.cylinder(vec3(0., hy, 0.), hx * 0.7, hy * 2., [0.3, 0.45, 0.4], 0.15, 12);
        }
        DisguiseKind::MantelClock => {
            t.box_(vec3(0., hy, 0.), vec3(hx, hy, hz), [0.35, 0.22, 0.12], 0.);
            t.cylinder(vec3(0., hy * 1.6, hz * 0.6), hx * 0.5, hz * 0.1, [0.9, 0.85, 0.7], 0.3, 12);
        }
        DisguiseKind::WovenBasket => {
            t.cylinder(vec3(0., hy * 0.5, 0.), hx, hy, [0.55, 0.42, 0.25], 0., 12);
        }
        DisguiseKind::Bowl => {
            t.cylinder(vec3(0., hy * 0.4, 0.), hx, hy * 0.8, [0.8, 0.78, 0.7], 0.1, 14);
        }
    }
    t
}

/// A plain humanoid, facing -Z with its feet on y = 0: the seeker, and a hider who has not yet confirmed
/// a disguise.
pub fn humanoid(color: [f32; 3]) -> Template {
    let mut t = Template::new();
    t.box_(vec3(0., 0.9, 0.), vec3(0.22, 0.5, 0.14), color, 0.);
    t.ball(vec3(0., 1.6, 0.), vec3(0.16, 0.18, 0.15), [0.8, 0.7, 0.6], 0., 10, 8);
    for x in [-0.28, 0.28] {
        t.box_(vec3(x, 0.9, 0.), vec3(0.06, 0.45, 0.06), color, 0.);
    }
    for x in [-0.11, 0.11] {
        t.box_(vec3(x, 0.25, 0.), vec3(0.08, 0.42, 0.08), [0.15, 0.15, 0.2], 0.);
    }
    t
}

// A lived-in house, not a horror game: wall, floor and ceiling colours are ordinary paint and timber
// tones, bright enough to read clearly under `house_look`'s daylight-based lighting.
fn wall_box(t: &mut Template, wall: &Collider) {
    let center = (wall.min + wall.max) * 0.5;
    let half = (wall.max - wall.min) * 0.5;
    t.box_(vec3(center.0, center.1, center.2), vec3(half.0, half.1, half.2), [0.72, 0.68, 0.60], 0.);
}

fn floor_and_ceiling(t: &mut Template, room: (f32, f32, f32, f32), floor: [f32; 3]) {
    let (x0, x1, z0, z1) = room;
    let (cx, cz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
    let (hx, hz) = ((x1 - x0) * 0.5, (z1 - z0) * 0.5);
    t.box_top(vec3(cx, -0.05, cz), vec3(hx, 0.05, hz), [0.30, 0.29, 0.28], floor, 0.);
    t.box_(vec3(cx, layout::WALL_HEIGHT, cz), vec3(hx, 0.05, hz), [0.85, 0.84, 0.80], 0.);
}

/// Static geometry: every wall (matching `layout::colliders()` exactly) and a floor/ceiling per room.
/// Decoy props and legal spots are drawn separately, from `layout::decoys()` and `disguise_mesh`.
pub fn house() -> Vec<Mesh> {
    let mut t = Template::new();
    for wall in layout::colliders() {
        wall_box(&mut t, &wall);
    }
    type RoomFloor = ((f32, f32, f32, f32), [f32; 3]);
    let rooms: [RoomFloor; 6] = [
        (layout::LIVING_ROOM, [0.55, 0.45, 0.32]),
        (layout::DEN, [0.42, 0.42, 0.48]),
        (layout::KITCHEN, [0.50, 0.46, 0.38]),
        (layout::BEDROOM, [0.38, 0.38, 0.48]),
        (layout::GARAGE, [0.35, 0.35, 0.36]),
        (layout::LAUNDRY, [0.40, 0.44, 0.46]),
    ];
    for (room, floor) in rooms {
        floor_and_ceiling(&mut t, room, floor);
    }
    let (cx0, cx1) = layout::CORRIDOR_X;
    let (cz0, cz1) = layout::CORRIDOR_Z;
    floor_and_ceiling(&mut t, (cx0, cx1, cz0, cz1), [0.42, 0.40, 0.42]);
    let (yx0, yx1, yz0, yz1) = layout::YARD;
    t.box_top(
        vec3((yx0 + yx1) * 0.5, -0.05, (yz0 + yz1) * 0.5),
        vec3((yx1 - yx0) * 0.5, 0.05, (yz1 - yz0) * 0.5),
        [0.25, 0.24, 0.20],
        [0.30, 0.42, 0.26],
        0.,
    );
    t.to_meshes()
}

/// A plain daytime sky, visible through the yard's open roof: a lived-in house, not a horror game.
pub fn sky() -> Vec<Mesh> {
    let mut t = Template::new();
    t.sky_dome(
        250.,
        |e| {
            let s = e.clamp(0., 1.).sqrt();
            [0.55 + 0.20 * (1. - s), 0.68 + 0.14 * (1. - s), 0.85 + 0.05 * (1. - s)]
        },
        28,
        14,
    );
    t.to_meshes()
}
