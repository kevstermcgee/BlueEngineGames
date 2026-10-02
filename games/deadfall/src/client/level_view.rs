//! Turns a [`Level`] into drawable meshes: coloured blocks (concrete, brick, corrugated metal, painted
//! containers, hazard stripes ...), glass, decoration built from low-poly parts, the ground, and a sky with
//! distant silhouettes so the map feels bigger than its walls.
//!
//! Everything comes out in world space (draw with an identity transform). Draw order: `sky` with
//! `Materials::sky` and `View::sky_camera`, then `solid`, `decor` and `glass` all with the WORLD material (plus
//! up to four point lights from [`LevelScene::nearest_lights`]).
//!
//! `glass` is opaque pale-blue panes (sky-coloured, a little self-lit), not blended: miniquad 0.4.8 switches the
//! depth test OFF for any pipeline with `depth_write: false`, so the engine's `fx_alpha` material draws on top of
//! nearer geometry (a window across the map shows through a container). Wire fences are built from thin
//! diagonal strips for the same reason.
//!
//! Conventions for decoration (`Decor`): `pos` is the point on the ground (for `Vent`, `PipeRun`, `Crane`
//! `pos.y` is the height of the part itself); `scale` is a uniform size multiplier, except for the run-shaped
//! kinds (`Fence`, `PipeRun`, `RailTrack`, `Cable`, `Sandbags`) where it is the LENGTH in metres and the run
//! goes from `pos` along the direction `(cos yaw, sin yaw)` in (x, z). A `Crane` above 1 m is the hall's
//! overhead bridge crane, below that a yard jib crane.
#![allow(clippy::too_many_arguments)]
use crate::level::{Block, Decor, DecorKind, Level, Material};
use macroquad::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use vesper3d::viewer::kit::{Look, PointLight, Template};

type Rgb = [f32; 3];

/// Everything needed to draw the map.
pub struct LevelScene {
    /// Opaque block geometry, each under 9000 vertices.
    pub solid: Vec<Template>,
    /// Window panes (opaque, sky-coloured; draw with the world material, see the module note).
    pub glass: Template,
    /// Plants, props, landmarks and the scenery outside the wall.
    pub decor: Vec<Template>,
    /// The sky dome, clouds and distant silhouettes (draw first, with the sky camera).
    pub sky: Template,
    /// Overcast afternoon.
    pub look: Look,
    /// Every lamp and furnace glow in the map.
    pub lights: Vec<PointLight>,
    /// Where each entry of `lights` is.
    pub light_pos: Vec<Vec3>,
}

