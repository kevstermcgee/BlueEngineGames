//! Procedural 3D models of the 33 weapons, built from kit [`Template`]s in real-world metres.
//!
//! Weapon-local space (see [`WeaponAnchors`]): the origin is the centre of the firing hand's grip, -Z points out
//! of the muzzle, +Y is up, +X is the weapon's right. Every model is authored with the grip at the origin.
//!
//! Conventions for the optional moving parts of a [`WeaponModel`]:
//! * `mag`: the template is built around its own origin, which is the point given as its rest offset; draw it
//!   at `Mat4::from_translation(offset)` (plus whatever the reload animation adds). The body has no magazine.
//! * `slide`: the template is already in weapon-local space at rest (draw it with the identity, or shifted by
//!   `travel * k` for k in 0..1 while firing). The body has no slide.
//! * `anchors.support` is the point on the weapon the support palm touches (the underside of a handguard, a
//!   foregrip, the pump, the rocket tube). `anchors.sight` is the eye point when aiming: behind the rear sight
//!   or the scope eyepiece, on the bore plane of the sights.
use super::anchors::WeaponAnchors;
use macroquad::prelude::*;
use vesper3d::viewer::kit::Template;

/// One weapon, ready to be placed with a [`vesper3d::viewer::kit::Batch`].
pub struct WeaponModel {
    /// The weapon without the parts below, in weapon-local space.
    pub body: Template,
    pub anchors: WeaponAnchors,
    /// Muzzle to rear (for grenades the height), metres.
    pub length: f32,
    /// Detachable magazine, belt box or loaded rocket: (template around its own origin, rest offset).
    pub mag: Option<(Template, Vec3)>,
    /// Reciprocating part (slide, bolt handle, pump): (template in weapon space at rest, travel vector).
    pub slide: Option<(Template, Vec3)>,
}

impl WeaponModel {
    /// Every template merged in its rest pose: the complete weapon as one mesh.
    pub fn assembled(&self) -> Template {
        let mut t = self.body.clone();
        if let Some((m, off)) = &self.mag {
            t.append(&m.transformed(Mat4::from_translation(*off)));
        }
        if let Some((s, _)) = &self.slide {
            t.append(s);
        }
        t
    }
}

const KEYS: [&str; 33] = [
    "k9", "m45", "hc50", "rv357", "mp9", "ump", "pdw", "vkr", "k47", "m4c", "fm2", "bpa", "gl4", "dmr20", "svd",
    "scout", "awm", "m82", "pump12", "auto12", "sawn", "para", "pk", "rpg", "thumper", "frag", "flash", "smoke",
    "incen", "knife", "machete", "axe", "crowbar",
];

/// All 33 roster keys, in roster order.
pub fn keys() -> &'static [&'static str] {
    &KEYS
}

/// The model for a roster key; `None` for an unknown key.
pub fn build(key: &str) -> Option<WeaponModel> {
    Some(match key {
        "k9" => k9(),
        "m45" => m45(),
        "hc50" => hc50(),
        "rv357" => rv357(),
        "mp9" => mp9(),
        "ump" => ump(),
        "pdw" => pdw(),
        "vkr" => vkr(),
        "k47" => k47(),
        "m4c" => m4c(),
        "fm2" => fm2(),
        "bpa" => bpa(),
        "gl4" => gl4(),
        "dmr20" => dmr20(),
        "svd" => svd(),
        "scout" => scout(),
        "awm" => awm(),
        "m82" => m82(),
        "pump12" => pump12(),
        "auto12" => auto12(),
        "sawn" => sawn(),
        "para" => para(),
        "pk" => pk(),
        "rpg" => rpg(),
        "thumper" => thumper(),
        "frag" => frag(),
        "flash" => flash(),
        "smoke" => smoke(),
        "incen" => incen(),
        "knife" => knife(),
        "machete" => machete(),
        "axe" => axe(),
        "crowbar" => crowbar(),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------------------------------------
type C = [f32; 3];
const STEEL: C = [0.20, 0.21, 0.23];
const GUNMETAL: C = [0.33, 0.34, 0.37];
const STEEL_L: C = [0.55, 0.56, 0.59];
const CHROME: C = [0.76, 0.77, 0.80];
const POLY: C = [0.15, 0.15, 0.165];
const POLY_G: C = [0.27, 0.275, 0.29];
const WOOD: C = [0.35, 0.22, 0.12];
const WOOD_D: C = [0.25, 0.15, 0.08];
const WOOD_L: C = [0.46, 0.30, 0.16];
const TAN: C = [0.55, 0.46, 0.32];
const OLIVE: C = [0.25, 0.30, 0.17];
const OLIVE_D: C = [0.17, 0.21, 0.12];
const BRASS: C = [0.76, 0.58, 0.20];
const GLASS: C = [0.07, 0.22, 0.32];
const RED: C = [0.65, 0.10, 0.08];

// ---------------------------------------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------------------------------------
struct M {
    t: Template,
}

fn v3(a: [f32; 3]) -> Vec3 {
    vec3(a[0], a[1], a[2])
}

/// Height `z` of a line through (`y0`, `z0`) that leans by `deg` degrees about X (negative = bottom further back).
fn zat(y: f32, y0: f32, z0: f32, deg: f32) -> f32 {
    z0 + (y - y0) * deg.to_radians().tan()
}

impl M {
    fn new() -> Self {
        Self { t: Template::new() }
    }

    /// Axis-aligned box from coordinate ranges.
    fn bx(&mut self, x: [f32; 2], y: [f32; 2], z: [f32; 2], c: C) {
        let lo = vec3(x[0].min(x[1]), y[0].min(y[1]), z[0].min(z[1]));
        let hi = vec3(x[0].max(x[1]), y[0].max(y[1]), z[0].max(z[1]));
        self.t.box_((lo + hi) * 0.5, (hi - lo) * 0.5, c, 0.);
    }

    /// Box rotated about X by `deg` around its own centre (for raked grips and magazines).
    fn tb(&mut self, c: [f32; 3], h: [f32; 3], deg: f32, col: C) {
        let mut t = Template::new();
        t.box_(Vec3::ZERO, v3(h), col, 0.);
        self.t.append(&t.transformed(Mat4::from_translation(v3(c)) * Mat4::from_rotation_x(deg.to_radians())));
    }

    /// Cone or cylinder between two points (radius `r0` at `a`, `r1` at `b`).
    fn rod(&mut self, a: Vec3, b: Vec3, r0: f32, r1: f32, col: C, glow: f32) {
        let d = b - a;
        let len = d.length();
        if len < 1e-5 {
            return;
        }
        let sides = ((6. + r0.max(r1) * 300.) as usize).clamp(6, 16);
        let mut t = Template::new();
        t.cone(Vec3::ZERO, r0, r1, len, col, glow, sides);
        let m = Mat4::from_translation(a) * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, d / len));
        self.t.append(&t.transformed(m));
    }
    /// Cylinder along Z from `z0` to `z1` at (x, y).
    fn cz(&mut self, x: f32, y: f32, z: [f32; 2], r: f32, c: C) {
        self.rod(vec3(x, y, z[0]), vec3(x, y, z[1]), r, r, c, 0.);
    }
    /// Frustum along Z, radius `r[0]` at `z[0]`, `r[1]` at `z[1]`.
    fn kz(&mut self, x: f32, y: f32, z: [f32; 2], r: [f32; 2], c: C) {
        self.rod(vec3(x, y, z[0]), vec3(x, y, z[1]), r[0], r[1], c, 0.);
    }
    /// Cylinder along X at height `y`, depth `z`.
    fn cx(&mut self, y: f32, z: f32, x: [f32; 2], r: f32, c: C) {
        self.rod(vec3(x[0], y, z), vec3(x[1], y, z), r, r, c, 0.);
    }
    /// Cylinder along Y.
    fn cy(&mut self, x: f32, z: f32, y: [f32; 2], r: f32, c: C) {
        self.rod(vec3(x, y[0], z), vec3(x, y[1], z), r, r, c, 0.);
    }
    fn ball(&mut self, c: [f32; 3], radii: [f32; 3], col: C) {
        self.t.ball(v3(c), v3(radii), col, 0., 10, 7);
    }
    /// A dark glass disc (slight glow) facing along Z at `z`.
    fn lens(&mut self, x: f32, y: f32, z: [f32; 2], r: f32) {
        self.rod(vec3(x, y, z[0]), vec3(x, y, z[1]), r, r, GLASS, 0.35);
    }
    /// Run `f` for the right side (s = 1) and the left side (s = -1).
    fn sym(&mut self, f: impl Fn(&mut M, f32)) {
        f(self, 1.);
        f(self, -1.);
    }

    /// Convex polygon given as (y, z) points, extruded along X between `x[0]` and `x[1]`.
    fn prism(&mut self, x: [f32; 2], pts: &[(f32, f32)], c: C) {
        let n = pts.len();
        if n < 3 {
            return;
        }
        let cen = pts.iter().fold((0., 0.), |a, p| (a.0 + p.0, a.1 + p.1));
        let cen = (cen.0 / n as f32, cen.1 / n as f32);
        let p = |x: f32, q: (f32, f32)| vec3(x, q.0, q.1);
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let mut nrm = vec3(0., b.1 - a.1, -(b.0 - a.0));
            let mid = ((a.0 + b.0) * 0.5 - cen.0, (a.1 + b.1) * 0.5 - cen.1);
            if nrm.y * mid.0 + nrm.z * mid.1 < 0. {
                nrm = -nrm;
            }
            self.t.quad_facing([p(x[0], a), p(x[1], a), p(x[1], b), p(x[0], b)], nrm.normalize_or_zero(), c, 0.);
        }
        for i in 1..n - 1 {
            for (xx, nx) in [(x[1], 1.), (x[0], -1.)] {
                self.t.quad_facing(
                    [p(xx, pts[0]), p(xx, pts[i]), p(xx, pts[i + 1]), p(xx, pts[i + 1])],
                    vec3(nx, 0., 0.),
                    c,
                    0.,
                );
            }
        }
    }

    /// A bent slab (magazine, curved stock): a chain of (y, z, thickness along the path normal) points.
    fn band(&mut self, x: [f32; 2], path: &[(f32, f32, f32)], c: C) {
        let n = path.len();
        let edge = |i: usize| -> ((f32, f32), (f32, f32)) {
            let a = path[i.saturating_sub(1)];
            let b = path[(i + 1).min(n - 1)];
            let (dy, dz) = (b.0 - a.0, b.1 - a.1);
            let l = (dy * dy + dz * dz).sqrt().max(1e-6);
            let (ny, nz) = (-dz / l, dy / l);
            let h = path[i].2 * 0.5;
            ((path[i].0 + ny * h, path[i].1 + nz * h), (path[i].0 - ny * h, path[i].1 - nz * h))
        };
        for i in 0..n - 1 {
            let (f0, b0) = edge(i);
            let (f1, b1) = edge(i + 1);
            self.prism(x, &[f0, f1, b1, b0], c);
        }
    }
}

/// Offset helper for `fin`.
type Off = [f32; 3];

fn anch(support: Option<Off>, sight: Off, muzzle: Off, eject: Off) -> WeaponAnchors {
    WeaponAnchors { grip: Vec3::ZERO, support: support.map(v3), sight: v3(sight), muzzle: v3(muzzle), eject: v3(eject) }
}

/// Assemble: `mag` is built in weapon space and re-centred on `off`; `slide` stays in weapon space.
fn fin(body: M, anchors: WeaponAnchors, mag: Option<(M, Off)>, slide: Option<(M, Off)>) -> WeaponModel {
    let mag = mag.map(|(m, off)| (m.t.transformed(Mat4::from_translation(-v3(off))), v3(off)));
    let slide = slide.map(|(m, travel)| (m.t, v3(travel)));
    let mut model = WeaponModel { body: body.t, anchors, length: 0., mag, slide };
    // Muzzle to rear of the weapon proper: a raked pistol grip or magazine bottom does not count.
    let all = model.assembled();
    let (lo, hi) =
        all.verts.iter().filter(|v| v.p.y > -0.02).fold((f32::MAX, f32::MIN), |a, v| (a.0.min(v.p.z), a.1.max(v.p.z)));
    model.length = hi - lo;
    model
}

/// Trigger guard: front bar, bottom bar and the trigger blade.
fn trigger_guard(m: &mut M, y_top: f32, depth: f32, z: [f32; 2], w: f32, c: C) {
    let y0 = y_top - depth;
    m.bx([-w, w], [y0, y_top], [z[0], z[0] + 0.008], c);
    m.bx([-w, w], [y0, y0 + 0.008], [z[0], z[1]], c);
    m.bx([-0.0025, 0.0025], [y_top - depth * 0.75, y_top], [z[1] - 0.026, z[1] - 0.019], STEEL);
}

/// Telescopic sight centred on (x, y) between `z_rear` (eyepiece) and `z_front` (objective), with mounts
/// down to `rail_y`, turrets and glass at both ends.
fn scope(m: &mut M, rail_y: f32, x: f32, y: f32, z_rear: f32, z_front: f32, r: f32, r_obj: f32) {
    let len = z_rear - z_front;
    let black = [0.045, 0.045, 0.05];
    let (za, zb) = (z_rear - len * 0.28, z_front + len * 0.28);
    for z in [za, zb] {
        m.bx([x - 0.009, x + 0.009], [rail_y, y - r * 0.4], [z - 0.011, z + 0.011], GUNMETAL);
        m.cz(x, y, [z - 0.008, z + 0.008], r * 1.22, GUNMETAL);
    }
    m.cz(x, y, [z_rear - 0.05, z_front + 0.06], r, black);
    m.kz(x, y, [z_front + 0.07, z_front], [r, r_obj], black);
    m.kz(x, y, [z_rear - 0.055, z_rear], [r, r * 1.4], black);
    m.cz(x, y, [z_rear - 0.012, z_rear - 0.003], r * 1.46, STEEL_L);
    m.cy(x, (z_rear + z_front) * 0.5 - 0.01, [y + r * 0.8, y + r + 0.014], 0.0085, GUNMETAL);
    m.cx(y, (z_rear + z_front) * 0.5 - 0.01, [x + r * 0.8, x + r + 0.014], 0.0085, GUNMETAL);
    m.lens(x, y, [z_front - 0.002, z_front + 0.001], r_obj * 0.88);
    m.lens(x, y, [z_rear + 0.003, z_rear - 0.001], r * 1.15);
}

