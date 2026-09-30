//! Everything drawn, built once from primitives: Haunted Hollow's ground, road, walls and scenery, and the
//! eight karts with their drivers. Presentation only; nothing here touches the simulation.
use macroquad::prelude::*;
use spooky_kart::track::{HALF_WIDTH, SHOULDER};
use spooky_kart::{Character, Track};
use std::f32::consts::{FRAC_PI_2, PI};
use vesper3d::viewer::devkit::Rng;
use vesper3d::viewer::kit::Template;

type Rgb = [f32; 3];

/// A driver's colours.
struct Palette {
    body: Rgb,
    trim: Rgb,
    skin: Rgb,
    cloth: Rgb,
    glow: Rgb,
}

pub fn colour(c: Character) -> Rgb {
    palette(c).body
}

fn palette(c: Character) -> Palette {
    match c {
        Character::Vampire => Palette {
            body: [0.45, 0.04, 0.10],
            trim: [0.08, 0.05, 0.10],
            skin: [0.85, 0.82, 0.88],
            cloth: [0.10, 0.03, 0.08],
            glow: [1.0, 0.1, 0.2],
        },
        Character::Frankenstein => Palette {
            body: [0.32, 0.36, 0.40],
            trim: [0.20, 0.22, 0.26],
            skin: [0.45, 0.68, 0.40],
            cloth: [0.12, 0.12, 0.16],
            glow: [0.5, 0.9, 1.0],
        },
        Character::Mummy => Palette {
            body: [0.78, 0.68, 0.45],
            trim: [0.55, 0.42, 0.22],
            skin: [0.90, 0.88, 0.78],
            cloth: [0.88, 0.85, 0.72],
            glow: [1.0, 0.85, 0.3],
        },
        Character::Ghost => Palette {
            body: [0.62, 0.78, 0.95],
            trim: [0.85, 0.92, 1.0],
            skin: [0.92, 0.96, 1.0],
            cloth: [0.92, 0.96, 1.0],
            glow: [0.6, 0.9, 1.0],
        },
        Character::Scarecrow => Palette {
            body: [0.70, 0.42, 0.12],
            trim: [0.45, 0.28, 0.10],
            skin: [0.75, 0.62, 0.38],
            cloth: [0.35, 0.22, 0.42],
            glow: [1.0, 0.6, 0.1],
        },
        Character::Zombie => Palette {
            body: [0.28, 0.42, 0.22],
            trim: [0.16, 0.20, 0.14],
            skin: [0.52, 0.66, 0.42],
            cloth: [0.30, 0.28, 0.40],
            glow: [0.6, 1.0, 0.3],
        },
        Character::Clown => Palette {
            body: [0.85, 0.15, 0.20],
            trim: [0.95, 0.95, 0.95],
            skin: [0.96, 0.92, 0.90],
            cloth: [0.25, 0.30, 0.85],
            glow: [1.0, 0.3, 0.6],
        },
        Character::Skeleton => Palette {
            body: [0.80, 0.80, 0.74],
            trim: [0.35, 0.16, 0.50],
            skin: [0.92, 0.92, 0.86],
            cloth: [0.15, 0.10, 0.20],
            glow: [0.7, 0.3, 1.0],
        },
    }
}

fn wheel(t: &mut Template, at: Vec3) {
    let mut w = Template::new();
    w.cylinder(vec3(0., -0.15, 0.), 0.4, 0.3, [0.05, 0.05, 0.06], 0., 14);
    w.cylinder(vec3(0., -0.17, 0.), 0.22, 0.34, [0.5, 0.5, 0.55], 0.05, 10);
    t.append(&w.transformed(Mat4::from_translation(at) * Mat4::from_rotation_z(FRAC_PI_2)));
}

