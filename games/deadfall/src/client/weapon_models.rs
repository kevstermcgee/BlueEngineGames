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
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use vesper3d::viewer::kit::{Template, Vert};

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
const STEEL: C = [0.25, 0.26, 0.28];
const GUNMETAL: C = [0.37, 0.38, 0.41];
const STEEL_L: C = [0.55, 0.56, 0.59];
const CHROME: C = [0.76, 0.77, 0.80];
const POLY: C = [0.19, 0.19, 0.205];
const POLY_G: C = [0.31, 0.315, 0.33];
const WOOD: C = [0.35, 0.22, 0.12];
const WOOD_D: C = [0.25, 0.15, 0.08];
const WOOD_L: C = [0.46, 0.30, 0.16];
const TAN: C = [0.55, 0.46, 0.32];
const OLIVE: C = [0.25, 0.30, 0.17];
const OLIVE_D: C = [0.17, 0.21, 0.12];
const BRASS: C = [0.76, 0.58, 0.20];
const GLASS: C = [0.07, 0.22, 0.32];
const RED: C = [0.65, 0.10, 0.08];
const GROOVE: C = [0.06, 0.06, 0.07];

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

// ---------------------------------------------------------------------------------------------------------
// Shape toolkit: lofts (smooth rounded sections along an axis or a path), rounded boxes, sweeps, curves
// ---------------------------------------------------------------------------------------------------------
/// One cross-section of a loft: a rounded rectangle (or ellipse) of half sizes `hw` along `ex` and `hh` along
/// `ey`, centred on `c`. `rt`/`rb` are the corner radii of the upper and lower pair of corners.
#[derive(Clone, Copy)]
struct Sec {
    c: Vec3,
    ex: Vec3,
    ey: Vec3,
    hw: f32,
    hh: f32,
    rt: f32,
    rb: f32,
}

/// Section standing across the Z axis at depth `z`, centred on (`x`, `y`).
fn sz(z: f32, x: f32, y: f32, hw: f32, hh: f32, r: f32) -> Sec {
    Sec { c: vec3(x, y, z), ex: Vec3::X, ey: Vec3::Y, hw, hh, rt: r, rb: r }
}

/// Like [`sz`] with different top and bottom corner radii.
#[allow(clippy::too_many_arguments)]
fn szb(z: f32, x: f32, y: f32, hw: f32, hh: f32, rt: f32, rb: f32) -> Sec {
    Sec { c: vec3(x, y, z), ex: Vec3::X, ey: Vec3::Y, hw, hh, rt, rb }
}

/// Section across a raked axis running from `a` to `b` (a grip, a magazine well), at fraction `t`: `hw` along X,
/// `hh` along the in-plane perpendicular (towards the rear when the axis leans back), `off` shifts the centre
/// along that perpendicular.
fn sa(a: Vec3, b: Vec3, t: f32, hw: f32, hh: f32, r: f32, off: f32) -> Sec {
    let ax = (b - a).normalize();
    let ey = ax.cross(Vec3::X).normalize();
    Sec { c: a + (b - a) * t + ey * off, ex: Vec3::X, ey, hw, hh, rt: r, rb: r }
}

#[derive(Clone, Copy)]
enum Ring {
    /// Rounded rectangle with this many segments per corner (0 = sharp corners).
    Rr(usize),
    /// Ellipse with this many sides.
    Ell(usize),
}

fn ring_pts(s: &Sec, ring: Ring, soft_t: bool, soft_b: bool) -> Vec<(Vec3, Vec3)> {
    let at = |x: f32, y: f32, nx: f32, ny: f32| (s.c + s.ex * x + s.ey * y, (s.ex * nx + s.ey * ny).normalize_or_zero());
    let mut out = vec![];
    match ring {
        Ring::Ell(n) => {
            for i in 0..n {
                let a = TAU * i as f32 / n as f32;
                let (sn, cs) = a.sin_cos();
                out.push(at(s.hw * cs, s.hh * sn, cs / s.hw.max(1e-5), sn / s.hh.max(1e-5)));
            }
        }
        Ring::Rr(segs) => {
            let lim = s.hw.min(s.hh);
            let (rt, rb) = (s.rt.clamp(0., lim), s.rb.clamp(0., lim));
            // corner centre, radius, start angle, soft?
            let corners = [
                (s.hw - rt, s.hh - rt, rt, 0., soft_t),
                (-(s.hw - rt), s.hh - rt, rt, FRAC_PI_2, soft_t),
                (-(s.hw - rb), -(s.hh - rb), rb, PI, soft_b),
                (s.hw - rb, -(s.hh - rb), rb, PI + FRAC_PI_2, soft_b),
            ];
            for (cx, cy, r, a0, soft) in corners {
                let k = if soft { segs.max(1) + 1 } else { 2 };
                for i in 0..k {
                    let a = a0 + FRAC_PI_2 * i as f32 / (k - 1) as f32;
                    let (sn, cs) = a.sin_cos();
                    out.push(at(cx + r * cs, cy + r * sn, cs, sn));
                }
            }
        }
    }
    out
}

/// Skin a list of sections into a smooth-shaded closed solid.
fn loft_mesh(secs: &[Sec], ring: Ring, caps: bool, col: C, glow: f32) -> Template {
    let mut t = Template::new();
    let k = secs.len();
    if k < 2 {
        return t;
    }
    let soft_t = secs.iter().any(|s| s.rt > 1e-5);
    let soft_b = secs.iter().any(|s| s.rb > 1e-5);
    let rings: Vec<Vec<(Vec3, Vec3)>> = secs.iter().map(|s| ring_pts(s, ring, soft_t, soft_b)).collect();
    let m = rings[0].len();
    for i in 0..k {
        for j in 0..m {
            let (p, np) = rings[i][j];
            let d = (rings[(i + 1).min(k - 1)][j].0 - rings[i.saturating_sub(1)][j].0).normalize_or_zero();
            let n = (np - d * np.dot(d)).normalize_or_zero();
            let n = if n == Vec3::ZERO { np } else { n };
            t.verts.push(Vert { p, n, c: col, e: glow, a: 1. });
        }
    }
    let mut tri = |t: &mut Template, a: usize, b: usize, c: usize| {
        let (pa, pb, pc) = (t.verts[a].p, t.verts[b].p, t.verts[c].p);
        let g = (pb - pa).cross(pc - pa);
        if g.length_squared() < 1e-14 {
            return;
        }
        let nn = t.verts[a].n + t.verts[b].n + t.verts[c].n;
        if g.dot(nn) >= 0. {
            t.idx.extend_from_slice(&[a as u16, b as u16, c as u16]);
        } else {
            t.idx.extend_from_slice(&[a as u16, c as u16, b as u16]);
        }
    };
    for i in 0..k - 1 {
        for j in 0..m {
            let j2 = (j + 1) % m;
            let (a, b, c, d) = (i * m + j, i * m + j2, (i + 1) * m + j2, (i + 1) * m + j);
            tri(&mut t, a, b, c);
            tri(&mut t, a, c, d);
        }
    }
    if caps {
        for (i, nb) in [(0usize, 1usize), (k - 1, k - 2)] {
            let s = &secs[i];
            if s.hw < 1e-4 && s.hh < 1e-4 {
                continue;
            }
            let mut nrm = (s.c - secs[nb].c).normalize_or_zero();
            if nrm == Vec3::ZERO {
                nrm = s.ex.cross(s.ey).normalize_or_zero();
            }
            let hub = t.verts.len();
            t.verts.push(Vert { p: s.c, n: nrm, c: col, e: glow, a: 1. });
            for j in 0..m {
                t.verts.push(Vert { p: rings[i][j].0, n: nrm, c: col, e: glow, a: 1. });
            }
            for j in 0..m {
                tri(&mut t, hub, hub + 1 + j, hub + 1 + (j + 1) % m);
            }
        }
    }
    t
}

/// Quadratic Bezier through (y, z) points.
fn bez(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let (a, b, c) = ((1. - t) * (1. - t), 2. * t * (1. - t), t * t);
            (a * p0.0 + b * p1.0 + c * p2.0, a * p0.1 + b * p1.1 + c * p2.1)
        })
        .collect()
}

/// (y, z) points lifted to 3D at the given x.
fn yz(x: f32, pts: &[(f32, f32)]) -> Vec<Vec3> {
    pts.iter().map(|p| vec3(x, p.0, p.1)).collect()
}

/// Points on an ellipse arc in the YZ plane: centre (cy, cz), radii (ry, rz), angles in degrees
/// (0 = +Z, 90 = +Y).
fn arc_yz(x: f32, cy: f32, cz: f32, ry: f32, rz: f32, a0: f32, a1: f32, n: usize) -> Vec<Vec3> {
    (0..=n)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
            vec3(x, cy + ry * a.sin(), cz + rz * a.cos())
        })
        .collect()
}

impl M {
    /// Smooth solid through rounded-rectangle sections (`segs` per corner, 0 = sharp).
    fn loft(&mut self, secs: &[Sec], segs: usize, c: C) {
        self.t.append(&loft_mesh(secs, Ring::Rr(segs), true, c, 0.));
    }
    /// Smooth solid through elliptical sections with `n` sides.
    fn lofte(&mut self, secs: &[Sec], n: usize, c: C) {
        self.t.append(&loft_mesh(secs, Ring::Ell(n), true, c, 0.));
    }
    /// A loft built in its own space, then moved by `m` (rotated tapers, slanted receivers).
    fn loft_m(&mut self, secs: &[Sec], segs: usize, m: Mat4, c: C) {
        self.t.append(&loft_mesh(secs, Ring::Rr(segs), true, c, 0.).transformed(m));
    }