/// Picatinny-style rail: a base plate with up to 16 cross ribs (spaced 1.4 cm or wider on long rails).
fn rail(m: &mut M, x: f32, y: f32, z: [f32; 2], c: C) {
    m.bx([x - 0.011, x + 0.011], [y - 0.004, y], z, c);
    let len = (z[1] - z[0]).abs();
    let step = (len / 16.).max(0.014);
    let n = (len / step) as usize;
    let z0 = z[0].min(z[1]);
    for i in 0..n {
        let a = z0 + i as f32 * step;
        m.bx([x - 0.0115, x + 0.0115], [y, y + 0.004], [a + 0.003, a + step * 0.75], c);
    }
}

/// A row of small dark vents/grooves on a side (for handguards): `n` slots along z.
fn slots(m: &mut M, x: f32, y: [f32; 2], z: [f32; 2], n: usize, c: C) {
    for i in 0..n {
        let z0 = z[0] + (z[1] - z[0]) * (i as f32 + 0.5) / n as f32;
        m.bx([x - 0.0008, x + 0.0008], y, [z0 - 0.003, z0 + 0.003], c);
    }
}

// ---------------------------------------------------------------------------------------------------------
// Pistols
// ---------------------------------------------------------------------------------------------------------
fn k9() -> WeaponModel {
    let (mut b, mut s, mut g) = (M::new(), M::new(), M::new());
    let sl = [0.20, 0.21, 0.23];
    // slide
    s.bx([-0.0135, 0.0135], [0.040, 0.072], [-0.175, 0.020], sl);
    s.bx([-0.0095, 0.0095], [0.072, 0.0765], [-0.172, 0.018], sl);
    s.sym(|s, k| {
        for i in 0..5 {
            s.bx(
                [k * 0.0134, k * 0.0139],
                [0.044, 0.068],
                [-0.004 + i as f32 * 0.0065, -0.0015 + i as f32 * 0.0065],
                STEEL,
            );
        }
        for i in 0..3 {
            s.bx(
                [k * 0.0134, k * 0.0139],
                [0.044, 0.068],
                [-0.166 + i as f32 * 0.0065, -0.1635 + i as f32 * 0.0065],
                STEEL,
            );
        }
        s.bx([k * 0.004, k * 0.0075], [0.0765, 0.0815], [0.010, 0.018], STEEL);
    });
    s.bx([0.0134, 0.0141], [0.052, 0.068], [-0.085, -0.050], STEEL);
    s.bx([-0.0025, 0.0025], [0.0765, 0.0815], [-0.170, -0.164], STEEL);
    s.bx([-0.0008, 0.0008], [0.0815, 0.0825], [-0.1695, -0.1665], CHROME);
    s.cz(0., 0.056, [-0.183, -0.160], 0.0058, STEEL_L);
    s.cz(0., 0.056, [-0.1835, -0.1825], 0.0034, [0., 0., 0.]);
    // frame
    b.bx([-0.0122, 0.0122], [0.027, 0.040], [-0.12, 0.014], POLY);
    b.bx([-0.0105, 0.0105], [0.029, 0.040], [-0.152, -0.12], POLY);
    b.bx([-0.0085, 0.0085], [0.022, 0.030], [-0.095, -0.06], POLY);
    trigger_guard(&mut b, 0.030, 0.027, [-0.066, -0.006], 0.0095, POLY);
    b.tb([0., -0.028, 0.004], [0.0155, 0.062, 0.0205], -14., POLY);
    b.tb([0., -0.0, -0.014], [0.0162, 0.014, 0.004], -14., POLY_G);
    for i in 0..4 {
        b.tb(
            [0., -0.02 - i as f32 * 0.016, zat(-0.02 - i as f32 * 0.016, -0.028, 0.004, -14.) + 0.0205],
            [0.013, 0.004, 0.0015],
            -14.,
            POLY_G,
        );
    }
    // magazine
    let zm = zat(-0.105, -0.028, 0.004, -14.);
    g.tb([0., -0.105, zm], [0.0150, 0.032, 0.0200], -14., GUNMETAL);
    g.tb([0., -0.1345, zat(-0.1345, -0.028, 0.004, -14.)], [0.0162, 0.004, 0.0215], -14., POLY);
    fin(
        b,
        anch(None, [0., 0.096, 0.115], [0., 0.056, -0.184], [0.014, 0.062, -0.03]),
        Some((g, [0., -0.073, zm])),
        Some((s, [0., 0., 0.04])),
    )
}

fn m45() -> WeaponModel {
    let (mut b, mut s, mut g) = (M::new(), M::new(), M::new());
    let sl = [0.22, 0.225, 0.24];
    s.bx([-0.0135, 0.0135], [0.040, 0.0785], [-0.170, 0.030], sl);
    s.bx([-0.0135, 0.0135], [0.0785, 0.0795], [-0.170, 0.030], STEEL_L);
    s.sym(|s, k| {
        for i in 0..6 {
            s.bx(
                [k * 0.0134, k * 0.0139],
                [0.044, 0.074],
                [0.004 + i as f32 * 0.0038, 0.0065 + i as f32 * 0.0038],
                STEEL,
            );
        }
        s.bx([k * 0.005, k * 0.0085], [0.0795, 0.0855], [0.018, 0.027], STEEL); // rear sight blades
    });
    s.bx([0.0134, 0.0141], [0.056, 0.074], [-0.08, -0.04], STEEL); // ejection port
    s.bx([-0.002, 0.002], [0.0795, 0.0855], [-0.165, -0.160], STEEL_L); // front sight
    s.cz(0., 0.060, [-0.178, -0.166], 0.0105, STEEL_L); // bushing
    s.cz(0., 0.060, [-0.183, -0.17], 0.0065, STEEL);
    s.cz(0., 0.060, [-0.1835, -0.1825], 0.004, [0., 0., 0.]);
    b.bx([-0.0125, 0.0125], [0.026, 0.040], [-0.12, 0.028], STEEL_L);
    b.bx([-0.0115, 0.0115], [0.028, 0.040], [-0.145, -0.12], STEEL_L);
    b.bx([-0.0125, 0.0125], [0.027, 0.033], [-0.125, -0.06], STEEL_L);
    b.cy(0., -0.118, [0.012, 0.028], 0.0055, STEEL_L);
    trigger_guard(&mut b, 0.030, 0.025, [-0.070, -0.006], 0.0095, STEEL_L);
    b.bx([-0.0125, 0.0125], [0.036, 0.058], [0.020, 0.044], STEEL_L); // beavertail
    b.bx([-0.0055, 0.0055], [0.046, 0.070], [0.042, 0.050], STEEL); // grip safety
    b.tb([0., 0.068, 0.040], [0.0035, 0.011, 0.005], 30., STEEL_L); // hammer
    b.bx([-0.0145, -0.0135], [0.034, 0.041], [-0.040, -0.008], STEEL_L); // slide stop
    b.tb([0., -0.025, 0.010], [0.0125, 0.058, 0.0215], -17., STEEL_L);
    b.sym(|b, k| {
        b.tb([k * 0.0147, -0.022, 0.0115], [0.0022, 0.056, 0.0195], -17., WOOD);
    });
    let zm = zat(-0.105, -0.025, 0.010, -17.);
    g.tb([0., -0.105, zm], [0.0122, 0.030, 0.0205], -17., STEEL_L);
    g.tb([0., -0.1345, zat(-0.1345, -0.025, 0.010, -17.)], [0.0145, 0.005, 0.0225], -17., POLY);
    fin(
        b,
        anch(None, [0., 0.104, 0.12], [0., 0.06, -0.184], [0.014, 0.065, -0.06]),
        Some((g, [0., -0.075, zm])),
        Some((s, [0., 0., 0.04])),
    )
}

fn hc50() -> WeaponModel {
    let (mut b, mut s, mut g) = (M::new(), M::new(), M::new());
    let sl = CHROME;
    s.bx([-0.0175, 0.0175], [0.046, 0.092], [-0.075, 0.052], sl);
    s.bx([-0.0115, 0.0115], [0.092, 0.097], [-0.075, 0.052], STEEL_L);
    s.sym(|s, k| {
        for i in 0..6 {
            s.bx(
                [k * 0.0174, k * 0.0179],
                [0.050, 0.088],
                [0.030 + i as f32 * 0.003, 0.0315 + i as f32 * 0.003 - 0.0008],
                STEEL,
            );
        }
        s.bx([k * 0.0174, k * 0.0178], [0.058, 0.080], [-0.045, -0.005], STEEL);
        s.bx([k * 0.0055, k * 0.0105], [0.097, 0.104], [0.040, 0.050], STEEL);
    });
    // barrel housing with its top rib
    b.bx([-0.0150, 0.0150], [0.046, 0.088], [-0.236, -0.075], sl);
    b.bx([-0.0055, 0.0055], [0.088, 0.096], [-0.236, -0.075], STEEL_L);
    b.sym(|b, k| {
        for i in 0..6 {
            b.bx(
                [k * 0.0020, k * 0.0045],
                [0.0965, 0.0990],
                [-0.225 + i as f32 * 0.020, -0.215 + i as f32 * 0.020],
                STEEL,
            );
        }
        b.bx([k * 0.0149, k * 0.0154], [0.052, 0.066], [-0.22, -0.11], STEEL);
    });
    b.bx([-0.0025, 0.0025], [0.096, 0.104], [-0.233, -0.227], STEEL);
    b.cz(0., 0.063, [-0.2395, -0.232], 0.0078, STEEL);
    b.cz(0., 0.063, [-0.2400, -0.2390], 0.0050, [0., 0., 0.]);
    b.bx([-0.0145, 0.0145], [0.030, 0.046], [-0.22, 0.040], GUNMETAL);
    b.bx([-0.0135, 0.0135], [0.030, 0.050], [0.038, 0.058], GUNMETAL);
    trigger_guard(&mut b, 0.032, 0.028, [-0.078, 0.004], 0.0115, GUNMETAL);
    b.tb([0., -0.028, 0.022], [0.0185, 0.062, 0.0250], -16., POLY);
    b.sym(|b, k| {
        for i in 0..5 {
            let y = -0.008 - i as f32 * 0.0115;
            b.tb([k * 0.0187, y, zat(y, -0.028, 0.022, -16.)], [0.0008, 0.0035, 0.022], -16., GUNMETAL);
        }
    });
    let zm = zat(-0.105, -0.028, 0.022, -16.);
    g.tb([0., -0.105, zm], [0.0172, 0.030, 0.0235], -16., GUNMETAL);
    g.tb([0., -0.1355, zat(-0.1355, -0.028, 0.022, -16.)], [0.0185, 0.0045, 0.0260], -16., POLY);
    fin(
        b,
        anch(None, [0., 0.118, 0.13], [0., 0.063, -0.240], [0.0176, 0.070, 0.0]),
        Some((g, [0., -0.075, zm])),
        Some((s, [0., 0., 0.045])),
    )
}

fn rv357() -> WeaponModel {
    let mut b = M::new();
    let blue = [0.16, 0.18, 0.25];
    let cyl = [0.20, 0.22, 0.29];
    // frame, top strap, barrel and underlug
    b.bx([-0.0105, 0.0105], [0.030, 0.0855], [-0.008, 0.050], blue);
    b.bx([-0.0100, 0.0100], [0.078, 0.0855], [-0.075, -0.008], blue);
    b.bx([-0.0105, 0.0105], [0.040, 0.078], [-0.080, -0.055], blue);
    b.cz(0., 0.0700, [-0.222, -0.055], 0.0108, blue);
    b.bx([-0.0075, 0.0075], [0.042, 0.062], [-0.205, -0.070], blue);
    b.bx([-0.004, 0.004], [0.0805, 0.0865], [-0.220, -0.075], STEEL);
    b.cz(0., 0.040, [-0.165, -0.070], 0.0045, STEEL_L);
    b.cz(0., 0.040, [-0.171, -0.165], 0.007, STEEL_L);
    b.cz(0., 0.0700, [-0.2225, -0.2215], 0.0055, [0., 0., 0.]);
    b.bx([-0.0025, 0.0025], [0.0805, 0.0925], [-0.218, -0.206], STEEL);
    b.bx([-0.006, 0.006], [0.0855, 0.0935], [0.020, 0.033], STEEL);
    b.tb([0., 0.081, 0.052], [0.004, 0.014, 0.0055], 38., STEEL_L);
    // cylinder with flutes and the crane
    b.cz(0., 0.058, [-0.056, -0.008], 0.0235, cyl);
    for i in 0..6 {
        let a = (i as f32 + 0.5) * std::f32::consts::TAU / 6.;
        b.cz(a.sin() * 0.0235, 0.058 + a.cos() * 0.0235, [-0.050, -0.014], 0.0046, STEEL);
    }
    b.cz(0., 0.058, [-0.0565, -0.056], 0.0120, STEEL_L);
    b.bx([-0.0085, 0.0085], [0.030, 0.045], [-0.035, 0.040], blue);
    trigger_guard(&mut b, 0.030, 0.026, [-0.062, 0.002], 0.0095, blue);
    // wooden grip
    b.tb([0., -0.028, 0.014], [0.0152, 0.056, 0.0215], -16., WOOD);
    b.ball([0., -0.0865, 0.030], [0.0170, 0.011, 0.023], WOOD_D);
    b.sym(|b, k| {
        b.tb([k * 0.0150, -0.028, 0.014], [0.0012, 0.040, 0.016], -16., WOOD_D);
    });
    fin(b, anch(None, [0., 0.110, 0.12], [0., 0.07, -0.2225], [0.011, 0.058, -0.03]), None, None)
}
// ---------------------------------------------------------------------------------------------------------
// Submachine guns
// ---------------------------------------------------------------------------------------------------------
fn mp9() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let blk = STEEL;
    // receiver: stamped round-topped tube
    b.bx([-0.0205, 0.0205], [0.028, 0.056], [-0.15, 0.10], blk);
    b.cz(0., 0.056, [-0.15, 0.10], 0.0205, blk);
    b.bx([0.0205, 0.0212], [0.050, 0.070], [-0.03, 0.03], POLY); // ejection port
    b.bx([-0.0085, 0.0085], [0.0, 0.030], [-0.13, -0.065], blk); // magwell
                                                                 // handguard with grooves, barrel, sight ring and muzzle
    b.bx([-0.0255, 0.0255], [0.012, 0.062], [-0.27, -0.13], POLY);
    b.sym(|b, k| {
        slots(b, k * 0.0256, [0.022, 0.052], [-0.26, -0.14], 5, STEEL);
    });
    b.cz(0., 0.060, [-0.355, -0.27], 0.0095, blk);
    b.cz(0., 0.060, [-0.322, -0.29], 0.0165, blk);
    b.cz(0., 0.060, [-0.3565, -0.3555], 0.0052, [0., 0., 0.]);
    b.bx([-0.002, 0.002], [0.0765, 0.098], [-0.312, -0.307], STEEL);
    b.sym(|b, k| b.bx([k * 0.0052, k * 0.0075], [0.0765, 0.096], [-0.318, -0.300], blk));
    // rear drum sight
    b.bx([-0.012, 0.012], [0.0765, 0.084], [0.048, 0.072], blk);
    b.cx(0.092, 0.06, [-0.012, 0.012], 0.0115, GUNMETAL);
    b.bx([-0.0012, 0.0012], [0.098, 0.104], [0.055, 0.065], STEEL);
    // cocking tube on the left
    b.cz(-0.0245, 0.052, [-0.15, -0.105], 0.0055, STEEL_L);
    b.bx([-0.030, -0.0245], [0.046, 0.058], [-0.108, -0.098], STEEL_L);
    // grip and trigger group
    b.bx([-0.0135, 0.0135], [-0.003, 0.030], [-0.062, 0.022], POLY);
    trigger_guard(&mut b, 0.0, 0.022, [-0.06, 0.004], 0.0105, POLY);
    b.tb([0., -0.022, 0.012], [0.0165, 0.065, 0.0215], -20., POLY);
    b.tb([0., -0.070, zat(-0.070, -0.022, 0.012, -20.)], [0.0172, 0.008, 0.0225], -20., POLY_G);
    // collapsible wire stock
    b.sym(|b, k| {
        b.rod(vec3(k * 0.014, 0.054, 0.10), vec3(k * 0.014, 0.054, 0.29), 0.0042, 0.0042, STEEL_L, 0.);
        b.rod(vec3(k * 0.014, 0.014, 0.12), vec3(k * 0.014, 0.014, 0.29), 0.0042, 0.0042, STEEL_L, 0.);
        b.rod(vec3(k * 0.014, 0.054, 0.10), vec3(k * 0.014, 0.014, 0.12), 0.0042, 0.0042, STEEL_L, 0.);
    });
    b.bx([-0.0235, 0.0235], [-0.040, 0.068], [0.285, 0.302], POLY);
    // curved 30-round magazine
    g.band(
        [-0.0115, 0.0115],
        &[
            (0.032, -0.092, 0.029),
            (-0.020, -0.097, 0.029),
            (-0.070, -0.110, 0.029),
            (-0.118, -0.132, 0.029),
            (-0.140, -0.148, 0.029),
        ],
        GUNMETAL,
    );
    g.bx([-0.0125, 0.0125], [-0.147, -0.137], [-0.165, -0.130], POLY);
    fin(
        b,
        anch(Some([0., 0.012, -0.20]), [0., 0.118, 0.15], [0., 0.06, -0.3565], [0.022, 0.062, 0.0]),
        Some((g, [0., 0.032, -0.092])),
        None,
    )
}