/// A kart and its driver, facing -Z with its wheels on y = 0.
pub fn kart(c: Character) -> Template {
    let p = palette(c);
    let mut t = Template::new();
    // Chassis, nose and rear wing.
    t.box_(vec3(0., 0.55, 0.1), vec3(0.78, 0.22, 1.25), p.body, 0.);
    t.box_(vec3(0., 0.5, -1.45), vec3(0.5, 0.16, 0.35), p.trim, 0.);
    t.box_(vec3(0., 1.05, 1.25), vec3(0.75, 0.05, 0.2), p.trim, 0.);
    for x in [-0.6, 0.6] {
        t.box_(vec3(x, 0.85, 1.25), vec3(0.05, 0.22, 0.05), p.trim, 0.);
    }
    for (x, z) in [(-0.88, -0.95), (0.88, -0.95), (-0.9, 1.0), (0.9, 1.0)] {
        wheel(&mut t, vec3(x, 0.4, z));
    }
    // Headlights glow in the driver's colour.
    for x in [-0.32, 0.32] {
        t.ball(vec3(x, 0.55, -1.8), vec3(0.1, 0.1, 0.06), p.glow, 0.9, 8, 5);
    }
    // Seat and driver.
    t.box_(vec3(0., 0.95, 0.3), vec3(0.42, 0.18, 0.3), p.cloth, 0.);
    t.box_(vec3(0., 1.35, 0.3), vec3(0.32, 0.36, 0.22), p.cloth, 0.);
    let head = vec3(0., 1.95, 0.25);
    match c {
        Character::Vampire => {
            t.ball(head, vec3(0.3, 0.32, 0.3), p.skin, 0., 12, 8);
            t.ball(head + vec3(0., 0.16, -0.02), vec3(0.31, 0.16, 0.31), [0.04, 0.03, 0.06], 0., 10, 6); // slicked hair
            t.box_(vec3(0., 1.4, 0.55), vec3(0.5, 0.5, 0.05), [0.5, 0.03, 0.08], 0.); // cape
            t.box_(vec3(0., 1.72, 0.32), vec3(0.42, 0.14, 0.14), [0.5, 0.03, 0.08], 0.); // high collar
            for x in [-0.09, 0.09] {
                t.cone(head + vec3(x, -0.2, -0.27), 0.03, 0.0, 0.1, [1., 1., 1.], 0.2, 6); // fangs
                t.ball(head + vec3(x, 0.04, -0.28), vec3(0.05, 0.03, 0.03), p.glow, 1., 6, 4);
            }
        }
        Character::Frankenstein => {
            t.box_(head + vec3(0., 0.03, 0.), vec3(0.36, 0.34, 0.32), p.skin, 0.); // flat head
            t.box_(head + vec3(0., 0.33, 0.), vec3(0.37, 0.09, 0.33), [0.05, 0.05, 0.07], 0.); // hair
            for x in [-0.4f32, 0.4] {
                t.cylinder(head + vec3(x - 0.06 * x.signum(), -0.1, 0.), 0.05, 0.16, [0.6, 0.6, 0.65], 0.1, 8); // bolts
                t.ball(head + vec3(x, -0.02, 0.), vec3(0.06, 0.06, 0.06), [0.7, 0.7, 0.75], 0.1, 8, 5);
            }
            t.box_(head + vec3(0., -0.02, -0.33), vec3(0.28, 0.03, 0.02), [0.1, 0.05, 0.08], 0.); // scar
            for x in [-0.12, 0.12] {
                t.ball(head + vec3(x, 0.06, -0.33), vec3(0.06, 0.05, 0.03), p.glow, 0.9, 6, 4);
            }
        }
        Character::Mummy => {
            t.ball(head, vec3(0.31, 0.33, 0.31), p.skin, 0., 12, 8);
            for y in [-0.2, -0.05, 0.1, 0.25] {
                t.box_(head + vec3(0., y, 0.), vec3(0.33, 0.035, 0.33), p.cloth, 0.);
            }
            t.box_(head + vec3(0., 0.07, -0.3), vec3(0.2, 0.05, 0.03), p.glow, 1.); // eye slit
            t.box_(vec3(0., 1.35, 0.3), vec3(0.34, 0.38, 0.24), p.cloth, 0.); // wrapped torso
            for y in [1.15, 1.35, 1.55] {
                t.box_(vec3(0., y, 0.3), vec3(0.35, 0.03, 0.25), [0.6, 0.55, 0.42], 0.);
            }
        }
        Character::Ghost => {
            t.cone(vec3(0., 0.95, 0.25), 0.52, 0.28, 0.75, [0.95, 0.98, 1.], 0.5, 12); // the sheet
            t.ball(head + vec3(0., -0.05, 0.), vec3(0.36, 0.38, 0.36), [0.95, 0.98, 1.], 0.6, 12, 8);
            for x in [-0.12, 0.12] {
                t.ball(head + vec3(x, 0.02, -0.32), vec3(0.07, 0.11, 0.04), [0.02, 0.02, 0.08], 0., 8, 5);
            }
            t.ball(head + vec3(0., -0.2, -0.33), vec3(0.06, 0.08, 0.04), [0.02, 0.02, 0.08], 0., 8, 5);
        }
        Character::Scarecrow => {
            t.ball(head, vec3(0.3, 0.31, 0.3), p.skin, 0., 12, 8);
            t.cone(head + vec3(0., 0.2, 0.), 0.28, 0.14, 0.42, [0.72, 0.58, 0.25], 0., 10); // hat crown
            t.ring(head + vec3(0., 0.2, 0.), 0.28, 0.55, [0.72, 0.58, 0.25], 0., 14); // brim
            for (x, y) in [(-0.3, -0.12), (0.3, -0.1), (-0.25, -0.3), (0.28, -0.28)] {
                t.box_(head + vec3(x, y, 0.05), vec3(0.14, 0.02, 0.02), [0.85, 0.7, 0.3], 0.);
                // straw
            }
            for x in [-0.11, 0.11] {
                t.ball(head + vec3(x, 0.04, -0.28), vec3(0.05, 0.05, 0.03), [0.05, 0.03, 0.02], 0., 6, 4);
            }
            t.box_(head + vec3(0., -0.1, -0.29), vec3(0.16, 0.02, 0.02), [0.1, 0.05, 0.03], 0.); // stitched smile
            t.box_(vec3(0.42, 1.35, 0.3), vec3(0.06, 0.06, 0.6), [0.35, 0.22, 0.1], 0.);
            // arm pole
        }
        Character::Zombie => {
            t.ball(head, vec3(0.3, 0.32, 0.3), p.skin, 0., 12, 8);
            t.ball(head + vec3(0.08, 0.2, 0.05), vec3(0.14, 0.08, 0.14), [0.20, 0.30, 0.16], 0., 8, 5); // patchy scalp
            for (x, s) in [(-0.11, 0.08), (0.11, 0.06)] {
                t.ball(head + vec3(x, 0.05, -0.28), vec3(s, s, 0.04), [0.95, 0.95, 0.7], 0.3, 8, 5);
                t.ball(head + vec3(x, 0.05, -0.31), vec3(0.025, 0.025, 0.02), [0.4, 0.05, 0.05], 0.2, 6, 4);
            }
            t.box_(head + vec3(0., -0.12, -0.28), vec3(0.14, 0.03, 0.02), [0.25, 0.04, 0.05], 0.);
            t.box_(vec3(0.45, 1.3, 0.05), vec3(0.08, 0.08, 0.42), p.skin, 0.); // reaching arm
        }
        Character::Clown => {
            t.ball(head, vec3(0.31, 0.33, 0.31), p.skin, 0., 12, 8);
            t.ball(head + vec3(0., -0.02, -0.33), vec3(0.11, 0.11, 0.11), [0.95, 0.1, 0.1], 0.6, 10, 6); // nose
            for (x, col) in [(-0.34, [1.0, 0.45, 0.05]), (0.34, [0.2, 0.7, 0.2])] {
                t.ball(head + vec3(x, 0.12, 0.), vec3(0.17, 0.17, 0.17), col, 0., 8, 6);
                // hair puffs
            }
            t.cone(head + vec3(0., 0.28, 0.), 0.16, 0.0, 0.42, [0.2, 0.3, 0.9], 0., 8); // party hat
            for x in [-0.12, 0.12] {
                t.ball(head + vec3(x, 0.08, -0.29), vec3(0.06, 0.07, 0.03), [0.03, 0.03, 0.05], 0., 6, 4);
            }
            t.box_(head + vec3(0., -0.14, -0.29), vec3(0.2, 0.04, 0.02), [0.9, 0.1, 0.15], 0.2); // grin
            t.ball(vec3(0., 1.72, 0.05), vec3(0.14, 0.14, 0.1), [0.9, 0.9, 0.2], 0., 8, 5);
            // ruffle
        }
        Character::Skeleton => {
            t.ball(head, vec3(0.3, 0.32, 0.3), p.skin, 0., 12, 8); // skull
            t.box_(head + vec3(0., -0.24, -0.1), vec3(0.2, 0.09, 0.14), p.skin, 0.); // jaw
            for x in [-0.12, 0.12] {
                t.ball(head + vec3(x, 0.05, -0.27), vec3(0.08, 0.09, 0.05), [0.03, 0.02, 0.05], 0., 8, 5);
                t.ball(head + vec3(x, 0.05, -0.31), vec3(0.03, 0.03, 0.02), p.glow, 1., 6, 4);
            }
            for y in [1.15, 1.3, 1.45, 1.6] {
                t.box_(vec3(0., y, 0.05), vec3(0.3, 0.025, 0.05), p.skin, 0.); // ribs
            }
            t.box_(vec3(0., 1.38, 0.05), vec3(0.03, 0.26, 0.05), p.skin, 0.); // spine
        }
    }
    t
}