    /// Box with all edges rounded by `r`.
    fn rbx(&mut self, x: [f32; 2], y: [f32; 2], z: [f32; 2], r: f32, c: C) {
        let (x0, x1, y0, y1, z0, z1) = (x[0].min(x[1]), x[0].max(x[1]), y[0].min(y[1]), y[0].max(y[1]), z[0].min(z[1]), z[0].max(z[1]));
        let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
        let (hw, hh) = ((x1 - x0) * 0.5, (y1 - y0) * 0.5);
        let r = r.min(hw).min(hh).min((z1 - z0) * 0.5).max(1e-4);
        let sec = |z: f32, inset: f32, rad: f32| {
            sz(z, cx, cy, (hw - inset).max(3e-4), (hh - inset).max(3e-4), rad.min(hw - inset).max(0.))
        };
        let k = 0.2929 * r;
        let secs = [
            sec(z0, r, 0.),
            sec(z0 + k, k, r - k),
            sec(z0 + r, 0., r),
            sec(z1 - r, 0., r),
            sec(z1 - k, k, r - k),
            sec(z1, r, 0.),
        ];
        self.loft(&secs, 2, c);
    }
    /// Box with its four long edges (parallel to Z) rounded by `r`, flat ends.
    fn bz(&mut self, x: [f32; 2], y: [f32; 2], z: [f32; 2], r: f32, c: C) {
        let (cx, cy) = ((x[0] + x[1]) * 0.5, (y[0] + y[1]) * 0.5);
        let (hw, hh) = ((x[1] - x[0]).abs() * 0.5, (y[1] - y[0]).abs() * 0.5);
        self.loft(&[sz(z[0].min(z[1]), cx, cy, hw, hh, r), sz(z[0].max(z[1]), cx, cy, hw, hh, r)], 2, c);
    }
    /// Box with only its top long edges rounded (receivers, slides, handguards with a flat bottom).
    fn bzt(&mut self, x: [f32; 2], y: [f32; 2], z: [f32; 2], r: f32, c: C) {
        let (cx, cy) = ((x[0] + x[1]) * 0.5, (y[0] + y[1]) * 0.5);
        let (hw, hh) = ((x[1] - x[0]).abs() * 0.5, (y[1] - y[0]).abs() * 0.5);
        self.loft(&[szb(z[0].min(z[1]), cx, cy, hw, hh, r, 0.), szb(z[0].max(z[1]), cx, cy, hw, hh, r, 0.)], 2, c);
    }
    /// Round tube along Z with `n` sides (smooth shaded), radius `r0` at `z[0]` and `r1` at `z[1]`.
    fn tz(&mut self, x: f32, y: f32, z: [f32; 2], r: [f32; 2], c: C) {
        let n = ((8. + r[0].max(r[1]) * 400.) as usize).clamp(10, 20);
        self.lofte(&[sz(z[0], x, y, r[0], r[0], 0.), sz(z[1], x, y, r[1], r[1], 0.)], n, c);
    }
    /// Tube with several (z, radius) stations: barrels with steps, flared brakes, tapered cones.
    fn tzs(&mut self, x: f32, y: f32, st: &[(f32, f32)], c: C) {
        let n = ((8. + st.iter().fold(0f32, |a, s| a.max(s.1)) * 400.) as usize).clamp(10, 20);
        let secs: Vec<Sec> = st.iter().map(|s| sz(s.0, x, y, s.1, s.1, 0.)).collect();
        self.lofte(&secs, n, c);
    }
    /// A hollow-looking muzzle: dark bore disc at `z`, facing forward.
    fn bore(&mut self, x: f32, y: f32, z: f32, r: f32) {
        self.rod(vec3(x, y, z + 0.0004), vec3(x, y, z - 0.0004), r, r, [0.01, 0.01, 0.012], 0.);
    }
    /// Round-section tube following a path; `n` is the plane the path lies in (its sideways direction); `hw`
    /// is the half size along `n`, `hh` the half size in the plane; both interpolate from first to last point.
    #[allow(clippy::too_many_arguments)]
    fn sweep(&mut self, path: &[Vec3], n: Vec3, hw: [f32; 2], hh: [f32; 2], round: bool, c: C) {
        let k = path.len();
        let ex = n.normalize();
        let secs: Vec<Sec> = (0..k)
            .map(|i| {
                let t = i as f32 / (k - 1) as f32;
                let tan = (path[(i + 1).min(k - 1)] - path[i.saturating_sub(1)]).normalize();
                let ey = tan.cross(ex).normalize();
                let l = |a: [f32; 2]| a[0] + (a[1] - a[0]) * t;
                let (w, h) = (l(hw), l(hh));
                Sec { c: path[i], ex, ey, hw: w, hh: h, rt: w.min(h) * if round { 1. } else { 0.4 }, rb: w.min(h) * if round { 1. } else { 0.4 } }
            })
            .collect();
        if round {
            self.lofte(&secs, 8, c);
        } else {
            self.loft(&secs, 1, c);
        }
    }
    /// A thin round wire through points (sling swivels, wire stocks, hooks).
    fn wire(&mut self, path: &[Vec3], r: f32, c: C) {
        for w in path.windows(2) {
            self.rod(w[0], w[1], r, r, c, 0.);
        }
        for p in &path[1..path.len() - 1] {
            self.t.ball(*p, Vec3::splat(r), c, 0., 6, 4);
        }
    }
    /// Scope bell / cone / tapered tube along Z: radius `r0` at `z0`, `r1` at `z1`.
    fn kone(&mut self, x: f32, y: f32, z: [f32; 2], r: [f32; 2], c: C) {
        self.tz(x, y, z, r, c);
    }
    /// A torus-like ring (a thin loop of wire) in the YZ plane (visible from the sides): centre (x, y, z),
    /// radius `r`, wire radius `w`.
    fn loop_yz(&mut self, x: f32, y: f32, z: f32, r: f32, w: f32, c: C) {
        let p = arc_yz(x, y, z, r, r, 0., 360., 14);
        self.sweep(&p, Vec3::X, [w, w], [w, w], true, c);
    }
    /// Rounded-rect, tapered-in-plan slab for blades and fins: polygon (y, z) outline smoothed by thickness `t`
    /// with bevelled edges: a central prism plus two thinner edge prisms.
    fn blade(&mut self, pts: &[(f32, f32)], half: f32, edge: &[(f32, f32)], c: C, ce: C) {
        self.prism([-half, half], pts, c);
        if edge.len() >= 3 {
            self.prism([-half * 0.55, half * 0.55], edge, ce);
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

/// Trigger guard: a rounded loop under the receiver (front strut, bottom run, rear curve) plus a curved trigger.
fn trigger_guard(m: &mut M, y_top: f32, depth: f32, z: [f32; 2], w: f32, c: C) {
    let y0 = y_top - depth;
    let (zf, zr) = (z[0], z[1]);
    let mut pts = bez((y_top, zf - 0.002), (y0 - 0.001, zf - 0.004), (y0, zf + depth * 0.75), 6);
    pts.extend(bez((y0, zr - depth * 0.55), (y0 - 0.0005, zr + 0.004), (y0 + depth * 0.55, zr + 0.002), 5));
    let hw = (w * 0.40).max(0.0030);
    m.sweep(&yz(0., &pts), Vec3::X, [hw, hw], [0.0022, 0.0022], false, c);
    let zt = zr - 0.021;
    let tr = bez((y_top - 0.001, zt + 0.004), (y_top - depth * 0.55, zt - 0.007), (y_top - depth * 0.78, zt - 0.003), 4);
    m.sweep(&yz(0., &tr), Vec3::X, [0.0026, 0.0026], [0.0016, 0.0016], false, STEEL);
}

/// Telescopic sight centred on (x, y) between `z_rear` (eyepiece) and `z_front` (objective), with ring mounts
/// down to `rail_y`, turrets, a flared objective bell and ocular, and glass at both ends.
#[allow(clippy::too_many_arguments)]
fn scope(m: &mut M, rail_y: f32, x: f32, y: f32, z_rear: f32, z_front: f32, r: f32, r_obj: f32) {
    let len = z_rear - z_front;
    let black = [0.06, 0.06, 0.068];
    let (za, zb) = (z_rear - len * 0.28, z_front + len * 0.28);
    for z in [za, zb] {
        m.rbx([x - 0.009, x + 0.009], [rail_y, y - r * 0.4], [z - 0.011, z + 0.011], 0.002, GUNMETAL);
        m.tz(x, y, [z - 0.008, z + 0.008], [r * 1.22, r * 1.22], GUNMETAL);
    }
    m.tz(x, y, [z_rear - 0.05, z_front + 0.07], [r, r], black);
    // objective bell: smooth flare out to the lens, with a lip
    let zf = z_front;
    m.tzs(x, y, &[(zf + 0.075, r), (zf + 0.050, r * 1.02), (zf + 0.028, (r + r_obj) * 0.5), (zf + 0.008, r_obj * 0.99), (zf, r_obj)], black);
    m.tz(x, y, [zf + 0.002, zf + 0.008], [r_obj * 1.04, r_obj * 1.04], GUNMETAL);
    // ocular bell and eyepiece ring
    let zr = z_rear;
    m.tzs(x, y, &[(zr - 0.058, r), (zr - 0.030, r * 1.10), (zr - 0.008, r * 1.36), (zr, r * 1.40)], black);
    m.tz(x, y, [zr - 0.012, zr - 0.003], [r * 1.46, r * 1.46], STEEL_L);
    let zt = (z_rear + z_front) * 0.5 - 0.01;
    m.cy(x, zt, [y + r * 0.8, y + r + 0.014], 0.0085, GUNMETAL);
    m.cy(x, zt, [y + r + 0.014, y + r + 0.0165], 0.0095, STEEL);
    m.cx(y, zt, [x + r * 0.8, x + r + 0.014], 0.0085, GUNMETAL);
    m.cx(y, zt, [x + r + 0.014, x + r + 0.0165], 0.0095, STEEL);
    m.lens(x, y, [z_front - 0.002, z_front + 0.001], r_obj * 0.88);
    m.lens(x, y, [z_rear + 0.003, z_rear - 0.001], r * 1.15);
}

/// Picatinny-style rail: a base plate with up to 16 cross ribs (spaced 1.4 cm or wider on long rails).
fn rail(m: &mut M, x: f32, y: f32, z: [f32; 2], c: C) {
    m.bx([x - 0.0105, x + 0.0105], [y - 0.004, y], z, [c[0] * 0.55, c[1] * 0.55, c[2] * 0.55]);
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

/// Part of a Bezier curve between parameters `t0` and `t1`.
fn bezr(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), t0: f32, t1: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| {
            let t = t0 + (t1 - t0) * i as f32 / n as f32;
            let (a, b, c) = ((1. - t) * (1. - t), 2. * t * (1. - t), t * t);
            (a * p0.0 + b * p1.0 + c * p2.0, a * p0.1 + b * p1.1 + c * p2.1)
        })
        .collect()
}

/// A curved (banana) magazine following a Bezier arc in (y, z), `hw` half width, thickness `hh` at the top and the
/// bottom, ribs at the given curve parameters, and a base plate.
#[allow(clippy::too_many_arguments)]
fn curved_mag(g: &mut M, p: [(f32, f32); 3], hw: f32, hh: [f32; 2], c: C, plate: C, ribs: &[(f32, f32)], rib_c: C) {
    let path = yz(0., &bez(p[0], p[1], p[2], 14));
    g.sweep(&path, Vec3::X, [hw, hw * 0.96], hh, false, c);
    for (t0, t1) in ribs {
        let pr = yz(0., &bezr(p[0], p[1], p[2], *t0, *t1, 2));
        let h = hh[0] + (hh[1] - hh[0]) * (t0 + t1) * 0.5;
        g.sweep(&pr, Vec3::X, [hw + 0.0004, hw + 0.0004], [h + 0.0004, h + 0.0004], false, rib_c);
    }
    let n = path.len();
    let tan = (path[n - 1] - path[n - 2]).normalize();
    let pp = [path[n - 1] - tan * 0.004, path[n - 1] + tan * 0.010];
    g.sweep(&pp, Vec3::X, [hw + 0.0012; 2], [hh[1] + 0.0016; 2], false, plate);
}

/// A ring (sight aperture, scope ring) standing across Z: centre (x, y) at depth `z`, ring radius `r`, wire `w`.
fn loop_xy(m: &mut M, x: f32, y: f32, z: f32, r: f32, w: f32, c: C) {
    let p: Vec<Vec3> = (0..=16)
        .map(|i| {
            let a = TAU * i as f32 / 16.;
            vec3(x + r * a.cos(), y + r * a.sin(), z)
        })
        .collect();
    m.sweep(&p, Vec3::Z, [w, w], [w, w], true, c);
}

/// Wooden or polymer buttstock seen from the side: stations (z, top y, bottom y, half width) smoothed with
/// rounded corners.
fn stock(m: &mut M, st: &[(f32, f32, f32, f32)], r: f32, c: C) {
    let secs: Vec<Sec> = st.iter().map(|s| sz(s.0, 0., (s.1 + s.2) * 0.5, s.3, (s.1 - s.2) * 0.5, r)).collect();
    m.loft(&secs, 2, c);
}

/// Sling swivel loop under or beside a part.
fn swivel(m: &mut M, x: f32, y: f32, z: f32) {
    m.loop_yz(x, y - 0.0045, z, 0.0042, 0.0011, STEEL_L);
}

// ---------------------------------------------------------------------------------------------------------
// Pistols
// ---------------------------------------------------------------------------------------------------------
fn k9() -> WeaponModel {
    let (mut b, mut s, mut g) = (M::new(), M::new(), M::new());
    let sl = [0.27, 0.28, 0.30];
    // slide: rounded top, bevelled muzzle end and rear
    s.loft(
        &[
            szb(-0.1765, 0., 0.0565, 0.0105, 0.0148, 0.0050, 0.0004),
            szb(-0.1680, 0., 0.0560, 0.0133, 0.0160, 0.0060, 0.0008),
            szb(0.0130, 0., 0.0560, 0.0135, 0.0160, 0.0060, 0.0008),
            szb(0.0202, 0., 0.0565, 0.0112, 0.0150, 0.0050, 0.0004),
        ],
        3,
        sl,
    );
    s.sym(|s, k| {
        for i in 0..6 {
            s.bx([k * 0.0134, k * 0.0138], [0.046, 0.0655], [-0.0120 + i as f32 * 0.0044, -0.0103 + i as f32 * 0.0044], GROOVE);
        }
        for i in 0..3 {
            s.bx([k * 0.0134, k * 0.0138], [0.046, 0.0655], [-0.1655 + i as f32 * 0.0058, -0.1633 + i as f32 * 0.0058], GROOVE);
        }
    });
    s.bx([0.0133, 0.0139], [0.0525, 0.0655], [-0.092, -0.046], GROOVE); // ejection port
    s.bx([0.0138, 0.0141], [0.0525, 0.0545], [-0.092, -0.046], BRASS); // the chambered case glints in it
    // rear sight (two blades with a notch) and front sight post with a dot
    s.sym(|s, k| s.rbx([k * 0.0018, k * 0.0058], [0.0715, 0.0782], [0.0085, 0.0185], 0.0007, STEEL));
    s.bx([-0.0018, 0.0018], [0.0715, 0.0742], [0.0095, 0.0178], GROOVE);
    s.rbx([-0.0024, 0.0024], [0.0715, 0.0800], [-0.1705, -0.1645], 0.0008, STEEL);
    s.ball([0., 0.0772, -0.1655], [0.0014, 0.0014, 0.0008], CHROME);
    // barrel end with its round bore, visible in the slide's mouth
    s.tz(0., 0.0565, [-0.1772, -0.1765], [0.0078, 0.0078], GROOVE);
    s.tz(0., 0.0565, [-0.1850, -0.1770], [0.0058, 0.0058], STEEL_L);
    s.bore(0., 0.0565, -0.1850, 0.0034);
    // frame: dust cover with accessory rail, trigger guard, trigger
    b.loft(
        &[
            sz(-0.1535, 0., 0.0315, 0.0090, 0.0060, 0.0030),
            sz(-0.1250, 0., 0.0318, 0.0112, 0.0085, 0.0035),
            sz(0.0140, 0., 0.0320, 0.0118, 0.0088, 0.0035),
        ],
        2,
        POLY,
    );
    for i in 0..2 {
        b.bx([-0.0100, 0.0100], [0.0262, 0.0268], [-0.1250 + i as f32 * 0.016, -0.1195 + i as f32 * 0.016], GROOVE);
    }
    trigger_guard(&mut b, 0.030, 0.027, [-0.068, -0.004], 0.0095, POLY);
    b.sym(|b, k| b.rbx([k * 0.0120, k * 0.0132], [0.0345, 0.0388], [-0.060, -0.020], 0.0015, if k > 0. { POLY } else { STEEL })); // slide stop
    b.cy(-0.0128, -0.012, [0.021, 0.027], 0.0035, STEEL); // magazine catch
    // grip raked 14 degrees back with finger grooves and a backstrap swell
    let (ga, gb) = (vec3(0., 0.0321, -0.011), vec3(0., -0.0881, 0.019));
    let mut secs = vec![];
    for i in 0..=14 {
        let t = i as f32 / 14.;
        let dip: f32 = [0.30, 0.47, 0.64].iter().map(|c| (-((t - c) / 0.035).powi(2)).exp()).sum();
        let d = 0.0022 * dip;
        secs.push(sa(ga, gb, t, 0.0148 - 0.0030 * (1. - (t * 5.).min(1.)), 0.0208 - d * 0.5, 0.0058, 0.0008 + d * 0.5 + 0.0012 * (1. - t * 3.).max(0.)));
    }
    b.loft(&secs, 2, POLY);
    for i in 0..5 {
        // stippled backstrap bands
        let t = 0.16 + i as f32 * 0.16;
        let sc = sa(ga, gb, t, 0.0, 0.0, 0.0, 0.0);
        b.rbx([-0.0100, 0.0100], [sc.c.y - 0.0025, sc.c.y + 0.0025], [sc.c.z + 0.0212 - 0.0008, sc.c.z + 0.0212 + 0.0014], 0.0008, POLY_G);
    }
    // magazine: steel body below the grip, polymer base plate
    let ma = |t: f32| sa(ga, gb, t, 0.0128, 0.0188, 0.0040, 0.0006);
    g.loft(&[ma(0.06), ma(1.02)], 2, GUNMETAL);
    g.loft(&[sa(ga, gb, 1.02, 0.0150, 0.0218, 0.0045, 0.0), sa(ga, gb, 1.11, 0.0150, 0.0226, 0.0045, 0.0003)], 2, POLY);
    let zm = ga.z + (gb.z - ga.z) * 0.5;
    fin(
        b,
        anch(None, [0., 0.0855, 0.115], [0., 0.0565, -0.185], [0.014, 0.062, -0.03]),
        Some((g, [0., -0.028, zm])),
        Some((s, [0., 0., 0.04])),
    )
}

fn m45() -> WeaponModel {
    let (mut b, mut s, mut g) = (M::new(), M::new(), M::new());
    let sl = [0.22, 0.225, 0.24];
    // flat-sided slide with a rounded top, bevelled rear
    s.loft(
        &[
            szb(-0.1700, 0., 0.0585, 0.0100, 0.0180, 0.0020, 0.0004),
            szb(-0.1640, 0., 0.0590, 0.0128, 0.0188, 0.0025, 0.0006),
            szb(0.0270, 0., 0.0590, 0.0130, 0.0188, 0.0025, 0.0006),
            szb(0.0300, 0., 0.0590, 0.0112, 0.0180, 0.0020, 0.0004),
        ],
        2,
        sl,
    );
    s.bx([-0.0050, 0.0050], [0.0770, 0.0778], [-0.165, 0.025], [0.40, 0.41, 0.43]); // flat top strip
    s.sym(|s, k| {
        for i in 0..7 {
            s.bx([k * 0.0129, k * 0.0134], [0.044, 0.0725], [0.0030 + i as f32 * 0.0040, 0.0044 + i as f32 * 0.0040], GROOVE);
        }
    });
    s.rbx([0.0128, 0.0136], [0.0560, 0.0735], [-0.082, -0.036], 0.0008, GROOVE); // ejection port
    // fixed sights: dovetailed blades and a ramped front blade
    s.rbx([-0.0070, 0.0070], [0.0770, 0.0845], [0.0150, 0.0265], 0.0008, STEEL);
    s.bx([-0.0022, 0.0022], [0.0775, 0.0822], [0.0148, 0.0170], GROOVE);
    s.prism([-0.0020, 0.0020], &[(0.0775, -0.1665), (0.0775, -0.1585), (0.0850, -0.1590), (0.0850, -0.1640)], STEEL);
    // barrel bushing, barrel and plug
    s.tz(0., 0.0610, [-0.1790, -0.1700], [0.0102, 0.0108], STEEL_L);
    s.tz(0., 0.0610, [-0.1840, -0.1790], [0.0072, 0.0072], STEEL);
    s.bore(0., 0.0610, -0.1840, 0.0044);
    s.tz(0., 0.0610, [-0.1706, -0.1700], [0.0040, 0.0040], GROOVE);
    // frame
    b.loft(
        &[
            sz(-0.1450, 0., 0.0335, 0.0100, 0.0072, 0.0028),
            sz(-0.1200, 0., 0.0330, 0.0120, 0.0078, 0.0030),
            sz(0.0300, 0., 0.0330, 0.0123, 0.0078, 0.0030),
        ],
        2,
        STEEL_L,
    );
    trigger_guard(&mut b, 0.030, 0.027, [-0.072, -0.004], 0.0095, STEEL_L);
    b.cz(0., 0.0395, [-0.150, -0.1235], 0.0045, STEEL_L); // guide-rod housing
    // beavertail, grip safety, hammer, thumb safety, slide stop
    b.loft(
        &[sz(0.0170, 0., 0.0420, 0.0100, 0.0040, 0.0018), sz(0.0330, 0., 0.0435, 0.0120, 0.0055, 0.0022), sz(0.0470, 0., 0.0440, 0.0120, 0.0075, 0.0024), sz(0.0510, 0., 0.0410, 0.0112, 0.0070, 0.0020)],
        2,
        STEEL_L,
    );
    b.sweep(&yz(0., &bez((0.0560, 0.0345), (0.0720, 0.0350), (0.0790, 0.0455), 6)), Vec3::X, [0.0040, 0.0042], [0.0042, 0.0030], false, STEEL); // hammer
    b.rbx([-0.0138, -0.0126], [0.0425, 0.0480], [0.0190, 0.0320], 0.0015, STEEL_L);
    b.rbx([-0.0140, -0.0126], [0.0345, 0.0390], [-0.040, -0.006], 0.0012, STEEL_L);
    // grip: raked frame with wooden panels and a bulged mainspring housing
    let (ga, gb) = (vec3(0., 0.0316, -0.0060), vec3(0., -0.0824, 0.0260));
    let fr = |t: f32, hw: f32, hh: f32, off: f32| sa(ga, gb, t, hw, hh, 0.0040, off);
    b.loft(&[fr(0., 0.0110, 0.0200, 0.0), fr(0.5, 0.0116, 0.0205, 0.0004), fr(1., 0.0116, 0.0210, 0.0010)], 2, STEEL_L);
    for k in [-1., 1.] {
        let mut secs = vec![];
        for i in 0..=8 {
            let t = 0.04 + 0.92 * i as f32 / 8.;
            let mut sc = sa(ga, gb, t, 0.0020, 0.0188 - 0.0014 * (t * 3.2).sin().abs(), 0.0016, 0.0);
            sc.c += Vec3::X * (k * 0.0143);
            secs.push(sc);
        }
        b.loft(&secs, 2, WOOD);
        for (dy, dz) in [(-0.026, 0.0035), (-0.060, 0.0165)] {
            b.cx(dy, dz, [k * 0.0160, k * 0.0166], 0.0030, BRASS); // grip screws
        }
    }
    b.rbx([-0.0100, 0.0100], [-0.0880, -0.0800], [0.0145, 0.0405], 0.003, STEEL_L);
    let ma = |t: f32| sa(ga, gb, t, 0.0108, 0.0180, 0.0030, 0.0);
    g.loft(&[ma(0.05), ma(1.03)], 2, STEEL_L);
    g.loft(&[sa(ga, gb, 1.03, 0.0132, 0.0212, 0.0035, 0.0), sa(ga, gb, 1.10, 0.0132, 0.0218, 0.0035, 0.0)], 2, POLY);
    let zm = ga.z + (gb.z - ga.z) * 0.5;
    fin(
        b,
        anch(None, [0., 0.0885, 0.12], [0., 0.061, -0.184], [0.014, 0.065, -0.06]),
        Some((g, [0., -0.025, zm])),
        Some((s, [0., 0., 0.04])),
    )
}

fn hc50() -> WeaponModel {
    let (mut b, mut s, mut g) = (M::new(), M::new(), M::new());
    let sl = CHROME;
    // massive slide with a notched top rail
    s.loft(
        &[
            szb(-0.0750, 0., 0.0690, 0.0160, 0.0220, 0.0080, 0.0010),
            szb(-0.0700, 0., 0.0690, 0.0176, 0.0230, 0.0090, 0.0012),
            szb(0.0480, 0., 0.0690, 0.0176, 0.0230, 0.0090, 0.0012),
            szb(0.0520, 0., 0.0690, 0.0158, 0.0215, 0.0080, 0.0010),
        ],
        3,
        sl,
    );
    rail(&mut s, 0., 0.0925, [-0.072, 0.048], STEEL_L);
    s.sym(|s, k| {
        for i in 0..6 {
            s.bx([k * 0.0175, k * 0.0180], [0.050, 0.088], [0.030 + i as f32 * 0.003, 0.0315 + i as f32 * 0.003 - 0.0008], GROOVE);
        }
    });
    s.rbx([0.0174, 0.0183], [0.058, 0.082], [-0.045, 0.000], 0.001, GROOVE); // large ejection port
    s.rbx([-0.0060, 0.0060], [0.0930, 0.1010], [0.0390, 0.0510], 0.001, STEEL); // rear sight
    s.bx([-0.0018, 0.0018], [0.0935, 0.0990], [0.0388, 0.0402], GROOVE);
    // barrel housing: wedge with a vented rib, flush with the slide
    b.loft(
        &[
            szb(-0.2395, 0., 0.0675, 0.0110, 0.0185, 0.0070, 0.0020),
            szb(-0.2300, 0., 0.0675, 0.0140, 0.0200, 0.0075, 0.0020),
            szb(-0.0750, 0., 0.0675, 0.0150, 0.0210, 0.0080, 0.0020),
        ],
        3,
        sl,
    );
    rail(&mut b, 0., 0.0890, [-0.2300, -0.0760], STEEL_L);
    b.sym(|b, k| {
        for i in 0..4 {
            b.bx([k * 0.0145, k * 0.0152], [0.056, 0.074], [-0.215 + i as f32 * 0.032, -0.200 + i as f32 * 0.032], GROOVE);
        }
    });
    b.rbx([-0.0030, 0.0030], [0.0885, 0.1030], [-0.2370, -0.2290], 0.0008, STEEL); // front sight
    b.tz(0., 0.0675, [-0.2430, -0.2390], [0.0080, 0.0080], STEEL);
    b.bore(0., 0.0675, -0.2430, 0.0050);
    // frame, heavy trigger guard, trigger
    b.loft(
        &[sz(-0.2200, 0., 0.0385, 0.0120, 0.0080, 0.0030), sz(-0.1000, 0., 0.0390, 0.0145, 0.0085, 0.0035), sz(0.0400, 0., 0.0385, 0.0148, 0.0090, 0.0038)],
        2,
        GUNMETAL,
    );
    trigger_guard(&mut b, 0.032, 0.030, [-0.082, 0.004], 0.0115, GUNMETAL);
    b.rbx([-0.0148, -0.0136], [0.0370, 0.0430], [-0.060, -0.020], 0.0015, STEEL_L); // slide stop
    // grip with wrap-around ribbing
    let (ga, gb) = (vec3(0., 0.0340, 0.0050), vec3(0., -0.0900, 0.0400));
    let mut secs = vec![];
    for i in 0..=12 {
        let t = i as f32 / 12.;
        secs.push(sa(ga, gb, t, 0.0185 - 0.001 * t, 0.0245 + 0.0010 * (t * 6.).sin(), 0.0070, 0.));
    }
    b.loft(&secs, 2, POLY);
    for k in [-1., 1.] {
        // rubber side panels with fine horizontal grooves
        let mut secs = vec![];
        for i in 0..=6 {
            let t = 0.08 + 0.84 * i as f32 / 6.;
            let mut sc = sa(ga, gb, t, 0.0014, 0.0198, 0.0010, 0.0);
            sc.c += Vec3::X * (k * 0.0182);
            secs.push(sc);
        }
        b.loft(&secs, 1, POLY_G);
        for i in 0..8 {
            let t = 0.12 + i as f32 * 0.10;
            let sc = sa(ga, gb, t, 0., 0., 0., 0.);
            b.bx([k * 0.0194, k * 0.0198], [sc.c.y - 0.0010, sc.c.y + 0.0010], [sc.c.z - 0.0180, sc.c.z + 0.0210], GROOVE);
        }
    }
    let ma = |t: f32| sa(ga, gb, t, 0.0165, 0.0225, 0.0050, 0.0);
    g.loft(&[ma(0.05), ma(1.03)], 2, GUNMETAL);
    g.loft(&[sa(ga, gb, 1.03, 0.0190, 0.0258, 0.0050, 0.0), sa(ga, gb, 1.10, 0.0190, 0.0262, 0.0050, 0.0)], 2, POLY);
    let zm = ga.z + (gb.z - ga.z) * 0.5;
    fin(
        b,
        anch(None, [0., 0.108, 0.13], [0., 0.0675, -0.243], [0.0176, 0.070, 0.0]),
        Some((g, [0., -0.028, zm])),
        Some((s, [0., 0., 0.045])),
    )
}

fn rv357() -> WeaponModel {
    let mut b = M::new();
    let blue = [0.16, 0.18, 0.25];
    let cyl = [0.20, 0.22, 0.29];
    // frame: top strap, sideplate, rear
    b.loft(
        &[sz(-0.0750, 0., 0.0790, 0.0090, 0.0072, 0.0035), sz(-0.0100, 0., 0.0780, 0.0100, 0.0090, 0.0040), sz(0.0500, 0., 0.0700, 0.0105, 0.0160, 0.0045)],
        2,
        blue,
    );
    b.rbx([-0.0105, 0.0105], [0.032, 0.080], [-0.0120, 0.0560], 0.004, blue);
    // heavy barrel with ventilated rib and full underlug
    b.tzs(0., 0.0700, &[(-0.2240, 0.0098), (-0.2150, 0.0112), (-0.0560, 0.0112)], blue);
    b.tz(0., 0.0700, [-0.2290, -0.2240], [0.0090, 0.0090], blue);
    b.bore(0., 0.0700, -0.2290, 0.0050);
    b.bz([-0.0040, 0.0040], [0.0790, 0.0830], [-0.2150, -0.0700], 0.0010, blue);
    for i in 0..9 {
        b.bx([-0.0042, 0.0042], [0.0795, 0.0832], [-0.2150 + i as f32 * 0.0160, -0.2050 + i as f32 * 0.0160], STEEL);
    }
    b.loft(&[sz(-0.1700, 0., 0.0545, 0.0035, 0.0070, 0.0030), sz(-0.1000, 0., 0.0505, 0.0070, 0.0100, 0.0035), sz(-0.0560, 0., 0.0520, 0.0080, 0.0120, 0.0035)], 2, blue);
    b.cz(0., 0.0360, [-0.1650, -0.0560], 0.0042, STEEL_L); // ejector rod
    b.tz(0., 0.0360, [-0.1700, -0.1640], [0.0062, 0.0062], STEEL_L);
    b.prism([-0.0020, 0.0020], &[(0.0770, -0.2200), (0.0770, -0.2060), (0.0905, -0.2100), (0.0915, -0.2180)], blue);
    b.bx([-0.0010, 0.0010], [0.0830, 0.0900], [-0.2190, -0.2150], RED);
    b.rbx([-0.0055, 0.0055], [0.0790, 0.0930], [0.0210, 0.0330], 0.001, STEEL); // adjustable rear sight
    // fluted cylinder with six chambers in its face, and the crane
    b.tz(0., 0.0580, [-0.0575, -0.0080], [0.0235, 0.0235], cyl);
    for i in 0..6 {
        let a = (i as f32 + 0.5) * TAU / 6.;
        let (sn, cs) = a.sin_cos();
        b.cz(sn * 0.0208, 0.0580 + cs * 0.0208, [-0.0520, -0.0140], 0.0030, [0.11, 0.12, 0.17]); // flutes
        b.rod(vec3(sn * 0.0150, 0.0580 + cs * 0.0150, -0.0572), vec3(sn * 0.0150, 0.0580 + cs * 0.0150, -0.0580), 0.0050, 0.0050, GROOVE, 0.);
    }
    b.tz(0., 0.0580, [-0.0595, -0.0575], [0.0110, 0.0100], STEEL_L);
    b.tz(0., 0.0580, [-0.0080, 0.0], [0.0100, 0.0100], STEEL_L);
    // hammer with a spur, trigger, guard
    b.sweep(&yz(0., &bez((0.0740, 0.0440), (0.0830, 0.0500), (0.0940, 0.0560), 5)), Vec3::X, [0.0035, 0.0050], [0.0030, 0.0022], false, STEEL_L);
    trigger_guard(&mut b, 0.034, 0.030, [-0.064, 0.004], 0.0095, blue);
    // wooden grip with a rounded butt
    let (ga, gb) = (vec3(0., 0.0350, 0.0020), vec3(0., -0.0850, 0.0300));
    let mut secs = vec![];
    for i in 0..=10 {
        let t = i as f32 / 10.;
        secs.push(sa(ga, gb, t, 0.0150 + 0.0012 * (t * 2.).min(1.), 0.0190 + 0.0030 * t, 0.0060, 0.0015 * t));
    }
    b.loft(&secs, 2, WOOD);
    b.ball([0., -0.0820, 0.0320], [0.0160, 0.0095, 0.0230], WOOD_D);
    b.sym(|b, k| b.rbx([k * 0.0160, k * 0.0166], [-0.0420, -0.0390], [0.0100, 0.0125], 0.001, BRASS));
    fin(b, anch(None, [0., 0.096, 0.12], [0., 0.07, -0.2290], [0.011, 0.058, -0.03]), None, None)
}
// ---------------------------------------------------------------------------------------------------------
// Submachine guns
// ---------------------------------------------------------------------------------------------------------
fn mp9() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let blk = STEEL;
    // stamped receiver: flat sides, round top; magwell below
    b.loft(
        &[
            szb(-0.1500, 0., 0.0520, 0.0198, 0.0245, 0.0180, 0.0035),
            szb(-0.1400, 0., 0.0520, 0.0205, 0.0245, 0.0190, 0.0035),
            szb(0.1000, 0., 0.0520, 0.0205, 0.0245, 0.0190, 0.0035),
        ],
        3,
        blk,
    );
    b.rbx([0.0204, 0.0214], [0.0480, 0.0700], [-0.034, 0.030], 0.001, GROOVE); // ejection port
    b.rbx([-0.0090, 0.0090], [0.0000, 0.0300], [-0.1300, -0.0640], 0.002, blk);
    // handguard with grip grooves, barrel, sight hood and muzzle
    b.loft(
        &[
            szb(-0.2720, 0., 0.0370, 0.0225, 0.0245, 0.0120, 0.0090),
            szb(-0.2630, 0., 0.0370, 0.0252, 0.0250, 0.0125, 0.0100),
            szb(-0.1300, 0., 0.0370, 0.0255, 0.0250, 0.0125, 0.0100),
        ],
        3,
        POLY,
    );
    b.sym(|b, k| slots(b, k * 0.0256, [0.024, 0.050], [-0.255, -0.145], 6, GROOVE));
    b.tzs(0., 0.0600, &[(-0.3560, 0.0092), (-0.3300, 0.0095), (-0.2700, 0.0095)], blk);
    b.tz(0., 0.0600, [-0.3300, -0.3120], [0.0120, 0.0120], blk); // muzzle ring
    b.bore(0., 0.0600, -0.3560, 0.0050);
    b.rbx([-0.0085, 0.0085], [0.0680, 0.0800], [-0.3180, -0.2980], 0.002, blk); // sight base
    for z in [-0.3175, -0.2985] {
        loop_xy(&mut b, 0., 0.0915, z, 0.0100, 0.0016, blk); // hood
    }
    b.rbx([-0.0016, 0.0016], [0.0780, 0.1000], [-0.3095, -0.3065], 0.0005, STEEL_L); // front post
    // rear drum sight with its dial and notch
    b.rbx([-0.0120, 0.0120], [0.0720, 0.0820], [0.0480, 0.0720], 0.002, blk);
    b.cx(0.0900, 0.0600, [-0.0125, 0.0125], 0.0115, GUNMETAL);
    b.cx(0.0900, 0.0600, [-0.0140, -0.0125], 0.0120, STEEL_L);
    for i in 0..4 {
        let a = i as f32 * FRAC_PI_2 + 0.4;
        b.rbx([-0.0100, 0.0100], [0.0900 + a.sin() * 0.0112 - 0.0012, 0.0900 + a.sin() * 0.0112 + 0.0012], [0.0600 + a.cos() * 0.0112 - 0.0012, 0.0600 + a.cos() * 0.0112 + 0.0012], 0.0004, GROOVE);
    }
    notch(&mut b, 0.1010, 0.0600, 0.0075, 0.0055);
    // cocking tube on the left
    b.cz(-0.0250, 0.0540, [-0.1500, -0.1050], 0.0052, STEEL_L);
    b.rbx([-0.0340, -0.0230], [0.0460, 0.0560], [-0.1100, -0.0980], 0.003, STEEL_L);
    // grip, trigger group
    pgrip(&mut b, vec3(0., 0.0390, -0.0102), vec3(0., -0.0830, 0.0342), 0.0165, 0.0215, POLY);
    b.rbx([-0.0135, 0.0135], [-0.0030, 0.0300], [-0.0640, 0.0200], 0.003, POLY);
    trigger_guard(&mut b, 0.0, 0.022, [-0.062, 0.004], 0.0105, POLY);
    swivel(&mut b, 0., 0.012, -0.235);
    // collapsible wire stock
    b.sym(|b, k| {
        b.sweep(&[vec3(k * 0.014, 0.054, 0.100), vec3(k * 0.014, 0.054, 0.290)], Vec3::X, [0.0042; 2], [0.0042; 2], true, STEEL_L);
        b.sweep(&[vec3(k * 0.014, 0.014, 0.120), vec3(k * 0.014, 0.014, 0.290)], Vec3::X, [0.0042; 2], [0.0042; 2], true, STEEL_L);
        b.sweep(&[vec3(k * 0.014, 0.054, 0.100), vec3(k * 0.014, 0.034, 0.108), vec3(k * 0.014, 0.014, 0.120)], Vec3::X, [0.0042; 2], [0.0042; 2], true, STEEL_L);
    });
    b.rbx([-0.0235, 0.0235], [-0.0400, 0.0680], [0.2850, 0.3020], 0.007, POLY);
    // curved 30-round magazine with ribs
    curved_mag(&mut g, [(0.032, -0.092), (-0.075, -0.100), (-0.142, -0.150)], 0.0112, [0.0140, 0.0150], GUNMETAL, POLY, &[(0.28, 0.31), (0.50, 0.53), (0.72, 0.75)], STEEL);
    fin(
        b,
        anch(Some([0., 0.012, -0.20]), [0., 0.112, 0.15], [0., 0.06, -0.3565], [0.022, 0.062, 0.0]),
        Some((g, [0., 0.032, -0.092])),
        None,
    )
}

fn ump() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let body = [0.25, 0.255, 0.265];
    // polymer upper: tapering handguard flowing into the receiver
    b.loft(
        &[
            szb(-0.2700, 0., 0.0420, 0.0190, 0.0285, 0.0120, 0.0080),
            szb(-0.2500, 0., 0.0435, 0.0212, 0.0295, 0.0130, 0.0085),
            szb(-0.1500, 0., 0.0455, 0.0222, 0.0315, 0.0140, 0.0085),
            szb(0.0000, 0., 0.0525, 0.0225, 0.0330, 0.0150, 0.0070),
            szb(0.1150, 0., 0.0520, 0.0215, 0.0320, 0.0140, 0.0070),
        ],
        3,
        body,
    );
    b.rbx([0.0224, 0.0232], [0.0500, 0.0740], [-0.030, 0.040], 0.001, GROOVE); // ejection port
    slots_both(&mut b, 0.0222, [0.024, 0.056], [-0.255, -0.165], 6, POLY);
    rail(&mut b, 0., 0.0845, [-0.265, 0.105], POLY);
    // barrel and ribbed muzzle sleeve
    b.tz(0., 0.0560, [-0.3450, -0.2700], [0.0100, 0.0100], STEEL);
    b.tzs(0., 0.0560, &[(-0.3455, 0.0138), (-0.3150, 0.0138)], STEEL);
    for i in 0..3 {
        b.tz(0., 0.0560, [-0.3380 + i as f32 * 0.0090, -0.3340 + i as f32 * 0.0090], [0.0148, 0.0148], POLY);
    }
    b.bore(0., 0.0560, -0.3455, 0.0058);
    // flip-up sights
    b.rbx([-0.0025, 0.0025], [0.0900, 0.1040], [-0.2500, -0.2440], 0.0008, STEEL);
    notch(&mut b, 0.0885 + 0.004, 0.0250, 0.0080, 0.0070);
    // grip, trigger guard, magwell
    pgrip(&mut b, vec3(0., 0.0408, -0.0048), vec3(0., -0.0848, 0.0288), 0.0165, 0.0215, body);
    b.rbx([-0.0125, 0.0125], [-0.0030, 0.0240], [-0.0650, 0.0200], 0.003, body);
    trigger_guard(&mut b, 0.0, 0.022, [-0.062, 0.004], 0.0105, body);
    b.rbx([-0.0150, 0.0150], [-0.0500, 0.0300], [-0.1200, -0.0520], 0.003, body);
    swivel(&mut b, 0., 0.014, -0.250);
    // folding tube stock
    b.sym(|b, k| {
        b.sweep(&[vec3(k * 0.013, 0.066, 0.115), vec3(k * 0.013, 0.066, 0.330)], Vec3::X, [0.0045; 2], [0.0045; 2], true, STEEL_L);
        b.sweep(&[vec3(k * 0.013, 0.024, 0.115), vec3(k * 0.013, 0.024, 0.330)], Vec3::X, [0.0045; 2], [0.0045; 2], true, STEEL_L);
    });
    b.rbx([-0.0225, 0.0225], [-0.0100, 0.0850], [0.3250, 0.3450], 0.008, body);
    // straight translucent 25-round magazine raked slightly forward
    let (ma, mb) = (vec3(0., 0.0298, -0.0831), vec3(0., -0.1098, -0.0929));
    let mg = |t: f32, hw: f32, hh: f32| sa(ma, mb, t, hw, hh, 0.004, 0.);
    g.loft(&[mg(0.0, 0.0125, 0.0170), mg(1.0, 0.0125, 0.0170)], 2, [0.24, 0.25, 0.28]);
    for i in 0..7 {
        let t = 0.10 + i as f32 * 0.13;
        g.loft(&[mg(t, 0.0129, 0.0174), mg(t + 0.03, 0.0129, 0.0174)], 2, [0.16, 0.17, 0.19]);
    }
    g.loft(&[mg(1.0, 0.0138, 0.0188), mg(1.045, 0.0138, 0.0192)], 2, POLY);
    fin(
        b,
        anch(Some([0., 0.014, -0.21]), [0., 0.107, 0.10], [0., 0.056, -0.3455], [0.023, 0.062, 0.0]),
        Some((g, [0., 0.03, -0.088])),
        None,
    )
}