impl LevelScene {
    /// The (up to four) lights that matter most to a viewer at `eye`: nearest by distance inside their radius.
    pub fn nearest_lights(&self, eye: Vec3) -> Vec<PointLight> {
        let mut order: Vec<usize> = (0..self.lights.len()).collect();
        order.sort_by(|&a, &b| {
            let da = self.light_pos[a].distance(eye);
            let db = self.light_pos[b].distance(eye);
            da.total_cmp(&db)
        });
        order.into_iter().take(4).map(|i| self.lights[i]).collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------
// colour helpers

fn mulc(c: Rgb, f: f32) -> Rgb {
    [c[0] * f, c[1] * f, c[2] * f]
}
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
/// A stable pseudo-random number in 0..1 from a position.
fn hash(x: f32, y: f32, z: f32) -> f32 {
    let h = ((x * 127.1 + y * 311.7 + z * 74.7).sin() * 43_758.547).fract();
    h.abs()
}
fn vary(c: Rgb, k: f32, amount: f32) -> Rgb {
    mulc(c, 1. + (k - 0.5) * 2. * amount)
}

const HAZARD_YELLOW: Rgb = [0.93, 0.74, 0.08];
const HAZARD_BLACK: Rgb = [0.07, 0.07, 0.08];

fn base_colour(m: Material) -> Rgb {
    match m {
        Material::Concrete => [0.50, 0.50, 0.48],
        Material::ConcreteDark => [0.33, 0.33, 0.35],
        Material::Asphalt => [0.19, 0.19, 0.20],
        Material::Gravel => [0.42, 0.39, 0.35],
        Material::Dirt => [0.33, 0.27, 0.19],
        Material::Grass => [0.27, 0.43, 0.17],
        Material::Brick => [0.56, 0.26, 0.19],
        Material::Plaster => [0.74, 0.70, 0.60],
        Material::Metal => [0.47, 0.53, 0.50],
        Material::RustMetal => [0.52, 0.29, 0.17],
        Material::Wood => [0.54, 0.38, 0.22],
        Material::Glass => [0.60, 0.78, 0.86],
        Material::ContainerRed => [0.64, 0.17, 0.13],
        Material::ContainerBlue => [0.14, 0.31, 0.57],
        Material::ContainerGreen => [0.17, 0.44, 0.28],
        Material::ContainerYellow => [0.88, 0.67, 0.12],
        Material::Hazard => HAZARD_YELLOW,
        Material::Water => [0.30, 0.40, 0.46],
    }
}

// ---------------------------------------------------------------------------------------------------------------
// flat faces

fn quad_z(t: &mut Template, x0: f32, x1: f32, y0: f32, y1: f32, z: f32, facing: f32, c: Rgb) {
    t.quad_facing([vec3(x0, y0, z), vec3(x1, y0, z), vec3(x1, y1, z), vec3(x0, y1, z)], vec3(0., 0., facing), c, 0.);
}
fn quad_x(t: &mut Template, z0: f32, z1: f32, y0: f32, y1: f32, x: f32, facing: f32, c: Rgb) {
    t.quad_facing([vec3(x, y0, z0), vec3(x, y0, z1), vec3(x, y1, z1), vec3(x, y1, z0)], vec3(facing, 0., 0.), c, 0.);
}
fn quad_y(t: &mut Template, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, c: Rgb) {
    t.quad_facing([vec3(x0, y, z0), vec3(x1, y, z0), vec3(x1, y, z1), vec3(x0, y, z1)], Vec3::Y, c, 0.);
}

/// A cylinder lying along +X starting at `from`.
fn cyl_x(t: &mut Template, from: Vec3, len: f32, r: f32, c: Rgb, sides: usize) {
    let mut tmp = Template::new();
    tmp.cylinder(Vec3::ZERO, r, len, c, 0., sides);
    t.append(&tmp.transformed(Mat4::from_translation(from) * Mat4::from_rotation_z(-FRAC_PI_2)));
}
/// A cylinder lying along +Z starting at `from`.
fn cyl_z(t: &mut Template, from: Vec3, len: f32, r: f32, c: Rgb, sides: usize) {
    let mut tmp = Template::new();
    tmp.cylinder(Vec3::ZERO, r, len, c, 0., sides);
    t.append(&tmp.transformed(Mat4::from_translation(from) * Mat4::from_rotation_x(FRAC_PI_2)));
}

/// Accumulates geometry into templates that stay under the u16 index limit.
struct Acc {
    parts: Vec<Template>,
}
impl Acc {
    fn new() -> Self {
        Self { parts: vec![Template::new()] }
    }
    fn t(&mut self) -> &mut Template {
        if self.parts.last().is_none_or(|p| p.verts.len() > 50_000) {
            self.parts.push(Template::new());
        }
        self.parts.last_mut().expect("a part exists")
    }
    fn finish(self) -> Vec<Template> {
        self.parts.iter().filter(|p| !p.is_empty()).flat_map(|p| p.split()).collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------
// blocks

fn add_block(acc: &mut Acc, glass: &mut Template, b: &Block) {
    let (mn, mx) = (vec3(b.min.0, b.min.1, b.min.2), vec3(b.max.0, b.max.1, b.max.2));
    let size = mx - mn;
    let c = (mn + mx) * 0.5;
    let half = size * 0.5;
    let k = hash(c.x, c.y, c.z);
    let base = base_colour(b.material);
    let t = acc.t();
    let long_x = size.x >= size.z;
    // Ground patches and slabs: a coloured top, speckled when large.
    if size.y <= 0.07 && (size.x > 1.5 || size.z > 1.5) || b.material == Material::Dirt && size.x > 50. {
        let top = vary(base, k, 0.05);
        t.box_top(c, half, mulc(top, 0.8), top, 0.);
        if size.x * size.z > 20. && b.material != Material::Dirt {
            speckle(t, b, base, size);
        }
        if b.material == Material::Asphalt && size.x.max(size.z) > 8. {
            road_markings(t, b, size);
        }
        return;
    }
    match b.material {
        Material::Glass => {
            glass_pane(acc, glass, mn, mx);
        }
        Material::Hazard => hazard(t, mn, mx),
        Material::ContainerRed | Material::ContainerBlue | Material::ContainerGreen | Material::ContainerYellow => {
            container(t, mn, mx, base, k)
        }
        Material::Metal if size.x.min(size.z) < 0.12 && size.y >= 1.8 => chain_link(t, mn, mx),
        Material::Metal if size.x.min(size.z) < 0.14 && size.y <= 1.3 => railing(t, mn, mx),
        Material::RustMetal if (3.2..3.6).contains(&size.y) && size.x.max(size.z) > 6. && size.x.min(size.z) > 2.5 => {
            furnace(t, mn, mx, base);
        }
        Material::RustMetal
            if size.y >= 3.0 && size.x.min(size.z) >= 1.0 && size.x.min(size.z) <= 2.5 && size.x.max(size.z) > 4. =>
        {
            pipe_bank(t, mn, mx, base, k);
        }
        Material::Metal if (2.2..3.2).contains(&size.y) && size.x.min(size.z) < 1.1 && size.x.max(size.z) > 3. => {
            rack(t, mn, mx, k);
        }
        Material::Metal | Material::RustMetal => {
            let col = vary(base, k, 0.06);
            t.box_top(c, half, col, mulc(col, 1.12), 0.);
            if size.y < 0.6 && size.x > 2. && size.z > 2. {
                // Roof sheets: ribs across the short side.
                let (n, along_x) =
                    if size.x > size.z { ((size.x / 0.7) as i32, true) } else { ((size.z / 0.7) as i32, false) };
                for i in 0..n {
                    let f = (i as f32 + 0.5) / n as f32;
                    if along_x {
                        let x = mn.x + f * size.x;
                        quad_y(t, x - 0.06, x + 0.06, mn.z, mx.z, mx.y + 0.006, mulc(col, 0.8));
                    } else {
                        let z = mn.z + f * size.z;
                        quad_y(t, mn.x, mx.x, z - 0.06, z + 0.06, mx.y + 0.006, mulc(col, 0.8));
                    }
                }
            } else if size.y > 0.6 && size.x.max(size.z) > 1.5 {
                // Corrugation: vertical ribs on the two big faces.
                let len = size.x.max(size.z);
                let n = (len / 0.55) as i32;
                for i in 0..n {
                    let f = (i as f32 + 0.5) / n as f32;
                    let dark = mulc(col, if i % 2 == 0 { 0.82 } else { 1.06 });
                    if long_x {
                        let x = mn.x + f * size.x;
                        for (z, face) in [(mn.z - 0.008, -1.), (mx.z + 0.008, 1.)] {
                            quad_z(t, x - 0.12, x + 0.12, mn.y, mx.y, z, face, dark);
                        }
                    } else {
                        let z = mn.z + f * size.z;
                        for (x, face) in [(mn.x - 0.008, -1.), (mx.x + 0.008, 1.)] {
                            quad_x(t, z - 0.12, z + 0.12, mn.y, mx.y, x, face, dark);
                        }
                    }
                }
            }
        }
        Material::Brick => {
            let col = vary(base, k, 0.07);
            t.box_top(c, half, col, mulc(col, 1.08), 0.);
            wall_detail(t, mn, mx, col, true);
        }
        Material::Concrete | Material::ConcreteDark | Material::Plaster => {
            let col = vary(base, k, 0.07);
            t.box_top(c, half, col, mulc(col, 1.1), 0.);
            if size.y > 1.4 && size.x.max(size.z) > 1.5 {
                wall_detail(t, mn, mx, col, false);
            }
        }
        Material::Wood => {
            let col = vary(base, k, 0.1);
            t.box_top(c, half, col, mulc(col, 1.12), 0.);
            if size.x.max(size.z) < 2.5 && size.y > 0.5 {
                // Crate battens.
                let dark = mulc(col, 0.68);
                for sx in [-1., 1.] {
                    for sz in [-1., 1.] {
                        t.box_(
                            vec3(c.x + sx * (half.x - 0.04), c.y, c.z + sz * (half.z - 0.04)),
                            vec3(0.055, half.y + 0.004, 0.055),
                            dark,
                            0.,
                        );
                    }
                }
                for f in [0.25, 0.75] {
                    let y = mn.y + size.y * f;
                    quad_z(t, mn.x, mx.x, y - 0.04, y + 0.04, mn.z - 0.007, -1., dark);
                    quad_z(t, mn.x, mx.x, y - 0.04, y + 0.04, mx.z + 0.007, 1., dark);
                    quad_x(t, mn.z, mx.z, y - 0.04, y + 0.04, mn.x - 0.007, -1., dark);
                    quad_x(t, mn.z, mx.z, y - 0.04, y + 0.04, mx.x + 0.007, 1., dark);
                }
            }
        }
        _ => {
            let col = vary(base, k, 0.06);
            t.box_top(c, half, col, mulc(col, 1.08), 0.);
        }
    }
}

/// Warehouse shelving: dark uprights, orange beams and cartons on every level of both faces.
fn rack(t: &mut Template, mn: Vec3, mx: Vec3, k: f32) {
    let size = mx - mn;
    let long_x = size.x >= size.z;
    let len = size.x.max(size.z);
    t.box_top((mn + mx) * 0.5, size * 0.5, [0.20, 0.24, 0.30], [0.26, 0.30, 0.36], 0.);
    let beam = [0.88, 0.42, 0.10];
    let cartons: [Rgb; 5] =
        [[0.66, 0.50, 0.32], [0.58, 0.42, 0.26], [0.78, 0.74, 0.66], [0.25, 0.38, 0.55], [0.62, 0.30, 0.22]];
    let levels = ((size.y - 0.1) / 0.75) as i32;
    let slots = (len / 0.75) as i32;
    for lv in 0..levels {
        let y = mn.y + 0.12 + lv as f32 * 0.75;
        for side in [-1., 1.] {
            let off = 0.012;
            if long_x {
                let z = if side < 0. { mn.z - off } else { mx.z + off };
                quad_z(t, mn.x, mx.x, y, y + 0.07, z, side, beam);
            } else {
                let x = if side < 0. { mn.x - off } else { mx.x + off };
                quad_x(t, mn.z, mx.z, y, y + 0.07, x, side, beam);
            }
            for sl in 0..slots {
                let h = hash(lv as f32 + k * 10., sl as f32, side);
                if h < 0.18 {
                    continue;
                }
                let a = sl as f32 * len / slots as f32;
                let w = len / slots as f32 - 0.1;
                let top = y + 0.1 + 0.4 + 0.15 * h;
                let c = cartons[(h * 5.) as usize % 5];
                if long_x {
                    let z = if side < 0. { mn.z - 0.02 } else { mx.z + 0.02 };
                    quad_z(t, mn.x + a + 0.05, mn.x + a + 0.05 + w, y + 0.07, top, z, side, c);
                } else {
                    let x = if side < 0. { mn.x - 0.02 } else { mx.x + 0.02 };
                    quad_x(t, mn.z + a + 0.05, mn.z + a + 0.05 + w, y + 0.07, top, x, side, c);
                }
            }
        }
    }
}

/// A furnace bank: a heavy rust-red housing with two glowing doors on each long face.
fn furnace(t: &mut Template, mn: Vec3, mx: Vec3, base: Rgb) {
    let size = mx - mn;
    let long_x = size.x >= size.z;
    let body = mulc(base, 1.05);
    t.box_top((mn + mx) * 0.5, size * 0.5, body, mulc(body, 0.8), 0.);
    let len = size.x.max(size.z);
    let glow = [1.0, 0.45, 0.12];
    let frame = [0.12, 0.11, 0.11];
    for door in 0..2 {
        let a = (0.27 + 0.46 * door as f32) * len;
        for side in [-1., 1.] {
            let (y0, y1) = (mn.y + 0.9, mn.y + 1.7);
            if long_x {
                let x = mn.x + a;
                let z = if side < 0. { mn.z - 0.012 } else { mx.z + 0.012 };
                quad_z(t, x - 0.95, x + 0.95, mn.y + 0.7, mn.y + 1.9, z, side, frame);
                let zz = z + 0.008 * side;
                t.quad_facing(
                    [vec3(x - 0.75, y0, zz), vec3(x + 0.75, y0, zz), vec3(x + 0.75, y1, zz), vec3(x - 0.75, y1, zz)],
                    vec3(0., 0., side),
                    glow,
                    1.,
                );
            } else {
                let z = mn.z + a;
                let x = if side < 0. { mn.x - 0.012 } else { mx.x + 0.012 };
                quad_x(t, z - 0.95, z + 0.95, mn.y + 0.7, mn.y + 1.9, x, side, frame);
                let xx = x + 0.008 * side;
                t.quad_facing(
                    [vec3(xx, y0, z - 0.75), vec3(xx, y0, z + 0.75), vec3(xx, y1, z + 0.75), vec3(xx, y1, z - 0.75)],
                    vec3(side, 0., 0.),
                    glow,
                    1.,
                );
            }
        }
    }
}

/// A pipe rack: a dark frame with rows of pipes along both faces.
fn pipe_bank(t: &mut Template, mn: Vec3, mx: Vec3, base: Rgb, k: f32) {
    let size = mx - mn;
    let long_x = size.x >= size.z;
    let (thick, len) = if long_x { (size.z, size.x) } else { (size.x, size.z) };
    t.box_top((mn + mx) * 0.5, size * 0.5, mulc(base, 0.45), mulc(base, 0.6), 0.);
    let palette: [Rgb; 4] = [[0.52, 0.28, 0.17], [0.44, 0.50, 0.52], [0.24, 0.38, 0.30], [0.62, 0.52, 0.16]];
    let rows = ((size.y - 0.2) / 0.8) as i32;
    for row in 0..rows {
        for side in [-1., 1.] {
            let h = hash(row as f32, side, k);
            let r = 0.18 + 0.05 * h;
            let y = mn.y + 0.45 + row as f32 * 0.8;
            let off = side * (thick * 0.5 - r - 0.01);
            let col = palette[((h * 4.) as usize + row as usize) % 4];
            let (cx, cz) = ((mn.x + mx.x) * 0.5, (mn.z + mx.z) * 0.5);
            if long_x {
                cyl_x(t, vec3(mn.x, y, cz + off), len, r, col, 8);
            } else {
                cyl_z(t, vec3(cx + off, y, mn.z), len, r, col, 8);
            }
            let mut d = 1.2;
            while d < len {
                if long_x {
                    cyl_x(t, vec3(mn.x + d, y, cz + off), 0.08, r * 1.3, mulc(col, 0.55), 8);
                } else {
                    cyl_z(t, vec3(cx + off, y, mn.z + d), 0.08, r * 1.3, mulc(col, 0.55), 8);
                }
                d += 3.5;
            }
        }
    }
}

/// Courses of brick or panel joints and a grimy foot on the big faces of a wall.
fn wall_detail(t: &mut Template, mn: Vec3, mx: Vec3, col: Rgb, brick: bool) {
    let size = mx - mn;
    let long_x = size.x >= size.z;
    let joint = mulc(col, if brick { 0.74 } else { 0.86 });
    let step = if brick { 0.45 } else { 1.5 };
    let mut y = mn.y + step;
    while y < mx.y - 0.1 {
        if long_x {
            quad_z(t, mn.x, mx.x, y, y + 0.025, mn.z - 0.006, -1., joint);
            quad_z(t, mn.x, mx.x, y, y + 0.025, mx.z + 0.006, 1., joint);
        } else {
            quad_x(t, mn.z, mx.z, y, y + 0.025, mn.x - 0.006, -1., joint);
            quad_x(t, mn.z, mx.z, y, y + 0.025, mx.x + 0.006, 1., joint);
        }
        y += step;
    }
    // Painted wainscot on plaster walls, a dirty foot on the rest.
    let foot = if !brick && col[0] > 0.6 { mix(col, [0.28, 0.46, 0.44], 0.7) } else { mulc(col, 0.72) };
    let wains = if !brick && col[0] > 0.6 { 1.0 } else { 0.45 };
    let top = (mn.y + wains).min(mx.y);
    if long_x {
        quad_z(t, mn.x, mx.x, mn.y, top, mn.z - 0.007, -1., foot);
        quad_z(t, mn.x, mx.x, mn.y, top, mx.z + 0.007, 1., foot);
    } else {
        quad_x(t, mn.z, mx.z, mn.y, top, mn.x - 0.007, -1., foot);
        quad_x(t, mn.z, mx.z, mn.y, top, mx.x + 0.007, 1., foot);
    }
}

fn speckle(t: &mut Template, b: &Block, base: Rgb, size: Vec3) {
    let n = ((size.x * size.z) / 7.).min(26.) as i32;
    for i in 0..n {
        let h1 = hash(b.min.0 + i as f32, b.min.2, 1.3);
        let h2 = hash(b.min.2 + i as f32, b.min.0, 2.7);
        let h3 = hash(i as f32, b.min.0 + b.min.2, 9.1);
        let w = 0.7 + h3 * 1.8;
        let x = b.min.0 + 0.3 + h1 * (size.x - 0.6 - w).max(0.);
        let z = b.min.2 + 0.3 + h2 * (size.z - 0.6 - w).max(0.);
        let col = mulc(base, 0.8 + h3 * 0.4);
        quad_y(
            t,
            x,
            (x + w).min(b.max.0 - 0.1),
            z,
            (z + w * (0.6 + h1 * 0.5)).min(b.max.2 - 0.1),
            b.max.1 + 0.008 + i as f32 * 0.0003,
            col,
        );
    }
}

fn road_markings(t: &mut Template, b: &Block, size: Vec3) {
    let col = [0.72, 0.66, 0.28];
    let y = b.max.1 + 0.006;
    if size.z >= size.x {
        let x = (b.min.0 + b.max.0) / 2.;
        let mut z = b.min.2 + 1.;
        while z + 1.4 < b.max.2 - 1. {
            quad_y(t, x - 0.07, x + 0.07, z, z + 1.4, y, col);
            z += 3.;
        }
    } else {
        let z = (b.min.2 + b.max.2) / 2.;
        let mut x = b.min.0 + 1.;
        while x + 1.4 < b.max.0 - 1. {
            quad_y(t, x, x + 1.4, z - 0.07, z + 0.07, y, col);
            x += 3.;
        }
    }
}

fn hazard(t: &mut Template, mn: Vec3, mx: Vec3) {
    let size = mx - mn;
    let along_x = size.x >= size.z;
    let len = if along_x { size.x } else { size.z };
    let n = ((len / 0.45).round() as i32).max(2);
    for i in 0..n {
        let (a, b) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        let col = if i % 2 == 0 { HAZARD_YELLOW } else { HAZARD_BLACK };
        let (lo, hi) = if along_x {
            (vec3(mn.x + a * size.x, mn.y, mn.z), vec3(mn.x + b * size.x, mx.y, mx.z))
        } else {
            (vec3(mn.x, mn.y, mn.z + a * size.z), vec3(mx.x, mx.y, mn.z + b * size.z))
        };
        t.box_((lo + hi) * 0.5, (hi - lo) * 0.5, col, 0.);
    }
}

fn container(t: &mut Template, mn: Vec3, mx: Vec3, base: Rgb, k: f32) {
    let size = mx - mn;
    let stack = ((size.y / 2.6).round() as i32).max(1);
    let body = vary(base, k, 0.05);
    let dark = mulc(body, 0.72);
    let light = mulc(body, 1.12);
    let along_x = size.x >= size.z;
    t.box_top((mn + mx) * 0.5, size * 0.5, body, light, 0.);
    let (len, y_unit) = (size.x.max(size.z), size.y / stack as f32);
    for s in 0..stack {
        let (y0, y1) = (mn.y + s as f32 * y_unit, mn.y + (s + 1) as f32 * y_unit);
        // Corrugation on the long faces.
        let n = (len / 0.34) as i32;
        for i in (0..n).step_by(2) {
            let f = (i as f32 + 0.5) / n as f32;
            if along_x {
                let x = mn.x + 0.15 + f * (size.x - 0.3);
                for (z, face) in [(mn.z - 0.006, -1.), (mx.z + 0.006, 1.)] {
                    quad_z(t, x - 0.07, x + 0.07, y0 + 0.14, y1 - 0.14, z, face, dark);
                }
            } else {
                let z = mn.z + 0.15 + f * (size.z - 0.3);
                for (x, face) in [(mn.x - 0.006, -1.), (mx.x + 0.006, 1.)] {
                    quad_x(t, z - 0.07, z + 0.07, y0 + 0.14, y1 - 0.14, x, face, dark);
                }
            }
        }
        // Corner posts, top and bottom rails.
        let frame = mulc(body, 0.55);
        for (px, pz) in [(mn.x, mn.z), (mx.x, mn.z), (mn.x, mx.z), (mx.x, mx.z)] {
            let ix = if px == mn.x { 0.06 } else { -0.06 };
            let iz = if pz == mn.z { 0.06 } else { -0.06 };
            t.box_(vec3(px + ix, (y0 + y1) * 0.5, pz + iz), vec3(0.07, (y1 - y0) * 0.5 + 0.004, 0.07), frame, 0.);
        }
        for y in [y0 + 0.05, y1 - 0.05] {
            if along_x {
                t.box_(vec3((mn.x + mx.x) * 0.5, y, mn.z + 0.04), vec3(size.x * 0.5, 0.05, 0.05), frame, 0.);
                t.box_(vec3((mn.x + mx.x) * 0.5, y, mx.z - 0.04), vec3(size.x * 0.5, 0.05, 0.05), frame, 0.);
            } else {
                t.box_(vec3(mn.x + 0.04, y, (mn.z + mx.z) * 0.5), vec3(0.05, 0.05, size.z * 0.5), frame, 0.);
                t.box_(vec3(mx.x - 0.04, y, (mn.z + mx.z) * 0.5), vec3(0.05, 0.05, size.z * 0.5), frame, 0.);
            }
        }
        // Door end: locking bars and a seam.
        let rod = mulc(body, 0.45);
        let ends: [(f32, f32); 2] =
            if along_x { [(mn.x - 0.008, -1.), (mx.x + 0.008, 1.)] } else { [(mn.z - 0.008, -1.), (mx.z + 0.008, 1.)] };
        for (pos, face) in ends {
            let mid = if along_x { (mn.z + mx.z) * 0.5 } else { (mn.x + mx.x) * 0.5 };
            for o in [-0.6, -0.35, 0.35, 0.6] {
                if along_x {
                    quad_x(t, mid + o - 0.025, mid + o + 0.025, y0 + 0.12, y1 - 0.12, pos, face, rod);
                } else {
                    quad_z(t, mid + o - 0.025, mid + o + 0.025, y0 + 0.12, y1 - 0.12, pos, face, rod);
                }
            }
            if along_x {
                quad_x(t, mid - 0.02, mid + 0.02, y0 + 0.1, y1 - 0.1, pos, face, mulc(body, 0.3));
            } else {
                quad_z(t, mid - 0.02, mid + 0.02, y0 + 0.1, y1 - 0.1, pos, face, mulc(body, 0.3));
            }
        }
    }
}

fn glass_pane(acc: &mut Acc, glass: &mut Template, mn: Vec3, mx: Vec3) {
    let size = mx - mn;
    glass.box_((mn + mx) * 0.5, size * 0.5, [0.30, 0.42, 0.50], 0.1);
    // Frame: rails along the long side and mullions every 2.5 m.
    let frame = [0.22, 0.24, 0.25];
    let t = acc.t();
    let along_x = size.x >= size.z;
    let len = size.x.max(size.z);
    let n = (len / 2.5).ceil().max(1.) as i32;
    let (cx, cz) = ((mn.x + mx.x) * 0.5, (mn.z + mx.z) * 0.5);
    let (hx, hz) = ((size.x * 0.5 + 0.03).max(0.06), (size.z * 0.5 + 0.03).max(0.06));
    let (rail_hx, rail_hz) = if along_x { (size.x * 0.5, hz) } else { (hx, size.z * 0.5) };
    for y in [mn.y + 0.05, mx.y - 0.05, (mn.y + mx.y) * 0.5] {
        t.box_(vec3(cx, y, cz), vec3(rail_hx, 0.05, rail_hz), frame, 0.);
    }
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let p = if along_x {
            vec3(mn.x + f * size.x, (mn.y + mx.y) * 0.5, cz)
        } else {
            vec3(cx, (mn.y + mx.y) * 0.5, mn.z + f * size.z)
        };
        t.box_(p, vec3(if along_x { 0.06 } else { hx }, size.y * 0.5, if along_x { hz } else { 0.06 }), frame, 0.);
    }
}

/// Posts, a top rail and a translucent mesh for a thin tall block.
fn chain_link(t: &mut Template, mn: Vec3, mx: Vec3) {
    let size = mx - mn;
    let along_x = size.x >= size.z;
    let len = size.x.max(size.z);
    let post = [0.30, 0.32, 0.33];
    let n = (len / 2.5).ceil().max(1.) as i32;
    let (cx, cz) = ((mn.x + mx.x) * 0.5, (mn.z + mx.z) * 0.5);
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let (px, pz) = if along_x { (mn.x + f * size.x, cz) } else { (cx, mn.z + f * size.z) };
        t.box_(vec3(px, mx.y * 0.5, pz), vec3(0.05, mx.y * 0.5, 0.05), post, 0.);
    }
    let (hx, hz) = if along_x { (size.x * 0.5, 0.025) } else { (0.025, size.z * 0.5) };
    t.box_(vec3(cx, mx.y - 0.03, cz), vec3(hx, 0.03, hz), post, 0.);
    t.box_(vec3(cx, 0.06, cz), vec3(hx, 0.03, hz), post, 0.);
    // Wire: crossing diagonals as thin strips, both ways round, so the fence stays see-through.
    let wire = [0.55, 0.58, 0.58];
    let (y0, y1) = (0.1, mx.y - 0.07);
    let h = y1 - y0;
    let mut s0 = -h;
    while s0 < len {
        for dir in [1., -1.] {
            // Segment from (s0, y0) rising by h along the run, clipped to 0..len.
            let (a, b) = if dir > 0. { (s0, s0 + h) } else { (s0 + h, s0) };
            let (ta, tb) = ((0f32).max(a.min(b)), len.min(a.max(b)));
            if tb - ta < 0.05 {
                continue;
            }
            let ya = if dir > 0. { y0 + (ta - s0) } else { y0 + (s0 + h - ta) };
            let yb = if dir > 0. { y0 + (tb - s0) } else { y0 + (s0 + h - tb) };
            let (pa, pb) = if along_x {
                (vec3(mn.x + ta, ya, cz), vec3(mn.x + tb, yb, cz))
            } else {
                (vec3(cx, ya, mn.z + ta), vec3(cx, yb, mn.z + tb))
            };
            let d = vec3(0., 0.012, 0.);
            let side = if along_x { vec3(0., 0., 1.) } else { vec3(1., 0., 0.) };
            for nrm in [side, -side] {
                t.quad_facing([pa - d, pb - d, pb + d, pa + d], nrm, wire, 0.);
            }
        }
        s0 += 0.55;
    }
}

fn railing(t: &mut Template, mn: Vec3, mx: Vec3) {
    let size = mx - mn;
    let along_x = size.x >= size.z;
    let len = size.x.max(size.z);
    let col = [0.88, 0.70, 0.10];
    let n = (len / 1.6).ceil().max(1.) as i32;
    let (cx, cz) = ((mn.x + mx.x) * 0.5, (mn.z + mx.z) * 0.5);
    let h = size.y;
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let (px, pz) = if along_x { (mn.x + f * size.x, cz) } else { (cx, mn.z + f * size.z) };
        t.box_(vec3(px, mn.y + h * 0.5, pz), vec3(0.035, h * 0.5, 0.035), col, 0.);
    }
    let (hx, hz) = if along_x { (size.x * 0.5, 0.03) } else { (0.03, size.z * 0.5) };
    for f in [1.0, 0.5] {
        t.box_(vec3(cx, mn.y + h * f - 0.03, cz), vec3(hx, 0.03, hz), col, 0.);
    }
    t.box_(vec3(cx, mn.y + 0.04, cz), vec3(hx, 0.04, hz), mulc(col, 0.6), 0.);
}

// ---------------------------------------------------------------------------------------------------------------
// decoration

fn leaf(t: &mut Template, base: Vec3, yaw: f32, tilt: f32, len: f32, width: f32, c: Rgb) {
    let mut tmp = Template::new();
    tmp.cone(Vec3::ZERO, width, 0., len, c, 0., 4);
    t.append(&tmp.transformed(Mat4::from_translation(base) * Mat4::from_rotation_y(yaw) * Mat4::from_rotation_z(tilt)));
}

fn blob(t: &mut Template, c: Vec3, r: Vec3, col: Rgb) {
    t.ball(c, r, col, 0., 8, 6);
}

fn build_decor(d: &Decor) -> Template {
    let mut t = Template::new();
    let s = d.scale;
    let k = hash(d.pos.0, d.pos.1, d.pos.2);
    let place = |local: &Template, run: bool| -> Template {
        let scale = if run { 1. } else { s };
        let rot = Mat4::from_rotation_y(-d.yaw);
        local.transformed(
            Mat4::from_translation(vec3(d.pos.0, d.pos.1, d.pos.2)) * rot * Mat4::from_scale(Vec3::splat(scale)),
        )
    };
    let mut run = false;
    match d.kind {
        DecorKind::Tree => {
            let bark = [0.30, 0.21, 0.13];
            t.cone(Vec3::ZERO, 0.32, 0.27, 3.2, bark, 0., 10);
            // Round, tapering branches grow into the crown. Flat leaf-shaped brown wedges looked carved.
            for (a, y) in [(0.6_f32, 2.4), (3.4, 2.9), (5.0, 2.0)] {
                let base = vec3(0., y, 0.);
                let delta = vec3(a.cos() * 1.1, 1.0, a.sin() * 1.1);
                let mut branch = Template::new();
                branch.cone(Vec3::ZERO, 0.13, 0.045, delta.length(), mulc(bark, 0.94), 0., 7);
                t.append(&branch.transformed(
                    Mat4::from_translation(base) * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, delta.normalize())),
                ));
            }
            let greens =
                [[0.17, 0.36, 0.12], [0.22, 0.44, 0.15], [0.26, 0.48, 0.17], [0.15, 0.32, 0.12], [0.30, 0.50, 0.20]];
            let pick = |i: usize| vary(greens[(i + (k * 5.) as usize) % 5], hash(i as f32, k, 1.), 0.08);
            blob(&mut t, vec3(0., 3.8, 0.), vec3(1.9, 1.4, 1.9), pick(0));
            blob(&mut t, vec3(1.0, 4.5, 0.4), vec3(1.4, 1.1, 1.3), pick(1));
            blob(&mut t, vec3(-0.9, 4.4, -0.6), vec3(1.4, 1.1, 1.4), pick(2));
            blob(&mut t, vec3(0.2, 5.4, -0.1), vec3(1.3, 1.0, 1.3), pick(3));
            blob(&mut t, vec3(-0.3, 3.3, 1.1), vec3(1.2, 0.9, 1.2), pick(4));
        }
        DecorKind::Pine => {
            t.cone(Vec3::ZERO, 0.25, 0.21, 3.2, [0.28, 0.19, 0.12], 0., 9);
            let layers = [
                (1.0, 1.9, 2.1, [0.10, 0.27, 0.15]),
                (2.4, 1.5, 1.9, [0.12, 0.31, 0.17]),
                (3.6, 1.1, 1.8, [0.10, 0.28, 0.16]),
                (4.7, 0.7, 1.6, [0.13, 0.33, 0.18]),
                (5.6, 0.4, 1.5, [0.11, 0.30, 0.17]),
            ];
            for (y, r, h, c) in layers {
                t.cone(vec3(0., y, 0.), r, 0., h, vary(c, k, 0.12), 0., 9);
            }
        }
        DecorKind::Bush => {
            let g = vary([0.20, 0.40, 0.14], k, 0.15);
            blob(&mut t, vec3(0., 0.45, 0.), vec3(0.7, 0.5, 0.65), g);
            blob(&mut t, vec3(0.5, 0.35, 0.2), vec3(0.5, 0.4, 0.5), mulc(g, 1.15));
            blob(&mut t, vec3(-0.45, 0.35, -0.2), vec3(0.5, 0.38, 0.5), mulc(g, 0.85));
            if k > 0.6 {
                for i in 0..4 {
                    let a = i as f32 * 1.6 + k;
                    t.ball(
                        vec3(a.cos() * 0.5, 0.75 + 0.05 * i as f32, a.sin() * 0.45),
                        vec3(0.06, 0.06, 0.06),
                        [0.92, 0.88, 0.45],
                        0.,
                        5,
                        4,
                    );
                }
            }
        }
        DecorKind::GrassTuft => {
            for i in 0..8 {
                let a = i as f32 * 0.8 + k * 6.;
                let h = 0.16 + 0.2 * hash(i as f32, k, 4.);
                let c = mix([0.30, 0.48, 0.16], [0.52, 0.56, 0.22], hash(i as f32, k, 7.));
                leaf(&mut t, vec3(a.cos() * 0.07, 0., a.sin() * 0.07), a, 0.25 + 0.15 * (i % 3) as f32, h, 0.025, c);
            }
        }
        DecorKind::Weeds => {
            for i in 0..6 {
                let a = i as f32 * 1.05 + k * 6.;
                let h = 0.28 + 0.3 * hash(i as f32, k, 3.);
                leaf(
                    &mut t,
                    vec3(a.cos() * 0.06, 0., a.sin() * 0.06),
                    a,
                    0.3 + 0.2 * (i % 3) as f32,
                    h,
                    0.045,
                    vary([0.22, 0.42, 0.13], hash(i as f32, k, 5.), 0.2),
                );
            }
            if k > 0.45 {
                let col = if k > 0.75 { [0.95, 0.85, 0.25] } else { [0.93, 0.93, 0.88] };
                for i in 0..3 {
                    let a = i as f32 * 2.1 + k;
                    t.ball(
                        vec3(a.cos() * 0.16, 0.5 + 0.06 * i as f32, a.sin() * 0.16),
                        vec3(0.04, 0.03, 0.04),
                        col,
                        0.,
                        5,
                        4,
                    );
                }
            }
        }
        DecorKind::PottedPlant => {
            t.cone(Vec3::ZERO, 0.15, 0.23, 0.36, [0.72, 0.36, 0.22], 0., 8);
            t.cone(vec3(0., 0.34, 0.), 0.25, 0.23, 0.05, [0.66, 0.32, 0.19], 0., 8);
            t.disc(vec3(0., 0.375, 0.), 0.21, [0.18, 0.12, 0.08], 0., 8);
            for i in 0..7 {
                let a = i as f32 * 0.9 + k;
                leaf(
                    &mut t,
                    vec3(0., 0.36, 0.),
                    a,
                    0.35 + 0.12 * (i % 4) as f32,
                    0.55 + 0.25 * hash(i as f32, k, 2.),
                    0.085,
                    vary([0.16, 0.44, 0.17], hash(i as f32, k, 8.), 0.15),
                );
            }
        }
        DecorKind::Barrel | DecorKind::OilDrum => {
            let pal: [Rgb; 4] = if d.kind == DecorKind::Barrel {
                [[0.50, 0.22, 0.14], [0.26, 0.38, 0.30], [0.45, 0.32, 0.16], [0.30, 0.30, 0.33]]
            } else {
                [[0.14, 0.26, 0.50], [0.75, 0.58, 0.12], [0.12, 0.12, 0.13], [0.62, 0.12, 0.10]]
            };
            let c = pal[(k * 4.) as usize % 4];
            let (r, h) = if d.kind == DecorKind::Barrel { (0.29, 0.92) } else { (0.30, 0.95) };
            t.cylinder(Vec3::ZERO, r, h, c, 0., 10);
            for y in [0.1, 0.45, 0.8] {
                t.cylinder(vec3(0., y, 0.), r + 0.012, 0.045, mulc(c, 0.65), 0., 10);
            }
            t.disc(vec3(0., h + 0.004, 0.), r - 0.03, mulc(c, 0.8), 0., 10);
            t.cylinder(vec3(0.12, h, 0.08), 0.04, 0.03, [0.15, 0.15, 0.16], 0., 6);
        }
        DecorKind::PipeRun => {
            run = true;
            let len = s;
            let pipes = [
                (0.0, 0.0, 0.22, [0.50, 0.28, 0.18]),
                (0.5, 0.05, 0.15, [0.45, 0.50, 0.52]),
                (-0.45, -0.02, 0.12, [0.25, 0.38, 0.30]),
            ];
            for (dz, dy, r, c) in pipes {
                cyl_x(&mut t, vec3(0., dy, dz), len, r, vary(c, k, 0.1), 8);
                let mut x = 0.4;
                while x < len {
                    let mut tmp = Template::new();
                    tmp.cylinder(Vec3::ZERO, r * 1.3, 0.07, mulc(c, 0.6), 0., 8);
                    t.append(
                        &tmp.transformed(Mat4::from_translation(vec3(x, dy, dz)) * Mat4::from_rotation_z(-FRAC_PI_2)),
                    );
                    x += 3.5;
                }
            }
            if d.pos.1 < 3. && d.pos.1 > 0.3 {
                let mut x = 0.3;
                while x < len {
                    t.box_(vec3(x, -d.pos.1 * 0.5, 0.), vec3(0.06, d.pos.1 * 0.5, 0.06), [0.3, 0.3, 0.32], 0.);
                    x += 5.;
                }
            }
        }
        DecorKind::LampPost => {
            let dark = [0.22, 0.24, 0.26];
            t.cylinder(Vec3::ZERO, 0.08, 4.5, dark, 0., 6);
            t.cylinder(Vec3::ZERO, 0.16, 0.25, dark, 0., 6);
            t.box_(vec3(0.45, 4.5, 0.), vec3(0.5, 0.04, 0.04), dark, 0.);
            t.box_(vec3(0.95, 4.45, 0.), vec3(0.3, 0.06, 0.13), [0.32, 0.34, 0.36], 0.);
            t.box_(vec3(0.95, 4.385, 0.), vec3(0.25, 0.02, 0.1), [1.0, 0.88, 0.6], 1.);
        }
        DecorKind::FloodLight => {
            let dark = [0.25, 0.26, 0.28];
            t.cylinder(Vec3::ZERO, 0.05, 1.4, dark, 0., 6);
            t.box_(vec3(0., 1.55, 0.), vec3(0.3, 0.2, 0.1), dark, 0.);
            t.box_(vec3(0., 1.55, 0.11), vec3(0.26, 0.16, 0.01), [1., 0.95, 0.75], 1.);
        }
        DecorKind::Crate => {
            let w = [0.56, 0.40, 0.23];
            for (x, y, z, h) in [(0., 0., 0., 0.9), (0.95, 0., 0.1, 0.7), (0.2, 0.9, 0.05, 0.6)] {
                t.box_top(
                    vec3(x, y + h * 0.5, z),
                    vec3(0.45, h * 0.5, 0.45),
                    vary(w, hash(x, y, k), 0.12),
                    mulc(w, 1.15),
                    0.,
                );
            }
        }
        DecorKind::Pallet => {
            let w = [0.62, 0.48, 0.30];
            for x in [-0.5, 0., 0.5] {
                t.box_(vec3(x, 0.06, 0.), vec3(0.07, 0.06, 0.5), mulc(w, 0.8), 0.);
            }
            for i in 0..5 {
                t.box_(
                    vec3(0., 0.15, -0.44 + i as f32 * 0.22),
                    vec3(0.6, 0.025, 0.09),
                    vary(w, hash(i as f32, k, 1.), 0.1),
                    0.,
                );
            }
            if k > 0.5 {
                t.box_top(vec3(0., 0.52, 0.), vec3(0.5, 0.35, 0.4), [0.70, 0.62, 0.45], [0.75, 0.68, 0.5], 0.);
            }
        }
        DecorKind::Tyres => {
            for (x, z, n) in [(0., 0., 3), (0.75, 0.2, 2), (0.3, -0.7, 1)] {
                for i in 0..n {
                    let y = i as f32 * 0.22;
                    t.cylinder(vec3(x, y, z), 0.34, 0.21, [0.09, 0.09, 0.10], 0., 10);
                    t.ring(vec3(x, y + 0.212, z), 0.14, 0.27, [0.16, 0.16, 0.17], 0., 10);
                    t.disc(vec3(x, y + 0.211, z), 0.14, [0.04, 0.04, 0.04], 0., 10);
                }
            }
        }
        DecorKind::Sign => {
            let dark = [0.25, 0.27, 0.28];
            t.box_(vec3(-0.45, 0.75, 0.), vec3(0.03, 0.75, 0.03), dark, 0.);
            t.box_(vec3(0.45, 0.75, 0.), vec3(0.03, 0.75, 0.03), dark, 0.);
            let (plate, band) = if k > 0.5 {
                ([0.92, 0.75, 0.10], [0.08, 0.08, 0.09])
            } else {
                ([0.15, 0.32, 0.58], [0.92, 0.92, 0.9])
            };
            t.box_(vec3(0., 1.15, 0.), vec3(0.6, 0.4, 0.025), plate, 0.);
            t.box_(vec3(0., 1.15, 0.03), vec3(0.5, 0.05, 0.005), band, 0.);
            t.box_(vec3(0., 1.3, 0.03), vec3(0.3, 0.05, 0.005), band, 0.);
            t.box_(vec3(0., 1.0, 0.03), vec3(0.4, 0.04, 0.005), band, 0.);
        }
        DecorKind::Fence => {
            run = true;
            let post = [0.30, 0.32, 0.33];
            let n = (s / 2.5).ceil().max(1.) as i32;
            for i in 0..=n {
                t.box_(vec3(i as f32 * s / n as f32, 1.1, 0.), vec3(0.05, 1.1, 0.05), post, 0.);
            }
            t.box_(vec3(s / 2., 2.18, 0.), vec3(s / 2., 0.03, 0.03), post, 0.);
            t.box_(vec3(s / 2., 0.1, 0.), vec3(s / 2., 0.03, 0.03), post, 0.);
            for i in 0..(s / 0.8) as i32 {
                let x = i as f32 * 0.8;
                t.quad_facing(
                    [vec3(x, 0.1, 0.), vec3(x + 0.05, 0.1, 0.), vec3(x + 0.85, 2.15, 0.), vec3(x + 0.8, 2.15, 0.)],
                    Vec3::Z,
                    [0.55, 0.58, 0.58],
                    0.,
                );
                t.quad_facing(
                    [vec3(x + 0.8, 0.1, 0.), vec3(x + 0.85, 0.1, 0.), vec3(x + 0.05, 2.15, 0.), vec3(x, 2.15, 0.)],
                    Vec3::Z,
                    [0.55, 0.58, 0.58],
                    0.,
                );
            }
        }
        DecorKind::Vent => {
            let m = [0.42, 0.46, 0.48];
            t.cylinder(Vec3::ZERO, 0.4, 0.3, m, 0., 8);
            for a in [0., FRAC_PI_2, PI, PI + FRAC_PI_2] {
                t.box_(vec3(a.cos() * 0.3, 0.45, a.sin() * 0.3), vec3(0.03, 0.15, 0.03), mulc(m, 0.7), 0.);
            }
            t.cone(vec3(0., 0.6, 0.), 0.7, 0.12, 0.3, mulc(m, 1.15), 0., 8);
        }
        DecorKind::Chimney => {
            let brick = [0.52, 0.26, 0.19];
            t.cone(Vec3::ZERO, 1.55, 1.0, 34., brick, 0., 12);
            t.cone(Vec3::ZERO, 1.75, 1.6, 1.0, mulc(brick, 0.7), 0., 12);
            // Red and white aviation bands near the top.
            for (i, y) in [24.0, 26.0, 28.0, 30.0].into_iter().enumerate() {
                let r0 = 1.55 - 0.55 * (y / 34.);
                let r1 = 1.55 - 0.55 * ((y + 2.0) / 34.);
                t.cone(
                    vec3(0., y, 0.),
                    r0 + 0.02,
                    r1 + 0.02,
                    2.,
                    if i % 2 == 0 { [0.80, 0.14, 0.10] } else { [0.88, 0.88, 0.86] },
                    0.,
                    12,
                );
            }
            t.cone(vec3(0., 32., 0.), 1.05, 0.95, 2.0, [0.15, 0.15, 0.16], 0., 12);
            t.ring(vec3(0., 34.05, 0.), 0.72, 1.2, [0.10, 0.10, 0.10], 0., 12);
            t.cylinder(vec3(0., 34., 0.), 0.05, 2.5, [0.3, 0.3, 0.3], 0., 4);
            t.cylinder(Vec3::ZERO, 0.03, 34., [0.2, 0.2, 0.22], 0., 4);
        }
        DecorKind::WaterTower => {
            let leg = [0.42, 0.26, 0.17];
            for (x, z) in [(-1.5, -2.), (1.5, -2.), (-1.5, 2.), (1.5, 2.)] {
                t.box_(vec3(x, 4.5, z), vec3(0.27, 4.5, 0.27), leg, 0.);
            }
            for y in [2.5, 6.0] {
                for z in [-2., 2.] {
                    t.box_(vec3(0., y, z), vec3(1.5, 0.07, 0.07), mulc(leg, 0.8), 0.);
                }
                for x in [-1.5, 1.5] {
                    t.box_(vec3(x, y, 0.), vec3(0.07, 0.07, 2.), mulc(leg, 0.8), 0.);
                }
            }
            for z in [-2.0, 2.0] {
                for sgn in [-1., 1.] {
                    let mut tmp = Template::new();
                    tmp.box_(Vec3::ZERO, vec3(0.05, 1.9, 0.05), mulc(leg, 0.8), 0.);
                    t.append(
                        &tmp.transformed(Mat4::from_translation(vec3(0., 4.25, z)) * Mat4::from_rotation_z(sgn * 0.75)),
                    );
                }
            }
            let tank = [0.44, 0.52, 0.56];
            t.cylinder(vec3(0., 9., 0.), 2.6, 4.4, tank, 0., 16);
            for y in [9.4, 11.0, 12.6] {
                t.cylinder(vec3(0., y, 0.), 2.64, 0.12, mulc(tank, 0.6), 0., 16);
            }
            t.cone(vec3(0., 13.4, 0.), 2.75, 0.2, 1.5, [0.36, 0.26, 0.20], 0., 16);
            t.ball(vec3(0., 14.95, 0.), vec3(0.25, 0.2, 0.25), [0.3, 0.3, 0.3], 0., 6, 4);
            t.box_(vec3(2.65, 6.5, 0.), vec3(0.03, 3.6, 0.3), [0.25, 0.25, 0.27], 0.);
            t.cylinder(vec3(0., 0., 0.), 0.18, 9., [0.22, 0.30, 0.35], 0., 8);
            // Stained streaks and a band of weathering.
            t.cylinder(vec3(0., 9., 0.), 2.62, 0.5, mulc(tank, 0.75), 0., 16);
        }
        DecorKind::CoolingTower => {
            let c = [0.66, 0.66, 0.64];
            let prof =
                |y: f32| -> f32 { 5.0 - 1.4 * (y / 15.).min(1.) + if y > 15. { 0.9 * ((y - 15.) / 11.) } else { 0. } };
            let mut y = 0.;
            while y < 26. {
                let h = 2.;
                let cc = mulc(c, 0.92 + 0.08 * hash(y, 0., 1.));
                t.cone(vec3(0., y, 0.), prof(y), prof(y + h), h, cc, 0., 18);
                y += h;
            }
            t.cone(Vec3::ZERO, 5.05, 5.0, 3.0, [0.30, 0.30, 0.30], 0., 18);
            t.ring(vec3(0., 26.03, 0.), 3.5, 4.3, [0.4, 0.4, 0.4], 0., 18);
            // Dark louvre band and streaks.
            for i in 0..6 {
                let a = i as f32 * TAU / 6.;
                t.box_(vec3(a.cos() * 4.9, 1.4, a.sin() * 4.9), vec3(0.6, 1.3, 0.6), [0.22, 0.22, 0.23], 0.);
            }
            // A weak plume.
            t.ball(vec3(0.3, 28.5, 0.), vec3(3., 1.6, 3.), [0.85, 0.86, 0.87], 0., 8, 5);
        }
        DecorKind::Crane => {
            if d.pos.1 > 1. {
                // Overhead bridge crane: runway beams along the hall, a bridge across, a hoist.
                let steel = [0.70, 0.58, 0.14];
                let dark = [0.25, 0.26, 0.28];
                for z in [-9.6, 9.6] {
                    t.box_(vec3(0., 0.35, z), vec3(14., 0.35, 0.22), dark, 0.);
                }
                t.box_(vec3(0., 0.2, 0.), vec3(0.6, 0.25, 9.8), steel, 0.);
                t.box_(vec3(0., 0.5, 0.), vec3(0.4, 0.1, 9.8), steel, 0.);
                t.box_(vec3(0., -0.3, 0.), vec3(0.9, 0.35, 0.6), [0.78, 0.62, 0.1], 0.);
                t.cylinder(vec3(0., -2.6, 0.), 0.02, 2.3, [0.15, 0.15, 0.15], 0., 4);
                t.box_(vec3(0., -2.8, 0.), vec3(0.22, 0.2, 0.15), dark, 0.);
            } else {
                let steel = [0.80, 0.62, 0.10];
                let dark = [0.25, 0.26, 0.28];
                t.box_(vec3(0., 0.4, 0.), vec3(1.4, 0.4, 1.4), dark, 0.);
                for (x, z) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
                    t.box_(vec3(x, 4.8, z), vec3(0.08, 4.4, 0.08), steel, 0.);
                }
                for y in [1.5, 3., 4.5, 6., 7.5] {
                    t.box_(vec3(0., y, -0.5), vec3(0.5, 0.04, 0.04), steel, 0.);
                    t.box_(vec3(0., y, 0.5), vec3(0.5, 0.04, 0.04), steel, 0.);
                    t.box_(vec3(-0.5, y, 0.), vec3(0.04, 0.04, 0.5), steel, 0.);
                    t.box_(vec3(0.5, y, 0.), vec3(0.04, 0.04, 0.5), steel, 0.);
                }
                t.box_(vec3(0., 9.4, 0.), vec3(0.7, 0.3, 0.7), steel, 0.);
                t.box_(vec3(3.5, 9.7, 0.), vec3(5., 0.14, 0.3), steel, 0.);
                t.box_(vec3(-2.2, 9.4, 0.), vec3(0.8, 0.5, 0.6), [0.45, 0.45, 0.47], 0.);
                t.box_(vec3(0., 9.0, 0.9), vec3(0.6, 0.5, 0.35), [0.75, 0.55, 0.12], 0.);
                t.cylinder(vec3(7., 6.6, 0.), 0.02, 3., dark, 0., 4);
                t.box_(vec3(7., 6.5, 0.), vec3(0.25, 0.22, 0.2), dark, 0.);
            }
        }
        DecorKind::RailTrack => {
            run = true;
            let len = s;
            t.box_(vec3(len / 2., 0.04, 0.), vec3(len / 2., 0.04, 1.6), [0.44, 0.41, 0.37], 0.);
            let mut x = 0.2;
            while x < len {
                t.box_(vec3(x, 0.14, 0.), vec3(0.13, 0.06, 1.25), vary([0.30, 0.22, 0.15], hash(x, k, 1.), 0.2), 0.);
                x += 0.65;
            }
            for z in [-0.72, 0.72] {
                t.box_(vec3(len / 2., 0.24, z), vec3(len / 2., 0.065, 0.04), [0.42, 0.26, 0.18], 0.);
                t.box_(vec3(len / 2., 0.31, z), vec3(len / 2., 0.015, 0.065), [0.50, 0.40, 0.33], 0.);
            }
        }
        DecorKind::Tank => {
            let c = [0.66, 0.68, 0.66];
            t.cylinder(Vec3::ZERO, 1.25, 3.0, c, 0., 14);
            t.ball(vec3(0., 3.0, 0.), vec3(1.25, 0.55, 1.25), mulc(c, 1.05), 0., 14, 4);
            for y in [0.2, 1.5, 2.8] {
                t.cylinder(vec3(0., y, 0.), 1.27, 0.08, mulc(c, 0.6), 0., 14);
            }
            t.cylinder(vec3(0.9, 3.2, 0.3), 0.14, 0.4, [0.3, 0.3, 0.32], 0., 6);
            t.box_(vec3(1.26, 1.6, 0.), vec3(0.03, 1.6, 0.25), [0.3, 0.3, 0.3], 0.);
            cyl_x(&mut t, vec3(1.0, 0.5, -0.4), 0.9, 0.12, [0.45, 0.28, 0.2], 6);
        }
        DecorKind::Puddle => {
            t.ball(vec3(0., 0.006, 0.), vec3(0.8, 0.006, 0.55), [0.42, 0.50, 0.56], 0., 10, 2);
            t.ball(vec3(0.15, 0.009, 0.05), vec3(0.4, 0.004, 0.28), [0.52, 0.60, 0.65], 0., 8, 2);
        }
        DecorKind::Rubble => {
            for i in 0..9 {
                let h1 = hash(i as f32, k, 1.);
                let h2 = hash(i as f32, k, 2.);
                let h3 = hash(i as f32, k, 3.);
                let mut tmp = Template::new();
                let sz = 0.12 + 0.25 * h3;
                tmp.box_(Vec3::ZERO, vec3(sz, sz * 0.7, sz * 0.8), mix([0.50, 0.49, 0.46], [0.45, 0.30, 0.22], h2), 0.);
                t.append(&tmp.transformed(
                    Mat4::from_translation(vec3(
                        (h1 - 0.5) * 1.3,
                        sz * 0.6 + 0.3 * (1. - (h1 - 0.5).abs() * 2.) * h2,
                        (h2 - 0.5) * 1.1,
                    )) * Mat4::from_rotation_y(h1 * 6.)
                        * Mat4::from_rotation_z((h3 - 0.5) * 0.8),
                ));
            }
            t.cylinder(vec3(0.3, 0.2, 0.1), 0.012, 0.8, [0.45, 0.25, 0.15], 0., 4);
        }
        DecorKind::Sandbags => {
            run = true;
            let n = (s / 0.5).round().max(1.) as i32;
            for row in 0..3 {
                for i in 0..n {
                    let off = if row % 2 == 1 { 0.25 } else { 0. };
                    let x = (i as f32 + 0.5) * s / n as f32 + off * 0.0;
                    if row % 2 == 1 && i == n - 1 {
                        continue;
                    }
                    let x = x + off;
                    let col = vary([0.60, 0.52, 0.36], hash(x, row as f32, k), 0.12);
                    t.ball(vec3(x, 0.12 + row as f32 * 0.2, 0.), vec3(0.27, 0.11, 0.17), col, 0., 6, 4);
                }
            }
        }
        DecorKind::Cable => {
            run = true;
            let c = [0.12, 0.12, 0.13];
            cyl_x(&mut t, vec3(0., 0.03, 0.), s, 0.03, c, 5);
            cyl_x(&mut t, vec3(0., 0.03, 0.09), s, 0.025, [0.72, 0.36, 0.08], 5);
            let mut x = 1.5;
            while x < s {
                t.box_(vec3(x, 0.04, 0.045), vec3(0.04, 0.04, 0.09), [0.22, 0.22, 0.24], 0.);
                x += 3.;
            }
        }
    }
    place(&t, run)
}