/// A bandage strip lying on the road.
pub fn bandage() -> Template {
    let mut t = Template::new();
    t.box_(vec3(0., 0.05, 0.), vec3(2.2, 0.04, 1.4), [0.9, 0.87, 0.72], 0.15);
    for z in [-0.9, -0.3, 0.3, 0.9] {
        t.box_(vec3(0., 0.09, z), vec3(2.25, 0.02, 0.07), [0.55, 0.5, 0.38], 0.);
    }
    t
}

/// A bone lying on the road.
pub fn bone() -> Template {
    let mut t = Template::new();
    let mut b = Template::new();
    b.cylinder(vec3(0., -0.9, 0.), 0.1, 1.8, [0.92, 0.92, 0.85], 0.3, 8);
    for y in [-0.9, 0.9] {
        b.ball(vec3(0.07, y, 0.), vec3(0.17, 0.15, 0.15), [0.92, 0.92, 0.85], 0.3, 8, 5);
        b.ball(vec3(-0.07, y, 0.), vec3(0.17, 0.15, 0.15), [0.92, 0.92, 0.85], 0.3, 8, 5);
    }
    t.append(&b.transformed(Mat4::from_translation(vec3(0., 0.3, 0.)) * Mat4::from_rotation_z(FRAC_PI_2)));
    t.ball(vec3(0., 0.5, 0.), vec3(0.6, 0.06, 0.6), [0.7, 0.2, 0.9], 0.8, 10, 4); // warning glow
    t
}

