//! First-person arms: sleeves, cuffs and gloved hands gripping a weapon, in WEAPON-LOCAL space (origin at
//! the firing hand's grip, -Z out of the muzzle), so they can be drawn with the same matrix as the weapon.
//!
//! The hands are built from rounded primitives in a canonical frame (see [`hand`]): the palm is the XZ plane, the
//! fingers point along -Z and curl towards -Y (the palm side), the thumb sits on the -X side of a right hand and
//! on the +X side of a left hand, the wrist is on +Z. Each pose gives the hand a position, the direction of the
//! wrist (towards the elbow) and the direction its thumb points, and a [`Fist`] with the finger curl.
use super::anchors::WeaponAnchors;
use super::character::{keys, limb, magwell_of, smooth, Hold, Palette};
use crate::team::Team;
use macroquad::prelude::*;
use vesper3d::viewer::kit::Template;

type Rgb = [f32; 3];

/// The animation state the arms need.
#[derive(Clone, Copy, Debug)]
pub struct ArmPose {
    /// 0 none, else 0..1 progress of the reload.
    pub reload: f32,
    /// Melee swing 0..1.
    pub swing: f32,
    /// Grenade throw 0..1.
    pub throwing: f32,
    /// 0 lowered .. 1 ready (weapon switching).
    pub draw: f32,
    /// 0..1 aiming down the sights.
    pub ads: f32,
    /// Grenade: the pin is out (the support hand holds the ring away from the grenade).
    pub pin: bool,
}

impl Default for ArmPose {
    fn default() -> Self {
        ArmPose { reload: 0., swing: 0., throwing: 0., draw: 1., ads: 0., pin: false }
    }
}

fn f(x: f32, d: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        d
    }
}

fn shade(c: Rgb, k: f32) -> Rgb {
    [(c[0] * k).min(1.), (c[1] * k).min(1.), (c[2] * k).min(1.)]
}

// ---------------------------------------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------------------------------------

/// A rounded tapered segment from `a` (radius `ra`) to `b` (radius `rb`).
fn seg(t: &mut Template, a: Vec3, b: Vec3, ra: f32, rb: f32, c: Rgb, sides: usize) {
    let len = (b - a).length();
    if len < 1e-5 {
        return;
    }
    let mut p = Template::new();
    p.cone(vec3(0., -len, 0.), rb, ra, len, c, 0., sides);
    t.append(&p.transformed(limb(a, b)));
}

fn blob(t: &mut Template, p: Vec3, r: Vec3, c: Rgb) {
    t.ball(p, r, c, 0., 6, 4);
}

// ---------------------------------------------------------------------------------------------------------
// The hand
// ---------------------------------------------------------------------------------------------------------

/// How the fingers and the thumb are bent. Finger angles are the flexion at the three joints (radians),
/// fingers ordered index, middle, ring, little.
#[derive(Clone, Copy, Debug)]
struct Fist {
    ang: [[f32; 3]; 4],
    splay: [f32; 4],
    /// Thumb: yaw out from the hand (rad), pitch towards the palm, extra bend at the second and third joint.
    thumb: [f32; 4],
}

impl Fist {
    /// A grip of tightness `k` (0 flat hand, 1 wrapped round a pistol grip, 1.3 a full fist).
    fn grip(k: f32, thumb: [f32; 4]) -> Fist {
        let base = [[0.95, 1.35, 0.85], [1.05, 1.5, 0.9], [1.1, 1.55, 0.95], [1.15, 1.6, 0.95]];
        let mut ang = base;
        for row in ang.iter_mut() {
            for a in row.iter_mut() {
                *a *= k;
            }
        }
        Fist { ang, splay: [0.04, 0.0, -0.03, -0.08], thumb }
    }

    /// Fingers climbing the side of a handguard from below (palm up), tips curling in: the support hand's C.
    fn clamp(k: f32, thumb: [f32; 4]) -> Fist {
        let mut r = Fist::grip(1.0, thumb);
        let base = [[1.30, 0.85, 0.50], [1.35, 0.90, 0.50], [1.40, 0.95, 0.50], [1.40, 1.00, 0.55]];
        for (row, original) in r.ang.iter_mut().zip(base) {
            for (angle, value) in row.iter_mut().zip(original) {
                *angle = value * k;
            }
        }
        r
    }