fn pdw() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let body = [0.23, 0.235, 0.25];
    // main body: a smooth rounded shell tapering to the muzzle
    b.loft(
        &[
            sz(-0.2350, 0., 0.0525, 0.0110, 0.0115, 0.0090),
            sz(-0.1900, 0., 0.0525, 0.0200, 0.0225, 0.0120),
            sz(-0.1500, 0., 0.0525, 0.0207, 0.0228, 0.0125),
            sz(0.2200, 0., 0.0525, 0.0207, 0.0228, 0.0125),
            sz(0.2400, 0., 0.0500, 0.0190, 0.0210, 0.0110),
        ],
        3,
        body,
    );
    b.tz(0., 0.0525, [-0.2700, -0.2350], [0.0085, 0.0085], STEEL);
    b.bore(0., 0.0525, -0.2700, 0.0045);
    for i in 0..3 {
        b.rbx([-0.0115, 0.0115], [0.0465, 0.0585], [-0.2680 + i as f32 * 0.0080, -0.2630 + i as f32 * 0.0080], 0.002, STEEL);
    }
    // top rail, ring sight housing with glass
    b.rbx([-0.0185, 0.0185], [0.0980, 0.1100], [-0.1740, 0.1740], 0.005, body);
    rail(&mut b, 0., 0.1160, [-0.1700, 0.1700], body);
    b.rbx([-0.0140, 0.0140], [0.1100, 0.1280], [-0.0900, 0.0200], 0.006, STEEL);
    b.tz(0., 0.1280, [-0.0950, 0.0200], [0.0125, 0.0125], STEEL);
    b.lens(0., 0.1280, [0.0210, 0.0190], 0.0100);
    b.lens(0., 0.1280, [-0.0960, -0.0940], 0.0100);
    b.rbx([-0.0012, 0.0012], [0.1180, 0.1260], [-0.1700, -0.1600], 0.0004, STEEL);
    // grip, forward finger stop, bottom strap and the rounded butt with its thumbhole
    pgrip(&mut b, vec3(0., 0.0300, -0.0020), vec3(0., -0.0700, 0.0020), 0.0165, 0.0185, body);
    b.rbx([-0.0205, 0.0205], [-0.0220, 0.0300], [-0.1250, -0.0660], 0.008, body);
    b.sweep(&yz(0., &bez((-0.0560, 0.0180), (-0.0700, 0.1500), (-0.0560, 0.2300), 8)), Vec3::X, [0.0170; 2], [0.0090; 2], false, body);
    b.loft(
        &[
            sz(0.1900, 0., 0.0100, 0.0200, 0.0660, 0.0120),
            sz(0.2500, 0., 0.0050, 0.0205, 0.0700, 0.0140),
            sz(0.2720, 0., 0.0030, 0.0198, 0.0640, 0.0140),
            sz(0.2800, 0., 0.0030, 0.0185, 0.0560, 0.0120),
        ],
        3,
        body,
    );
    b.rbx([-0.0198, 0.0198], [-0.0500, 0.0560], [0.2760, 0.2830], 0.003, POLY);
    b.rbx([0.0205, 0.0212], [-0.0300, -0.0050], [0.1200, 0.2000], 0.001, GROOVE); // ejection chute
    // translucent magazine lying on top of the body, rounds showing
    g.rbx([-0.0235, 0.0235], [0.0750, 0.1080], [-0.1700, 0.1200], 0.009, [0.33, 0.34, 0.37]);
    for i in 0..13 {
        let z = -0.1200 + i as f32 * 0.0160;
        g.rbx([-0.0240, 0.0240], [0.0850, 0.0990], [z, z + 0.0100], 0.0035, if i % 2 == 0 { BRASS } else { [0.62, 0.50, 0.22] });
    }
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
    // angular body: a flat top running forward then sloping down, bevelled edges
    b.loft(
        &[
            szb(-0.1450, 0., 0.0470, 0.0200, 0.0250, 0.0100, 0.0060),
            szb(-0.0900, 0., 0.0585, 0.0225, 0.0365, 0.0090, 0.0060),
            szb(0.1400, 0., 0.0585, 0.0225, 0.0365, 0.0090, 0.0060),
        ],
        2,
        tan,
    );
    b.tz(0., 0.0500, [-0.2700, -0.1400], [0.0200, 0.0215], tan_d);
    b.tz(0., 0.0500, [-0.3100, -0.2700], [0.0125, 0.0135], STEEL);
    b.bore(0., 0.0500, -0.3100, 0.0062);
    for i in 0..3 {
        b.rbx([-0.0145, 0.0145], [0.0440, 0.0560], [-0.3050 + i as f32 * 0.0090, -0.3000 + i as f32 * 0.0090], 0.002, STEEL);
    }
    slots_both(&mut b, 0.0216, [0.040, 0.062], [-0.250, -0.160], 6, POLY);
    rail(&mut b, 0., 0.0950, [-0.130, 0.140], tan_d);
    b.rbx([-0.0025, 0.0025], [0.0990, 0.1180], [-0.1050, -0.0990], 0.0008, STEEL);
    notch(&mut b, 0.1030, 0.1030, 0.0090, 0.0140);
    b.rbx([0.0224, 0.0234], [0.0600, 0.0850], [-0.040, 0.030], 0.001, GROOVE);
    trigger_guard(&mut b, 0.022, 0.034, [-0.085, 0.004], 0.0105, tan);
    pgrip(&mut b, vec3(0., 0.0310, -0.0070), vec3(0., -0.0870, 0.0270), 0.0170, 0.0235, tan);
    // folding stock
    b.sym(|b, k| {
        b.sweep(&[vec3(k * 0.014, 0.085, 0.140), vec3(k * 0.014, 0.060, 0.270)], Vec3::X, [0.0048; 2], [0.0048; 2], true, STEEL_L);
        b.sweep(&[vec3(k * 0.014, 0.030, 0.120), vec3(k * 0.014, -0.020, 0.270)], Vec3::X, [0.0048; 2], [0.0048; 2], true, STEEL_L);
    });
    b.rbx([-0.0225, 0.0225], [-0.0450, 0.0750], [0.2650, 0.2820], 0.007, POLY);
    let (ma, mb) = (vec3(0., 0.0310, -0.0070), vec3(0., -0.0870, 0.0270));
    let mg = |t: f32, hw: f32, hh: f32| sa(ma, mb, t, hw, hh, 0.004, 0.);
    g.loft(&[mg(0.45, 0.0150, 0.0215), mg(1.00, 0.0150, 0.0215)], 2, tan_d);
    g.loft(&[mg(1.00, 0.0165, 0.0232), mg(1.06, 0.0165, 0.0232)], 2, POLY);
    let zm = ma.z + (mb.z - ma.z) * 0.72;
    fin(
        b,
        anch(Some([0., 0.028, -0.20]), [0., 0.121, 0.17], [0., 0.05, -0.3105], [0.0235, 0.072, 0.0]),
        Some((g, [0., 0.0310 - 0.118 * 0.72, zm])),
        None,
    )
}