fn ump() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let body = [0.19, 0.195, 0.20];
    b.bx([-0.0225, 0.0225], [0.022, 0.082], [-0.15, 0.115], body);
    b.bx([-0.0195, 0.0195], [0.082, 0.087], [-0.15, 0.115], body);
    b.bx([0.0225, 0.0231], [0.050, 0.074], [-0.03, 0.04], POLY); // ejection port
    b.bx([-0.0215, 0.0215], [0.014, 0.074], [-0.27, -0.15], body);
    slots_both(&mut b, 0.0216, [0.024, 0.060], [-0.26, -0.16], 6, POLY);
    rail(&mut b, 0., 0.087, [-0.265, 0.11], POLY);
    b.cz(0., 0.056, [-0.345, -0.27], 0.0100, STEEL);
    b.cz(0., 0.056, [-0.345, -0.315], 0.0140, STEEL);
    b.cz(0., 0.056, [-0.3455, -0.3445], 0.0058, [0., 0., 0.]);
    for i in 0..3 {
        b.bx([-0.0145, 0.0145], [0.052, 0.060], [-0.338 + i as f32 * 0.009, -0.333 + i as f32 * 0.009], POLY);
    }
    b.bx([-0.0025, 0.0025], [0.0915, 0.104], [-0.250, -0.244], STEEL);
    b.bx([-0.008, 0.008], [0.0915, 0.102], [0.020, 0.030], STEEL);
    b.bx([-0.0125, 0.0125], [-0.003, 0.024], [-0.065, 0.02], body);
    trigger_guard(&mut b, 0.0, 0.022, [-0.062, 0.004], 0.0105, body);
    b.tb([0., -0.022, 0.012], [0.0165, 0.065, 0.0215], -15., body);
    b.bx([-0.015, 0.015], [-0.05, 0.03], [-0.120, -0.052], body); // magwell
                                                                  // folding tube stock
    b.sym(|b, k| {
        b.rod(vec3(k * 0.013, 0.066, 0.115), vec3(k * 0.013, 0.066, 0.33), 0.0045, 0.0045, STEEL_L, 0.);
        b.rod(vec3(k * 0.013, 0.024, 0.115), vec3(k * 0.013, 0.024, 0.33), 0.0045, 0.0045, STEEL_L, 0.);
    });
    b.bx([-0.0225, 0.0225], [-0.010, 0.085], [0.325, 0.345], body);
    // straight translucent 25-round magazine, raked slightly forward
    g.tb([0., -0.040, -0.088], [0.0125, 0.070, 0.0170], 4., [0.24, 0.25, 0.28]);
    g.tb([0., -0.112, -0.0885 + 0.005], [0.0135, 0.004, 0.0185], 4., POLY);
    fin(
        b,
        anch(Some([0., 0.014, -0.21]), [0., 0.115, 0.10], [0., 0.056, -0.3455], [0.023, 0.062, 0.0]),
        Some((g, [0., 0.03, -0.088])),
        None,
    )
}

fn pdw() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let body = [0.17, 0.175, 0.185];
    // main body and translucent magazine channel
    b.bx([-0.0205, 0.0205], [0.030, 0.075], [-0.19, 0.24], body);
    b.kz(0., 0.0525, [-0.19, -0.235], [0.0205, 0.012], body);
    b.cz(0., 0.0525, [-0.27, -0.235], 0.0085, STEEL);
    b.cz(0., 0.0525, [-0.2705, -0.2695], 0.0045, [0., 0., 0.]);
    for i in 0..3 {
        b.bx([-0.0115, 0.0115], [0.0465, 0.0585], [-0.268 + i as f32 * 0.008, -0.263 + i as f32 * 0.008], STEEL);
    }
    // top rail with the ring sight
    b.bx([-0.0135, 0.0135], [0.1085, 0.113], [-0.17, 0.17], body);
    b.bx([-0.014, 0.014], [0.113, 0.128], [-0.095, 0.02], STEEL);
    b.cz(0., 0.128, [-0.095, 0.02], 0.0125, STEEL);
    b.lens(0., 0.128, [0.021, 0.019], 0.0100);
    b.lens(0., 0.128, [-0.096, -0.094], 0.0100);
    b.bx([-0.0012, 0.0012], [0.113, 0.124], [-0.17, -0.16], STEEL);
    // grip, forward finger stop, bottom strap and stock with the thumbhole
    b.tb([0., -0.012, 0.0], [0.0165, 0.058, 0.0185], -8., body);
    b.bx([-0.0205, 0.0205], [-0.022, 0.030], [-0.125, -0.066], body);
    b.bx([-0.017, 0.017], [-0.066, -0.048], [-0.012, 0.22], body);
    b.prism(
        [-0.0205, 0.0205],
        &[(0.075, 0.115), (0.075, 0.27), (0.04, 0.277), (-0.06, 0.272), (-0.07, 0.22), (-0.06, 0.115)],
        body,
    );
    b.bx([-0.0195, 0.0195], [-0.055, 0.05], [0.272, 0.282], POLY);
    b.bx([0.0206, 0.0212], [-0.03, -0.005], [0.12, 0.20], STEEL); // ejection chute
                                                                  // magazine lying on top
    g.bx([-0.0235, 0.0235], [0.075, 0.108], [-0.17, 0.12], [0.33, 0.34, 0.37]);
    g.bx([-0.0238, 0.0238], [0.082, 0.101], [-0.12, 0.08], [0.62, 0.50, 0.22]);
    g.bx([-0.0245, 0.0245], [0.075, 0.108], [-0.172, -0.165], POLY);
    g.bx([-0.0245, 0.0245], [0.075, 0.108], [0.112, 0.122], POLY);
    fin(
        b,
        anch(Some([0., -0.022, -0.095]), [0., 0.130, 0.11], [0., 0.0525, -0.2705], [0.0, -0.03, 0.14]),
        Some((g, [0., 0.0915, -0.025])),
        None,
    )
}

fn vkr() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let tan = TAN;
    let tan_d = [0.38, 0.32, 0.22];
    b.prism([-0.0225, 0.0225], &[(0.095, 0.14), (0.095, -0.09), (0.072, -0.145), (0.022, -0.145), (0.022, 0.14)], tan);
    b.cz(0., 0.050, [-0.27, -0.14], 0.0215, tan_d);
    b.cz(0., 0.050, [-0.31, -0.27], 0.0135, STEEL);
    b.cz(0., 0.050, [-0.3105, -0.3095], 0.0065, [0., 0., 0.]);
    for i in 0..3 {
        b.bx([-0.0145, 0.0145], [0.044, 0.056], [-0.305 + i as f32 * 0.009, -0.300 + i as f32 * 0.009], STEEL);
    }
    slots_both(&mut b, 0.0216, [0.040, 0.062], [-0.25, -0.16], 6, POLY);
    rail(&mut b, 0., 0.095, [-0.13, 0.14], tan_d);
    b.bx([-0.0025, 0.0025], [0.099, 0.118], [-0.105, -0.099], STEEL);
    b.bx([-0.009, 0.009], [0.099, 0.117], [0.098, 0.108], STEEL);
    b.bx([0.0226, 0.0232], [0.060, 0.085], [-0.04, 0.03], STEEL);
    trigger_guard(&mut b, 0.022, 0.034, [-0.085, 0.004], 0.0105, tan);
    b.tb([0., -0.028, 0.010], [0.0170, 0.058, 0.0235], -22., tan);
    // folding stock
    b.sym(|b, k| {
        b.rod(vec3(k * 0.014, 0.085, 0.14), vec3(k * 0.014, 0.060, 0.27), 0.0048, 0.0048, STEEL_L, 0.);
        b.rod(vec3(k * 0.014, 0.030, 0.12), vec3(k * 0.014, -0.020, 0.27), 0.0048, 0.0048, STEEL_L, 0.);
    });
    b.bx([-0.0225, 0.0225], [-0.045, 0.075], [0.265, 0.282], POLY);
    let zm = zat(-0.098, -0.028, 0.010, -22.);
    g.tb([0., -0.098, zm], [0.0150, 0.034, 0.0215], -22., tan_d);
    g.tb([0., -0.1325, zat(-0.1325, -0.028, 0.010, -22.)], [0.0165, 0.004, 0.0232], -22., POLY);
    fin(
        b,
        anch(Some([0., 0.028, -0.20]), [0., 0.130, 0.17], [0., 0.05, -0.3105], [0.0235, 0.072, 0.0]),
        Some((g, [0., -0.064, zm])),
        None,
    )
}

/// Dark slots on both side faces (at +-x).
fn slots_both(m: &mut M, x: f32, y: [f32; 2], z: [f32; 2], n: usize, c: C) {
    slots(m, x, y, z, n, c);
    slots(m, -x, y, z, n, c);
}
// ---------------------------------------------------------------------------------------------------------
// Assault rifles
// ---------------------------------------------------------------------------------------------------------
fn k47() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let blk = STEEL;
    // stamped receiver, arched dust cover and trunnion
    b.bx([-0.0185, 0.0185], [0.030, 0.078], [-0.17, 0.115], blk);
    b.prism([-0.0175, 0.0175], &[(0.078, 0.115), (0.090, 0.10), (0.095, -0.03), (0.090, -0.17), (0.078, -0.17)], blk);
    b.bx([-0.0195, 0.0195], [0.040, 0.087], [-0.215, -0.17], GUNMETAL);
    b.bx([0.0186, 0.0192], [0.055, 0.078], [-0.105, 0.0], POLY);
    b.bx([0.0186, 0.0208], [0.056, 0.062], [-0.02, 0.085], STEEL_L);
    // barrel, gas block and tube, wooden handguards
    b.cz(0., 0.067, [-0.60, -0.17], 0.0075, blk);
    b.bx([-0.0115, 0.0115], [0.056, 0.104], [-0.445, -0.41], GUNMETAL);
    b.cz(0., 0.0805, [-0.41, -0.24], 0.0178, WOOD_L);
    b.bx([-0.0185, 0.0185], [0.030, 0.066], [-0.41, -0.215], WOOD_L);
    b.bx([-0.0200, 0.0200], [0.040, 0.078], [-0.255, -0.240], STEEL_L);
    b.bx([-0.0200, 0.0200], [0.026, 0.066], [-0.415, -0.405], STEEL_L);
    b.cz(0., 0.043, [-0.55, -0.41], 0.0028, STEEL_L);
    // sights
    b.bx([-0.0095, 0.0095], [0.090, 0.100], [-0.232, -0.180], blk);
    b.bx([-0.0075, 0.0075], [0.100, 0.108], [-0.210, -0.196], blk);
    b.sym(|b, k| b.bx([k * 0.0025, k * 0.0060], [0.108, 0.114], [-0.209, -0.197], blk));
    b.bx([-0.0065, 0.0065], [0.060, 0.088], [-0.575, -0.535], GUNMETAL);
    b.bx([-0.0015, 0.0015], [0.088, 0.117], [-0.560, -0.556], STEEL);
    b.sym(|b, k| b.bx([k * 0.0058, k * 0.0086], [0.088, 0.117], [-0.566, -0.550], GUNMETAL));
    b.bx([-0.0088, 0.0088], [0.115, 0.119], [-0.566, -0.550], GUNMETAL);
    // muzzle brake
    b.cz(0., 0.067, [-0.604, -0.590], 0.0105, GUNMETAL);
    b.cz(0., 0.067, [-0.604, -0.600], 0.0122, STEEL_L);
    b.cz(0., 0.067, [-0.6045, -0.6035], 0.0055, [0., 0., 0.]);
    // grip, trigger guard, wooden stock
    b.tb([0., -0.030, 0.012], [0.0165, 0.070, 0.022], -23., WOOD_D);
    trigger_guard(&mut b, 0.030, 0.030, [-0.068, 0.004], 0.0095, blk);
    b.prism(
        [-0.0170, 0.0170],
        &[(0.072, 0.11), (0.060, 0.265), (0.052, 0.278), (-0.085, 0.278), (-0.088, 0.25), (-0.02, 0.11)],
        WOOD,
    );
    b.bx([-0.0173, 0.0173], [-0.088, 0.052], [0.278, 0.285], STEEL);
    // curved magazine
    let rib = GUNMETAL;
    g.band(
        [-0.0145, 0.0145],
        &[
            (0.032, -0.052, 0.038),
            (-0.020, -0.060, 0.038),
            (-0.070, -0.076, 0.038),
            (-0.115, -0.100, 0.038),
            (-0.150, -0.133, 0.038),
        ],
        STEEL,
    );
    g.band([-0.0150, 0.0150], &[(-0.040, -0.064, 0.030), (-0.046, -0.066, 0.030), (-0.050, -0.067, 0.030)], rib);
    g.band([-0.0150, 0.0150], &[(-0.085, -0.084, 0.030), (-0.091, -0.087, 0.030), (-0.095, -0.089, 0.030)], rib);
    // charging handle
    s.bx([0.0186, 0.0246], [0.060, 0.066], [-0.036, -0.016], STEEL_L);
    s.bx([0.0246, 0.0296], [0.056, 0.072], [-0.040, -0.012], STEEL_L);
    fin(
        b,
        anch(Some([0., 0.030, -0.32]), [0., 0.126, -0.10], [0., 0.067, -0.6045], [0.02, 0.066, -0.05]),
        Some((g, [0., 0.032, -0.052])),
        Some((s, [0., 0., 0.07])),
    )
}