/// A jack-o'-lantern for the wall.
fn pumpkin() -> Template {
    let mut t = Template::new();
    t.ball(vec3(0., 0.45, 0.), vec3(0.55, 0.45, 0.55), [0.95, 0.42, 0.05], 0.55, 12, 8);
    t.cylinder(vec3(0., 0.85, 0.), 0.07, 0.2, [0.25, 0.4, 0.1], 0., 6);
    t.box_(vec3(-0.17, 0.5, -0.5), vec3(0.09, 0.09, 0.03), [1., 0.85, 0.3], 1.);
    t.box_(vec3(0.17, 0.5, -0.5), vec3(0.09, 0.09, 0.03), [1., 0.85, 0.3], 1.);
    t.box_(vec3(0., 0.28, -0.52), vec3(0.24, 0.05, 0.03), [1., 0.85, 0.3], 1.);
    t
}

fn gravestone() -> Template {
    let mut t = Template::new();
    t.box_(vec3(0., 0.7, 0.), vec3(0.5, 0.7, 0.14), [0.32, 0.32, 0.38], 0.);
    t.ball(vec3(0., 1.4, 0.), vec3(0.5, 0.32, 0.14), [0.32, 0.32, 0.38], 0., 10, 5);
    t.box_(vec3(0., 1.0, -0.15), vec3(0.06, 0.28, 0.02), [0.15, 0.15, 0.2], 0.);
    t.box_(vec3(0., 1.12, -0.15), vec3(0.2, 0.06, 0.02), [0.15, 0.15, 0.2], 0.);
    t
}

fn dead_tree(rng: &mut Rng) -> Template {
    let mut t = Template::new();
    let h = rng.range(4., 8.);
    t.cone(vec3(0., 0., 0.), 0.55, 0.22, h, [0.16, 0.11, 0.09], 0., 7);
    for i in 0..4 {
        let a = rng.range(0., 2. * PI);
        let y = h * (0.45 + 0.13 * i as f32);
        let mut b = Template::new();
        b.cone(vec3(0., 0., 0.), 0.18, 0.04, rng.range(1.6, 2.6), [0.16, 0.11, 0.09], 0., 5);
        t.append(&b.transformed(
            Mat4::from_translation(vec3(0., y, 0.)) * Mat4::from_rotation_y(a) * Mat4::from_rotation_z(-1.0),
        ));
    }
    t
}