// ---------------------------------------------------------------------------------------------------------------
// sky, horizon, ground

fn sky_template(look: &Look) -> Template {
    let fog = look.fog_color;
    let mut t = Template::new();
    let zenith = [0.47, 0.54, 0.61];
    let grad = move |e: f32| -> Rgb {
        if e < 0. {
            mix(fog, [0.35, 0.38, 0.37], (-e * 3.).min(1.))
        } else {
            mix(fog, zenith, e.powf(0.55))
        }
    };
    t.sky_dome(450., grad, 32, 16);
    // Soft sun glow behind the overcast and pale cloud banks.
    let sun = Vec3::from(look.key_direction).normalize();
    t.alpha = 0.25;
    t.ball(sun * 400., vec3(70., 70., 12.), [1., 0.95, 0.82], 0., 10, 6);
    t.alpha = 0.35;
    t.ball(sun * 398., vec3(30., 30., 8.), [1., 0.97, 0.88], 0., 10, 6);
    for i in 0..34 {
        let a = i as f32 * 0.185 + 0.4;
        let el = 0.35 + 0.5 * hash(i as f32, 1., 2.);
        let r = 420.;
        let (ce, se) = (el.cos(), el.sin());
        let p = vec3(a.cos() * ce * r, se * r, a.sin() * ce * r);
        let tone = 0.74 + 0.10 * hash(i as f32, 2., 3.);
        t.alpha = 0.16;
        t.ball(p, vec3(75. + 40. * hash(i as f32, 3., 4.), 7., 28.), [tone, tone + 0.02, tone + 0.04], 0., 8, 4);
    }
    t.alpha = 1.;
    silhouettes(&mut t, fog);
    t
}