fn m4c() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let up = [0.17, 0.175, 0.185];
    let lo = POLY;
    // upper and lower receivers
    b.bx([-0.0195, 0.0195], [0.050, 0.094], [-0.14, 0.055], up);
    b.bx([-0.0185, 0.0185], [0.0, 0.050], [-0.12, 0.06], lo);
    b.bx([-0.0135, 0.0135], [-0.020, 0.0], [-0.098, -0.052], lo);
    b.bx([0.0196, 0.0202], [0.060, 0.084], [-0.06, 0.02], STEEL);
    b.bx([0.0196, 0.0228], [0.066, 0.078], [0.022, 0.042], STEEL_L);
    b.cy(0.0196, -0.05, [0.084, 0.092], 0.007, STEEL_L);
    // flat-top rail and carry handle
    rail(&mut b, 0., 0.098, [-0.13, 0.05], up);
    b.sym(|b, k| {
        b.bx([k * 0.0075, k * 0.0135], [0.102, 0.128], [-0.115, -0.092], up);
        b.bx([k * 0.0075, k * 0.0135], [0.102, 0.128], [0.018, 0.048], up);
    });
    b.bx([-0.0135, 0.0135], [0.122, 0.136], [-0.115, 0.048], up);
    b.bx([-0.0085, 0.0085], [0.136, 0.144], [0.030, 0.046], STEEL);
    b.bx([-0.0135, 0.0135], [0.088, 0.100], [0.052, 0.082], STEEL); // charging handle
                                                                    // round handguard, barrel, gas block, muzzle device
    b.cz(0., 0.072, [-0.39, -0.135], 0.0215, lo);
    b.cz(0., 0.072, [-0.148, -0.135], 0.0250, STEEL);
    slots_both(&mut b, 0.0214, [0.060, 0.084], [-0.37, -0.17], 7, STEEL);
    b.cz(0., 0.072, [-0.50, -0.39], 0.0075, STEEL);
    b.bx([-0.0100, 0.0100], [0.064, 0.100], [-0.432, -0.396], STEEL);
    b.bx([-0.0015, 0.0015], [0.100, 0.138], [-0.425, -0.421], STEEL);
    b.sym(|b, k| b.bx([k * 0.0055, k * 0.0075], [0.100, 0.132], [-0.431, -0.415], STEEL));
    b.cz(0., 0.072, [-0.535, -0.490], 0.0105, STEEL);
    b.cz(0., 0.072, [-0.5355, -0.5345], 0.0055, [0., 0., 0.]);
    for i in 0..3 {
        b.bx([-0.0112, 0.0112], [0.066, 0.078], [-0.528 + i as f32 * 0.011, -0.523 + i as f32 * 0.011], lo);
    }
    // pistol grip and trigger guard
    b.tb([0., -0.030, 0.012], [0.0165, 0.060, 0.0215], -18., lo);
    trigger_guard(&mut b, 0.0, 0.030, [-0.058, 0.0], 0.0095, lo);
    // buffer tube and collapsible stock
    b.cz(0., 0.060, [0.055, 0.275], 0.0185, STEEL);
    b.prism(
        [-0.019, 0.019],
        &[(0.070, 0.14), (0.070, 0.295), (0.030, 0.312), (-0.045, 0.308), (-0.048, 0.27), (0.0, 0.14)],
        lo,
    );
    b.bx([-0.0195, 0.0195], [-0.050, 0.040], [0.306, 0.318], POLY_G);
    // STANAG magazine
    g.band(
        [-0.0115, 0.0115],
        &[(0.0, -0.075, 0.028), (-0.045, -0.078, 0.028), (-0.095, -0.086, 0.028), (-0.132, -0.098, 0.028)],
        GUNMETAL,
    );
    g.bx([-0.0125, 0.0125], [-0.142, -0.130], [-0.114, -0.084], POLY);
    fin(
        b,
        anch(Some([0., 0.050, -0.26]), [0., 0.148, 0.11], [0., 0.072, -0.5355], [0.022, 0.072, -0.01]),
        Some((g, [0., 0.0, -0.075])),
        None,
    )
}

fn fm2() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let ol = OLIVE;
    let ol_d = OLIVE_D;
    // body: front receiver/shroud block and the bullpup rear section
    b.bx([-0.0255, 0.0255], [0.0, 0.078], [-0.30, 0.04], ol);
    b.prism(
        [-0.0245, 0.0245],
        &[(0.078, 0.03), (0.078, 0.27), (-0.055, 0.275), (-0.06, 0.225), (-0.03, 0.10), (0.0, 0.03)],
        ol,
    );
    b.bx([-0.0252, 0.0252], [-0.066, 0.040], [0.268, 0.282], POLY);
    b.bx([0.0256, 0.0262], [0.040, 0.066], [0.06, 0.13], POLY);
    // barrel, sleeve, flash hider and folded bipod legs
    b.cz(0., 0.060, [-0.47, -0.30], 0.0075, STEEL);
    b.cz(0., 0.060, [-0.38, -0.30], 0.0135, ol_d);
    b.cz(0., 0.060, [-0.475, -0.445], 0.0098, STEEL);
    b.cz(0., 0.060, [-0.4755, -0.4745], 0.0052, [0., 0., 0.]);
    b.cz(0., 0.060, [-0.462, -0.450], 0.0140, STEEL_L);
    b.sym(|b, k| {
        b.rod(vec3(k * 0.0290, 0.052, -0.43), vec3(k * 0.0290, 0.052, -0.25), 0.0034, 0.0034, STEEL_L, 0.);
    });
    b.bx([-0.0265, 0.0265], [0.008, 0.070], [-0.26, -0.235], ol_d);
    // carry handle with the sights
    b.sym(|b, k| {
        b.bx([k * 0.0075, k * 0.0155], [0.078, 0.128], [-0.236, -0.216], ol_d);
        b.bx([k * 0.0075, k * 0.0155], [0.078, 0.128], [0.105, 0.125], ol_d);
    });
    b.bx([-0.0165, 0.0165], [0.120, 0.132], [-0.236, 0.125], ol_d);
    b.bx([-0.008, 0.008], [0.132, 0.142], [0.110, 0.125], STEEL);
    b.bx([-0.005, 0.005], [0.078, 0.112], [-0.320, -0.290], STEEL);
    b.bx([-0.0012, 0.0012], [0.112, 0.124], [-0.311, -0.306], STEEL);
    // grip and trigger guard
    b.tb([0., -0.032, 0.0], [0.0165, 0.056, 0.0215], -14., POLY);
    trigger_guard(&mut b, 0.0, 0.028, [-0.062, -0.002], 0.0105, POLY);
    // magazine behind the grip
    g.band([-0.0120, 0.0120], &[(-0.030, 0.088, 0.032), (-0.075, 0.091, 0.032), (-0.123, 0.100, 0.032)], GUNMETAL);
    g.bx([-0.0130, 0.0130], [-0.133, -0.121], [0.082, 0.120], POLY);
    fin(
        b,
        anch(Some([0., 0.0, -0.20]), [0., 0.150, 0.20], [0., 0.06, -0.4755], [0.0263, 0.052, 0.10]),
        Some((g, [0., -0.030, 0.088])),
        None,
    )
}

fn bpa() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let ol = OLIVE;
    let blk = [0.07, 0.075, 0.08];
    b.prism(
        [-0.0285, 0.0285],
        &[(0.085, -0.25), (0.085, 0.25), (0.05, 0.278), (-0.06, 0.278), (-0.065, 0.20), (-0.03, 0.05), (-0.02, -0.25)],
        ol,
    );
    b.bx([-0.0295, 0.0295], [-0.062, 0.048], [0.272, 0.288], POLY);
    b.bx([0.0286, 0.0292], [0.040, 0.070], [0.06, 0.12], STEEL);
    // barrel and sleeve
    b.cz(0., 0.060, [-0.50, -0.25], 0.0078, STEEL);
    b.cz(0., 0.060, [-0.36, -0.25], 0.0135, ol);
    b.cz(0., 0.060, [-0.505, -0.470], 0.0105, STEEL);
    b.cz(0., 0.060, [-0.5055, -0.5045], 0.0055, [0., 0., 0.]);
    b.cz(0., 0.060, [-0.478, -0.468], 0.0135, STEEL_L);
    // optic handle
    b.bx([-0.0195, 0.0195], [0.085, 0.104], [-0.22, 0.10], ol);
    b.cz(0., 0.116, [-0.10, 0.11], 0.0205, ol);
    b.kz(0., 0.116, [-0.10, -0.135], [0.0205, 0.0245], blk);
    b.cz(0., 0.116, [-0.02, 0.0], 0.0214, blk);
    b.cz(0., 0.116, [0.085, 0.10], 0.0214, blk);
    b.cz(0., 0.116, [0.11, 0.123], 0.0230, STEEL_L);
    b.lens(0., 0.116, [-0.137, -0.134], 0.0210);
    b.lens(0., 0.116, [0.124, 0.122], 0.0160);
    b.bx([-0.0012, 0.0012], [0.104, 0.120], [-0.218, -0.212], STEEL);
    b.cy(0., -0.02, [0.136, 0.150], 0.0085, STEEL); // elevation cap
                                                    // grip, big trigger guard, vertical foregrip
    b.tb([0., -0.062, 0.002], [0.0165, 0.050, 0.0220], -10., ol);
    trigger_guard(&mut b, -0.018, 0.034, [-0.070, 0.0], 0.0105, ol);
    b.tb([0., -0.045, -0.215], [0.0145, 0.052, 0.0205], 6., ol);
    // translucent magazine behind the grip
    g.band(
        [-0.0140, 0.0140],
        &[(-0.040, 0.088, 0.036), (-0.090, 0.092, 0.036), (-0.135, 0.100, 0.036)],
        [0.30, 0.32, 0.34],
    );
    g.bx([-0.0145, 0.0145], [-0.145, -0.133], [0.084, 0.124], blk);
    fin(
        b,
        anch(Some([0., -0.05, -0.215]), [0., 0.120, 0.18], [0., 0.06, -0.5055], [0.029, 0.05, 0.10]),
        Some((g, [0., -0.040, 0.088])),
        None,
    )
}
// ---------------------------------------------------------------------------------------------------------
// Battle rifles, marksman rifles and sniper rifles
// ---------------------------------------------------------------------------------------------------------
fn gl4() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let rc = [0.25, 0.26, 0.27];
    let fur = [0.10, 0.10, 0.11];
    // stamped receiver with ejection port and drum sight
    b.bx([-0.0195, 0.0195], [0.030, 0.085], [-0.13, 0.23], rc);
    b.bx([0.0196, 0.0202], [0.055, 0.080], [-0.06, 0.03], STEEL);
    b.bx([-0.012, 0.012], [0.085, 0.094], [0.17, 0.215], STEEL);
    b.cx(0.103, 0.195, [-0.012, 0.012], 0.0105, GUNMETAL);
    b.bx([-0.0012, 0.0012], [0.109, 0.116], [0.190, 0.200], STEEL);
    // handguard, barrel, hooded front sight and flash hider
    b.bx([-0.0225, 0.0225], [0.012, 0.074], [-0.36, -0.13], fur);
    slots_both(&mut b, 0.0226, [0.026, 0.060], [-0.34, -0.15], 8, STEEL);
    b.cz(0., 0.066, [-0.60, -0.36], 0.0092, STEEL);
    b.cz(0., 0.066, [-0.605, -0.560], 0.0118, GUNMETAL);
    b.cz(0., 0.066, [-0.6055, -0.6045], 0.0058, [0., 0., 0.]);
    b.bx([-0.0120, 0.0120], [0.052, 0.078], [-0.545, -0.512], GUNMETAL);
    b.bx([-0.0015, 0.0015], [0.078, 0.100], [-0.532, -0.528], STEEL);
    b.sym(|b, k| b.bx([k * 0.0058, k * 0.0085], [0.078, 0.102], [-0.538, -0.522], GUNMETAL));
    // grip, trigger group, fixed stock
    b.bx([-0.0165, 0.0165], [-0.0, 0.030], [-0.055, 0.03], fur);
    trigger_guard(&mut b, 0.0, 0.028, [-0.058, 0.004], 0.0105, fur);
    b.tb([0., -0.030, 0.014], [0.0165, 0.065, 0.022], -20., fur);
    b.prism(
        [-0.0185, 0.0185],
        &[(0.078, 0.22), (0.070, 0.40), (0.062, 0.425), (-0.052, 0.425), (-0.058, 0.40), (0.015, 0.22)],
        fur,
    );
    b.bx([-0.0190, 0.0190], [-0.056, 0.064], [0.422, 0.434], POLY);
    // 20-round magazine
    g.band([-0.0145, 0.0145], &[(0.030, -0.060, 0.043), (-0.050, -0.066, 0.043), (-0.115, -0.074, 0.043)], GUNMETAL);
    g.band([-0.0150, 0.0150], &[(-0.040, -0.063, 0.035), (-0.047, -0.064, 0.035)], STEEL);
    g.band([-0.0150, 0.0150], &[(-0.085, -0.069, 0.035), (-0.092, -0.070, 0.035)], STEEL);
    fin(
        b,
        anch(Some([0., 0.012, -0.24]), [0., 0.122, 0.29], [0., 0.066, -0.6055], [0.02, 0.068, 0.0]),
        Some((g, [0., 0.030, -0.060])),
        None,
    )
}