/// A rear aperture: a ring on a short post, centred on (0, y) at depth `z`.
fn aperture(m: &mut M, y: f32, z: f32, outer: f32, hole: f32) {
    let w = (outer - hole) * 0.5;
    loop_xy(m, 0., y, z, (outer + hole) * 0.5, w, STEEL);
    m.rbx([-outer * 0.6, outer * 0.6], [y - outer - 0.004, y - outer + 0.001], [z - 0.006, z + 0.006], 0.001, STEEL);
}

/// A notch rear sight: two posts on a base whose top is at `y`.
fn notch(m: &mut M, y: f32, z: f32, w: f32, h: f32) {
    m.rbx([-w, w], [y - 0.003, y], [z - 0.005, z + 0.005], 0.001, STEEL);
    m.sym(|m, k| m.rbx([k * 0.0022, k * w], [y, y + h], [z - 0.005, z + 0.005], 0.0007, STEEL));
}

/// Dark slots on both side faces (at +-x).
fn slots_both(m: &mut M, x: f32, y: [f32; 2], z: [f32; 2], n: usize, c: C) {
    slots(m, x, y, z, n, c);
    slots(m, -x, y, z, n, c);
}

/// Pistol grip on the axis a -> b: a gentle swell, a rounded heel.
fn pgrip(m: &mut M, a: Vec3, b: Vec3, hw: f32, hh: f32, c: C) {
    let mut secs = vec![];
    for i in 0..=8 {
        let t = i as f32 / 8.;
        let sw = 1. + 0.08 * (t * PI).sin();
        let heel = if t > 0.8 { 1. + 0.10 * (t - 0.8) / 0.2 } else { 1. };
        secs.push(sa(a, b, t, hw * sw * (0.92 + 0.08 * t), hh * sw * heel, hw * 0.45, 0.0012 * (t * PI).sin() + 0.0008 * heel));
    }
    m.loft(&secs, 2, c);
}

// ---------------------------------------------------------------------------------------------------------
// Assault rifles
// ---------------------------------------------------------------------------------------------------------
fn k47() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let blk = STEEL;
    // stamped receiver with the arched dust cover, front trunnion, magazine well
    b.loft(
        &[
            szb(-0.1700, 0., 0.0600, 0.0190, 0.0300, 0.0120, 0.0040),
            szb(-0.1550, 0., 0.0605, 0.0186, 0.0305, 0.0125, 0.0040),
            szb(0.0950, 0., 0.0605, 0.0186, 0.0300, 0.0125, 0.0040),
            szb(0.1150, 0., 0.0580, 0.0170, 0.0275, 0.0115, 0.0040),
        ],
        3,
        blk,
    );
    b.rbx([-0.0200, 0.0200], [0.0400, 0.0880], [-0.2150, -0.1650], 0.004, GUNMETAL);
    b.rbx([-0.0140, 0.0140], [0.0200, 0.0400], [-0.1000, -0.0050], 0.003, blk);
    b.rbx([0.0184, 0.0192], [0.0560, 0.0780], [-0.1050, 0.0000], 0.001, GROOVE); // ejection port
    b.rbx([0.0186, 0.0206], [0.0560, 0.0620], [-0.0200, 0.0850], 0.001, STEEL_L); // charging handle slot
    for dz in [0.0, 0.012] {
        b.bx([-0.0189, 0.0189], [0.0885, 0.0900], [-0.0900 + dz, -0.0870 + dz], GROOVE); // cover ribs
    }
    // barrel, gas block and tube, wooden handguards
    b.tz(0., 0.0670, [-0.6000, -0.1700], [0.0072, 0.0075], blk);
    b.rbx([-0.0115, 0.0115], [0.0560, 0.1040], [-0.4450, -0.4100], 0.004, GUNMETAL);
    b.loft(
        &[
            szb(-0.4100, 0., 0.0820, 0.0148, 0.0170, 0.0148, 0.0120),
            szb(-0.3900, 0., 0.0820, 0.0172, 0.0182, 0.0170, 0.0130),
            szb(-0.2400, 0., 0.0820, 0.0180, 0.0180, 0.0175, 0.0150),
        ],
        3,
        WOOD_L,
    );
    b.loft(
        &[
            szb(-0.4100, 0., 0.0480, 0.0130, 0.0185, 0.0040, 0.0110),
            szb(-0.3900, 0., 0.0480, 0.0172, 0.0185, 0.0050, 0.0140),
            szb(-0.2150, 0., 0.0480, 0.0188, 0.0185, 0.0050, 0.0150),
        ],
        3,
        WOOD_L,
    );
    b.rbx([-0.0200, 0.0200], [0.0400, 0.0780], [-0.2550, -0.2400], 0.003, STEEL_L); // retaining ring
    b.rbx([-0.0200, 0.0200], [0.0260, 0.0660], [-0.4150, -0.4050], 0.003, STEEL_L);
    b.cz(0., 0.0430, [-0.5500, -0.4100], 0.0028, STEEL_L); // cleaning rod
    swivel(&mut b, 0., 0.030, -0.370);
    // rear sight ramp and leaf, hooded front sight with its post
    b.rbx([-0.0095, 0.0095], [0.0900, 0.1000], [-0.2320, -0.1800], 0.003, blk);
    b.rbx([-0.0075, 0.0075], [0.1000, 0.1080], [-0.2100, -0.1960], 0.002, blk);
    b.sym(|b, k| b.rbx([k * 0.0025, k * 0.0060], [0.1080, 0.1140], [-0.2090, -0.1970], 0.0008, blk));
    b.rbx([-0.0065, 0.0065], [0.0600, 0.0880], [-0.5750, -0.5350], 0.003, GUNMETAL);
    b.rbx([-0.0015, 0.0015], [0.0880, 0.1170], [-0.5600, -0.5560], 0.0006, STEEL);
    b.sym(|b, k| b.rbx([k * 0.0058, k * 0.0086], [0.0880, 0.1170], [-0.5660, -0.5500], 0.0012, GUNMETAL));
    b.rbx([-0.0088, 0.0088], [0.1150, 0.1190], [-0.5660, -0.5500], 0.0012, GUNMETAL);
    // slant muzzle brake
    b.tzs(0., 0.0670, &[(-0.5900, 0.0100), (-0.5990, 0.0100), (-0.6040, 0.0112)], GUNMETAL);
    b.prism([-0.0095, 0.0095], &[(0.0775, -0.6040), (0.0775, -0.5930), (0.0700, -0.6040)], STEEL_L);
    b.bore(0., 0.0670, -0.6040, 0.0050);
    // wooden pistol grip, trigger guard, stock
    pgrip(&mut b, vec3(0., 0.0344, -0.0153), vec3(0., -0.0944, 0.0393), 0.0165, 0.0222, WOOD_D);
    trigger_guard(&mut b, 0.030, 0.030, [-0.068, 0.004], 0.0095, blk);
    stock(
        &mut b,
        &[(0.1100, 0.0700, -0.0100, 0.0148), (0.1800, 0.0640, -0.0420, 0.0160), (0.2500, 0.0560, -0.0760, 0.0165), (0.2745, 0.0520, -0.0740, 0.0162)],
        0.0085,
        WOOD,
    );
    b.rbx([-0.0166, 0.0166], [-0.0780, 0.0540], [0.2745, 0.2830], 0.004, STEEL);
    swivel(&mut b, 0., -0.075, 0.250);
    // curved magazine, ribbed
    curved_mag(&mut g, [(0.032, -0.052), (-0.065, -0.060), (-0.150, -0.135)], 0.0145, [0.0190, 0.0190], STEEL, GUNMETAL, &[(0.22, 0.26), (0.42, 0.46), (0.62, 0.66), (0.80, 0.84)], GUNMETAL);
    // charging handle
    s.rbx([0.0186, 0.0246], [0.0600, 0.0660], [-0.0360, -0.0160], 0.002, STEEL_L);
    s.rbx([0.0246, 0.0296], [0.0560, 0.0720], [-0.0400, -0.0120], 0.003, STEEL_L);
    fin(
        b,
        anch(Some([0., 0.030, -0.32]), [0., 0.121, -0.10], [0., 0.067, -0.6045], [0.02, 0.066, -0.05]),
        Some((g, [0., 0.032, -0.052])),
        Some((s, [0., 0., 0.07])),
    )
}

fn m4c() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let up = [0.26, 0.265, 0.28];
    let lo = POLY;
    // upper and lower receivers, rounded and blended
    b.loft(
        &[
            szb(-0.1400, 0., 0.0720, 0.0190, 0.0220, 0.0080, 0.0030),
            szb(-0.1200, 0., 0.0720, 0.0195, 0.0220, 0.0085, 0.0030),
            szb(0.0550, 0., 0.0720, 0.0195, 0.0220, 0.0085, 0.0030),
        ],
        3,
        up,
    );
    b.loft(&[szb(-0.1200, 0., 0.0250, 0.0185, 0.0250, 0.0020, 0.0060), szb(0.0600, 0., 0.0250, 0.0185, 0.0250, 0.0020, 0.0060)], 3, lo);
    b.rbx([-0.0135, 0.0135], [-0.0200, 0.0000], [-0.0980, -0.0520], 0.003, lo);
    b.rbx([0.0194, 0.0202], [0.0600, 0.0840], [-0.0600, 0.0200], 0.001, STEEL); // ejection port cover
    b.rbx([0.0194, 0.0230], [0.0660, 0.0780], [0.0220, 0.0420], 0.003, STEEL_L); // forward assist
    b.cy(0.0196, -0.0500, [0.0840, 0.0920], 0.0070, STEEL_L);
    // flat-top rail and the carry handle with its sights
    rail(&mut b, 0., 0.0985, [-0.130, 0.050], up);
    let mut handle = bez((0.1000, -0.1150), (0.1400, -0.1100), (0.1330, -0.0750), 5);
    handle.extend(bez((0.1330, -0.0750), (0.1360, -0.0100), (0.1330, 0.0300), 4).into_iter().skip(1));
    handle.extend(bez((0.1330, 0.0300), (0.1330, 0.0480), (0.1000, 0.0500), 4).into_iter().skip(1));
    b.sweep(&yz(0., &handle), Vec3::X, [0.0125; 2], [0.0070; 2], false, up);
    b.sym(|b, k| b.rbx([k * 0.0075, k * 0.0135], [0.1000, 0.1180], [-0.1150, -0.0920], 0.003, up));
    aperture(&mut b, 0.1460, 0.034, 0.0075, 0.0025);
    b.rbx([-0.0135, 0.0135], [0.0880, 0.1000], [0.0520, 0.0820], 0.003, STEEL); // charging handle
    // round ribbed handguard, barrel, gas block with its A-frame sight post, flash hider
    b.tzs(0., 0.0720, &[(-0.3900, 0.0205), (-0.3800, 0.0215), (-0.1350, 0.0215)], lo);
    for i in 0..8 {
        let z = -0.3700 + i as f32 * 0.0300;
        b.tz(0., 0.0720, [z, z + 0.0030], [0.0222, 0.0222], STEEL);
    }
    b.tz(0., 0.0720, [-0.1480, -0.1350], [0.0250, 0.0250], STEEL); // delta ring
    slots_both(&mut b, 0.0214, [0.060, 0.084], [-0.370, -0.170], 7, STEEL);
    b.tz(0., 0.0720, [-0.4300, -0.3900], [0.0075, 0.0075], STEEL);
    b.rbx([-0.0100, 0.0100], [0.0640, 0.1000], [-0.4320, -0.3960], 0.004, STEEL);
    b.prism([-0.0014, 0.0014], &[(0.0990, -0.4280), (0.0990, -0.4180), (0.1455, -0.4230)], STEEL);
    b.sym(|b, k| b.rbx([k * 0.0055, k * 0.0075], [0.1000, 0.1320], [-0.4310, -0.4150], 0.001, STEEL));
    b.tz(0., 0.0720, [-0.5350, -0.4300], [0.0075, 0.0075], STEEL);
    b.tzs(0., 0.0720, &[(-0.4900, 0.0105), (-0.5340, 0.0105)], STEEL);
    for i in 0..3 {
        b.tz(0., 0.0720, [-0.5300 + i as f32 * 0.0110, -0.5260 + i as f32 * 0.0110], [0.0112, 0.0112], GROOVE);
    }
    b.bore(0., 0.0720, -0.5350, 0.0058);
    // pistol grip, trigger guard
    pgrip(&mut b, vec3(0., 0.0260, -0.0160), vec3(0., -0.0860, 0.0430), 0.0165, 0.0215, lo);
    trigger_guard(&mut b, 0.0, 0.030, [-0.058, 0.0], 0.0095, lo);
    // buffer tube, collapsible stock with a rounded butt pad
    b.tz(0., 0.0600, [0.0550, 0.2750], [0.0185, 0.0185], STEEL);
    b.loft(
        &[
            szb(0.1300, 0., 0.0320, 0.0185, 0.0380, 0.0100, 0.0050),
            szb(0.2000, 0., 0.0240, 0.0190, 0.0460, 0.0120, 0.0070),
            szb(0.3050, 0., 0.0120, 0.0192, 0.0520, 0.0140, 0.0070),
        ],
        3,
        lo,
    );
    b.rbx([-0.0194, 0.0194], [-0.0400, 0.0640], [0.3040, 0.3180], 0.006, POLY_G);
    b.sym(|b, k| b.rbx([k * 0.0188, k * 0.0198], [-0.0100, 0.0250], [0.2100, 0.2800], 0.003, GROOVE));
    swivel(&mut b, 0., -0.040, 0.290);
    // STANAG magazine
    curved_mag(&mut g, [(0.0, -0.075), (-0.062, -0.078), (-0.132, -0.098)], 0.0125, [0.0185, 0.0185], GUNMETAL, POLY, &[(0.30, 0.34), (0.55, 0.59)], STEEL);
    fin(
        b,
        anch(Some([0., 0.050, -0.26]), [0., 0.146, 0.11], [0., 0.072, -0.5355], [0.022, 0.072, -0.01]),
        Some((g, [0., 0.0, -0.075])),
        None,
    )
}