/// Hills, a far factory skyline and a treeline, from far to near, hazy against the horizon.
fn silhouettes(t: &mut Template, fog: Rgb) {
    let place = |t: &mut Template, local: &Template, a: f32, r: f32, y: f32| {
        t.append(
            &local.transformed(Mat4::from_translation(vec3(a.cos() * r, y, a.sin() * r)) * Mat4::from_rotation_y(-a)),
        );
    };
    // Hills.
    let hill = mix(fog, [0.52, 0.60, 0.62], 0.55);
    for i in 0..26 {
        let a = i as f32 / 26. * TAU;
        let mut m = Template::new();
        m.ball(
            Vec3::ZERO,
            vec3(70. + 40. * hash(i as f32, 1., 1.), 14. + 14. * hash(i as f32, 2., 1.), 50.),
            hill,
            0.,
            8,
            4,
        );
        place(t, &m, a, 395., -8.);
    }
    // Factory skyline: sheds, stacks, a cooling tower or two.
    let far = mix(fog, [0.46, 0.50, 0.54], 0.5);
    for i in 0..40 {
        let a = i as f32 / 40. * TAU + hash(i as f32, 5., 1.) * 0.08;
        let h1 = hash(i as f32, 1., 7.);
        let h2 = hash(i as f32, 2., 7.);
        if h1 < 0.3 {
            continue;
        }
        let mut m = Template::new();
        let col = mulc(far, 0.94 + 0.12 * h2);
        let w = 12. + 25. * h2;
        m.box_(vec3(0., 4. + 4. * h1, 0.), vec3(w, 7. + 4. * h1, 10.), col, 0.);
        if h2 > 0.5 {
            m.cone(vec3(w * 0.6, 0., 0.), 2.0, 1.4, 30. + 18. * h1, mulc(col, 0.95), 0., 7);
            m.ball(
                vec3(w * 0.6 + 5., 36. + 18. * h1, 0.),
                vec3(10., 3., 6.),
                mix(col, [0.8, 0.82, 0.84], 0.6),
                0.,
                6,
                3,
            );
        }
        if h1 > 0.85 {
            m.cone(vec3(-w * 0.5, 0., 4.), 7.5, 5., 24., col, 0., 10);
        }
        place(t, &m, a, 340., -8.);
    }
    // Treeline: dense, low, darker.
    let tree = mix(fog, [0.30, 0.40, 0.34], 0.6);
    for i in 0..150 {
        let a = i as f32 / 150. * TAU + hash(i as f32, 9., 3.) * 0.03;
        let h = hash(i as f32, 4., 3.);
        let mut m = Template::new();
        let col = mulc(tree, 0.88 + 0.22 * hash(i as f32, 6., 3.));
        if h < 0.35 {
            m.cone(Vec3::ZERO, 4.5 + 2. * h, 0., 26. + 14. * h, col, 0., 6);
        } else {
            m.ball(vec3(0., 11. + 6. * h, 0.), vec3(8. + 4. * h, 7. + 4. * h, 7.), col, 0., 7, 4);
        }
        place(t, &m, a, 300. + 10. * h, -8.);
    }
}