fn dmr20() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let tan = TAN;
    let tan_d = [0.40, 0.33, 0.22];
    b.bx([-0.0215, 0.0215], [0.020, 0.090], [-0.14, 0.17], tan);
    b.bx([0.0216, 0.0222], [0.050, 0.078], [-0.04, 0.03], POLY);
    // free-float handguard with slots, heavy barrel and muzzle brake
    b.bx([-0.0225, 0.0225], [0.030, 0.092], [-0.46, -0.14], tan);
    slots_both(&mut b, 0.0226, [0.045, 0.075], [-0.44, -0.16], 9, tan_d);
    rail(&mut b, 0., 0.098, [-0.46, 0.17], tan_d);
    b.cz(0., 0.066, [-0.66, -0.46], 0.0098, STEEL);
    b.cz(0., 0.066, [-0.685, -0.640], 0.0122, STEEL);
    b.cz(0., 0.066, [-0.6855, -0.6845], 0.0058, [0., 0., 0.]);
    for i in 0..3 {
        b.bx([-0.0128, 0.0128], [0.058, 0.074], [-0.676 + i as f32 * 0.012, -0.669 + i as f32 * 0.012], POLY);
    }
    // grip, guard, magwell, stock with cheek riser
    b.tb([0., -0.030, 0.012], [0.0170, 0.065, 0.0225], -20., tan_d);
    trigger_guard(&mut b, 0.020, 0.048, [-0.070, 0.0], 0.0105, tan);
    b.bx([-0.0145, 0.0145], [-0.012, 0.020], [-0.115, -0.045], tan);
    b.prism(
        [-0.0195, 0.0195],
        &[(0.085, 0.17), (0.085, 0.31), (0.03, 0.335), (-0.06, 0.335), (-0.06, 0.30), (0.0, 0.17)],
        tan,
    );
    b.bx([-0.0175, 0.0175], [0.085, 0.106], [0.18, 0.31], tan_d);
    b.bx([-0.0200, 0.0200], [-0.064, 0.085], [0.332, 0.346], POLY);
    // 3x scope
    scope(&mut b, 0.102, 0., 0.138, 0.06, -0.20, 0.0165, 0.027);
    g.band([-0.0120, 0.0120], &[(0.020, -0.080, 0.035), (-0.050, -0.082, 0.035), (-0.120, -0.086, 0.035)], POLY);
    g.bx([-0.0130, 0.0130], [-0.130, -0.118], [-0.106, -0.058], GUNMETAL);
    s.bx([0.0216, 0.0296], [0.060, 0.068], [-0.085, -0.065], STEEL_L);
    s.bx([0.0296, 0.0346], [0.055, 0.073], [-0.090, -0.060], STEEL_L);
    fin(
        b,
        anch(Some([0., 0.030, -0.30]), [0., 0.138, 0.125], [0., 0.066, -0.6855], [0.0222, 0.064, 0.0]),
        Some((g, [0., 0.020, -0.080])),
        Some((s, [0., 0., 0.06])),
    )
}

fn svd() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let blk = STEEL;
    // receiver with the arched cover, barrel, gas system and wooden handguards
    b.bx([-0.0185, 0.0185], [0.030, 0.080], [-0.17, 0.115], blk);
    b.prism([-0.0175, 0.0175], &[(0.080, 0.115), (0.090, 0.10), (0.095, -0.03), (0.090, -0.17), (0.080, -0.17)], blk);
    b.bx([-0.0195, 0.0195], [0.040, 0.087], [-0.21, -0.17], GUNMETAL);
    b.bx([0.0186, 0.0192], [0.055, 0.078], [-0.105, 0.0], POLY);
    b.cz(0., 0.067, [-0.80, -0.17], 0.0085, blk);
    b.bx([-0.0110, 0.0110], [0.056, 0.100], [-0.455, -0.42], GUNMETAL);
    b.cz(0., 0.0795, [-0.42, -0.25], 0.0172, WOOD);
    b.bx([-0.0185, 0.0185], [0.030, 0.066], [-0.42, -0.215], WOOD);
    b.bx([-0.0195, 0.0195], [0.026, 0.066], [-0.425, -0.415], STEEL_L);
    b.bx([-0.0095, 0.0095], [0.090, 0.100], [-0.225, -0.180], blk);
    // front sight, flash hider
    b.bx([-0.0065, 0.0065], [0.060, 0.085], [-0.775, -0.745], GUNMETAL);
    b.bx([-0.0015, 0.0015], [0.085, 0.110], [-0.762, -0.758], STEEL);
    b.sym(|b, k| b.bx([k * 0.0058, k * 0.0086], [0.085, 0.110], [-0.768, -0.752], GUNMETAL));
    b.cz(0., 0.067, [-0.835, -0.785], 0.0112, GUNMETAL);
    b.cz(0., 0.067, [-0.8355, -0.8345], 0.0055, [0., 0., 0.]);
    for i in 0..3 {
        b.bx([-0.0120, 0.0120], [0.060, 0.074], [-0.825 + i as f32 * 0.012, -0.818 + i as f32 * 0.012], blk);
    }
    // grip and the skeleton thumbhole stock
    b.tb([0., -0.030, 0.012], [0.0165, 0.068, 0.022], -17., WOOD);
    trigger_guard(&mut b, 0.030, 0.030, [-0.068, 0.004], 0.0095, blk);
    b.prism([-0.0155, 0.0155], &[(0.078, 0.11), (0.080, 0.30), (0.072, 0.40), (0.030, 0.40), (0.030, 0.11)], WOOD);
    b.bx([-0.0085, 0.0085], [-0.092, -0.066], [0.0, 0.40], WOOD_D);
    b.bx([-0.0165, 0.0165], [-0.092, 0.078], [0.385, 0.402], WOOD);
    b.bx([-0.0170, 0.0170], [-0.095, 0.060], [0.402, 0.410], POLY);
    // PSO-1 style scope
    scope(&mut b, 0.092, 0., 0.131, 0.05, -0.20, 0.0175, 0.0275);
    b.cz(0., 0.131, [0.05, 0.075], 0.0215, POLY);
    g.band(
        [-0.0145, 0.0145],
        &[(0.030, -0.056, 0.036), (-0.020, -0.063, 0.036), (-0.062, -0.078, 0.036), (-0.092, -0.098, 0.036)],
        STEEL,
    );
    s.bx([0.0186, 0.0246], [0.060, 0.066], [-0.036, -0.016], STEEL_L);
    s.bx([0.0246, 0.0296], [0.056, 0.072], [-0.040, -0.012], STEEL_L);
    fin(
        b,
        anch(Some([0., 0.030, -0.32]), [0., 0.131, 0.13], [0., 0.067, -0.8355], [0.0192, 0.066, -0.05]),
        Some((g, [0., 0.030, -0.056])),
        Some((s, [0., 0., 0.07])),
    )
}

fn scout() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let st = [0.17, 0.19, 0.15];
    let st_d = [0.11, 0.125, 0.10];
    // action, barrel, muzzle
    b.bx([-0.0175, 0.0175], [0.030, 0.082], [-0.10, 0.11], STEEL);
    b.cz(0., 0.066, [-0.63, -0.10], 0.0092, STEEL);
    b.cz(0., 0.066, [-0.645, -0.625], 0.0108, GUNMETAL);
    b.cz(0., 0.066, [-0.6455, -0.6445], 0.0055, [0., 0., 0.]);
    // fore-end, grip, stock
    b.bx([-0.0205, 0.0205], [0.012, 0.062], [-0.34, -0.10], st);
    slots_both(&mut b, 0.0206, [0.025, 0.048], [-0.33, -0.12], 7, st_d);
    b.tb([0., -0.030, 0.012], [0.0165, 0.060, 0.0215], -18., st);
    b.prism(
        [-0.0185, 0.0185],
        &[(0.070, 0.10), (0.078, 0.30), (0.066, 0.35), (-0.058, 0.35), (-0.058, 0.31), (-0.02, 0.10)],
        st,
    );
    b.bx([-0.0190, 0.0190], [-0.062, 0.070], [0.348, 0.360], POLY);
    trigger_guard(&mut b, 0.030, 0.030, [-0.070, 0.002], 0.0095, st_d);
    // ghost ring rear sight, forward-mounted scope on a rail along the barrel
    b.bx([-0.0055, 0.0055], [0.082, 0.098], [0.092, 0.102], STEEL);
    b.bx([-0.0085, 0.0085], [0.070, 0.090], [-0.28, -0.05], STEEL);
    scope(&mut b, 0.090, 0., 0.1085, -0.06, -0.26, 0.0135, 0.0225);
    // five-round magazine
    g.bx([-0.0125, 0.0125], [-0.050, 0.030], [-0.072, -0.008], st_d);
    g.bx([-0.0135, 0.0135], [-0.058, -0.048], [-0.076, -0.004], POLY);
    // bolt handle
    s.rod(vec3(0.0175, 0.066, 0.06), vec3(0.046, 0.052, 0.06), 0.0042, 0.0042, STEEL_L, 0.);
    s.ball([0.050, 0.048, 0.06], [0.0105, 0.0105, 0.0105], STEEL_L);
    s.cz(0., 0.0835, [0.085, 0.118], 0.0115, STEEL_L);
    fin(
        b,
        anch(Some([0., 0.012, -0.24]), [0., 0.1085, 0.145], [0., 0.066, -0.6455], [0.0176, 0.070, 0.04]),
        Some((g, [0., 0.030, -0.040])),
        Some((s, [0., 0., 0.07])),
    )
}

fn awm() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let gr = [0.20, 0.27, 0.19];
    let gr_d = [0.13, 0.17, 0.12];
    // long action, fluted barrel, big muzzle brake
    b.bx([-0.0195, 0.0195], [0.030, 0.090], [-0.24, 0.12], STEEL);
    b.cz(0., 0.072, [-0.80, -0.24], 0.0115, GUNMETAL);
    for i in 0..4 {
        b.cz(0., 0.072, [-0.56 - i as f32 * 0.05, -0.565 - i as f32 * 0.05], 0.0128, STEEL);
    }
    b.cz(0., 0.072, [-0.845, -0.790], 0.0185, STEEL);
    b.sym(|b, k| {
        for i in 0..3 {
            b.bx(
                [k * 0.0186, k * 0.0192],
                [0.060, 0.085],
                [-0.835 + i as f32 * 0.016, -0.827 + i as f32 * 0.016],
                [0.02, 0.02, 0.02],
            );
        }
    });
    b.cz(0., 0.072, [-0.8455, -0.8445], 0.0075, [0., 0., 0.]);
    // green chassis: fore-end, grip, stock with cheek riser
    b.bx([-0.0225, 0.0225], [0.008, 0.066], [-0.50, -0.24], gr);
    b.bx([-0.0210, 0.0210], [0.010, 0.060], [-0.24, 0.0], gr);
    slots_both(&mut b, 0.0226, [0.022, 0.050], [-0.47, -0.27], 6, gr_d);
    b.tb([0., -0.030, 0.012], [0.0168, 0.062, 0.0220], -18., gr);
    trigger_guard(&mut b, 0.030, 0.030, [-0.070, 0.004], 0.0095, gr_d);
    b.prism(
        [-0.0185, 0.0185],
        &[(0.072, 0.10), (0.084, 0.30), (0.084, 0.36), (-0.060, 0.36), (-0.064, 0.30), (-0.02, 0.10)],
        gr,
    );
    b.bx([-0.0160, 0.0160], [0.084, 0.102], [0.17, 0.30], gr_d);
    b.bx([-0.0195, 0.0195], [-0.064, 0.084], [0.358, 0.372], POLY);
    // folded bipod
    b.bx([-0.030, 0.030], [-0.004, 0.010], [-0.38, -0.34], STEEL);
    b.sym(|b, k| b.rod(vec3(k * 0.027, 0.0, -0.36), vec3(k * 0.027, -0.010, -0.58), 0.0045, 0.0045, STEEL_L, 0.));
    // 6x scope
    b.bx([-0.0085, 0.0085], [0.090, 0.104], [-0.24, 0.10], STEEL);
    scope(&mut b, 0.104, 0., 0.1365, 0.06, -0.26, 0.0165, 0.029);
    g.bx([-0.0135, 0.0135], [-0.070, 0.030], [-0.090, -0.015], gr_d);
    g.bx([-0.0145, 0.0145], [-0.078, -0.068], [-0.094, -0.011], POLY);
    s.rod(vec3(0.0195, 0.078, 0.06), vec3(0.052, 0.058, 0.065), 0.0045, 0.0045, STEEL_L, 0.);
    s.ball([0.056, 0.054, 0.066], [0.0125, 0.0125, 0.0125], gr_d);
    s.cz(0., 0.084, [0.10, 0.145], 0.0125, STEEL_L);
    fin(
        b,
        anch(Some([0., 0.008, -0.34]), [0., 0.1365, 0.13], [0., 0.072, -0.8455], [0.0196, 0.078, 0.0]),
        Some((g, [0., 0.030, -0.050])),
        Some((s, [0., 0., 0.075])),
    )
}