fn fm2() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let ol = OLIVE;
    let ol_d = OLIVE_D;
    // one smooth bullpup body: heavy front block, bottom sweeping back to the butt
    let st = |z: f32, top: f32, bot: f32, hw: f32| sz(z, 0., (top + bot) * 0.5, hw, (top - bot) * 0.5, 0.0130);
    b.loft(
        &[st(-0.3000, 0.078, 0.0, 0.0245), st(0.0300, 0.078, 0.0, 0.0255), st(0.1000, 0.078, -0.030, 0.0255), st(0.2250, 0.078, -0.060, 0.0250), st(0.2740, 0.074, -0.056, 0.0235)],
        3,
        ol,
    );
    b.rbx([-0.0252, 0.0252], [-0.0660, 0.0400], [0.2680, 0.2830], 0.006, POLY);
    b.rbx([0.0252, 0.0262], [0.0400, 0.0660], [0.0600, 0.1300], 0.001, GROOVE); // ejection port
    // barrel, sleeve, flash hider and folded bipod legs
    b.tz(0., 0.0600, [-0.4700, -0.3000], [0.0075, 0.0075], STEEL);
    b.tz(0., 0.0600, [-0.3800, -0.3000], [0.0135, 0.0135], ol_d);
    b.tzs(0., 0.0600, &[(-0.4690, 0.0098), (-0.4480, 0.0098)], STEEL);
    b.tz(0., 0.0600, [-0.4620, -0.4500], [0.0140, 0.0140], STEEL_L);
    b.bore(0., 0.0600, -0.4755, 0.0052);
    b.sym(|b, k| {
        b.sweep(&[vec3(k * 0.0290, 0.052, -0.430), vec3(k * 0.0290, 0.052, -0.250)], Vec3::X, [0.0034; 2], [0.0034; 2], true, STEEL_L);
    });
    b.rbx([-0.0265, 0.0265], [0.0080, 0.0700], [-0.2600, -0.2350], 0.005, ol_d);
    // carry handle running over the whole body, sights at both ends
    b.sym(|b, k| {
        b.rbx([k * 0.0075, k * 0.0155], [0.0780, 0.1280], [-0.2360, -0.2160], 0.004, ol_d);
        b.rbx([k * 0.0075, k * 0.0155], [0.0780, 0.1280], [0.1050, 0.1250], 0.004, ol_d);
    });
    b.rbx([-0.0165, 0.0165], [0.1200, 0.1320], [-0.2360, 0.1250], 0.005, ol_d);
    aperture(&mut b, 0.1415, 0.118, 0.0075, 0.0025);
    b.rbx([-0.0050, 0.0050], [0.0780, 0.1120], [-0.3200, -0.2900], 0.003, STEEL);
    b.rbx([-0.0012, 0.0012], [0.1120, 0.1425], [-0.3110, -0.3060], 0.0005, STEEL);
    // grip and trigger guard
    pgrip(&mut b, vec3(0., 0.0220, -0.0040), vec3(0., -0.0860, 0.0040), 0.0165, 0.0215, POLY);
    trigger_guard(&mut b, 0.0, 0.028, [-0.062, -0.002], 0.0105, POLY);
    curved_mag(&mut g, [(-0.030, 0.088), (-0.078, 0.092), (-0.123, 0.100)], 0.0115, [0.0155, 0.0155], GUNMETAL, POLY, &[(0.40, 0.44)], STEEL);
    fin(
        b,
        anch(Some([0., 0.0, -0.20]), [0., 0.1415, 0.20], [0., 0.06, -0.4755], [0.0263, 0.052, 0.10]),
        Some((g, [0., -0.030, 0.088])),
        None,
    )
}

fn bpa() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let ol = OLIVE;
    let blk = [0.07, 0.075, 0.08];
    let st = |z: f32, top: f32, bot: f32, hw: f32| sz(z, 0., (top + bot) * 0.5, hw, (top - bot) * 0.5, 0.0140);
    b.loft(
        &[st(-0.2500, 0.085, -0.020, 0.0280), st(0.0500, 0.085, -0.030, 0.0285), st(0.2000, 0.085, -0.065, 0.0280), st(0.2700, 0.072, -0.062, 0.0270), st(0.2800, 0.052, -0.060, 0.0250)],
        3,
        ol,
    );
    b.rbx([-0.0295, 0.0295], [-0.0620, 0.0480], [0.2740, 0.2880], 0.006, POLY);
    b.rbx([0.0284, 0.0292], [0.0400, 0.0700], [0.0600, 0.1200], 0.001, GROOVE);
    // barrel and sleeve
    b.tz(0., 0.0600, [-0.5000, -0.2500], [0.0078, 0.0078], STEEL);
    b.tz(0., 0.0600, [-0.3600, -0.2500], [0.0135, 0.0135], ol);
    b.tzs(0., 0.0600, &[(-0.4990, 0.0105), (-0.4700, 0.0105)], STEEL);
    b.tz(0., 0.0600, [-0.4780, -0.4680], [0.0135, 0.0135], STEEL_L);
    b.bore(0., 0.0600, -0.5055, 0.0055);
    // integral optic: carry handle with the 1.5x telescope
    b.rbx([-0.0195, 0.0195], [0.0850, 0.1040], [-0.2200, 0.1000], 0.006, ol);
    b.tz(0., 0.1160, [-0.1000, 0.1100], [0.0205, 0.0205], ol);
    b.tzs(0., 0.1160, &[(-0.1000, 0.0205), (-0.1200, 0.0225), (-0.1350, 0.0250)], blk);
    b.tz(0., 0.1160, [-0.0200, 0.0000], [0.0214, 0.0214], blk);
    b.tz(0., 0.1160, [0.0850, 0.1000], [0.0214, 0.0214], blk);
    b.tzs(0., 0.1160, &[(0.1100, 0.0205), (0.1230, 0.0230)], STEEL_L);
    b.lens(0., 0.1160, [-0.1370, -0.1340], 0.0215);
    b.lens(0., 0.1160, [0.1240, 0.1220], 0.0160);
    b.rbx([-0.0012, 0.0012], [0.1040, 0.1200], [-0.2180, -0.2120], 0.0005, STEEL);
    b.cy(0., -0.0200, [0.1360, 0.1500], 0.0085, STEEL); // elevation cap
    // grip, big trigger guard, vertical foregrip
    pgrip(&mut b, vec3(0., -0.0120, -0.0100), vec3(0., -0.1120, 0.0140), 0.0165, 0.0220, ol);
    trigger_guard(&mut b, -0.018, 0.034, [-0.070, 0.0], 0.0105, ol);
    pgrip(&mut b, vec3(0., -0.0050, -0.2110), vec3(0., -0.0960, -0.2190), 0.0145, 0.0205, ol);
    curved_mag(&mut g, [(-0.040, 0.088), (-0.092, 0.092), (-0.135, 0.100)], 0.0135, [0.0170, 0.0170], [0.30, 0.32, 0.34], blk, &[], STEEL);
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
    let rc = [0.32, 0.33, 0.34];
    let fur = [0.17, 0.17, 0.185];
    // stamped receiver with its long ridges, ejection port, drum sight
    b.loft(
        &[
            szb(-0.1300, 0., 0.0575, 0.0195, 0.0275, 0.0060, 0.0050),
            szb(-0.1150, 0., 0.0575, 0.0198, 0.0275, 0.0090, 0.0050),
            szb(0.2300, 0., 0.0575, 0.0198, 0.0275, 0.0090, 0.0050),
        ],
        3,
        rc,
    );
    b.bx([-0.0199, 0.0199], [0.0690, 0.0705], [-0.1150, 0.2250], STEEL);
    b.bx([-0.0199, 0.0199], [0.0420, 0.0435], [-0.1150, 0.2250], STEEL);
    b.rbx([0.0196, 0.0206], [0.0550, 0.0800], [-0.0600, 0.0300], 0.001, GROOVE);
    b.rbx([-0.0120, 0.0120], [0.0850, 0.0940], [0.1700, 0.2150], 0.003, STEEL);
    b.cx(0.1030, 0.1950, [-0.0120, 0.0120], 0.0105, GUNMETAL);
    notch(&mut b, 0.1135, 0.1950, 0.0080, 0.0060);
    // slotted polymer handguard, barrel, hooded front sight, flash hider
    b.loft(
        &[
            szb(-0.3600, 0., 0.0430, 0.0200, 0.0285, 0.0130, 0.0100),
            szb(-0.3400, 0., 0.0430, 0.0222, 0.0305, 0.0140, 0.0120),
            szb(-0.1300, 0., 0.0430, 0.0225, 0.0310, 0.0140, 0.0120),
        ],
        3,
        fur,
    );
    slots_both(&mut b, 0.0224, [0.026, 0.060], [-0.340, -0.150], 8, GROOVE);
    b.tz(0., 0.0660, [-0.6000, -0.3600], [0.0090, 0.0092], STEEL);
    b.tzs(0., 0.0660, &[(-0.6050, 0.0108), (-0.5900, 0.0120), (-0.5600, 0.0120)], GUNMETAL);
    for i in 0..4 {
        b.sym(|b, k| b.bx([k * 0.0116, k * 0.0124], [0.060, 0.072], [-0.6000 + i as f32 * 0.0100, -0.5950 + i as f32 * 0.0100], GROOVE));
    }
    b.bore(0., 0.0660, -0.6050, 0.0058);
    b.rbx([-0.0120, 0.0120], [0.0520, 0.0780], [-0.5450, -0.5120], 0.003, GUNMETAL);
    b.rbx([-0.0015, 0.0015], [0.0780, 0.1200], [-0.5320, -0.5280], 0.0006, STEEL);
    b.sym(|b, k| b.rbx([k * 0.0058, k * 0.0085], [0.0780, 0.1020], [-0.5380, -0.5220], 0.001, GUNMETAL));
    swivel(&mut b, 0., 0.014, -0.340);
    // grip, trigger group, fixed stock
    b.rbx([-0.0165, 0.0165], [0.0000, 0.0300], [-0.0550, 0.0300], 0.004, fur);
    trigger_guard(&mut b, 0.0, 0.028, [-0.058, 0.004], 0.0105, fur);
    pgrip(&mut b, vec3(0., 0.0285, -0.0120), vec3(0., -0.0985, 0.0420), 0.0165, 0.0220, fur);
    stock(&mut b, &[(0.2200, 0.0780, 0.0150, 0.0180), (0.3200, 0.0740, -0.0300, 0.0185), (0.4000, 0.0700, -0.0580, 0.0185), (0.4250, 0.0620, -0.0520, 0.0182)], 0.0070, fur);
    b.rbx([-0.0190, 0.0190], [-0.0560, 0.0640], [0.4220, 0.4340], 0.005, POLY);
    // 20-round ribbed magazine
    curved_mag(&mut g, [(0.030, -0.060), (-0.060, -0.064), (-0.115, -0.080)], 0.0145, [0.0215, 0.0215], GUNMETAL, POLY, &[(0.28, 0.33), (0.55, 0.60), (0.80, 0.85)], STEEL);
    fin(
        b,
        anch(Some([0., 0.012, -0.24]), [0., 0.123, 0.29], [0., 0.066, -0.6055], [0.02, 0.068, 0.0]),
        Some((g, [0., 0.030, -0.060])),
        None,
    )
}

fn dmr20() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let tan = TAN;
    let tan_d = [0.40, 0.33, 0.22];
    // upper receiver and free-float handguard as one smooth tan body
    b.loft(
        &[
            szb(-0.1400, 0., 0.0550, 0.0215, 0.0350, 0.0090, 0.0060),
            szb(0.1700, 0., 0.0550, 0.0215, 0.0350, 0.0090, 0.0060),
        ],
        3,
        tan,
    );
    b.rbx([0.0214, 0.0224], [0.0500, 0.0780], [-0.0400, 0.0300], 0.001, GROOVE);
    b.loft(
        &[
            szb(-0.4600, 0., 0.0610, 0.0190, 0.0310, 0.0100, 0.0100),
            szb(-0.4400, 0., 0.0610, 0.0222, 0.0310, 0.0110, 0.0110),
            szb(-0.1400, 0., 0.0610, 0.0225, 0.0310, 0.0110, 0.0110),
        ],
        3,
        tan,
    );
    slots_both(&mut b, 0.0224, [0.045, 0.075], [-0.440, -0.160], 9, tan_d);
    rail(&mut b, 0., 0.0975, [-0.460, 0.170], tan_d);
    // heavy barrel and brake
    b.tz(0., 0.0660, [-0.6600, -0.4600], [0.0098, 0.0098], STEEL);
    b.tzs(0., 0.0660, &[(-0.6850, 0.0100), (-0.6780, 0.0125), (-0.6450, 0.0125), (-0.6400, 0.0098)], STEEL);
    for i in 0..3 {
        b.sym(|b, k| b.bx([k * 0.0123, k * 0.0130], [0.0580, 0.0740], [-0.6760 + i as f32 * 0.0120, -0.6690 + i as f32 * 0.0120], GROOVE));
    }
    b.bore(0., 0.0660, -0.6855, 0.0058);
    // pistol grip, guard, magwell, stock with the cheek riser
    pgrip(&mut b, vec3(0., 0.0345, -0.0170), vec3(0., -0.0955, 0.0470), 0.0170, 0.0225, tan_d);
    trigger_guard(&mut b, 0.020, 0.048, [-0.070, 0.0], 0.0105, tan);
    b.rbx([-0.0145, 0.0145], [-0.0120, 0.0200], [-0.1150, -0.0450], 0.004, tan);
    stock(&mut b, &[(0.1700, 0.0850, 0.0000, 0.0190), (0.2500, 0.0850, -0.0300, 0.0195), (0.3300, 0.0810, -0.0600, 0.0195), (0.3450, 0.0300, -0.0600, 0.0190)], 0.0080, tan);
    b.rbx([-0.0175, 0.0175], [0.0850, 0.1060], [0.1800, 0.3100], 0.006, tan_d);
    b.rbx([-0.0200, 0.0200], [-0.0640, 0.0850], [0.3420, 0.3560], 0.006, POLY);
    swivel(&mut b, 0., 0.032, -0.300);
    scope(&mut b, 0.102, 0., 0.138, 0.06, -0.20, 0.0165, 0.027);
    let mg = |t: f32| sa(vec3(0., 0.0200, -0.0800), vec3(0., -0.1200, -0.0860), t, 0.0120, 0.0230, 0.004, 0.);
    g.loft(&[mg(0.0), mg(1.0)], 2, POLY);
    g.loft(&[sa(vec3(0., 0.0200, -0.0800), vec3(0., -0.1200, -0.0860), 1.0, 0.0132, 0.0245, 0.004, 0.), sa(vec3(0., 0.0200, -0.0800), vec3(0., -0.1200, -0.0860), 1.06, 0.0132, 0.0245, 0.004, 0.)], 2, GUNMETAL);
    s.rbx([0.0216, 0.0296], [0.0600, 0.0680], [-0.0850, -0.0650], 0.002, STEEL_L);
    s.rbx([0.0296, 0.0346], [0.0550, 0.0730], [-0.0900, -0.0600], 0.003, STEEL_L);
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
    // receiver, barrel, gas system, slotted wooden handguards
    b.loft(
        &[
            szb(-0.1700, 0., 0.0600, 0.0190, 0.0300, 0.0120, 0.0040),
            szb(0.0950, 0., 0.0600, 0.0186, 0.0300, 0.0125, 0.0040),
            szb(0.1150, 0., 0.0580, 0.0170, 0.0275, 0.0115, 0.0040),
        ],
        3,
        blk,
    );
    b.rbx([-0.0195, 0.0195], [0.0400, 0.0870], [-0.2100, -0.1700], 0.004, GUNMETAL);
    b.rbx([0.0184, 0.0192], [0.0550, 0.0780], [-0.1050, 0.0000], 0.001, GROOVE);
    b.tz(0., 0.0670, [-0.8000, -0.1700], [0.0082, 0.0085], blk);
    b.rbx([-0.0110, 0.0110], [0.0560, 0.1000], [-0.4550, -0.4200], 0.004, GUNMETAL);
    b.loft(
        &[szb(-0.4200, 0., 0.0795, 0.0150, 0.0172, 0.0150, 0.0120), szb(-0.4000, 0., 0.0795, 0.0170, 0.0178, 0.0165, 0.0130), szb(-0.2500, 0., 0.0795, 0.0172, 0.0178, 0.0170, 0.0150)],
        3,
        WOOD,
    );
    b.loft(&[szb(-0.4200, 0., 0.0480, 0.0150, 0.0185, 0.0040, 0.0110), szb(-0.4000, 0., 0.0480, 0.0172, 0.0185, 0.0050, 0.0140), szb(-0.2150, 0., 0.0480, 0.0185, 0.0185, 0.0050, 0.0150)], 3, WOOD);
    b.rbx([-0.0195, 0.0195], [0.0260, 0.0660], [-0.4250, -0.4150], 0.003, STEEL_L);
    b.rbx([-0.0095, 0.0095], [0.0900, 0.1000], [-0.2250, -0.1800], 0.003, blk);
    b.rbx([-0.0065, 0.0065], [0.0600, 0.0850], [-0.7750, -0.7450], 0.003, GUNMETAL);
    b.rbx([-0.0015, 0.0015], [0.0850, 0.1100], [-0.7620, -0.7580], 0.0006, STEEL);
    b.sym(|b, k| b.rbx([k * 0.0058, k * 0.0086], [0.0850, 0.1100], [-0.7680, -0.7520], 0.001, GUNMETAL));
    // slotted flash hider
    b.tzs(0., 0.0670, &[(-0.7850, 0.0090), (-0.8000, 0.0112), (-0.8300, 0.0112), (-0.8350, 0.0095)], GUNMETAL);
    for i in 0..3 {
        b.sym(|b, k| b.bx([k * 0.0110, k * 0.0118], [0.059, 0.075], [-0.825 + i as f32 * 0.012, -0.819 + i as f32 * 0.012], GROOVE));
    }
    b.bore(0., 0.0670, -0.8355, 0.0055);
    // wooden grip and the skeleton thumbhole stock: top beam, butt, lower strut, thumbhole
    pgrip(&mut b, vec3(0., 0.0344, -0.0153), vec3(0., -0.0944, 0.0393), 0.0165, 0.0222, WOOD);
    trigger_guard(&mut b, 0.030, 0.030, [-0.068, 0.004], 0.0095, blk);
    b.loft(&[sz(0.1100, 0., 0.0610, 0.0150, 0.0170, 0.0070), sz(0.3000, 0., 0.0650, 0.0155, 0.0150, 0.0070), sz(0.4000, 0., 0.0560, 0.0158, 0.0250, 0.0070)], 3, WOOD);
    b.sweep(&yz(0., &bez((-0.0750, 0.0450), (-0.0950, 0.2400), (-0.0600, 0.3900), 10)), Vec3::X, [0.0090; 2], [0.0130; 2], false, WOOD_D);
    b.rbx([-0.0165, 0.0165], [-0.0920, 0.0780], [0.3850, 0.4020], 0.005, WOOD);
    b.rbx([-0.0170, 0.0170], [-0.0950, 0.0600], [0.4020, 0.4100], 0.003, POLY);
    scope(&mut b, 0.092, 0., 0.131, 0.05, -0.20, 0.0175, 0.0275);
    b.tz(0., 0.131, [0.050, 0.075], [0.0215, 0.0215], POLY);
    curved_mag(&mut g, [(0.030, -0.056), (-0.050, -0.062), (-0.092, -0.098)], 0.0145, [0.0180, 0.0180], STEEL, GUNMETAL, &[(0.45, 0.50)], GUNMETAL);
    s.rbx([0.0186, 0.0246], [0.0600, 0.0660], [-0.0360, -0.0160], 0.002, STEEL_L);
    s.rbx([0.0246, 0.0296], [0.0560, 0.0720], [-0.0400, -0.0120], 0.003, STEEL_L);
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
    b.loft(&[szb(-0.1000, 0., 0.0560, 0.0170, 0.0260, 0.0100, 0.0040), szb(0.1100, 0., 0.0560, 0.0178, 0.0260, 0.0100, 0.0040)], 3, STEEL);
    b.tz(0., 0.0660, [-0.6300, -0.1000], [0.0092, 0.0100], STEEL);
    b.tzs(0., 0.0660, &[(-0.6450, 0.0108), (-0.6250, 0.0108)], GUNMETAL);
    b.bore(0., 0.0660, -0.6455, 0.0055);
    // fore-end, grip, stock
    b.loft(&[szb(-0.3400, 0., 0.0370, 0.0185, 0.0250, 0.0100, 0.0130), szb(-0.3200, 0., 0.0370, 0.0205, 0.0250, 0.0100, 0.0140), szb(-0.1000, 0., 0.0370, 0.0205, 0.0250, 0.0100, 0.0140)], 3, st);
    slots_both(&mut b, 0.0206, [0.025, 0.048], [-0.330, -0.120], 7, st_d);
    pgrip(&mut b, vec3(0., 0.0340, -0.0170), vec3(0., -0.0860, 0.0410), 0.0165, 0.0215, st);
    stock(&mut b, &[(0.1000, 0.0700, -0.0200, 0.0175), (0.2000, 0.0760, -0.0300, 0.0182), (0.3000, 0.0780, -0.0500, 0.0185), (0.3480, 0.0660, -0.0580, 0.0185)], 0.0080, st);
    b.rbx([-0.0190, 0.0190], [-0.0620, 0.0700], [0.3480, 0.3600], 0.005, POLY);
    trigger_guard(&mut b, 0.030, 0.030, [-0.070, 0.002], 0.0095, st_d);
    // ghost ring rear sight, forward-mounted scope on a rail along the barrel
    loop_xy(&mut b, 0., 0.0920, 0.0970, 0.0060, 0.0016, STEEL);
    b.rbx([-0.0055, 0.0055], [0.0800, 0.0880], [0.0920, 0.1020], 0.002, STEEL);
    rail(&mut b, 0., 0.0880, [-0.2800, -0.0500], STEEL);
    scope(&mut b, 0.090, 0., 0.1085, -0.06, -0.26, 0.0135, 0.0225);
    // five-round magazine
    g.rbx([-0.0125, 0.0125], [-0.0500, 0.0300], [-0.0720, -0.0080], 0.003, st_d);
    g.rbx([-0.0135, 0.0135], [-0.0580, -0.0480], [-0.0760, -0.0040], 0.003, POLY);
    // bolt handle with the ball knob
    s.sweep(&[vec3(0.0175, 0.066, 0.06), vec3(0.036, 0.062, 0.06), vec3(0.046, 0.050, 0.06)], Vec3::Z, [0.0042; 2], [0.0042; 2], true, STEEL_L);
    s.ball([0.050, 0.046, 0.06], [0.0105, 0.0105, 0.0105], STEEL_L);
    s.tz(0., 0.0835, [0.0850, 0.1180], [0.0115, 0.0115], STEEL_L);
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
    b.loft(&[szb(-0.2400, 0., 0.0600, 0.0190, 0.0300, 0.0100, 0.0040), szb(0.1200, 0., 0.0600, 0.0195, 0.0300, 0.0100, 0.0040)], 3, STEEL);
    b.tz(0., 0.0720, [-0.8000, -0.2400], [0.0115, 0.0130], GUNMETAL);
    for i in 0..5 {
        b.tz(0., 0.0720, [-0.4800 - i as f32 * 0.0600, -0.4900 - i as f32 * 0.0600], [0.0128, 0.0128], STEEL);
    }
    b.tzs(0., 0.0720, &[(-0.7900, 0.0125), (-0.7950, 0.0185), (-0.8400, 0.0185), (-0.8450, 0.0150)], STEEL);
    b.sym(|b, k| {
        for i in 0..3 {
            b.rbx([k * 0.0184, k * 0.0194], [0.0600, 0.0850], [-0.8350 + i as f32 * 0.0160, -0.8270 + i as f32 * 0.0160], 0.001, [0.02, 0.02, 0.02]);
        }
    });
    b.bore(0., 0.0720, -0.8455, 0.0075);
    // green chassis: fore-end, grip, thumbhole-free stock with the cheek riser and spacer
    b.loft(&[szb(-0.5000, 0., 0.0370, 0.0190, 0.0290, 0.0100, 0.0100), szb(-0.4800, 0., 0.0370, 0.0225, 0.0290, 0.0100, 0.0100), szb(-0.2400, 0., 0.0370, 0.0225, 0.0290, 0.0100, 0.0100)], 3, gr);
    b.rbx([-0.0210, 0.0210], [0.0100, 0.0600], [-0.2400, 0.0000], 0.005, gr);
    slots_both(&mut b, 0.0226, [0.022, 0.050], [-0.470, -0.270], 6, gr_d);
    pgrip(&mut b, vec3(0., 0.0340, -0.0170), vec3(0., -0.0900, 0.0420), 0.0168, 0.0220, gr);
    trigger_guard(&mut b, 0.030, 0.030, [-0.070, 0.004], 0.0095, gr_d);
    stock(&mut b, &[(0.1000, 0.0720, -0.0200, 0.0175), (0.2000, 0.0840, -0.0350, 0.0185), (0.3000, 0.0840, -0.0600, 0.0185), (0.3600, 0.0840, -0.0640, 0.0185)], 0.0080, gr);
    b.rbx([-0.0160, 0.0160], [0.0840, 0.1020], [0.1700, 0.3000], 0.006, gr_d);
    b.rbx([-0.0195, 0.0195], [-0.0640, 0.0840], [0.3580, 0.3720], 0.005, POLY);
    swivel(&mut b, 0., 0.008, -0.450);
    // folded bipod
    b.rbx([-0.0300, 0.0300], [-0.0040, 0.0100], [-0.3800, -0.3400], 0.003, STEEL);
    b.sym(|b, k| b.sweep(&[vec3(k * 0.027, 0.0, -0.360), vec3(k * 0.027, -0.010, -0.580)], Vec3::X, [0.0045; 2], [0.0045; 2], true, STEEL_L));
    // 6x scope
    b.rbx([-0.0085, 0.0085], [0.0900, 0.1040], [-0.2400, 0.1000], 0.002, STEEL);
    scope(&mut b, 0.104, 0., 0.1365, 0.06, -0.26, 0.0165, 0.029);
    g.rbx([-0.0135, 0.0135], [-0.0700, 0.0300], [-0.0900, -0.0150], 0.003, gr_d);
    g.rbx([-0.0145, 0.0145], [-0.0780, -0.0680], [-0.0940, -0.0110], 0.003, POLY);
    s.sweep(&[vec3(0.0195, 0.078, 0.06), vec3(0.036, 0.070, 0.062), vec3(0.052, 0.058, 0.065)], Vec3::Z, [0.0045; 2], [0.0045; 2], true, STEEL_L);
    s.ball([0.056, 0.054, 0.066], [0.0125, 0.0125, 0.0125], gr_d);
    s.tz(0., 0.0840, [0.1000, 0.1450], [0.0125, 0.0125], STEEL_L);
    fin(
        b,
        anch(Some([0., 0.008, -0.34]), [0., 0.1365, 0.13], [0., 0.072, -0.8455], [0.0196, 0.078, 0.0]),
        Some((g, [0., 0.030, -0.050])),
        Some((s, [0., 0., 0.075])),
    )
}