fn lantern_post() -> Template {
    let mut t = Template::new();
    t.cylinder(vec3(0., 0., 0.), 0.1, 3.2, [0.12, 0.1, 0.14], 0., 6);
    t.ball(vec3(0., 3.4, 0.), vec3(0.28, 0.34, 0.28), [0.5, 1.0, 0.45], 1., 8, 6);
    t
}

/// A flat quad wound so it faces the way `normal` says, whatever order the corners came in.
fn face(t: &mut Template, q: [Vec3; 4], normal: Vec3, c: Rgb, e: f32) {
    let geometric = (q[1] - q[0]).cross(q[2] - q[0]);
    if geometric.dot(normal) >= 0. {
        t.quad(q, normal, c, e);
    } else {
        t.quad([q[3], q[2], q[1], q[0]], normal, c, e);
    }
}

/// Packs small pieces into templates that stay under the engine's per-mesh vertex limit (a template over
/// the limit is skipped without a word, so the whole world would silently vanish).
struct Chunks {
    done: Vec<Template>,
    cur: Template,
    verts: usize,
    indices: usize,
}

impl Chunks {
    const VERTS: usize = 8_000;
    const INDICES: usize = 24_000;

    fn new() -> Self {
        Self { done: Vec::new(), cur: Template::new(), verts: 0, indices: 0 }
    }

    fn add(&mut self, piece: &Template) {
        let (v, i) = piece.to_meshes().iter().fold((0, 0), |a, m| (a.0 + m.vertices.len(), a.1 + m.indices.len()));
        if self.verts + v > Self::VERTS || self.indices + i > Self::INDICES {
            self.flush();
        }
        self.cur.append(piece);
        self.verts += v;
        self.indices += i;
    }

    fn flush(&mut self) {
        if !self.cur.is_empty() {
            self.done.push(std::mem::take(&mut self.cur));
        }
        self.verts = 0;
        self.indices = 0;
    }

    fn finish(mut self) -> Vec<Template> {
        self.flush();
        self.done
    }
}