/// The scenery beyond the perimeter wall: a ring of tall trees and dark pines so the upper floors and
/// catwalks look out over something, plus a few low factory roofs.
fn outside(level: &Level, acc: &mut Acc) {
    let (hx, hz) = (level.half_x, level.half_z);
    for i in 0..170 {
        let f = i as f32 / 170.;
        // Walk round a rectangle 8-30 m outside the wall.
        let per = 2. * (2. * hx + 2. * hz);
        let mut d = f * per;
        let off = 7. + 26. * hash(i as f32, 1., 11.);
        let (x, z) = if d < 2. * hx {
            (-hx + d, -hz - off)
        } else {
            d -= 2. * hx;
            if d < 2. * hz {
                (hx + off, -hz + d)
            } else {
                d -= 2. * hz;
                if d < 2. * hx {
                    (hx - d, hz + off)
                } else {
                    d -= 2. * hx;
                    (-hx - off, hz - d)
                }
            }
        };
        let k = hash(x, z, 5.);
        let s = 1.6 + 1.6 * k;
        let kind = if k > 0.55 { DecorKind::Pine } else { DecorKind::Tree };
        acc.t().append(&build_decor(&Decor { kind, pos: vesper3d::math::V(x, -0.05, z), yaw: k * 6., scale: s }));
    }
    // A few low factory roofs with stacks, further out.
    for i in 0..9 {
        let a = i as f32 / 9. * TAU + 0.3;
        let (x, z) = (a.cos() * (hx + 70.), a.sin() * (hz + 62.));
        let k = hash(x, z, 1.);
        let col = [0.45 + 0.1 * k, 0.47, 0.5];
        acc.t().box_(vec3(x, 4., z), vec3(16. + 10. * k, 4., 10. + 8. * k), col, 0.);
        acc.t().box_(vec3(x, 8.3, z), vec3(14. + 10. * k, 0.3, 9. + 8. * k), mulc(col, 0.8), 0.);
        acc.t().cone(vec3(x + 12., 0., z), 1.6, 1.1, 22. + 8. * k, [0.5, 0.3, 0.24], 0., 7);
    }
}