fn m82() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let rc = [0.22, 0.225, 0.23];
    // receivers
    b.bx([-0.0250, 0.0250], [0.030, 0.108], [-0.30, 0.12], rc);
    b.bx([-0.0240, 0.0240], [0.0, 0.030], [-0.30, 0.12], GUNMETAL);
    b.bx([0.0251, 0.0257], [0.065, 0.098], [-0.08, 0.02], STEEL);
    rail(&mut b, 0., 0.112, [-0.30, 0.10], STEEL);
    // heavy finned barrel, huge muzzle brake
    b.cz(0., 0.075, [-0.92, -0.30], 0.0205, GUNMETAL);
    for i in 0..9 {
        b.cz(0., 0.075, [-0.50 - i as f32 * 0.04, -0.51 - i as f32 * 0.04], 0.0248, STEEL);
    }
    b.cz(0., 0.075, [-1.02, -0.92], 0.0285, STEEL);
    b.cz(0., 0.075, [-1.0205, -1.0195], 0.0140, [0., 0., 0.]);
    b.sym(|b, k| {
        for i in 0..3 {
            b.bx(
                [k * 0.0270, k * 0.0300],
                [0.048, 0.102],
                [-1.005 + i as f32 * 0.028, -0.985 + i as f32 * 0.028],
                [0.015, 0.015, 0.015],
            );
        }
    });
    // carry handle over the barrel
    b.sym(|b, k| b.bx([k * 0.003, k * 0.010], [0.095, 0.150], [-0.635, -0.615], STEEL));
    b.sym(|b, k| b.bx([k * 0.003, k * 0.010], [0.095, 0.150], [-0.465, -0.445], STEEL));
    b.bx([-0.0105, 0.0105], [0.146, 0.160], [-0.635, -0.445], STEEL);
    // folded bipod
    b.bx([-0.032, 0.032], [0.030, 0.062], [-0.66, -0.62], STEEL);
    b.sym(|b, k| b.rod(vec3(k * 0.030, 0.045, -0.64), vec3(k * 0.030, 0.045, -0.88), 0.0065, 0.0065, STEEL_L, 0.));
    // grip, guard, stock
    b.tb([0., -0.040, 0.012], [0.0175, 0.065, 0.0230], -18., POLY);
    trigger_guard(&mut b, 0.0, 0.034, [-0.072, 0.0], 0.0115, POLY);
    b.bx([-0.0195, 0.0195], [0.018, 0.085], [0.12, 0.40], POLY);
    b.bx([-0.0160, 0.0160], [0.085, 0.100], [0.20, 0.38], STEEL);
    b.rod(vec3(0., 0.0, 0.14), vec3(0., -0.090, 0.41), 0.009, 0.009, STEEL, 0.);
    b.bx([-0.0300, 0.0300], [-0.095, 0.082], [0.398, 0.432], POLY);
    b.cy(0., 0.38, [-0.135, -0.090], 0.0075, STEEL_L);
    // 8x scope
    scope(&mut b, 0.116, 0., 0.1485, 0.06, -0.36, 0.0195, 0.032);
    // ten-round box magazine
    g.bx([-0.0205, 0.0205], [-0.100, 0.002], [-0.175, -0.085], GUNMETAL);
    g.bx([-0.0215, 0.0215], [-0.108, -0.096], [-0.180, -0.080], POLY);
    // charging handle
    s.bx([0.0250, 0.058], [0.068, 0.078], [-0.135, -0.115], STEEL_L);
    s.bx([0.058, 0.070], [0.060, 0.086], [-0.140, -0.110], STEEL_L);
    fin(
        b,
        anch(Some([0., 0.0, -0.22]), [0., 0.1485, 0.135], [0., 0.075, -1.0205], [0.0258, 0.085, -0.02]),
        Some((g, [0., 0.0, -0.130])),
        Some((s, [0., 0., 0.06])),
    )
}
// ---------------------------------------------------------------------------------------------------------
// Shotguns
// ---------------------------------------------------------------------------------------------------------
fn pump12() -> WeaponModel {
    let (mut b, mut s) = (M::new(), M::new());
    // receiver, barrel, magazine tube
    b.bx([-0.0175, 0.0175], [0.030, 0.090], [-0.215, 0.0], STEEL);
    b.bx([0.0176, 0.0182], [0.056, 0.084], [-0.105, -0.045], POLY);
    b.bx([-0.0100, 0.0100], [0.026, 0.032], [-0.14, -0.08], GUNMETAL);
    b.cz(0., 0.0785, [-0.745, -0.215], 0.0115, STEEL);
    b.cz(0., 0.0785, [-0.745, -0.730], 0.0125, GUNMETAL);
    b.cz(0., 0.0500, [-0.705, -0.215], 0.0125, GUNMETAL);
    b.cz(0., 0.0500, [-0.712, -0.698], 0.0142, STEEL);
    b.ball([0., 0.0935, -0.742], [0.004, 0.004, 0.004], CHROME);
    // wooden stock and guard
    b.prism(
        [-0.0185, 0.0185],
        &[(0.088, -0.005), (0.070, 0.235), (0.060, 0.305), (-0.062, 0.310), (-0.060, 0.275), (-0.030, -0.015)],
        WOOD,
    );
    b.bx([-0.0190, 0.0190], [-0.066, 0.064], [0.308, 0.322], POLY);
    trigger_guard(&mut b, 0.030, 0.030, [-0.105, -0.022], 0.0095, STEEL);
    // pump forend (moves with the action bars)
    s.bx([-0.0235, 0.0235], [0.030, 0.070], [-0.47, -0.30], WOOD_D);
    for i in 0..6 {
        s.bx([-0.0240, 0.0240], [0.034, 0.064], [-0.462 + i as f32 * 0.010, -0.458 + i as f32 * 0.010], WOOD);
    }
    s.sym(|s, k| s.cz(k * 0.0105, 0.047, [-0.30, -0.215], 0.003, STEEL_L));
    fin(
        b,
        anch(Some([0., 0.030, -0.385]), [0., 0.114, 0.08], [0., 0.0785, -0.745], [0.0183, 0.070, -0.075]),
        None,
        Some((s, [0., 0., 0.095])),
    )
}

fn auto12() -> WeaponModel {
    let mut b = M::new();
    let rc = [0.19, 0.195, 0.20];
    b.bx([-0.0195, 0.0195], [0.030, 0.092], [-0.20, 0.04], rc);
    b.bx([0.0196, 0.0202], [0.056, 0.084], [-0.11, -0.05], POLY);
    rail(&mut b, 0., 0.096, [-0.19, 0.03], rc);
    b.bx([-0.008, 0.008], [0.100, 0.118], [0.015, 0.032], STEEL);
    b.bx([-0.0012, 0.0012], [0.100, 0.122], [0.0, 0.0], STEEL);
    // barrel, tube, ribbed round handguard, muzzle
    b.cz(0., 0.078, [-0.68, -0.20], 0.0115, STEEL);
    b.cz(0., 0.050, [-0.64, -0.20], 0.0125, STEEL);
    b.cz(0., 0.0665, [-0.46, -0.20], 0.0238, POLY);
    slots_both(&mut b, 0.0237, [0.056, 0.078], [-0.44, -0.22], 8, POLY_G);
    b.cz(0., 0.078, [-0.685, -0.655], 0.0138, GUNMETAL);
    b.cz(0., 0.050, [-0.648, -0.638], 0.0142, GUNMETAL);
    b.bx([-0.0015, 0.0015], [0.088, 0.108], [-0.680, -0.672], STEEL);
    // pistol grip, guard, telescoping tube stock
    b.tb([0., -0.030, 0.012], [0.0165, 0.062, 0.0215], -20., POLY);
    trigger_guard(&mut b, 0.030, 0.030, [-0.105, 0.0], 0.0100, POLY);
    b.bx([-0.0140, 0.0140], [0.030, 0.090], [0.040, 0.100], POLY);
    b.sym(|b, k| {
        b.rod(vec3(k * 0.015, 0.078, 0.10), vec3(k * 0.015, 0.078, 0.30), 0.0055, 0.0055, STEEL_L, 0.);
        b.rod(vec3(k * 0.015, 0.032, 0.10), vec3(k * 0.015, 0.032, 0.30), 0.0055, 0.0055, STEEL_L, 0.);
    });
    b.bx([-0.0240, 0.0240], [-0.065, 0.098], [0.295, 0.318], POLY);
    fin(b, anch(Some([0., 0.0405, -0.33]), [0., 0.122, 0.09], [0., 0.078, -0.685], [0.0203, 0.070, -0.08]), None, None)
}

fn sawn() -> WeaponModel {
    let mut b = M::new();
    let barrel = [0.14, 0.145, 0.16];
    b.cz(-0.0098, 0.052, [-0.41, -0.08], 0.0098, barrel);
    b.cz(0.0098, 0.052, [-0.41, -0.08], 0.0098, barrel);
    b.bx([-0.0040, 0.0040], [0.0605, 0.0660], [-0.41, -0.08], STEEL_L);
    b.ball([0., 0.069, -0.405], [0.0035, 0.0035, 0.0035], CHROME);
    for k in [-1., 1.] {
        b.cz(k * 0.0098, 0.052, [-0.4105, -0.4095], 0.0062, [0., 0., 0.]);
    }
    // action, top lever, hammers
    b.bx([-0.0215, 0.0215], [0.028, 0.078], [-0.10, 0.005], GUNMETAL);
    b.bx([-0.0035, 0.0035], [0.078, 0.083], [-0.04, 0.012], STEEL_L);
    b.sym(|b, k| {
        b.tb([k * 0.0115, 0.088, 0.008], [0.0035, 0.011, 0.005], 35., STEEL_L);
        b.bx([k * 0.0216, k * 0.0222], [0.040, 0.068], [-0.085, -0.020], STEEL_L);
    });
    // forend, trigger guard, cut-down wooden stock
    b.bx([-0.0225, 0.0225], [0.020, 0.046], [-0.30, -0.10], WOOD);
    b.bx([-0.0100, 0.0100], [0.012, 0.022], [-0.29, -0.23], GUNMETAL);
    trigger_guard(&mut b, 0.028, 0.030, [-0.085, -0.014], 0.0095, STEEL);
    b.prism([-0.0185, 0.0185], &[(0.072, -0.01), (0.068, 0.11), (-0.03, 0.12), (-0.065, 0.08), (-0.05, -0.02)], WOOD);
    b.bx([-0.0190, 0.0190], [-0.066, 0.072], [0.110, 0.122], STEEL);
    fin(b, anch(Some([0., 0.020, -0.20]), [0., 0.105, 0.06], [0., 0.052, -0.4105], [0.022, 0.065, -0.05]), None, None)
}

// ---------------------------------------------------------------------------------------------------------
// Heavy weapons
// ---------------------------------------------------------------------------------------------------------
/// An ammunition box under the receiver with a short run of belted rounds climbing the left side.
fn belt_box(g: &mut M, z: [f32; 2], bottom: f32, col: C) {
    g.bx([-0.048, 0.048], [bottom, 0.030], z, col);
    g.bx([-0.050, 0.050], [0.022, 0.030], [z[0] - 0.002, z[1] + 0.002], [col[0] * 0.6, col[1] * 0.6, col[2] * 0.6]);
    g.bx([-0.012, 0.012], [bottom - 0.004, bottom], [z[0] + 0.01, z[1] - 0.01], STEEL);
    let zc = (z[0] + z[1]) * 0.5;
    for i in 0..6 {
        let y = 0.034 + i as f32 * 0.0105;
        let x = -0.030 - 0.012 * (1. - i as f32 / 5.);
        g.cx(y, zc, [x - 0.012, -0.0225], 0.0052, BRASS);
        g.bx([-0.0300, -0.0225], [y - 0.0015, y + 0.0015], [zc - 0.0062, zc + 0.0062], STEEL_L);
        g.cx(y, zc, [x - 0.022, x - 0.012], 0.0040, [0.45, 0.30, 0.15]);
    }
}

fn para() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let blk = [0.15, 0.155, 0.16];
    // receiver with the raised feed cover
    b.bx([-0.0225, 0.0225], [0.030, 0.095], [-0.26, 0.11], blk);
    b.bx([-0.0215, 0.0215], [0.095, 0.112], [-0.22, 0.02], STEEL);
    b.bx([-0.0090, 0.0090], [0.112, 0.126], [0.0, 0.022], STEEL);
    b.bx([0.0226, 0.0232], [0.060, 0.090], [-0.06, 0.02], POLY);
    // handguard, gas tube, barrel, flash hider
    b.bx([-0.0230, 0.0230], [0.030, 0.078], [-0.40, -0.26], POLY);
    b.cz(0., 0.095, [-0.50, -0.26], 0.0078, STEEL);
    b.cz(0., 0.072, [-0.615, -0.26], 0.0098, STEEL);
    b.bx([-0.0115, 0.0115], [0.060, 0.104], [-0.505, -0.470], GUNMETAL);
    b.cz(0., 0.072, [-0.665, -0.610], 0.0118, STEEL);
    b.cz(0., 0.072, [-0.6655, -0.6645], 0.0058, [0., 0., 0.]);
    b.bx([-0.0015, 0.0015], [0.104, 0.130], [-0.492, -0.488], STEEL);
    b.sym(|b, k| b.bx([k * 0.0058, k * 0.0086], [0.104, 0.130], [-0.498, -0.482], GUNMETAL));
    // carry handle
    b.sym(|b, k| {
        b.bx([k * 0.003, k * 0.010], [0.078, 0.150], [-0.44, -0.425], STEEL);
        b.bx([k * 0.003, k * 0.010], [0.078, 0.150], [-0.335, -0.320], STEEL);
    });
    b.bx([-0.0105, 0.0105], [0.145, 0.158], [-0.44, -0.32], STEEL);
    // folded bipod
    b.sym(|b, k| b.rod(vec3(k * 0.022, 0.050, -0.50), vec3(k * 0.022, 0.046, -0.64), 0.0045, 0.0045, STEEL_L, 0.));
    // grip, guard, hollow stock
    b.tb([0., -0.030, 0.012], [0.0165, 0.062, 0.0215], -15., POLY);
    trigger_guard(&mut b, 0.030, 0.030, [-0.075, 0.004], 0.0105, POLY);
    b.bx([-0.0155, 0.0155], [0.050, 0.092], [0.11, 0.39], POLY);
    b.rod(vec3(0., 0.030, 0.11), vec3(0., -0.072, 0.385), 0.0095, 0.0095, POLY, 0.);
    b.bx([-0.0195, 0.0195], [-0.085, 0.092], [0.383, 0.398], [0.06, 0.06, 0.065]);
    // ammunition box and belt
    belt_box(&mut g, [-0.20, -0.07], -0.085, [0.20, 0.26, 0.17]);
    // charging handle
    s.bx([0.0225, 0.046], [0.070, 0.080], [-0.142, -0.122], STEEL_L);
    s.bx([0.046, 0.056], [0.062, 0.088], [-0.147, -0.117], STEEL_L);
    fin(
        b,
        anch(Some([0., 0.030, -0.33]), [0., 0.150, 0.10], [0., 0.072, -0.6655], [0.0233, 0.075, -0.05]),
        Some((g, [0., 0.030, -0.135])),
        Some((s, [0., 0., 0.06])),
    )
}