/// Ground, road, walls and scenery for Haunted Hollow, as templates that each fit one mesh.
pub fn world(track: &Track) -> Vec<Template> {
    let mut out = Chunks::new();
    // The grass: one large slab.
    let mut ground = Template::new();
    face(
        &mut ground,
        [vec3(-700., -0.06, -700.), vec3(700., -0.06, -700.), vec3(700., -0.06, 700.), vec3(-700., -0.06, 700.)],
        Vec3::Y,
        [0.05, 0.11, 0.08],
        0.,
    );
    out.add(&ground);
    let pts = track.samples();
    let n = pts.len();
    let edge = |i: usize, lateral: f32| {
        let (a, b) = (pts[i % n], pts[(i + 1) % n]);
        let tangent = (b - a).norm();
        let right = vec3(-tangent.2, 0., tangent.0);
        vec3(a.0, 0., a.2) + right * lateral
    };
    let wall = HALF_WIDTH + SHOULDER;
    for i in 0..n {
        let mut t = Template::new();
        let shade = if (i / 3) % 2 == 0 { 0.0 } else { 0.012 };
        // Road.
        face(
            &mut t,
            [edge(i, -HALF_WIDTH), edge(i, HALF_WIDTH), edge(i + 1, HALF_WIDTH), edge(i + 1, -HALF_WIDTH)],
            Vec3::Y,
            [0.16 + shade, 0.15 + shade, 0.19 + shade],
            0.,
        );
        for side in [-1., 1.] {
            // Verges.
            face(
                &mut t,
                [
                    edge(i, side * HALF_WIDTH),
                    edge(i, side * wall),
                    edge(i + 1, side * wall),
                    edge(i + 1, side * HALF_WIDTH),
                ],
                Vec3::Y,
                [0.10, 0.20, 0.10],
                0.,
            );
            // Kerb stripes on the road edge, alternating orange and bone.
            let kerb = if (i / 2) % 2 == 0 { [0.95, 0.45, 0.05] } else { [0.9, 0.9, 0.82] };
            let (inner, outer) = (side * (HALF_WIDTH - 0.6), side * HALF_WIDTH);
            face(&mut t, [edge(i, inner), edge(i, outer), edge(i + 1, outer), edge(i + 1, inner)], Vec3::Y, kerb, 0.25);
            // The wall: a low purple barrier with a glowing top rail.
            let (w0, w1) = (edge(i, side * wall), edge(i + 1, side * wall));
            let up = vec3(0., 1.1, 0.);
            let tangent = (w1 - w0).normalize_or_zero();
            let normal = vec3(-tangent.z, 0., tangent.x) * -side;
            face(&mut t, [w0, w1, w1 + up, w0 + up], normal, [0.22, 0.15, 0.32], 0.05);
            let rail = vec3(0., 0.12, 0.);
            face(&mut t, [w0 + up, w1 + up, w1 + up + rail, w0 + up + rail], normal, [0.55, 0.2, 0.9], 0.9);
        }
        out.add(&t);
    }
    // The start line: a chequered strip and two glowing arch posts.
    let mut line = Template::new();
    let (a, b) = (edge(0, 0.), edge(1, 0.));
    let tangent = (b - a).normalize_or_zero();
    let right = vec3(-tangent.z, 0., tangent.x);
    for k in 0..12 {
        for row in 0..2 {
            let col = if (k + row) % 2 == 0 { [0.95, 0.95, 0.95] } else { [0.03, 0.03, 0.04] };
            let x0 = -HALF_WIDTH + k as f32 * (2. * HALF_WIDTH / 12.);
            let x1 = x0 + 2. * HALF_WIDTH / 12.;
            let z0 = row as f32;
            let p = |x: f32, z: f32| a + right * x + tangent * z + vec3(0., 0.02, 0.);
            face(&mut line, [p(x0, z0), p(x1, z0), p(x1, z0 + 1.), p(x0, z0 + 1.)], Vec3::Y, col, 0.1);
        }
    }
    for side in [-1., 1.] {
        let post = a + right * (side * (HALF_WIDTH + 1.5));
        line.cylinder(post, 0.3, 7., [0.2, 0.15, 0.3], 0., 8);
        line.ball(post + vec3(0., 7.3, 0.), vec3(0.6, 0.6, 0.6), [1., 0.5, 0.05], 1., 10, 6);
    }
    let mut bar = Template::new();
    bar.box_(vec3(0., 7.4, 0.), vec3(HALF_WIDTH + 1.6, 0.35, 0.2), [0.95, 0.45, 0.05], 0.8);
    let heading = tangent.x.atan2(-tangent.z);
    line.append(&bar.transformed(Mat4::from_translation(a) * Mat4::from_rotation_y(-heading)));
    out.add(&line);

    // Scenery: seeded, so every peer sees the same hollow. Kept clear of the road.
    let mut rng = Rng::new(0x5EED_CAFE);
    let placed =
        |m: &Template, p: Vec3, yaw: f32| m.transformed(Mat4::from_translation(p) * Mat4::from_rotation_y(yaw));
    let (stone, pump, post) = (gravestone(), pumpkin(), lantern_post());
    let clear_of_road = |p: Vec3, margin: f32| {
        let near = track.nearest_global(spooky_kart_v(p));
        (p - vec3(near.center.0, 0., near.center.2)).length() > wall + margin
    };
    let near_track = |rng: &mut Rng, margin_lo: f32, margin_hi: f32| -> Option<Vec3> {
        let i = rng.below(n);
        let side = rng.sign();
        let p = edge(i, side * (wall + rng.range(margin_lo, margin_hi)));
        clear_of_road(p, 2.).then_some(p)
    };
    for _ in 0..320 {
        if let Some(p) = near_track(&mut rng, 3., 50.) {
            out.add(&placed(&stone, p, rng.range(-0.5, 0.5) + PI));
        }
    }
    for _ in 0..140 {
        if let Some(p) = near_track(&mut rng, 5., 60.) {
            let tree = dead_tree(&mut rng);
            out.add(&placed(&tree, p, rng.range(0., 2. * PI)));
        }
    }
    for i in (0..n).step_by(4) {
        let side = if (i / 4) % 2 == 0 { 1. } else { -1. };
        out.add(&placed(&pump, edge(i, side * (wall + 0.6)), 0.));
        if i % 12 == 0 {
            out.add(&placed(&post, edge(i + 2, -side * (wall + 1.8)), 0.));
        }
    }
    out.finish()
}

fn spooky_kart_v(p: Vec3) -> vesper3d::math::V {
    vesper3d::math::V(p.x, 0., p.z)
}