    /// The same grip with the index finger laid out along the frame (`trigger` 0 straight .. 1 on the trigger).
    fn with_index(mut self, trigger: f32) -> Fist {
        let straight = [0.04, 0.03, 0.02];
        let on = [0.55, 0.85, 0.45];
        for i in 0..3 {
            self.ang[0][i] = straight[i] + (on[i] - straight[i]) * trigger;
        }
        self.splay[0] = 0.0;
        self
    }

    fn lerp(self, o: Fist, t: f32) -> Fist {
        let mut r = self;
        for i in 0..4 {
            for j in 0..3 {
                r.ang[i][j] += (o.ang[i][j] - self.ang[i][j]) * t;
            }
            r.splay[i] += (o.splay[i] - self.splay[i]) * t;
            r.thumb[i] += (o.thumb[i] - self.thumb[i]) * t;
        }
        r
    }
}

/// Thumb positions.
const THUMB_FWD: [f32; 4] = [0.12, 0.10, 0.25, 0.15];
const THUMB_PISTOL: [f32; 4] = [0.30, 0.85, 0.15, 0.15];
const THUMB_WRAP: [f32; 4] = [0.40, 0.95, 0.45, 0.35];
const THUMB_OVER: [f32; 4] = [0.35, 0.55, 0.25, 0.30];
const THUMB_OPEN: [f32; 4] = [0.75, 0.15, 0.10, 0.05];

/// Where the object a grip closes around sits, in the hand's own frame (about 2 cm below the knuckles).
const GRASP: Vec3 = Vec3::new(0., -0.026, -0.054);
/// The same for a wide handguard held in a C from below.
const CLAMP_GRASP: Vec3 = Vec3::new(0., -0.044, -0.015);
/// Wrist end of the glove's cuff, in the hand's frame.
const CUFF_END: Vec3 = Vec3::new(0., 0.004, 0.098);

/// A padded tactical glove on a hand (palm centre at the origin), built for a right hand, mirrored for a left.
fn hand(pal: &Palette, left: bool, fist: &Fist) -> Template {
    let ts = if left { 1. } else { -1. }; // x of the thumb side
    let x = |u: f32| ts * u; // u > 0 towards the thumb
    let c = pal.glove;
    let cd = pal.glove_dark;
    let cl = shade(pal.glove, 1.08);
    let mut t = Template::new();

    // palm with a domed back of the hand
    t.ball(vec3(0., -0.002, 0.006), vec3(0.041, 0.015, 0.050), c, 0., 12, 8);
    t.ball(vec3(x(-0.002), 0.005, 0.016), vec3(0.034, 0.012, 0.040), c, 0., 12, 8);
    t.ball(vec3(x(0.0), 0.0155, 0.014), vec3(0.021, 0.0048, 0.025), cd, 0., 10, 6); // padded back plate
                                                                                    // padded palm panel, heel (little-finger side) pad and thumb ball
    t.ball(vec3(x(0.0), -0.011, 0.0), vec3(0.030, 0.0065, 0.038), cd, 0., 8, 6);
    t.ball(vec3(x(-0.026), -0.006, 0.027), vec3(0.015, 0.011, 0.024), cd, 0., 8, 6);
    t.ball(vec3(x(0.029), -0.006, 0.020), vec3(0.016, 0.012, 0.026), c, 0., 8, 6);

    // fingers: three rounded segments each, knuckle bumps on the back
    let bases =
        [(0.0295, -0.050, 0.0096), (0.0098, -0.054, 0.0098), (-0.0102, -0.051, 0.0094), (-0.0290, -0.044, 0.0086)];
    let lens = [[0.040, 0.025, 0.022], [0.044, 0.027, 0.023], [0.040, 0.025, 0.021], [0.032, 0.020, 0.019]];
    for (i, &(u, z, r0)) in bases.iter().enumerate() {
        let mut p = vec3(x(u), 0.0, z);
        blob(&mut t, p + vec3(0., 0.008, 0.004), vec3(0.0095, 0.0075, 0.010), cd); // knuckle guard
        blob(&mut t, p, Vec3::splat(r0 * 1.12), c);
        let mut a = 0.;
        let radii = [r0, r0 * 0.94, r0 * 0.88, r0 * 0.80];
        for s in 0..3 {
            a += fist.ang[i][s];
            let sp = fist.splay[i];
            let d = vec3(-ts * sp.sin() * a.cos(), -a.sin(), -sp.cos() * a.cos());
            let q = p + d * lens[i][s];
            let col = if s == 1 { shade(c, 0.93) } else { c };
            seg(&mut t, p, q, radii[s], radii[s + 1], col, 8);
            blob(&mut t, q, Vec3::splat(radii[s + 1] * 1.04), col);
            p = q;
        }
        // fingertip pad
        blob(&mut t, p, Vec3::splat(radii[3] * 1.06), cd);
    }

    // thumb: metacarpal, proximal and distal segments
    let [yaw, pitch, b1, b2] = fist.thumb;
    let tl = [0.032, 0.027, 0.023];
    let tr = [0.0128, 0.0118, 0.0106, 0.0096];
    let mut p = vec3(x(0.030), -0.004, 0.016);
    blob(&mut t, p, Vec3::splat(tr[0] * 1.05), c);
    for s in 0..3 {
        let pt = pitch + [0., b1, b1 + b2][s];
        let yw = yaw * [1., 0.7, 0.5][s];
        let d = vec3(ts * yw.sin() * pt.cos(), -pt.sin(), -yw.cos() * pt.cos());
        let q = p + d * tl[s];
        seg(&mut t, p, q, tr[s], tr[s + 1], if s == 1 { shade(c, 0.93) } else { c }, 8);
        blob(&mut t, q, Vec3::splat(tr[s + 1] * 1.05), c);
        p = q;
    }
    blob(&mut t, p, Vec3::splat(tr[3] * 1.0), cd);

    // wrist cuff (neoprene) with a velcro strap and tab
    let mut cuff = Template::new();
    seg(&mut cuff, vec3(0., 0., 0.050), vec3(0., 0., 0.074), 0.0325, 0.0305, cd, 12);
    seg(&mut cuff, vec3(0., 0., 0.074), vec3(0., 0., CUFF_END.z), 0.0305, 0.0315, cd, 12);
    seg(&mut cuff, vec3(0., 0., 0.062), vec3(0., 0., 0.078), 0.0328, 0.0328, cl, 12);
    t.append(&cuff.transformed(Mat4::from_translation(vec3(0., 0.004, 0.)) * Mat4::from_scale(vec3(1., 0.80, 1.))));
    t.box_(vec3(x(-0.012), 0.0312, 0.068), vec3(0.012, 0.0022, 0.0075), shade(c, 0.85), 0.); // strap tab
    t.box_(vec3(x(-0.012), 0.0336, 0.068), vec3(0.006, 0.0008, 0.0055), pal.sole, 0.); // buckle patch
    t
}