fn pk() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let blk = [0.13, 0.135, 0.14];
    // receiver with its hump-backed cover
    b.bx([-0.0225, 0.0225], [0.020, 0.095], [-0.25, 0.14], blk);
    b.bx([-0.0215, 0.0215], [0.095, 0.118], [-0.18, 0.05], STEEL);
    b.bx([-0.0080, 0.0080], [0.118, 0.131], [0.022, 0.042], STEEL);
    b.bx([0.0226, 0.0232], [0.050, 0.085], [-0.08, 0.04], POLY);
    // fluted heavy barrel, gas tube below, flash hider
    b.cz(0., 0.072, [-0.74, -0.25], 0.0115, STEEL);
    for i in 0..5 {
        b.cz(0., 0.072, [-0.40 - i as f32 * 0.06, -0.41 - i as f32 * 0.06], 0.0138, GUNMETAL);
    }
    b.cz(0., 0.045, [-0.52, -0.25], 0.0105, STEEL_L);
    b.bx([-0.0125, 0.0125], [0.036, 0.098], [-0.555, -0.505], GUNMETAL);
    b.cz(0., 0.072, [-0.775, -0.735], 0.0145, STEEL);
    b.cz(0., 0.072, [-0.7755, -0.7745], 0.0060, [0., 0., 0.]);
    b.bx([-0.0015, 0.0015], [0.083, 0.112], [-0.728, -0.722], STEEL);
    b.sym(|b, k| b.bx([k * 0.0058, k * 0.0086], [0.083, 0.112], [-0.732, -0.718], GUNMETAL));
    // carry handle
    b.sym(|b, k| {
        b.bx([k * 0.003, k * 0.010], [0.084, 0.150], [-0.63, -0.615], STEEL);
        b.bx([k * 0.003, k * 0.010], [0.084, 0.150], [-0.56, -0.545], STEEL);
    });
    b.bx([-0.0105, 0.0105], [0.145, 0.158], [-0.63, -0.545], STEEL);
    // folded bipod
    b.sym(|b, k| b.rod(vec3(k * 0.024, 0.040, -0.52), vec3(k * 0.024, 0.036, -0.70), 0.0048, 0.0048, STEEL_L, 0.));
    // pistol grip, guard and the wooden skeleton stock
    b.tb([0., -0.030, 0.012], [0.0165, 0.062, 0.0215], -17., WOOD_D);
    trigger_guard(&mut b, 0.020, 0.040, [-0.075, 0.004], 0.0100, STEEL);
    b.prism([-0.0160, 0.0160], &[(0.095, 0.14), (0.085, 0.40), (0.040, 0.40), (0.040, 0.14)], WOOD);
    b.bx([-0.0090, 0.0090], [-0.090, -0.064], [0.0, 0.40], WOOD_D);
    b.bx([-0.0170, 0.0170], [-0.092, 0.085], [0.385, 0.402], WOOD);
    b.bx([-0.0175, 0.0175], [-0.095, 0.060], [0.402, 0.410], POLY);
    belt_box(&mut g, [-0.17, -0.06], -0.075, [0.17, 0.20, 0.15]);
    s.bx([0.0225, 0.050], [0.064, 0.074], [-0.060, -0.040], STEEL_L);
    s.bx([0.050, 0.060], [0.056, 0.082], [-0.065, -0.035], STEEL_L);
    fin(
        b,
        anch(Some([0., 0.030, -0.34]), [0., 0.150, 0.10], [0., 0.072, -0.7755], [0.0233, 0.070, 0.0]),
        Some((g, [0., 0.030, -0.115])),
        Some((s, [0., 0., 0.06])),
    )
}

fn rpg() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let tube = [0.24, 0.27, 0.19];
    let y = 0.085;
    // launch tube, muzzle collar and rear venturi
    b.cz(0., y, [-0.26, 0.36], 0.0205, tube);
    b.cz(0., y, [-0.268, -0.255], 0.0232, STEEL);
    b.kz(0., y, [0.36, 0.42], [0.0205, 0.0400], [0.20, 0.22, 0.17]);
    b.cz(0., y, [0.414, 0.422], 0.0410, STEEL);
    b.kz(0., y, [0.355, 0.375], [0.0232, 0.0232], STEEL);
    // wooden heat shield, trigger group, grip
    b.cz(0., y, [-0.22, -0.06], 0.0225, WOOD);
    b.cz(0., y, [-0.222, -0.218], 0.0232, WOOD_D);
    b.cz(0., y, [-0.064, -0.060], 0.0232, WOOD_D);
    b.bx([-0.0140, 0.0140], [0.028, 0.0655], [-0.05, 0.035], STEEL);
    trigger_guard(&mut b, 0.030, 0.030, [-0.065, 0.004], 0.0095, STEEL);
    b.tb([0., -0.030, 0.012], [0.0165, 0.062, 0.0215], -20., WOOD_D);
    // PGO-7 optical sight on the left
    b.bx([-0.030, -0.018], [0.078, 0.092], [-0.02, 0.06], STEEL);
    b.bx([-0.040, -0.030], [0.092, 0.106], [-0.02, 0.06], STEEL);
    b.cz(-0.047, 0.118, [-0.03, 0.11], 0.0145, [0.05, 0.05, 0.055]);
    b.kz(-0.047, 0.118, [-0.03, -0.065], [0.0145, 0.022], [0.05, 0.05, 0.055]);
    b.lens(-0.047, 0.118, [-0.067, -0.064], 0.0195);
    b.kz(-0.047, 0.118, [0.11, 0.135], [0.0145, 0.0200], [0.05, 0.05, 0.055]);
    b.lens(-0.047, 0.118, [0.136, 0.134], 0.0115);
    // the loaded rocket: booster, then the warhead with its nose probe
    g.cz(0., y, [-0.12, -0.34], 0.0185, [0.30, 0.31, 0.27]);
    g.kz(0., y, [-0.30, -0.34], [0.0185, 0.0425], [0.30, 0.31, 0.27]);
    g.cz(0., y, [-0.34, -0.40], 0.0425, [0.27, 0.32, 0.19]);
    g.cz(0., y, [-0.356, -0.364], 0.0432, BRASS);
    g.kz(0., y, [-0.40, -0.47], [0.0425, 0.0115], [0.27, 0.32, 0.19]);
    g.cz(0., y, [-0.47, -0.515], 0.0095, [0.16, 0.17, 0.18]);
    g.kz(0., y, [-0.515, -0.53], [0.0095, 0.002], STEEL_L);
    fin(
        b,
        anch(Some([0., 0.062, -0.14]), [-0.047, 0.118, 0.17], [0., y, -0.53], [0., y, 0.422]),
        Some((g, [0., y, -0.26])),
        None,
    )
}

fn thumper() -> WeaponModel {
    let mut b = M::new();
    // thick break-action barrel with its sights
    b.cz(0., 0.060, [-0.43, -0.03], 0.0235, STEEL);
    b.cz(0., 0.060, [-0.435, -0.415], 0.0262, GUNMETAL);
    b.cz(0., 0.060, [-0.4355, -0.4345], 0.0190, [0., 0., 0.]);
    b.bx([-0.0015, 0.0015], [0.083, 0.106], [-0.408, -0.402], STEEL);
    b.bx([-0.0095, 0.0095], [0.080, 0.090], [-0.095, -0.055], GUNMETAL);
    b.sym(|b, k| b.bx([k * 0.0030, k * 0.0055], [0.090, 0.150], [-0.085, -0.078], STEEL));
    b.bx([-0.0055, 0.0055], [0.130, 0.136], [-0.085, -0.078], STEEL);
    // receiver, hammer, forearm
    b.bx([-0.0225, 0.0225], [0.030, 0.078], [-0.05, 0.04], GUNMETAL);
    b.tb([0., 0.084, 0.042], [0.004, 0.012, 0.005], 35., STEEL_L);
    b.bx([-0.0230, 0.0230], [0.020, 0.048], [-0.26, -0.05], WOOD);
    b.bx([-0.0100, 0.0100], [0.012, 0.022], [-0.26, -0.20], STEEL);
    trigger_guard(&mut b, 0.030, 0.030, [-0.075, -0.004], 0.0095, STEEL);
    // walnut stock
    b.prism(
        [-0.0190, 0.0190],
        &[(0.078, 0.0), (0.066, 0.26), (0.050, 0.30), (-0.055, 0.30), (-0.052, 0.26), (-0.03, -0.01)],
        WOOD,
    );
    b.bx([-0.0195, 0.0195], [-0.058, 0.052], [0.298, 0.310], STEEL);
    fin(b, anch(Some([0., 0.020, -0.15]), [0., 0.150, 0.05], [0., 0.060, -0.4355], [0.0226, 0.060, 0.0]), None, None)
}
// ---------------------------------------------------------------------------------------------------------
// Grenades: upright, origin at the middle of the body, fuze up, the spoon on the right
// ---------------------------------------------------------------------------------------------------------
/// The pull pin and its ring (the ring lies in the YZ plane so it reads from the sides), at height `y`.
fn pin_ring(m: &mut M, y: f32, z: f32) {
    m.cz(0., y, [z - 0.006, z + 0.014], 0.0016, STEEL_L);
    for dx in [-0.0008, 0.0008] {
        let mut t = Template::new();
        t.ring(Vec3::ZERO, 0.0098, 0.0135, CHROME, 0., 14);
        let flip = if dx > 0. { Mat4::IDENTITY } else { Mat4::from_rotation_x(std::f32::consts::PI) };
        m.t.append(&t.transformed(
            Mat4::from_translation(vec3(dx, y, z + 0.024)) * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2) * flip,
        ));
    }
}

fn grenade(body: M, height: f32) -> WeaponModel {
    // The hand holds the body; a thrown weapon leaves the hand forwards and up. Its "length" is its height.
    let mut m = fin(body, anch(None, [0., 0.10, 0.06], [0., 0.03, -0.06], [0., 0., 0.]), None, None);
    m.length = height;
    m
}

fn frag() -> WeaponModel {
    let mut b = M::new();
    let ol = [0.28, 0.33, 0.18];
    b.ball([0., 0., 0.], [0.0315, 0.0330, 0.0315], ol);
    b.cy(0., 0., [-0.005, 0.005], 0.0322, [0.80, 0.72, 0.14]);
    b.cy(0., 0., [0.029, 0.058], 0.0115, STEEL_L);
    b.cy(0., 0., [0.050, 0.056], 0.0135, GUNMETAL);
    // spoon: lever hugging the body and over the fuze
    b.bx([0.0275, 0.0320], [-0.022, 0.014], [-0.0055, 0.0055], STEEL_L);
    b.bx([0.0275, 0.0320], [0.014, 0.060], [-0.0055, 0.0055], STEEL_L);
    b.bx([0.0, 0.0320], [0.058, 0.0635], [-0.0055, 0.0055], STEEL_L);
    pin_ring(&mut b, 0.040, 0.0);
    grenade(b, 0.096)
}

fn flash() -> WeaponModel {
    let mut b = M::new();
    let al = [0.52, 0.54, 0.57];
    b.cy(0., 0., [-0.048, 0.030], 0.0265, al);
    b.cy(0., 0., [-0.052, -0.048], 0.0272, [0.08, 0.08, 0.09]);
    b.cy(0., 0., [0.030, 0.036], 0.0272, [0.08, 0.08, 0.09]);
    b.cy(0., 0., [-0.040, -0.034], 0.0268, [0.25, 0.26, 0.28]);
    b.cy(0., 0., [0.020, 0.026], 0.0268, [0.25, 0.26, 0.28]);
    b.cy(0., 0., [0.036, 0.050], 0.0185, STEEL);
    b.cy(0., 0., [0.050, 0.055], 0.0205, GUNMETAL);
    b.bx([0.0262, 0.0305], [-0.030, 0.040], [-0.0055, 0.0055], STEEL_L);
    b.bx([0.0, 0.0305], [0.050, 0.0575], [-0.0055, 0.0055], STEEL_L);
    // stencilled warning stripe
    b.cy(0., 0., [-0.012, -0.004], 0.0268, [0.85, 0.78, 0.15]);
    pin_ring(&mut b, 0.043, 0.0);
    grenade(b, 0.107)
}

fn smoke() -> WeaponModel {
    let mut b = M::new();
    let body = [0.42, 0.44, 0.40];
    b.cy(0., 0., [-0.050, 0.040], 0.0325, body);
    b.cy(0., 0., [-0.054, -0.050], 0.0335, [0.10, 0.10, 0.11]);
    b.cy(0., 0., [0.040, 0.047], 0.0335, [0.10, 0.10, 0.11]);
    b.cy(0., 0., [0.002, 0.018], 0.0330, [0.88, 0.76, 0.10]);
    for i in 0..3 {
        let a = i as f32 * std::f32::consts::TAU / 3.;
        b.cy(a.cos() * 0.021, a.sin() * 0.021, [0.0465, 0.0485], 0.0042, [0.02, 0.02, 0.02]);
    }
    b.cy(0., 0., [0.047, 0.062], 0.0125, STEEL);
    b.cy(0., 0., [0.062, 0.067], 0.0148, GUNMETAL);
    b.bx([0.0322, 0.0366], [-0.040, 0.050], [-0.0055, 0.0055], STEEL_L);
    b.bx([0.0, 0.0366], [0.062, 0.0695], [-0.0055, 0.0055], STEEL_L);
    pin_ring(&mut b, 0.054, 0.0);
    grenade(b, 0.121)
}