fn m82() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let rc = [0.29, 0.295, 0.30];
    // receivers
    b.loft(&[szb(-0.3000, 0., 0.0690, 0.0245, 0.0390, 0.0120, 0.0060), szb(0.1200, 0., 0.0690, 0.0250, 0.0390, 0.0120, 0.0060)], 3, rc);
    b.rbx([-0.0240, 0.0240], [0.0000, 0.0300], [-0.3000, 0.1200], 0.006, GUNMETAL);
    b.rbx([0.0250, 0.0258], [0.0650, 0.0980], [-0.0800, 0.0200], 0.001, GROOVE);
    rail(&mut b, 0., 0.1115, [-0.300, 0.100], STEEL);
    // heavy finned barrel, huge muzzle brake with ports
    b.tz(0., 0.0750, [-0.9200, -0.3000], [0.0205, 0.0215], GUNMETAL);
    for i in 0..9 {
        b.tz(0., 0.0750, [-0.5000 - i as f32 * 0.0400, -0.5100 - i as f32 * 0.0400], [0.0248, 0.0248], STEEL);
    }
    b.tzs(0., 0.0750, &[(-0.9200, 0.0230), (-0.9350, 0.0290), (-1.0100, 0.0290), (-1.0200, 0.0220)], STEEL);
    b.bore(0., 0.0750, -1.0205, 0.0140);
    b.sym(|b, k| {
        for i in 0..3 {
            b.rbx([k * 0.0285, k * 0.0300], [0.0480, 0.1020], [-1.0050 + i as f32 * 0.0280, -0.9850 + i as f32 * 0.0280], 0.002, [0.015, 0.015, 0.015]);
        }
    });
    // carry handle over the barrel
    b.sweep(&yz(0., &bez((0.0950, -0.6350), (0.2050, -0.5400), (0.0950, -0.4450), 8)), Vec3::X, [0.0105; 2], [0.0070; 2], false, STEEL);
    // folded bipod
    b.rbx([-0.0320, 0.0320], [0.0300, 0.0620], [-0.6600, -0.6200], 0.004, STEEL);
    b.sym(|b, k| b.sweep(&[vec3(k * 0.030, 0.045, -0.640), vec3(k * 0.030, 0.045, -0.880)], Vec3::X, [0.0065; 2], [0.0065; 2], true, STEEL_L));
    // grip, guard, rear frame with the spring tube and the rounded recoil pad
    pgrip(&mut b, vec3(0., 0.0260, -0.0170), vec3(0., -0.1060, 0.0400), 0.0175, 0.0230, POLY);
    trigger_guard(&mut b, 0.0, 0.034, [-0.072, 0.0], 0.0115, POLY);
    b.loft(&[szb(0.1200, 0., 0.0520, 0.0195, 0.0340, 0.0100, 0.0060), szb(0.4000, 0., 0.0500, 0.0195, 0.0320, 0.0100, 0.0060)], 3, POLY);
    b.rbx([-0.0160, 0.0160], [0.0850, 0.1000], [0.2000, 0.3800], 0.005, STEEL);
    b.sweep(&yz(0., &bez((-0.0050, 0.1400), (-0.0450, 0.2800), (-0.0900, 0.4100), 8)), Vec3::X, [0.0090; 2], [0.0090; 2], true, STEEL);
    b.loft(&[szb(0.3980, 0., -0.0065, 0.0260, 0.0880, 0.0200, 0.0200), szb(0.4180, 0., -0.0065, 0.0300, 0.0880, 0.0200, 0.0200), szb(0.4320, 0., -0.0065, 0.0280, 0.0840, 0.0200, 0.0200)], 3, POLY);
    b.cy(0., 0.380, [-0.1350, -0.0900], 0.0075, STEEL_L);
    scope(&mut b, 0.116, 0., 0.1485, 0.06, -0.36, 0.0195, 0.032);
    // ten-round box magazine
    g.rbx([-0.0205, 0.0205], [-0.1000, 0.0020], [-0.1750, -0.0850], 0.004, GUNMETAL);
    g.rbx([-0.0215, 0.0215], [-0.1080, -0.0960], [-0.1800, -0.0800], 0.004, POLY);
    s.rbx([0.0250, 0.0580], [0.0680, 0.0780], [-0.1350, -0.1150], 0.003, STEEL_L);
    s.rbx([0.0580, 0.0700], [0.0600, 0.0860], [-0.1400, -0.1100], 0.004, STEEL_L);
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
    // receiver with its ejection port, barrel with a bead sight, magazine tube
    b.loft(&[szb(-0.2150, 0., 0.0600, 0.0170, 0.0300, 0.0120, 0.0060), szb(0.0000, 0., 0.0600, 0.0175, 0.0300, 0.0120, 0.0060)], 3, STEEL);
    b.rbx([0.0174, 0.0184], [0.0560, 0.0840], [-0.1050, -0.0450], 0.002, GROOVE);
    b.rbx([-0.0100, 0.0100], [0.0260, 0.0320], [-0.1400, -0.0800], 0.002, GUNMETAL);
    b.tz(0., 0.0785, [-0.7450, -0.2150], [0.0115, 0.0115], STEEL);
    b.tzs(0., 0.0785, &[(-0.7450, 0.0118), (-0.7300, 0.0122)], GUNMETAL);
    b.bore(0., 0.0785, -0.7455, 0.0080);
    b.tz(0., 0.0500, [-0.7050, -0.2150], [0.0125, 0.0125], GUNMETAL);
    b.tz(0., 0.0500, [-0.7120, -0.6980], [0.0142, 0.0142], STEEL);
    b.ball([0., 0.0935, -0.7420], [0.0040, 0.0040, 0.0040], CHROME);
    swivel(&mut b, 0., 0.034, -0.700);
    // wooden stock: slim wrist, drop, rounded recoil pad
    b.rbx([-0.0110, 0.0110], [0.0560, 0.0700], [-0.0100, 0.0200], 0.003, WOOD_D);
    stock(&mut b, &[(-0.0050, 0.0880, 0.0100, 0.0175), (0.0800, 0.0780, -0.0180, 0.0165), (0.2350, 0.0700, -0.0520, 0.0172), (0.3050, 0.0600, -0.0620, 0.0182)], 0.0070, WOOD);
    for i in 0..4 {
        b.sym(|b, k| b.bx([k * 0.0160, k * 0.0166], [0.0180 + i as f32 * 0.0052, 0.0200 + i as f32 * 0.0052], [0.0100, 0.0620], GROOVE));
    }
    b.rbx([-0.0190, 0.0190], [-0.0660, 0.0640], [0.3080, 0.3220], 0.006, POLY);
    trigger_guard(&mut b, 0.030, 0.030, [-0.105, -0.022], 0.0095, STEEL);
    // pump forend with corrugations (moves with the action bars)
    s.loft(&[szb(-0.4700, 0., 0.0500, 0.0210, 0.0200, 0.0100, 0.0140), szb(-0.4500, 0., 0.0500, 0.0235, 0.0210, 0.0110, 0.0150), szb(-0.3000, 0., 0.0500, 0.0235, 0.0210, 0.0110, 0.0150)], 3, WOOD_D);
    for i in 0..8 {
        s.bx([-0.0238, 0.0238], [0.0330, 0.0640], [-0.4600 + i as f32 * 0.0090, -0.4560 + i as f32 * 0.0090], WOOD);
    }
    s.sym(|s, k| s.cz(k * 0.0105, 0.0470, [-0.3000, -0.2150], 0.0030, STEEL_L));
    fin(
        b,
        anch(Some([0., 0.030, -0.385]), [0., 0.101, 0.08], [0., 0.0785, -0.745], [0.0183, 0.070, -0.075]),
        None,
        Some((s, [0., 0., 0.095])),
    )
}

fn auto12() -> WeaponModel {
    let mut b = M::new();
    let rc = [0.26, 0.265, 0.275];
    b.loft(&[szb(-0.2000, 0., 0.0610, 0.0190, 0.0310, 0.0110, 0.0060), szb(0.0400, 0., 0.0610, 0.0195, 0.0310, 0.0110, 0.0060)], 3, rc);
    b.rbx([0.0194, 0.0204], [0.0560, 0.0840], [-0.1100, -0.0500], 0.002, GROOVE);
    rail(&mut b, 0., 0.0960, [-0.190, 0.030], rc);
    aperture(&mut b, 0.1115, 0.024, 0.0075, 0.0025);
    // barrel and magazine tube, ribbed round handguard, muzzle with ghost-ring front sight
    b.tz(0., 0.0780, [-0.6800, -0.2000], [0.0115, 0.0115], STEEL);
    b.tz(0., 0.0500, [-0.6400, -0.2000], [0.0125, 0.0125], STEEL);
    b.tzs(0., 0.0665, &[(-0.4600, 0.0215), (-0.4500, 0.0238), (-0.2000, 0.0238)], POLY);
    for i in 0..9 {
        let z = -0.4300 + i as f32 * 0.0230;
        b.tz(0., 0.0665, [z, z + 0.0070], [0.0243, 0.0243], POLY_G);
    }
    b.tzs(0., 0.0780, &[(-0.6850, 0.0138), (-0.6550, 0.0138)], GUNMETAL);
    b.bore(0., 0.0780, -0.6850, 0.0085);
    b.tz(0., 0.0500, [-0.6480, -0.6380], [0.0142, 0.0142], GUNMETAL);
    loop_xy(&mut b, 0., 0.1000, -0.6700, 0.0085, 0.0014, STEEL);
    b.rbx([-0.0015, 0.0015], [0.0880, 0.0920], [-0.6760, -0.6640], 0.0006, STEEL);
    // pistol grip, guard, telescoping tube stock
    pgrip(&mut b, vec3(0., 0.0350, -0.0160), vec3(0., -0.0950, 0.0440), 0.0165, 0.0215, POLY);
    trigger_guard(&mut b, 0.030, 0.030, [-0.105, 0.0], 0.0100, POLY);
    b.rbx([-0.0140, 0.0140], [0.0300, 0.0900], [0.0400, 0.1000], 0.005, POLY);
    b.sym(|b, k| {
        b.sweep(&[vec3(k * 0.015, 0.078, 0.100), vec3(k * 0.015, 0.078, 0.300)], Vec3::X, [0.0055; 2], [0.0055; 2], true, STEEL_L);
        b.sweep(&[vec3(k * 0.015, 0.032, 0.100), vec3(k * 0.015, 0.032, 0.300)], Vec3::X, [0.0055; 2], [0.0055; 2], true, STEEL_L);
    });
    b.rbx([-0.0240, 0.0240], [-0.0650, 0.0980], [0.2950, 0.3190], 0.008, POLY);
    swivel(&mut b, 0., 0.034, -0.600);
    fin(b, anch(Some([0., 0.0405, -0.33]), [0., 0.1115, 0.09], [0., 0.078, -0.685], [0.0203, 0.070, -0.08]), None, None)
}