/// Right-handed frame from the direction of the wrist (hand -> elbow) and the direction the thumb points.
fn orient(z: Vec3, thumb: Vec3, left: bool) -> Mat4 {
    let z = z.normalize_or_zero();
    let z = if z == Vec3::ZERO { Vec3::Z } else { z };
    let tx = if left { thumb } else { -thumb };
    let mut x = tx - z * tx.dot(z);
    if x.length_squared() < 1e-6 {
        x = z.any_orthonormal_vector();
    }
    let x = x.normalize();
    let y = z.cross(x);
    Mat4::from_cols(x.extend(0.), y.extend(0.), z.extend(0.), Vec4::W)
}

/// A hand at rest in some pose: palm position, orientation and finger curl.
#[derive(Clone, Copy)]
struct HandPose {
    pos: Vec3,
    z: Vec3,
    thumb: Vec3,
    fist: Fist,
}

impl HandPose {
    /// A hand whose grasped object (see [`GRASP`]) is at `anchor`.
    fn gripping(anchor: Vec3, z: Vec3, thumb: Vec3, left: bool, fist: Fist) -> HandPose {
        HandPose::holding(anchor, GRASP, z, thumb, left, fist)
    }

    fn holding(anchor: Vec3, grasp: Vec3, z: Vec3, thumb: Vec3, left: bool, fist: Fist) -> HandPose {
        let r = orient(z, thumb, left);
        let g = if left { vec3(-grasp.x, grasp.y, grasp.z) } else { grasp };
        HandPose { pos: anchor - r.transform_vector3(g), z, thumb, fist }
    }