fn ground_outer() -> Template {
    let mut t = Template::new();
    t.box_top(vec3(0., -0.6, 0.), vec3(330., 0.55, 330.), [0.30, 0.34, 0.24], [0.33, 0.38, 0.25], 0.);
    t
}

pub fn overcast_afternoon() -> Look {
    Look {
        ambient_sky: [0.46, 0.50, 0.56],
        ambient_ground: [0.46, 0.45, 0.44],
        key_direction: [-0.35, 0.75, -0.45],
        key_color: [0.74, 0.69, 0.61],
        rim_color: [0.55, 0.60, 0.68],
        rim_strength: 0.08,
        fog_color: [0.70, 0.74, 0.77],
        fog_density: 0.0032,
        exposure: 1.0,
    }
}

/// Builds the meshes for `level`.
fn tree_collision_block(b: &Block, decor: &[Decor]) -> bool {
    b.material == Material::Wood
        && decor.iter().any(|d| {
            if !matches!(d.kind, DecorKind::Tree | DecorKind::Pine) {
                return false;
            }
            let r = if d.kind == DecorKind::Pine { 0.2 } else { 0.26 } * d.scale;
            let expected_min = vec3(d.pos.0 - r, d.pos.1, d.pos.2 - r);
            let expected_max = vec3(d.pos.0 + r, d.pos.1 + 3.2 * d.scale, d.pos.2 + r);
            (vec3(b.min.0, b.min.1, b.min.2) - expected_min).length() < 0.001
                && (vec3(b.max.0, b.max.1, b.max.2) - expected_max).length() < 0.001
        })
}