fn sawn() -> WeaponModel {
    let mut b = M::new();
    let barrel = [0.14, 0.145, 0.16];
    // two round barrels joined by a rib, with a bead sight
    b.tz(-0.0098, 0.0520, [-0.4100, -0.0800], [0.0098, 0.0100], barrel);
    b.tz(0.0098, 0.0520, [-0.4100, -0.0800], [0.0098, 0.0100], barrel);
    b.rbx([-0.0040, 0.0040], [0.0585, 0.0640], [-0.4100, -0.0800], 0.0012, STEEL_L);
    b.ball([0., 0.0675, -0.4050], [0.0035, 0.0035, 0.0035], CHROME);
    for k in [-1., 1.] {
        b.bore(k * 0.0098, 0.0520, -0.4105, 0.0068);
        b.tz(k * 0.0098, 0.0520, [-0.4100, -0.4020], [0.0108, 0.0108], STEEL);
    }
    // action, top lever, hammers
    b.loft(&[szb(-0.1000, 0., 0.0530, 0.0210, 0.0250, 0.0110, 0.0100), szb(0.0050, 0., 0.0530, 0.0215, 0.0250, 0.0110, 0.0100)], 3, GUNMETAL);
    b.sweep(&yz(0., &bez((0.0780, -0.0400), (0.0840, -0.0200), (0.0830, 0.0150), 5)), Vec3::X, [0.0040; 2], [0.0030; 2], false, STEEL_L);
    b.sym(|b, k| {
        b.sweep(&yz(k * 0.0115, &bez((0.0780, 0.0060), (0.0920, 0.0100), (0.1000, 0.0200), 5)), Vec3::X, [0.0030, 0.0040], [0.0030, 0.0022], false, STEEL_L);
        b.rbx([k * 0.0214, k * 0.0224], [0.0400, 0.0680], [-0.0850, -0.0200], 0.002, STEEL_L);
        b.cx(0.0530, -0.0500, [k * 0.0212, k * 0.0228], 0.0035, BRASS);
    });
    // forend, trigger guard, cut-down wooden stock with a rounded butt
    b.loft(&[szb(-0.3000, 0., 0.0330, 0.0205, 0.0140, 0.0100, 0.0100), szb(-0.2850, 0., 0.0330, 0.0228, 0.0150, 0.0110, 0.0110), szb(-0.1000, 0., 0.0330, 0.0228, 0.0150, 0.0110, 0.0110)], 3, WOOD);
    b.rbx([-0.0100, 0.0100], [0.0120, 0.0220], [-0.2900, -0.2300], 0.003, GUNMETAL);
    trigger_guard(&mut b, 0.028, 0.030, [-0.085, -0.014], 0.0095, STEEL);
    stock(&mut b, &[(-0.0100, 0.0720, 0.0000, 0.0180), (0.0500, 0.0720, -0.0150, 0.0176), (0.1100, 0.0600, -0.0620, 0.0180)], 0.0080, WOOD);
    b.rbx([-0.0190, 0.0190], [-0.0660, 0.0640], [0.1100, 0.1220], 0.005, STEEL);
    fin(b, anch(Some([0., 0.020, -0.20]), [0., 0.077, 0.06], [0., 0.052, -0.4105], [0.022, 0.065, -0.05]), None, None)
}
// ---------------------------------------------------------------------------------------------------------
// Heavy weapons
// ---------------------------------------------------------------------------------------------------------
/// An ammunition box under the receiver with a short run of belted rounds climbing the left side.
fn belt_box(g: &mut M, z: [f32; 2], bottom: f32, col: C) {
    let dark = [col[0] * 0.6, col[1] * 0.6, col[2] * 0.6];
    g.rbx([-0.048, 0.048], [bottom, 0.030], z, 0.008, col);
    g.rbx([-0.050, 0.050], [0.020, 0.030], [z[0] - 0.002, z[1] + 0.002], 0.004, dark);
    g.rbx([-0.012, 0.012], [bottom - 0.004, bottom], [z[0] + 0.01, z[1] - 0.01], 0.002, STEEL);
    g.rbx([-0.040, 0.040], [bottom + 0.030, bottom + 0.034], [z[0] + 0.005, z[1] - 0.005], 0.0015, dark); // embossed rib
    let zc = (z[0] + z[1]) * 0.5;
    for i in 0..6 {
        let y = 0.034 + i as f32 * 0.0105;
        let x = -0.030 - 0.012 * (1. - i as f32 / 5.);
        g.cx(y, zc, [x - 0.012, -0.0225], 0.0052, BRASS);
        g.rbx([-0.0300, -0.0225], [y - 0.0015, y + 0.0015], [zc - 0.0062, zc + 0.0062], 0.0008, STEEL_L);
        g.rod(vec3(x - 0.012, y, zc), vec3(x - 0.026, y, zc), 0.0052, 0.0020, [0.45, 0.30, 0.15], 0.);
    }
}

fn para() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let blk = [0.21, 0.215, 0.22];
    // receiver with the raised feed cover
    b.loft(&[szb(-0.2600, 0., 0.0625, 0.0220, 0.0325, 0.0080, 0.0060), szb(0.1100, 0., 0.0625, 0.0225, 0.0325, 0.0080, 0.0060)], 3, blk);
    b.loft(&[szb(-0.2200, 0., 0.1035, 0.0195, 0.0085, 0.0070, 0.0020), szb(0.0200, 0., 0.1035, 0.0205, 0.0085, 0.0070, 0.0020)], 3, STEEL);
    notch(&mut b, 0.1275, 0.011, 0.009, 0.007);
    b.rbx([0.0224, 0.0234], [0.0600, 0.0900], [-0.0600, 0.0200], 0.001, GROOVE);
    // handguard, gas tube, barrel, flash hider
    b.loft(&[szb(-0.4000, 0., 0.0540, 0.0200, 0.0240, 0.0120, 0.0100), szb(-0.3800, 0., 0.0540, 0.0228, 0.0240, 0.0130, 0.0100), szb(-0.2600, 0., 0.0540, 0.0230, 0.0240, 0.0130, 0.0100)], 3, POLY);
    slots_both(&mut b, 0.0229, [0.040, 0.068], [-0.370, -0.280], 5, GROOVE);
    b.tz(0., 0.0950, [-0.5000, -0.2600], [0.0078, 0.0078], STEEL);
    b.tz(0., 0.0720, [-0.6150, -0.2600], [0.0098, 0.0098], STEEL);
    b.rbx([-0.0115, 0.0115], [0.0600, 0.1040], [-0.5050, -0.4700], 0.004, GUNMETAL);
    b.tzs(0., 0.0720, &[(-0.6650, 0.0100), (-0.6600, 0.0122), (-0.6200, 0.0122), (-0.6100, 0.0098)], STEEL);
    b.bore(0., 0.0720, -0.6655, 0.0058);
    b.rbx([-0.0015, 0.0015], [0.1040, 0.1345], [-0.4920, -0.4880], 0.0006, STEEL);
    b.sym(|b, k| b.rbx([k * 0.0058, k * 0.0086], [0.1040, 0.1345], [-0.4980, -0.4820], 0.001, GUNMETAL));
    // carry handle
    b.sweep(&yz(0., &bez((0.0780, -0.4400), (0.2000, -0.3800), (0.0780, -0.3200), 8)), Vec3::X, [0.0105; 2], [0.0070; 2], false, STEEL);
    // folded bipod
    b.sym(|b, k| b.sweep(&[vec3(k * 0.022, 0.050, -0.500), vec3(k * 0.022, 0.046, -0.640)], Vec3::X, [0.0045; 2], [0.0045; 2], true, STEEL_L));
    // grip, guard, hollow stock: top and bottom beams, rounded butt
    pgrip(&mut b, vec3(0., 0.0320, -0.0170), vec3(0., -0.0920, 0.0400), 0.0165, 0.0215, POLY);
    trigger_guard(&mut b, 0.030, 0.030, [-0.075, 0.004], 0.0105, POLY);
    b.loft(&[szb(0.1100, 0., 0.0710, 0.0155, 0.0210, 0.0100, 0.0050), szb(0.3900, 0., 0.0710, 0.0155, 0.0210, 0.0100, 0.0050)], 3, POLY);
    b.sweep(&yz(0., &bez((0.0300, 0.1100), (-0.0650, 0.2800), (-0.0720, 0.3850), 10)), Vec3::X, [0.0150; 2], [0.0090; 2], false, POLY);
    b.rbx([-0.0195, 0.0195], [-0.0850, 0.0920], [0.3830, 0.3980], 0.006, [0.06, 0.06, 0.065]);
    swivel(&mut b, 0., 0.030, -0.350);
    belt_box(&mut g, [-0.20, -0.07], -0.085, [0.20, 0.26, 0.17]);
    s.rbx([0.0225, 0.0460], [0.0700, 0.0800], [-0.1420, -0.1220], 0.003, STEEL_L);
    s.rbx([0.0460, 0.0560], [0.0620, 0.0880], [-0.1470, -0.1170], 0.004, STEEL_L);
    fin(
        b,
        anch(Some([0., 0.030, -0.33]), [0., 0.136, 0.10], [0., 0.072, -0.6655], [0.0233, 0.075, -0.05]),
        Some((g, [0., 0.030, -0.135])),
        Some((s, [0., 0., 0.06])),
    )
}

fn pk() -> WeaponModel {
    let (mut b, mut g, mut s) = (M::new(), M::new(), M::new());
    let blk = [0.20, 0.205, 0.21];
    // receiver with its hump-backed cover
    b.loft(&[szb(-0.2500, 0., 0.0575, 0.0220, 0.0375, 0.0090, 0.0060), szb(0.1400, 0., 0.0575, 0.0225, 0.0375, 0.0090, 0.0060)], 3, blk);
    b.loft(&[szb(-0.1800, 0., 0.1065, 0.0185, 0.0115, 0.0090, 0.0020), szb(0.0500, 0., 0.1065, 0.0212, 0.0115, 0.0100, 0.0020)], 3, STEEL);
    notch(&mut b, 0.1335, 0.032, 0.009, 0.007);
    b.rbx([0.0224, 0.0234], [0.0500, 0.0850], [-0.0800, 0.0400], 0.001, GROOVE);
    // fluted heavy barrel, gas tube below, flash hider
    b.tz(0., 0.0720, [-0.7400, -0.2500], [0.0115, 0.0115], STEEL);
    for i in 0..5 {
        b.tz(0., 0.0720, [-0.4000 - i as f32 * 0.0600, -0.4100 - i as f32 * 0.0600], [0.0138, 0.0138], GUNMETAL);
    }
    b.tz(0., 0.0450, [-0.5200, -0.2500], [0.0105, 0.0105], STEEL_L);
    b.rbx([-0.0125, 0.0125], [0.0360, 0.0980], [-0.5550, -0.5050], 0.004, GUNMETAL);
    b.tzs(0., 0.0720, &[(-0.7750, 0.0120), (-0.7700, 0.0148), (-0.7400, 0.0148), (-0.7350, 0.0115)], STEEL);
    b.bore(0., 0.0720, -0.7755, 0.0060);
    b.rbx([-0.0015, 0.0015], [0.0830, 0.1355], [-0.7280, -0.7220], 0.0006, STEEL);
    b.sym(|b, k| b.rbx([k * 0.0058, k * 0.0086], [0.0830, 0.1355], [-0.7320, -0.7180], 0.001, GUNMETAL));
    // carry handle
    b.sweep(&yz(0., &bez((0.0840, -0.6300), (0.2100, -0.5800), (0.0840, -0.5450), 8)), Vec3::X, [0.0105; 2], [0.0070; 2], false, STEEL);
    // folded bipod
    b.sym(|b, k| b.sweep(&[vec3(k * 0.024, 0.040, -0.520), vec3(k * 0.024, 0.036, -0.700)], Vec3::X, [0.0048; 2], [0.0048; 2], true, STEEL_L));
    // pistol grip, guard and the wooden skeleton stock
    pgrip(&mut b, vec3(0., 0.0320, -0.0170), vec3(0., -0.0920, 0.0410), 0.0165, 0.0215, WOOD_D);
    trigger_guard(&mut b, 0.020, 0.040, [-0.075, 0.004], 0.0100, STEEL);
    b.loft(&[sz(0.1400, 0., 0.0770, 0.0160, 0.0180, 0.0070), sz(0.4000, 0., 0.0630, 0.0160, 0.0220, 0.0070)], 3, WOOD);
    b.sweep(&yz(0., &bez((-0.0650, 0.0350), (-0.0950, 0.2600), (-0.0600, 0.3900), 10)), Vec3::X, [0.0090; 2], [0.0130; 2], false, WOOD_D);
    b.rbx([-0.0170, 0.0170], [-0.0920, 0.0850], [0.3850, 0.4020], 0.005, WOOD);
    b.rbx([-0.0175, 0.0175], [-0.0950, 0.0600], [0.4020, 0.4100], 0.003, POLY);
    belt_box(&mut g, [-0.17, -0.06], -0.075, [0.17, 0.20, 0.15]);
    s.rbx([0.0225, 0.0500], [0.0640, 0.0740], [-0.0600, -0.0400], 0.003, STEEL_L);
    s.rbx([0.0500, 0.0600], [0.0560, 0.0820], [-0.0650, -0.0350], 0.004, STEEL_L);
    fin(
        b,
        anch(Some([0., 0.030, -0.34]), [0., 0.140, 0.10], [0., 0.072, -0.7755], [0.0233, 0.070, 0.0]),
        Some((g, [0., 0.030, -0.115])),
        Some((s, [0., 0., 0.06])),
    )
}

fn rpg() -> WeaponModel {
    let (mut b, mut g) = (M::new(), M::new());
    let tube = [0.24, 0.27, 0.19];
    let y = 0.085;
    // launch tube, muzzle collar and the flared rear venturi
    b.tzs(0., y, &[(-0.2680, 0.0232), (-0.2550, 0.0232), (-0.2530, 0.0205), (0.3600, 0.0205), (0.3800, 0.0250), (0.4000, 0.0340), (0.4120, 0.0405), (0.4220, 0.0410)], tube);
    b.tz(0., y, [0.4120, 0.4220], [0.0415, 0.0415], STEEL);
    // wooden heat shield with its straps, trigger group, grip
    b.tz(0., y, [-0.2200, -0.0600], [0.0225, 0.0225], WOOD);
    b.tz(0., y, [-0.2220, -0.2180], [0.0233, 0.0233], WOOD_D);
    b.tz(0., y, [-0.0640, -0.0600], [0.0233, 0.0233], WOOD_D);
    b.rbx([-0.0140, 0.0140], [0.0280, 0.0655], [-0.0500, 0.0350], 0.004, STEEL);
    trigger_guard(&mut b, 0.030, 0.030, [-0.065, 0.004], 0.0095, STEEL);
    pgrip(&mut b, vec3(0., 0.0340, -0.0200), vec3(0., -0.0900, 0.0440), 0.0165, 0.0215, WOOD_D);
    // PGO-7 optical sight on the left
    b.rbx([-0.0300, -0.0180], [0.0780, 0.0920], [-0.0200, 0.0600], 0.003, STEEL);
    b.rbx([-0.0400, -0.0300], [0.0920, 0.1060], [-0.0200, 0.0600], 0.003, STEEL);
    let dk = [0.05, 0.05, 0.055];
    b.tz(-0.0470, 0.1180, [-0.0300, 0.1100], [0.0145, 0.0145], dk);
    b.tzs(-0.0470, 0.1180, &[(-0.0300, 0.0145), (-0.0450, 0.0170), (-0.0650, 0.0220)], dk);
    b.lens(-0.0470, 0.1180, [-0.0670, -0.0640], 0.0195);
    b.tzs(-0.0470, 0.1180, &[(0.1100, 0.0145), (0.1350, 0.0200)], dk);
    b.lens(-0.0470, 0.1180, [0.1360, 0.1340], 0.0115);
    // the loaded rocket: finned booster, then the bulbous warhead with its ogive and nose probe
    let gc = [0.30, 0.31, 0.27];
    let wc = [0.27, 0.32, 0.19];
    g.tzs(0., y, &[(-0.1200, 0.0185), (-0.3000, 0.0185)], gc);
    g.tzs(0., y, &[(-0.3000, 0.0185), (-0.3200, 0.0300), (-0.3450, 0.0410), (-0.4000, 0.0425), (-0.4250, 0.0400), (-0.4500, 0.0330), (-0.4750, 0.0215), (-0.4950, 0.0130), (-0.5100, 0.0098)], wc);
    g.tz(0., y, [-0.3560, -0.3640], [0.0432, 0.0432], BRASS);
    g.tzs(0., y, &[(-0.5100, 0.0098), (-0.5300, 0.0085), (-0.5300, 0.0010)], STEEL_L);
    for k in 0..4 {
        let a = k as f32 * FRAC_PI_2;
        g.rbx([a.cos().abs() * 0.0235 - 0.0006, a.cos().abs() * 0.0235 + 0.0006], [y - 0.0006 + 0.0, y + 0.0006], [-0.18, -0.12], 0.0003, gc);
    }
    g.rbx([-0.0006, 0.0006], [y - 0.0235, y + 0.0235], [-0.18, -0.12], 0.0003, gc);
    g.rbx([-0.0235, 0.0235], [y - 0.0006, y + 0.0006], [-0.18, -0.12], 0.0003, gc);
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
    b.tzs(0., 0.0600, &[(-0.4350, 0.0262), (-0.4300, 0.0240), (-0.0300, 0.0240)], STEEL);
    b.tzs(0., 0.0600, &[(-0.4350, 0.0262), (-0.4150, 0.0262)], GUNMETAL);
    b.bore(0., 0.0600, -0.4355, 0.0190);
    b.rbx([-0.0015, 0.0015], [0.0830, 0.1060], [-0.4080, -0.4020], 0.0006, STEEL);
    b.rbx([-0.0095, 0.0095], [0.0780, 0.0900], [-0.0950, -0.0550], 0.003, GUNMETAL);
    b.sym(|b, k| b.rbx([k * 0.0030, k * 0.0055], [0.0900, 0.1500], [-0.0850, -0.0780], 0.001, STEEL));
    b.rbx([-0.0055, 0.0055], [0.1300, 0.1360], [-0.0850, -0.0780], 0.001, STEEL);
    // receiver, hammer, forearm
    b.loft(&[szb(-0.0500, 0., 0.0540, 0.0222, 0.0240, 0.0100, 0.0080), szb(0.0400, 0., 0.0540, 0.0225, 0.0240, 0.0100, 0.0080)], 3, GUNMETAL);
    b.sweep(&yz(0., &bez((0.0740, 0.0300), (0.0850, 0.0400), (0.0960, 0.0500), 5)), Vec3::X, [0.0035, 0.0045], [0.0030, 0.0022], false, STEEL_L);
    b.loft(&[szb(-0.2600, 0., 0.0340, 0.0200, 0.0140, 0.0100, 0.0100), szb(-0.2450, 0., 0.0340, 0.0230, 0.0145, 0.0110, 0.0110), szb(-0.0500, 0., 0.0340, 0.0230, 0.0145, 0.0110, 0.0110)], 3, WOOD);
    b.rbx([-0.0100, 0.0100], [0.0120, 0.0220], [-0.2600, -0.2000], 0.003, STEEL);
    trigger_guard(&mut b, 0.030, 0.030, [-0.075, -0.004], 0.0095, STEEL);
    // walnut stock with a rounded heel
    stock(&mut b, &[(0.0000, 0.0780, 0.0000, 0.0185), (0.1000, 0.0740, -0.0200, 0.0182), (0.2600, 0.0660, -0.0520, 0.0188), (0.3000, 0.0500, -0.0550, 0.0190)], 0.0080, WOOD);
    b.rbx([-0.0195, 0.0195], [-0.0580, 0.0520], [0.2980, 0.3100], 0.005, STEEL);
    fin(b, anch(Some([0., 0.020, -0.15]), [0., 0.143, 0.05], [0., 0.060, -0.4355], [0.0226, 0.060, 0.0]), None, None)
}