    fn lerp(self, o: HandPose, t: f32) -> HandPose {
        HandPose {
            pos: self.pos.lerp(o.pos, t),
            z: self.z.lerp(o.z, t),
            thumb: self.thumb.lerp(o.thumb, t),
            fist: self.fist.lerp(o.fist, t),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// The forearm
// ---------------------------------------------------------------------------------------------------------

/// A sleeve from the glove's cuff to somewhere past the elbow (off screen): tapered, bent twice, ending in an
/// elastic cuff with a gathered edge; the left arm also wears a watch.
fn forearm(pal: &Palette, hp: &HandPose, left: bool, elbow_dir: Vec3) -> Template {
    let r = orient(hp.z, hp.thumb, left);
    let w = hp.pos + r.transform_point3(CUFF_END) - hp.pos + hp.pos - r.transform_vector3(Vec3::ZERO);
    let w = w - r.transform_vector3(vec3(0., 0.004, 0.)) * 0.0;
    let hz = r.transform_vector3(Vec3::Z).normalize_or_zero();
    let ed = elbow_dir.normalize_or_zero();
    let u = pal.uniform;
    let mut t = Template::new();
    let d0 = hz.lerp(ed, 0.30).normalize_or_zero();
    let p0 = w - d0 * 0.012; // tuck under the glove cuff
    let p1 = w + d0 * 0.12;
    let d1 = d0.lerp(ed, 0.7).normalize_or_zero();
    let p2 = p1 + d1 * 0.17;
    let p3 = p2 + ed * 0.20;
    let d3 = (ed + vec3(0.15, -0.35, 0.1)).normalize_or_zero();
    let p4 = p3 + d3 * 0.30;
    seg(&mut t, p0, p1, 0.0305, 0.0325, u, 10);
    seg(&mut t, p1, p2, 0.0325, 0.0375, u, 10);
    seg(&mut t, p2, p3, 0.0375, 0.041, u, 10);
    seg(&mut t, p3, p4, 0.041, 0.043, u, 10);
    t.ball(p1, Vec3::splat(0.0325), u, 0., 8, 6);
    t.ball(p2, Vec3::splat(0.0375), u, 0., 8, 6);
    t.ball(p3, Vec3::splat(0.041), u, 0., 8, 6);
    // elastic cuff and the gathered edge above it
    let dark = shade(u, 0.78);
    seg(&mut t, w - d0 * 0.004, w + d0 * 0.030, 0.0335, 0.0335, dark, 10);
    seg(&mut t, w + d0 * 0.030, w + d0 * 0.040, 0.0358, 0.0358, shade(u, 1.12), 10);
    seg(&mut t, w + d0 * 0.040, w + d0 * 0.050, 0.0340, 0.0340, shade(u, 0.9), 10);
    if left {
        // wristwatch strap and head, on the back of the wrist
        let back = r.transform_vector3(Vec3::Y).normalize_or_zero();
        let a = w + d0 * 0.075;
        seg(&mut t, a - d0 * 0.010, a + d0 * 0.010, 0.0345, 0.0345, pal.sole, 10);
        t.ball(a + back * 0.034, vec3(0.016, 0.005, 0.016), [0.07, 0.07, 0.08], 0., 8, 6);
        t.ball(a + back * 0.038, vec3(0.011, 0.002, 0.011), [0.28, 0.36, 0.34], 0.15, 8, 4);
    }
    t
}

// ---------------------------------------------------------------------------------------------------------
// Poses
// ---------------------------------------------------------------------------------------------------------

/// The firing hand on the grip, and where its forearm heads (off screen towards the elbow).
fn right_hand(hold: Hold, anchors: &WeaponAnchors, pose: &ArmPose, throwing: f32) -> HandPose {
    let g = anchors.grip;
    let ads = pose.ads;
    match hold {
        Hold::Pistol => {
            let fist = Fist::grip(1.0, THUMB_PISTOL).with_index(0.0);
            HandPose::gripping(
                g + vec3(0.0, -0.016, 0.0),
                vec3(0.18, -0.16, 1.).lerp(vec3(0.10, -0.10, 1.), ads),
                vec3(0., 1., -0.30),
                false,
                fist,
            )
        }
        Hold::Grenade => {
            let open = smooth(0.5, 0.75, throwing);
            let fist = Fist::grip(0.9, THUMB_OVER).lerp(Fist::grip(0.15, THUMB_OPEN), open);
            HandPose::gripping(g + vec3(0.0, -0.002, 0.0), vec3(0.35, -0.55, 0.9), vec3(0.15, 1., -0.35), false, fist)
        }
        Hold::Melee => {
            let fist = Fist::grip(1.15, [0.2, 0.3, 0.2, 0.2]);
            HandPose::gripping(g, vec3(0.50, -0.50, 0.70), vec3(0.0, 0.25, -1.), false, fist)
        }
        Hold::Unarmed => {
            HandPose::gripping(g, vec3(0.3, -0.5, 1.), vec3(0., 1., -0.3), false, Fist::grip(0.5, THUMB_FWD))
        }
        Hold::Sniper => {
            let fist = Fist::grip(1.0, THUMB_WRAP).with_index(0.7);
            HandPose::gripping(g + vec3(0., -0.010, 0.), vec3(0.22, -0.30, 1.), vec3(0., 1., -0.3), false, fist)
        }
        _ => {
            let fist = Fist::grip(1.0, THUMB_WRAP).with_index(0.8);
            HandPose::gripping(g + vec3(0., -0.010, 0.), vec3(0.22, -0.28, 1.), vec3(0., 1., -0.3), false, fist)
        }
    }
}

/// Elbow direction of the right arm (from the wrist, weapon-local).
fn right_elbow(hold: Hold, pose: &ArmPose) -> Vec3 {
    let hip = match hold {
        Hold::Melee => vec3(0.45, -0.70, 0.55),
        Hold::Grenade => vec3(0.40, -0.70, 0.60),
        _ => vec3(0.30, -0.55, 0.80),
    };
    let mut d = hip.lerp(vec3(0.38, -0.85, 0.40), pose.ads);
    d.y -= (1. - pose.draw) * 0.7;
    if hold == Hold::Melee && pose.swing > 0. {
        let sweep = -1.0 + 2.2 * pose.swing;
        d = Mat4::from_rotation_z(sweep * 0.5).transform_vector3(d);
    }
    d
}

/// The support hand for a two-handed weapon: a C-clamp under the handguard, thumb along the rail.
fn support_hand(s: Vec3, hold: Hold) -> HandPose {
    let thumb = [1.1, 0.50, 0.25, 0.25];
    let k = if hold == Hold::Launcher { 0.9 } else { 1.0 };
    HandPose::holding(s, CLAMP_GRASP, vec3(-0.80, -0.25, 0.60), vec3(0.15, 0.25, -1.), true, Fist::clamp(k, thumb))
}

/// Left hand when the weapon has no support point: hanging ready, low and to the left.
fn rest_hand(g: Vec3) -> HandPose {
    HandPose {
        pos: g + vec3(-0.30, -0.24, 0.10),
        z: vec3(-0.3, -0.4, 1.),
        thumb: vec3(0., 1., -0.4),
        fist: Fist::grip(0.55, THUMB_FWD),
    }
}

/// Forearms and gloved hands for the first-person view: the right hand grips at `anchors.grip`, the left at
/// `anchors.support` (see the module docs of the design brief for the `None` cases).
#[allow(dead_code)]
pub fn first_person_arms_detailed(
    team: Team,
    skin: u8,
    anchors: &WeaponAnchors,
    hold: Hold,
    pose: &ArmPose,
) -> Template {
    let pal = Palette::new(team, skin);
    let pose = ArmPose {
        ads: f(pose.ads, 0.).clamp(0., 1.),
        draw: f(pose.draw, 1.).clamp(0., 1.),
        reload: f(pose.reload, 0.).clamp(0., 1.),
        throwing: f(pose.throwing, 0.).clamp(0., 1.),
        swing: f(pose.swing, 0.).clamp(0., 1.),
        pin: pose.pin,
    };
    let (ads, draw, reload) = (pose.ads, pose.draw, pose.reload);
    let firearm = matches!(hold, Hold::Pistol | Hold::Smg | Hold::Rifle | Hold::Sniper | Hold::Launcher);
    let g = anchors.grip;
    let mut out = Template::new();

    // ---- right hand: on the grip ----
    let rh = right_hand(hold, anchors, &pose, pose.throwing);
    out.append(
        &hand(&pal, false, &rh.fist).transformed(Mat4::from_translation(rh.pos) * orient(rh.z, rh.thumb, false)),
    );
    out.append(&forearm(&pal, &rh, false, right_elbow(hold, &pose)));

    // ---- left hand ----
    let mut lh = match (anchors.support, hold) {
        (Some(s), _) => support_hand(s, hold),
        (None, Hold::Pistol) => {
            // two-handed grip when aiming: the left hand cups the right one from the other side
            let wrap = HandPose::gripping(
                g + vec3(-0.004, -0.034, -0.012),
                vec3(-0.20, -0.20, 1.),
                vec3(0., 1., -0.5),
                true,
                Fist::grip(1.0, THUMB_FWD),
            );
            rest_hand(g).lerp(wrap, smooth(0., 0.3, ads))
        }
        (None, Hold::Grenade) => {
            // the left hand takes the pin ring and pulls it away; it drops out of sight as the throw starts
            let ring = g + vec3(0., 0.040, 0.034);
            let near = HandPose::gripping(
                ring + vec3(-0.012, -0.004, 0.0),
                vec3(-0.6, -0.4, 0.8),
                vec3(0.3, 1., -0.2),
                true,
                Fist::grip(0.85, THUMB_FWD),
            );
            let pulled = HandPose { pos: near.pos + vec3(-0.06, 0.0, 0.10), ..near };
            let base = if pose.pin { pulled } else { rest_hand(g).lerp(near, 0.35) };
            base.lerp(rest_hand(g), smooth(0.05, 0.4, pose.throwing))
        }
        (None, _) => rest_hand(g),
    };
    if reload > 0. && firearm {
        let base = lh;
        let mw = g + magwell_of(hold);
        let mag_hand = HandPose::gripping(
            mw + vec3(0., 0.0, 0.0),
            vec3(-0.8, -0.3, 0.6),
            vec3(0.2, 1., -0.4),
            true,
            Fist::grip(0.9, THUMB_FWD),
        );
        let away = HandPose { pos: g + vec3(-0.06, -0.32, 0.30), ..mag_hand };
        let reach = HandPose { pos: mw + vec3(0., -0.02, 0.0), ..mag_hand };
        let at = HandPose { fist: Fist::grip(0.6, THUMB_OPEN), ..mag_hand };
        let seat = HandPose { pos: mw + vec3(0., 0.03, 0.), ..mag_hand };
        let stages = [
            (0., base),
            (0.2, reach),
            (0.28, mag_hand),
            (0.4, away),
            (0.66, at),
            (0.78, seat),
            (0.86, mag_hand),
            (1., base),
        ];
        let mut k = 0;
        while k + 2 < stages.len() && reload > stages[k + 1].0 {
            k += 1;
        }
        let u = smooth(stages[k].0, stages[k + 1].0, reload);
        lh = stages[k].1.lerp(stages[k + 1].1, u);
        // keep the hand position on the smoother path used before for the big reach
        let p = keys(
            reload,
            &[(0., base.pos), (0.2, reach.pos), (0.4, away.pos), (0.66, at.pos), (0.8, seat.pos), (1., base.pos)],
        );
        lh.pos = p;
    }
    let hand_l = hand(&pal, true, &lh.fist);
    out.append(&hand_l.transformed(Mat4::from_translation(lh.pos) * orient(lh.z, lh.thumb, true)));
    let mut e = vec3(-0.45, -0.55, 0.70);
    e.y -= (1. - draw) * 0.7;
    if ads > 0. {
        e = e.lerp(vec3(-0.55, -0.8, 0.35), ads);
    }
    out.append(&forearm(&pal, &lh, true, e));
    out
}

/// One plain gloved hand: a rounded palm, a block of curled fingers, a thumb, a cuff. `side` is +1 for the right hand,
/// -1 for the left; `palm_up` turns it to cup something from below.
fn simple_hand(t: &mut Template, at: Vec3, side: f32, palm_up: bool, glove: Rgb, dark: Rgb) {
    let up = if palm_up { -1. } else { 1. };
    blob(t, at + vec3(0., 0.012 * up, 0.012), vec3(0.040, 0.036, 0.052), glove);
    blob(t, at + vec3(0., -0.004 * up, -0.030), vec3(0.036, 0.034, 0.030), shade(glove, 0.92));
    blob(t, at + vec3(0.034 * side, 0.024 * up, -0.012), vec3(0.016, 0.016, 0.034), glove);
    seg(
        t,
        at + vec3(0.004 * side, -0.030 * up, 0.052),
        at + vec3(0.012 * side, -0.058 * up, 0.086),
        0.040,
        0.044,
        dark,
        8,
    );
}

/// One forearm from the wrist out of the screen towards the camera.
fn simple_sleeve(t: &mut Template, wrist: Vec3, dir: Vec3, colour: Rgb) {
    seg(t, wrist, wrist + dir.normalize() * 0.62, 0.040, 0.054, colour, 8);
}

/// The first-person arms: articulated fingers for the exposed knife grip, plain gloves for firearms,
/// with tapered sleeves in the team's colours. The right hand holds the grip; the left supports under
/// the handguard, cups the pistol grip, takes the grenade pin, and goes to the magazine on a reload.
pub fn first_person_arms(team: Team, skin: u8, anchors: &WeaponAnchors, hold: Hold, pose: &ArmPose) -> Template {
    let pal = Palette::new(team, skin);
    let ads = f(pose.ads, 0.).clamp(0., 1.);
    let draw = f(pose.draw, 1.).clamp(0., 1.);
    let reload = f(pose.reload, 0.).clamp(0., 1.);
    let g = anchors.grip;
    let (glove, dark, sleeve) = (pal.glove, pal.glove_dark, pal.uniform);
    let mut t = Template::new();
    let drop = (1. - draw) * 0.5;

    // Right hand and forearm.
    if hold == Hold::Melee {
        // Fingers lie across the handle along Z and curl around its cross section.
        // Keep the thumb on the guard side and the wrist below the palm in a hammer grip.
        let hp =
            HandPose::gripping(g, vec3(0.65, -0.75, 0.), -Vec3::Z, false, Fist::grip(1.05, [0.15, 0.12, 0.20, 0.55]));
        let r = Mat4::from_translation(hp.pos) * orient(hp.z, hp.thumb, false);
        t.append(&hand(&pal, false, &hp.fist).transformed(r));
        t.append(&forearm(&pal, &hp, false, vec3(0.30, -0.55 - drop, 1.)));
        return t;
    }
    simple_hand(&mut t, g, 1., false, glove, dark);
    let wrist = g + vec3(0.012, -0.058, 0.086);
    simple_sleeve(&mut t, wrist, vec3(0.28, -0.52 - 0.35 * ads, 1.0) + vec3(0., -drop, 0.), sleeve);

    // Left hand: only on the big, two-handed weapons (the ones with a support point: rifles, shotguns, snipers,
    // launchers). Pistols, grenades and knives are held in one hand.
    let Some(support) = anchors.support else { return t };
    let mut pos = support + vec3(0., -0.02, 0.01);
    let mut reach = vec3(-0.30, -0.55 - 0.3 * ads, 1.0);
    if reload > 0. {
        // Down and back to the magazine, then up again to the handguard.
        let r = (reload * std::f32::consts::PI).sin();
        let mag = g + vec3(-0.01, -0.13, -0.02);
        pos = pos.lerp(mag + vec3(0., -0.08, 0.08), r);
        reach += vec3(0., -0.3 * r, 0.);
    }
    pos += vec3(0., -drop, 0.);
    simple_hand(&mut t, pos, -1., true, glove, dark);
    simple_sleeve(&mut t, pos + vec3(-0.012, -0.058, 0.086), reach, sleeve);
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchors(support: bool) -> WeaponAnchors {
        WeaponAnchors {
            grip: Vec3::ZERO,
            support: support.then(|| vec3(0., 0.02, -0.30)),
            sight: vec3(0., 0.08, 0.05),
            muzzle: vec3(0., 0.02, -0.85),
            eject: vec3(0.03, 0.04, -0.1),
        }
    }

    fn poses() -> Vec<ArmPose> {
        vec![
            ArmPose::default(),
            ArmPose { reload: 0.4, ads: 1., ..ArmPose::default() },
            ArmPose { throwing: 0.9, draw: 0.2, swing: 0.5, ..ArmPose::default() },
            ArmPose { throwing: 0.1, pin: true, ..ArmPose::default() },
            ArmPose { reload: f32::NAN, ads: f32::INFINITY, draw: f32::NAN, ..ArmPose::default() },
            ArmPose { reload: 1.0, swing: 1.0, throwing: 1.0, draw: 0.0, ads: 0.0, pin: true },
        ]
    }

    #[test]
    fn non_empty_and_finite_for_every_hold() {
        for team in Team::ALL {
            for hold in Hold::ALL {
                for support in [false, true] {
                    for pose in poses() {
                        let t = first_person_arms(team, 2, &anchors(support), hold, &pose);
                        assert!(t.verts.len() > 100, "{hold:?}");
                        assert!(t.verts.len() < 9000, "{hold:?} {} vertices", t.verts.len());
                        assert!(t.verts.iter().all(|v| v.p.is_finite() && v.n.is_finite()), "{hold:?} {pose:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn vertex_budget_is_a_few_thousand() {
        for hold in Hold::ALL {
            let t = first_person_arms(Team::Ironclad, 0, &anchors(true), hold, &ArmPose::default());
            assert!(t.verts.len() < 6500, "{hold:?}: {}", t.verts.len());
        }
    }

    #[test]
    fn sleeves_and_gloves_use_the_team_colours() {
        for team in Team::ALL {
            for hold in [Hold::Rifle, Hold::Pistol, Hold::Melee, Hold::Grenade] {
                let pal = Palette::new(team, 0);
                let t = first_person_arms(team, 0, &anchors(true), hold, &ArmPose::default());
                let n = |c: Rgb| t.verts.iter().filter(|v| v.c == c).count();
                assert!(n(pal.uniform) >= 8, "sleeve colour missing");
                assert!(n(pal.glove) >= 8, "glove colour missing");
                assert!(n(pal.glove_dark) >= 8, "glove cuff missing");
            }
        }
    }

    #[test]
    fn forearms_run_back_towards_the_camera_and_down() {
        let t = first_person_arms(Team::Ironclad, 0, &anchors(true), Hold::Rifle, &ArmPose::default());
        let max_z = t.verts.iter().map(|v| v.p.z).fold(f32::MIN, f32::max);
        let min_y = t.verts.iter().map(|v| v.p.y).fold(f32::MAX, f32::min);
        assert!(max_z > 0.4, "arms reach back to z={max_z}");
        assert!(min_y < -0.2, "arms drop to y={min_y}");
    }

    #[test]
    fn ads_keeps_the_arms_out_of_the_line_of_sight() {
        // Looking from the sight point straight down -Z, nothing may sit in the narrow cone around the axis.
        let a = anchors(true);
        for hold in [Hold::Pistol, Hold::Smg, Hold::Rifle, Hold::Sniper] {
            let t = first_person_arms(Team::Ironclad, 0, &a, hold, &ArmPose { ads: 1., ..ArmPose::default() });
            for v in &t.verts {
                let rel = v.p - a.sight;
                if rel.z < -0.02 {
                    let slope = vec2(rel.x, rel.y).length() / -rel.z;
                    assert!(slope > 0.05 || rel.y < -0.012, "{hold:?}: arm vertex {:?} blocks the sight", v.p);
                }
            }
        }
    }

    #[test]
    fn both_hands_are_there_and_the_arms_stay_small() {
        let a = anchors(true);
        for hold in Hold::ALL {
            let t = first_person_arms(Team::Nightwatch, 0, &a, hold, &ArmPose::default());
            let budget = if hold == Hold::Melee { 6500 } else { 1400 };
            assert!(t.verts.len() < budget, "{hold:?}: {} vertices", t.verts.len());
        }
        let t = first_person_arms(Team::Nightwatch, 0, &a, Hold::Rifle, &ArmPose::default());
        let support = a.support.unwrap();
        assert!(t.verts.iter().any(|v| (v.p - support).length() < 0.06), "the left hand sits on the handguard");
    }

    #[test]
    fn reload_takes_the_left_hand_down_and_back() {
        let a = anchors(true);
        let rest = first_person_arms(Team::Ironclad, 0, &a, Hold::Rifle, &ArmPose::default());
        let mid = first_person_arms(Team::Ironclad, 0, &a, Hold::Rifle, &ArmPose { reload: 0.5, ..ArmPose::default() });
        let low = |t: &Template| t.verts.iter().map(|v| v.p.y).fold(f32::MAX, f32::min);
        assert!(low(&mid) < low(&rest) - 0.03);
        let end = first_person_arms(Team::Ironclad, 0, &a, Hold::Rifle, &ArmPose { reload: 1.0, ..ArmPose::default() });
        assert!((low(&end) - low(&rest)).abs() < 0.02);
    }

    #[test]
    fn a_hand_is_about_an_adult_hand_in_size() {
        let pal = Palette::new(Team::Ironclad, 0);
        let h = hand(&pal, false, &Fist::grip(0.0, THUMB_OPEN));
        let (mut lo, mut hi) = (Vec3::splat(1e9), Vec3::splat(-1e9));
        for v in &h.verts {
            lo = lo.min(v.p);
            hi = hi.max(v.p);
        }
        let len = hi.z - lo.z;
        assert!((0.17..0.27).contains(&len), "hand + cuff length {len}");
        assert!((0.07..0.14).contains(&(hi.x - lo.x)), "hand width {}", hi.x - lo.x);
    }
}