pub fn build(level: &Level) -> LevelScene {
    let mut solid = Acc::new();
    let mut glass = Template::new();
    for b in &level.blocks {
        // The tree mesh supplies its bark. Drawing the solid box as well adds plank/furniture detail
        // across the trunk; retain the block in the level for collision, bullets and navigation.
        if b.material == Material::Water || tree_collision_block(b, &level.decor) {
            continue;
        }
        add_block(&mut solid, &mut glass, b);
    }
    solid.t().append(&ground_outer());
    // Mottled ground where no patch covers the dirt: lighter dust, darker damp, a little green.
    let dirt = base_colour(Material::Dirt);
    for i in 0..260 {
        let (x, z) = (-58. + 116. * hash(i as f32, 1., 21.), -43. + 86. * hash(i as f32, 2., 21.));
        let w = 1.5 + 3.5 * hash(i as f32, 3., 21.);
        let tone = hash(i as f32, 4., 21.);
        let col = if tone < 0.15 { mix(dirt, [0.30, 0.38, 0.18], 0.5) } else { mulc(dirt, 0.82 + 0.4 * tone) };
        quad_y(solid.t(), x, x + w, z, z + w * (0.5 + hash(i as f32, 5., 21.)), 0.003 + i as f32 * 0.00001, col);
    }
    let mut decor = Acc::new();
    for d in &level.decor {
        let local = build_decor(d);
        decor.t().append(&local);
    }
    outside(level, &mut decor);
    // Ceiling fixtures under the indoor lights (not the street lamps, which have their own heads).
    let lamp_posts: Vec<_> = level.decor.iter().filter(|d| d.kind == DecorKind::LampPost).collect();
    for l in &level.lights {
        let near_post = lamp_posts.iter().any(|d| (d.pos.0 - l.pos.0).hypot(d.pos.2 - l.pos.2) < 1.5);
        if !near_post && l.pos.1 >= 2.4 && l.rgb[2] > 0.5 {
            decor.t().box_(vec3(l.pos.0, l.pos.1 + 0.3, l.pos.2), vec3(0.4, 0.03, 0.14), [1., 0.95, 0.8], 1.);
        }
    }
    let look = overcast_afternoon();
    let sky = sky_template(&look);
    let mut lights = Vec::new();
    let mut light_pos = Vec::new();
    for l in &level.lights {
        let p = vec3(l.pos.0, l.pos.1, l.pos.2);
        if let Ok(pl) = PointLight::new(p, l.radius, l.rgb, l.intensity) {
            lights.push(pl);
            light_pos.push(p);
        }
    }
    LevelScene { solid: solid.finish(), glass, decor: decor.finish(), sky, look, lights, light_pos }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tree_collision_boxes_are_replaced_by_their_trunk_meshes() {
        let level = crate::slagworks::build();
        let trees = level.decor.iter().filter(|d| matches!(d.kind, DecorKind::Tree | DecorKind::Pine)).count();
        let hidden = level.blocks.iter().filter(|b| tree_collision_block(b, &level.decor)).count();
        assert_eq!(hidden, trees);
        assert!(hidden > 10);
        assert!(
            level.blocks.iter().any(|b| b.material == Material::Wood && !tree_collision_block(b, &level.decor)),
            "furniture and crates must still render"
        );
        for d in level.decor.iter().filter(|d| matches!(d.kind, DecorKind::Tree | DecorKind::Pine)) {
            let mesh = build_decor(d);
            assert!(!mesh.verts.is_empty() && mesh.verts.iter().all(|v| v.p.is_finite()));
        }
    }
}