fn incen() -> WeaponModel {
    let mut b = M::new();
    let red = [0.66, 0.12, 0.08];
    b.cy(0., 0., [-0.048, 0.034], 0.0305, red);
    b.cy(0., 0., [-0.052, -0.048], 0.0315, [0.10, 0.10, 0.11]);
    b.cy(0., 0., [0.034, 0.044], 0.0315, [0.12, 0.12, 0.13]);
    b.cy(0., 0., [0.006, 0.016], 0.0310, [0.12, 0.12, 0.13]);
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.;
        b.bx(
            [a.cos() * 0.0245 - 0.003, a.cos() * 0.0245 + 0.003],
            [0.0435, 0.0445],
            [a.sin() * 0.0245 - 0.003, a.sin() * 0.0245 + 0.003],
            [0.9, 0.55, 0.1],
        );
    }
    b.cy(0., 0., [0.044, 0.058], 0.0120, STEEL);
    b.cy(0., 0., [0.058, 0.063], 0.0142, GUNMETAL);
    b.bx([0.0308, 0.0350], [-0.038, 0.046], [-0.0055, 0.0055], STEEL_L);
    b.bx([0.0, 0.0350], [0.058, 0.0655], [-0.0055, 0.0055], STEEL_L);
    pin_ring(&mut b, 0.050, 0.0);
    grenade(b, 0.115)
}

// ---------------------------------------------------------------------------------------------------------
// Melee: the blade points along -Z, the origin is the middle of the handle (the axe and crowbar: the hand)
// ---------------------------------------------------------------------------------------------------------
fn melee(body: M, muzzle: Off) -> WeaponModel {
    fin(body, anch(None, [0., 0.05, -0.1], muzzle, [0., 0., 0.]), None, None)
}

fn knife() -> WeaponModel {
    let mut b = M::new();
    // stacked leather washers, steel pommel and guard
    for i in 0..8 {
        let z0 = -0.060 + i as f32 * 0.0150;
        b.cz(
            0.,
            0.,
            [z0, z0 + 0.0150],
            if i % 2 == 0 { 0.0128 } else { 0.0120 },
            if i % 2 == 0 { WOOD } else { WOOD_D },
        );
    }
    b.cz(0., 0., [0.060, 0.074], 0.0140, STEEL_L);
    b.cz(0., 0., [0.074, 0.078], 0.0100, GUNMETAL);
    b.bx([-0.0070, 0.0070], [-0.0200, 0.0300], [-0.072, -0.060], GUNMETAL);
    // clip-point blade with a polished edge and a fuller
    b.prism(
        [-0.0018, 0.0018],
        &[(0.016, -0.072), (0.016, -0.165), (0.004, -0.232), (-0.010, -0.212), (-0.0165, -0.170), (-0.0165, -0.072)],
        [0.16, 0.165, 0.18],
    );
    b.prism(
        [-0.0020, 0.0020],
        &[(-0.0165, -0.072), (-0.0165, -0.170), (-0.010, -0.212), (-0.0035, -0.200), (-0.0035, -0.072)],
        CHROME,
    );
    b.bx([-0.0021, 0.0021], [0.004, 0.0075], [-0.150, -0.080], [0.07, 0.07, 0.08]);
    melee(b, [0., 0.004, -0.232])
}

fn machete() -> WeaponModel {
    let mut b = M::new();
    b.bx([-0.0130, 0.0130], [-0.024, 0.020], [-0.050, 0.085], POLY);
    b.bx([-0.0135, 0.0135], [-0.026, 0.022], [0.080, 0.090], POLY_G);
    for z in [-0.025, 0.020, 0.062] {
        b.cx(-0.002, z, [-0.0138, 0.0138], 0.0035, BRASS);
    }
    b.bx([-0.0070, 0.0070], [-0.028, 0.030], [-0.058, -0.050], GUNMETAL);
    b.prism(
        [-0.0016, 0.0016],
        &[
            (0.022, -0.058),
            (0.030, -0.28),
            (0.030, -0.43),
            (0.0, -0.50),
            (-0.022, -0.46),
            (-0.020, -0.28),
            (-0.014, -0.058),
        ],
        [0.22, 0.23, 0.25],
    );
    b.prism(
        [-0.0018, 0.0018],
        &[(-0.014, -0.058), (-0.020, -0.28), (-0.022, -0.46), (-0.002, -0.49), (-0.005, -0.40), (-0.004, -0.058)],
        CHROME,
    );
    melee(b, [0., 0.0, -0.50])
}

fn axe() -> WeaponModel {
    let mut b = M::new();
    let haft = WOOD_L;
    b.cz(0., 0., [-0.72, 0.090], 0.0160, haft);
    b.cz(0., 0., [0.050, 0.112], 0.0200, WOOD);
    b.cz(0., 0., [-0.12, 0.04], 0.0185, POLY);
    // head: eye block, flared blade with a bevel, pick
    let red = RED;
    b.bx([-0.0145, 0.0145], [-0.030, 0.038], [-0.748, -0.680], red);
    b.prism([-0.0110, 0.0110], &[(0.030, -0.690), (0.030, -0.745), (-0.080, -0.785), (-0.080, -0.655)], red);
    b.prism([-0.0050, 0.0050], &[(-0.080, -0.655), (-0.080, -0.785), (-0.140, -0.815), (-0.140, -0.645)], CHROME);
    b.prism([-0.0085, 0.0085], &[(0.036, -0.745), (0.036, -0.685), (0.150, -0.672)], red);
    melee(b, [0., -0.11, -0.73])
}

fn crowbar() -> WeaponModel {
    let mut b = M::new();
    let st = [0.30, 0.32, 0.37];
    b.cz(0., 0., [-0.32, 0.27], 0.0112, st);
    b.cz(0., 0., [-0.12, 0.10], 0.0122, RED);
    // curved claw end
    b.rod(vec3(0., 0., -0.32), vec3(0., -0.018, -0.372), 0.0112, 0.0112, st, 0.);
    b.rod(vec3(0., -0.018, -0.372), vec3(0., -0.050, -0.405), 0.0112, 0.0100, st, 0.);
    b.rod(vec3(0., -0.050, -0.405), vec3(0., -0.090, -0.405), 0.0100, 0.0085, st, 0.);
    b.sym(|b, k| {
        b.prism(
            [k * 0.0020, k * 0.0100],
            &[(-0.090, -0.396), (-0.090, -0.414), (-0.128, -0.392), (-0.128, -0.376)],
            CHROME,
        );
    });
    // flat chisel end
    b.bx([-0.0150, 0.0150], [-0.0045, 0.0045], [0.270, 0.335], CHROME);
    b.bx([-0.0120, 0.0120], [-0.0030, 0.0030], [0.335, 0.350], CHROME);
    melee(b, [0., -0.09, -0.41])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROSTER: [&str; 33] = [
        "k9", "m45", "hc50", "rv357", "mp9", "ump", "pdw", "vkr", "k47", "m4c", "fm2", "bpa", "gl4", "dmr20", "svd",
        "scout", "awm", "m82", "pump12", "auto12", "sawn", "para", "pk", "rpg", "thumper", "frag", "flash", "smoke",
        "incen", "knife", "machete", "axe", "crowbar",
    ];

    /// (key, min length, max length) in metres: the real-world size class of each weapon.
    const SIZES: [(&str, f32, f32); 33] = [
        ("k9", 0.17, 0.24),
        ("m45", 0.18, 0.26),
        ("hc50", 0.22, 0.32),
        ("rv357", 0.22, 0.32),
        ("mp9", 0.55, 0.75),
        ("ump", 0.55, 0.75),
        ("pdw", 0.45, 0.60),
        ("vkr", 0.45, 0.68),
        ("k47", 0.80, 0.95),
        ("m4c", 0.75, 0.90),
        ("fm2", 0.68, 0.82),
        ("bpa", 0.70, 0.85),
        ("gl4", 0.95, 1.10),
        ("dmr20", 0.90, 1.10),
        ("svd", 1.15, 1.30),
        ("scout", 0.90, 1.10),
        ("awm", 1.10, 1.30),
        ("m82", 1.35, 1.55),
        ("pump12", 0.95, 1.15),
        ("auto12", 0.90, 1.10),
        ("sawn", 0.45, 0.65),
        ("para", 0.95, 1.12),
        ("pk", 1.05, 1.25),
        ("rpg", 0.88, 1.05),
        ("thumper", 0.65, 0.80),
        ("frag", 0.07, 0.13),
        ("flash", 0.07, 0.13),
        ("smoke", 0.07, 0.13),
        ("incen", 0.07, 0.13),
        ("knife", 0.27, 0.33),
        ("machete", 0.50, 0.65),
        ("axe", 0.80, 1.00),
        ("crowbar", 0.60, 0.85),
    ];

    fn finite(v: Vec3) -> bool {
        v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
    }

    /// `cargo test --lib weapon_models -- --ignored --nocapture` lists vertex counts.
    #[test]
    #[ignore]
    fn print_vertex_counts() {
        for k in keys() {
            let m = build(k).unwrap();
            eprintln!("{k:8} {:5} verts  {:.2} m", m.assembled().verts.len(), m.length);
        }
    }

    #[test]
    fn keys_match_the_design_roster() {
        assert_eq!(keys(), &ROSTER[..]);
    }

    #[test]
    fn every_key_builds_and_unknown_does_not() {
        for k in keys() {
            assert!(build(k).is_some(), "{k} does not build");
        }
        assert!(build("nope").is_none());
        assert!(build("").is_none());
    }

    #[test]
    fn geometry_is_sound_and_within_budget() {
        for k in keys() {
            let m = build(k).unwrap();
            let all = m.assembled();
            assert!(!m.body.is_empty(), "{k}: empty body");
            assert!(all.verts.len() < 3500, "{k}: {} vertices", all.verts.len());
            assert_eq!(all.idx.len() % 3, 0, "{k}");
            assert!(all.idx.iter().all(|&i| (i as usize) < all.verts.len()), "{k}: index out of range");
            for v in &all.verts {
                assert!(finite(v.p) && finite(v.n), "{k}: non-finite vertex");
                assert!((v.n.length() - 1.).abs() < 1e-3, "{k}: normal not unit");
                assert!(v.p.abs().max_element() < 2., "{k}: vertex far from the grip");
            }
        }
    }

    #[test]
    fn length_matches_the_real_size_class() {
        for (k, lo, hi) in SIZES {
            let m = build(k).unwrap();
            assert!(m.length >= lo && m.length <= hi, "{k}: length {} not in {lo}..{hi}", m.length);
        }
    }

    #[test]
    fn anchors_are_finite_and_sensible() {
        for k in keys() {
            let m = build(k).unwrap();
            let a = m.anchors;
            assert!(finite(a.grip) && finite(a.sight) && finite(a.muzzle) && finite(a.eject), "{k}");
            if let Some(s) = a.support {
                assert!(finite(s), "{k}");
            }
            assert_eq!(a.grip, Vec3::ZERO, "{k}: the grip is the origin");
            assert!(a.muzzle.z < a.grip.z, "{k}: muzzle must be in front of the grip");
            let all = m.assembled();
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for v in &all.verts {
                lo = lo.min(v.p);
                hi = hi.max(v.p);
            }
            let centre = (lo + hi) * 0.5;
            assert!(a.sight.y > centre.y, "{k}: sight {} not above the body centre {}", a.sight.y, centre.y);
            // The muzzle lies on the weapon: inside its bounding box (a grenade's release point is just ahead of it).
            let thrown = ["frag", "flash", "smoke", "incen"].contains(k);
            if !thrown {
                assert!(a.muzzle.z >= lo.z - 0.02 && a.muzzle.z <= hi.z, "{k}: muzzle off the model");
                assert!(a.muzzle.y >= lo.y - 0.02 && a.muzzle.y <= hi.y + 0.02, "{k}: muzzle off the model");
            }
        }
    }

    #[test]
    fn support_hand_only_on_two_handed_weapons() {
        for k in keys() {
            let m = build(k).unwrap();
            let one_handed =
                ["k9", "m45", "hc50", "rv357", "frag", "flash", "smoke", "incen", "knife", "machete", "axe", "crowbar"];
            assert_eq!(m.anchors.support.is_none(), one_handed.contains(k), "{k}");
        }
    }

    #[test]
    fn moving_parts_are_consistent() {
        for k in keys() {
            let m = build(k).unwrap();
            if let Some((t, off)) = &m.mag {
                assert!(!t.is_empty() && finite(*off), "{k}: mag");
            }
            if let Some((t, travel)) = &m.slide {
                assert!(!t.is_empty() && finite(*travel), "{k}: slide");
                assert!(travel.z > 0. && travel.z < 0.15, "{k}: slide travels backwards a few centimetres");
            }
        }
        // Pistols have a slide and a magazine (except the revolver); the rocket launcher reloads its rocket.
        for k in ["k9", "m45", "hc50"] {
            let m = build(k).unwrap();
            assert!(m.slide.is_some() && m.mag.is_some(), "{k}");
        }
        assert!(build("rv357").unwrap().mag.is_none());
        assert!(build("rpg").unwrap().mag.is_some());
        assert!(build("pump12").unwrap().slide.is_some());
        assert!(build("para").unwrap().mag.is_some() && build("pk").unwrap().mag.is_some());
    }

    #[test]
    fn firearms_use_several_colours_and_optics_have_glass() {
        let distinct = |t: &Template| {
            let mut c: Vec<[u8; 3]> =
                t.verts.iter().map(|v| [(v.c[0] * 255.) as u8, (v.c[1] * 255.) as u8, (v.c[2] * 255.) as u8]).collect();
            c.sort_unstable();
            c.dedup();
            c.len()
        };
        for k in keys() {
            let m = build(k).unwrap();
            assert!(distinct(&m.assembled()) >= 3, "{k}: too flat a palette");
        }
        for k in ["bpa", "dmr20", "svd", "scout", "awm", "m82", "rpg", "pdw"] {
            let glows = build(k).unwrap().assembled().verts.iter().any(|v| v.e > 0.1);
            assert!(glows, "{k}: the optic needs a glowing lens");
        }
    }

    #[test]
    fn rifle_sight_lines_sit_above_the_barrel_and_behind_the_front() {
        for k in ["k47", "m4c", "gl4", "svd", "awm", "m82", "pump12", "mp9"] {
            let m = build(k).unwrap();
            assert!(m.anchors.sight.z > m.anchors.muzzle.z + 0.3, "{k}: eye well behind the muzzle");
            assert!(m.anchors.sight.y > m.anchors.muzzle.y, "{k}: eye above the bore");
        }
    }
}