// ---------------------------------------------------------------------------------------------------------
// Grenades: upright, origin at the middle of the body, fuze up, the spoon on the left
// ---------------------------------------------------------------------------------------------------------
impl M {
    /// Round body of revolution standing on the Y axis: (height, radius) stations, smooth.
    fn body_y(&mut self, st: &[(f32, f32)], c: C) {
        let secs: Vec<Sec> = st.iter().map(|s| sz(s.0, 0., 0., s.1, s.1, 0.)).collect();
        self.t.append(&loft_mesh(&secs, Ring::Ell(18), true, c, 0.).transformed(Mat4::from_rotation_x(-FRAC_PI_2)));
    }
}

/// The pull pin and its ring (the ring lies in the YZ plane so it reads from the sides), at height `y`.
fn pin_ring(m: &mut M, y: f32, z: f32) {
    m.cx(y, z, [-0.0050, 0.0110], 0.0016, STEEL_L);
    m.loop_yz(0.0110, y, z + 0.0112, 0.0112, 0.0014, CHROME);
}

/// A curved spoon (safety lever) hugging a body: path in the XY plane, strip `w` wide.
fn spoon(m: &mut M, pts: &[(f32, f32)], w: f32) {
    let path: Vec<Vec3> = pts.iter().map(|p| vec3(p.0, p.1, 0.)).collect();
    m.sweep(&path, Vec3::Z, [w, w], [0.0013, 0.0013], false, STEEL_L);
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
    b.t.ball(vec3(0., 0., 0.), vec3(0.0315, 0.0335, 0.0315), ol, 0., 18, 12);
    // segmented seam grooves around the body and the stencilled yellow band
    for y in [-0.0180, 0.0180] {
        let r = 0.0315 * (1. - (y / 0.0335f32).powi(2)).sqrt() + 0.0003;
        b.body_y(&[(y - 0.0016, r), (y + 0.0016, r)], [0.14, 0.17, 0.09]);
    }
    b.body_y(&[(-0.0050, 0.0320), (0.0050, 0.0320)], [0.80, 0.72, 0.14]);
    // neck, fuze body and the safety lever over it
    b.body_y(&[(0.0280, 0.0125), (0.0300, 0.0125), (0.0340, 0.0120)], STEEL_L);
    b.body_y(&[(0.0300, 0.0118), (0.0500, 0.0118), (0.0520, 0.0128), (0.0590, 0.0128), (0.0600, 0.0100)], STEEL_L);
    b.body_y(&[(0.0500, 0.0135), (0.0535, 0.0135)], GUNMETAL);
    spoon(
        &mut b,
        &[(0.0030, 0.0620), (-0.0090, 0.0618), (-0.0145, 0.0590), (-0.0150, 0.0420), (-0.0175, 0.0310), (-0.0240, 0.0215), (-0.0300, 0.0090), (-0.0338, -0.0030), (-0.0330, -0.0150), (-0.0280, -0.0235)],
        0.0055,
    );
    pin_ring(&mut b, 0.0400, 0.0);
    grenade(b, 0.096)
}

fn flash() -> WeaponModel {
    let mut b = M::new();
    let al = [0.52, 0.54, 0.57];
    let dk = [0.08, 0.08, 0.09];
    b.body_y(&[(-0.0500, 0.0245), (-0.0475, 0.0265), (-0.0420, 0.0268), (0.0270, 0.0268), (0.0295, 0.0258), (0.0305, 0.0235)], al);
    b.body_y(&[(-0.0545, 0.0225), (-0.0525, 0.0272), (-0.0470, 0.0274), (-0.0460, 0.0272)], dk);
    b.body_y(&[(0.0300, 0.0272), (0.0345, 0.0274), (0.0360, 0.0262), (0.0370, 0.0236)], dk);
    for y in [-0.0380, 0.0220] {
        b.body_y(&[(y, 0.0270), (y + 0.0055, 0.0270)], [0.25, 0.26, 0.28]);
    }
    b.body_y(&[(-0.0120, 0.0270), (-0.0040, 0.0270)], [0.85, 0.78, 0.15]); // warning band
    b.body_y(&[(0.0360, 0.0185), (0.0500, 0.0185)], STEEL);
    b.body_y(&[(0.0500, 0.0205), (0.0550, 0.0205)], GUNMETAL);
    spoon(&mut b, &[(0.0000, 0.0590), (-0.0150, 0.0588), (-0.0240, 0.0555), (-0.0290, 0.0460), (-0.0292, 0.0300), (-0.0292, -0.0200)], 0.0055);
    pin_ring(&mut b, 0.0435, 0.0);
    grenade(b, 0.107)
}

fn smoke() -> WeaponModel {
    let mut b = M::new();
    let body = [0.42, 0.44, 0.40];
    let dk = [0.10, 0.10, 0.11];
    b.body_y(&[(-0.0500, 0.0290), (-0.0480, 0.0320), (-0.0420, 0.0325), (0.0360, 0.0325), (0.0395, 0.0310), (0.0400, 0.0285)], body);
    b.body_y(&[(-0.0545, 0.0270), (-0.0525, 0.0335), (-0.0480, 0.0338), (-0.0470, 0.0334)], dk);
    b.body_y(&[(0.0400, 0.0335), (0.0450, 0.0338), (0.0470, 0.0320), (0.0480, 0.0290)], dk);
    b.body_y(&[(0.0020, 0.0329), (0.0180, 0.0329)], [0.88, 0.76, 0.10]);
    for i in 0..3 {
        let a = i as f32 * TAU / 3.;
        b.cy(a.cos() * 0.021, a.sin() * 0.021, [0.0470, 0.0495], 0.0042, [0.02, 0.02, 0.02]);
    }
    b.body_y(&[(0.0480, 0.0125), (0.0620, 0.0125)], STEEL);
    b.body_y(&[(0.0620, 0.0148), (0.0670, 0.0148)], GUNMETAL);
    spoon(&mut b, &[(0.0000, 0.0710), (-0.0150, 0.0705), (-0.0260, 0.0670), (-0.0335, 0.0560), (-0.0340, 0.0400), (-0.0340, -0.0300)], 0.0055);
    pin_ring(&mut b, 0.0540, 0.0);
    grenade(b, 0.121)
}

fn incen() -> WeaponModel {
    let mut b = M::new();
    let red = [0.66, 0.12, 0.08];
    let dk = [0.12, 0.12, 0.13];
    b.body_y(&[(-0.0480, 0.0280), (-0.0460, 0.0302), (-0.0400, 0.0306), (0.0300, 0.0306), (0.0335, 0.0292), (0.0340, 0.0270)], red);
    b.body_y(&[(-0.0525, 0.0255), (-0.0505, 0.0316), (-0.0480, 0.0318)], [0.10, 0.10, 0.11]);
    b.body_y(&[(0.0340, 0.0318), (0.0400, 0.0318), (0.0430, 0.0300), (0.0440, 0.0270)], dk);
    for y in [0.0060, -0.0200] {
        b.body_y(&[(y, 0.0311), (y + 0.0100, 0.0311)], dk);
    }
    for i in 0..8 {
        let a = i as f32 * TAU / 8.;
        b.cy(a.cos() * 0.0245, a.sin() * 0.0245, [0.0436, 0.0450], 0.0034, [0.9, 0.55, 0.1]);
    }
    b.body_y(&[(0.0440, 0.0120), (0.0580, 0.0120)], STEEL);
    b.body_y(&[(0.0580, 0.0142), (0.0630, 0.0142)], GUNMETAL);
    spoon(&mut b, &[(0.0000, 0.0670), (-0.0150, 0.0665), (-0.0250, 0.0630), (-0.0325, 0.0520), (-0.0335, 0.0360), (-0.0335, -0.0290)], 0.0055);
    pin_ring(&mut b, 0.0500, 0.0);
    grenade(b, 0.115)
}

// ---------------------------------------------------------------------------------------------------------
// Melee: the blade points along -Z, the origin is the middle of the handle (the axe and crowbar: the hand)
// ---------------------------------------------------------------------------------------------------------
impl M {
    /// A flat tool plate lying in the YZ plane: outline points (y, z, half thickness along X) skinned as two
    /// bevelled faces (thick where `t` is large, an edge where it is near zero).
    fn plate(&mut self, pts: &[(f32, f32, f32)], c: C) {
        let n = pts.len();
        if n < 3 {
            return;
        }
        let (cy, cz) = (pts.iter().map(|p| p.0).sum::<f32>() / n as f32, pts.iter().map(|p| p.1).sum::<f32>() / n as f32);
        let ct = pts.iter().map(|p| p.2).sum::<f32>() / n as f32;
        let p3 = |s: f32, y: f32, z: f32, t: f32| vec3(s * t, y, z);
        let mut tri = |m: &mut M, a: Vec3, b: Vec3, c2: Vec3, out: Vec3| {
            let mut nrm = (b - a).cross(c2 - a);
            if nrm.length_squared() < 1e-16 {
                return;
            }
            nrm = nrm.normalize();
            let (b, c2) = if nrm.dot(out) < 0. { (c2, b) } else { (b, c2) };
            let nrm = if nrm.dot(out) < 0. { -nrm } else { nrm };
            let base = m.t.verts.len() as u16;
            for p in [a, b, c2] {
                m.t.verts.push(Vert { p, n: nrm, c, e: 0., a: 1. });
            }
            m.t.idx.extend_from_slice(&[base, base + 1, base + 2]);
        };
        for s in [1., -1.] {
            for i in 0..n {
                let (p, q) = (pts[i], pts[(i + 1) % n]);
                tri(self, p3(s, cy, cz, ct), p3(s, p.0, p.1, p.2), p3(s, q.0, q.1, q.2), vec3(s, 0., 0.));
            }
        }
        for i in 0..n {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            let out = vec3(0., (p.0 + q.0) * 0.5 - cy, (p.1 + q.1) * 0.5 - cz);
            tri(self, p3(1., p.0, p.1, p.2), p3(-1., p.0, p.1, p.2), p3(1., q.0, q.1, q.2), out);
            tri(self, p3(-1., p.0, p.1, p.2), p3(-1., q.0, q.1, q.2), p3(1., q.0, q.1, q.2), out);
        }
    }
}

fn melee(body: M, muzzle: Off) -> WeaponModel {
    fin(body, anch(None, [0., 0.05, -0.1], muzzle, [0., 0., 0.]), None, None)
}

fn knife() -> WeaponModel {
    let mut b = M::new();
    // stacked leather washers with a palm swell, steel pommel and guard
    for i in 0..8 {
        let z0 = -0.060 + i as f32 * 0.0150;
        let r = 0.0116 + 0.0012 * (1. - ((i as f32 - 3.5) / 3.5).powi(2));
        let c = if i % 2 == 0 { WOOD } else { WOOD_D };
        b.lofte(&[sz(z0, 0., 0., r * 1.04, r, 0.), sz(z0 + 0.0150, 0., 0., r * 1.04, r, 0.)], 14, c);
    }
    b.lofte(
        &[sz(0.0600, 0., 0., 0.0125, 0.0120, 0.), sz(0.0650, 0., 0., 0.0142, 0.0138, 0.), sz(0.0720, 0., 0., 0.0140, 0.0136, 0.), sz(0.0770, 0., 0., 0.0100, 0.0098, 0.), sz(0.0792, 0., 0., 0.0050, 0.0050, 0.)],
        14,
        STEEL_L,
    );
    b.loft(&[sz(-0.0720, 0., 0.0040, 0.0070, 0.0245, 0.0030), sz(-0.0600, 0., 0.0040, 0.0070, 0.0245, 0.0030)], 2, GUNMETAL);
    // clip-point blade: thick spine, hollow-ground bevel, fuller
    b.plate(
        &[
            (0.0160, -0.0720, 0.0023),
            (0.0160, -0.1600, 0.0023),
            (0.0128, -0.1950, 0.0017),
            (0.0045, -0.2320, 0.0004),
            (-0.0040, -0.2250, 0.0003),
            (-0.0120, -0.2020, 0.0002),
            (-0.0165, -0.1650, 0.0002),
            (-0.0172, -0.1200, 0.0002),
            (-0.0165, -0.0720, 0.0004),
        ],
        [0.34, 0.35, 0.38],
    );
    b.bx([-0.0026, 0.0026], [0.0050, 0.0075], [-0.1750, -0.0900], [0.07, 0.07, 0.08]);
    b.plate(&[(-0.0172, -0.1200, 0.0005), (-0.0165, -0.1650, 0.0005), (-0.0120, -0.2020, 0.0005), (-0.0040, -0.2250, 0.0005), (-0.0100, -0.2000, 0.0012), (-0.0120, -0.1200, 0.0012)], CHROME);
    melee(b, [0., 0.004, -0.232])
}

fn machete() -> WeaponModel {
    let mut b = M::new();
    // polymer handle with finger swells, rivets and a flared butt
    b.loft(
        &[
            sz(-0.0500, 0., -0.0020, 0.0100, 0.0195, 0.0060),
            sz(-0.0380, 0., -0.0020, 0.0125, 0.0200, 0.0060),
            sz(-0.0240, 0., -0.0020, 0.0132, 0.0212, 0.0065),
            sz(-0.0100, 0., -0.0020, 0.0128, 0.0200, 0.0062),
            sz(0.0060, 0., -0.0020, 0.0134, 0.0215, 0.0066),
            sz(0.0180, 0., -0.0020, 0.0130, 0.0205, 0.0062),
            sz(0.0600, 0., -0.0020, 0.0136, 0.0225, 0.0066),
            sz(0.0820, 0., -0.0020, 0.0148, 0.0248, 0.0070),
            sz(0.0900, 0., -0.0020, 0.0140, 0.0240, 0.0066),
        ],
        2,
        POLY,
    );
    for z in [-0.025, 0.020, 0.062] {
        b.cx(-0.002, z, [-0.0140, 0.0140], 0.0033, BRASS);
    }
    b.cx(-0.012, 0.075, [-0.0142, 0.0142], 0.0042, GROOVE); // lanyard hole
    b.loft(&[sz(-0.0580, 0., 0.0030, 0.0070, 0.0270, 0.0030), sz(-0.0500, 0., 0.0030, 0.0070, 0.0270, 0.0030)], 2, GUNMETAL);
    b.plate(
        &[
            (0.0220, -0.0580, 0.0020),
            (0.0260, -0.2000, 0.0018),
            (0.0300, -0.3000, 0.0018),
            (0.0300, -0.4200, 0.0016),
            (0.0180, -0.4800, 0.0010),
            (0.0000, -0.5000, 0.0003),
            (-0.0160, -0.4700, 0.0002),
            (-0.0220, -0.4000, 0.0002),
            (-0.0210, -0.2800, 0.0002),
            (-0.0180, -0.1500, 0.0002),
            (-0.0140, -0.0580, 0.0004),
        ],
        [0.22, 0.23, 0.25],
    );
    b.plate(&[(-0.0140, -0.0580, 0.0005), (-0.0180, -0.1500, 0.0005), (-0.0210, -0.2800, 0.0005), (-0.0220, -0.4000, 0.0005), (-0.0160, -0.4700, 0.0005), (-0.0000, -0.4950, 0.0005), (-0.0050, -0.4000, 0.0016), (-0.0070, -0.0580, 0.0016)], CHROME);
    melee(b, [0., 0.0, -0.50])
}

fn axe() -> WeaponModel {
    let mut b = M::new();
    let haft = WOOD_L;
    // oval haft: slim shaft, swelling grip, flared knob
    b.lofte(
        &[sz(-0.7400, 0., 0., 0.0150, 0.0175, 0.), sz(-0.5000, 0., 0., 0.0145, 0.0185, 0.), sz(-0.2000, 0., 0., 0.0150, 0.0190, 0.), sz(0.0000, 0., 0., 0.0158, 0.0198, 0.), sz(0.0600, 0., 0., 0.0185, 0.0218, 0.), sz(0.0950, 0., 0., 0.0150, 0.0160, 0.), sz(0.1050, 0., 0., 0.0050, 0.0050, 0.)],
        14,
        haft,
    );
    b.tz(0., 0., [-0.1200, 0.0400], [0.0190, 0.0190], POLY);
    // head: eye block, flared wedge with a curved edge, pick
    let red = RED;
    b.rbx([-0.0150, 0.0150], [-0.0270, 0.0340], [-0.7500, -0.6800], 0.004, red);
    b.plate(
        &[
            (0.0300, -0.6900, 0.0110),
            (0.0300, -0.7450, 0.0110),
            (-0.0200, -0.7620, 0.0100),
            (-0.0750, -0.7800, 0.0050),
            (-0.0750, -0.6420, 0.0050),
            (-0.0200, -0.6620, 0.0100),
        ],
        red,
    );
    b.plate(
        &[
            (-0.0750, -0.7800, 0.0050),
            (-0.1000, -0.7880, 0.0020),
            (-0.1100, -0.7600, 0.0006),
            (-0.1135, -0.7150, 0.0004),
            (-0.1100, -0.6700, 0.0006),
            (-0.1000, -0.6400, 0.0020),
            (-0.0750, -0.6420, 0.0050),
        ],
        CHROME,
    );
    b.plate(&[(0.0300, -0.7440, 0.0085), (0.0300, -0.6860, 0.0085), (0.0750, -0.6800, 0.0045), (0.1080, -0.6720, 0.0006), (0.1000, -0.7000, 0.0010), (0.0600, -0.7350, 0.0045)], red);
    melee(b, [0., -0.09, -0.72])
}

fn crowbar() -> WeaponModel {
    let mut b = M::new();
    let st = [0.30, 0.32, 0.37];
    b.tz(0., 0., [-0.3200, 0.2700], [0.0112, 0.0112], st);
    b.tz(0., 0., [-0.1200, 0.1000], [0.0122, 0.0122], RED);
    // hooked end: the shaft bends through a curve and flattens into the claw
    let mut hook = bez((0., -0.3200), (0., -0.4050), (-0.0620, -0.4080), 8);
    hook.extend(bez((-0.0620, -0.4080), (-0.0900, -0.4090), (-0.1010, -0.4000), 4).into_iter().skip(1));
    b.sweep(&yz(0., &hook), Vec3::X, [0.0112, 0.0085], [0.0112, 0.0090], true, st);
    b.plate(
        &[
            (-0.0900, -0.4170, 0.0075),
            (-0.0900, -0.3930, 0.0075),
            (-0.1090, -0.3800, 0.0040),
            (-0.1280, -0.3610, 0.0010),
            (-0.1120, -0.3920, 0.0015),
            (-0.1020, -0.4000, 0.0030),
            (-0.1120, -0.4080, 0.0015),
            (-0.1300, -0.4190, 0.0010),
            (-0.1100, -0.4290, 0.0040),
        ],
        CHROME,
    );
    // flat chisel end
    b.loft(&[sz(0.2700, 0., 0., 0.0112, 0.0112, 0.0112), sz(0.3000, 0., 0., 0.0145, 0.0050, 0.0030), sz(0.3350, 0., 0., 0.0150, 0.0040, 0.0020), sz(0.3500, 0., 0., 0.0125, 0.0030, 0.0015)], 2, CHROME);
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
            assert!(all.verts.len() < 6000, "{k}: {} vertices", all.verts.len());
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
