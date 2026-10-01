//! The soldiers: a procedurally built, forward-kinematics rig made of separate parts (pelvis, torso, head,
//! arms, hands, thighs, shins, boots), one per (team, skin).
//!
//! Space: the character's own frame has the feet at the origin, +Y up, facing -Z (yaw 0); `draw` and
//! `weapon_mount` turn it into the world with `yaw` (positive turns towards +X) and the feet position.
//! Index 0 of every pair is the right side (+X), index 1 the left.
use crate::team::Team;
use macroquad::prelude::*;
use vesper3d::viewer::kit::{Batch, Template, Tint};

type Rgb = [f32; 3];

// ---------------------------------------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------------------------------------

/// How the weapon is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Hold {
    #[default]
    Unarmed,
    Pistol,
    Smg,
    Rifle,
    Sniper,
    Launcher,
    Grenade,
    Melee,
}

impl Hold {
    pub const ALL: [Hold; 8] =
        [Hold::Unarmed, Hold::Pistol, Hold::Smg, Hold::Rifle, Hold::Sniper, Hold::Launcher, Hold::Grenade, Hold::Melee];

    pub fn from_name(name: &str) -> Option<Hold> {
        Some(match name {
            "unarmed" => Hold::Unarmed,
            "pistol" => Hold::Pistol,
            "smg" => Hold::Smg,
            "rifle" => Hold::Rifle,
            "sniper" => Hold::Sniper,
            "launcher" => Hold::Launcher,
            "grenade" => Hold::Grenade,
            "melee" => Hold::Melee,
            _ => return None,
        })
    }
}

/// Everything the animation needs to know about one soldier this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    /// Horizontal speed, m/s.
    pub speed: f32,
    /// Radians; the caller advances it by distance travelled (one full turn = two steps).
    pub walk_phase: f32,
    /// 0 standing .. 1 crouched.
    pub crouch: f32,
    /// Radians, positive looks up.
    pub pitch: f32,
    /// 0..1 aiming down the sights.
    pub aim: f32,
    pub hold: Hold,
    /// 0..1 kick of the last shot.
    pub recoil: f32,
    /// 0 none, else 0..1 progress of the reload.
    pub reload: f32,
    /// Melee swing 0..1 (0 = not swinging).
    pub swing: f32,
    /// Grenade throw 0..1 (0 = not throwing).
    pub throwing: f32,
    /// 0 alive .. 1 collapsed on the ground.
    pub dead: f32,
    pub airborne: bool,
}

/// The colours of one soldier.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub skin: Rgb,
    pub skin_dark: Rgb,
    pub uniform: Rgb,
    pub trousers: Rgb,
    pub vest: Rgb,
    pub vest_dark: Rgb,
    pub plate: Rgb,
    pub helmet: Rgb,
    pub net: Rgb,
    pub strap: Rgb,
    pub glove: Rgb,
    pub glove_dark: Rgb,
    pub boot: Rgb,
    pub sole: Rgb,
    pub metal: Rgb,
}

pub const SKIN_TONES: [Rgb; 4] = [[0.93, 0.76, 0.62], [0.80, 0.60, 0.44], [0.58, 0.40, 0.28], [0.36, 0.23, 0.16]];

impl Palette {
    pub fn new(team: Team, skin: u8) -> Palette {
        let skin_c = SKIN_TONES[(skin as usize).min(3)];
        let skin_dark = mul(skin_c, 0.72);
        match team {
            Team::Ironclad => Palette {
                skin: skin_c,
                skin_dark,
                uniform: [0.26, 0.31, 0.12],
                trousers: [0.24, 0.29, 0.115],
                vest: [0.66, 0.56, 0.36],
                vest_dark: [0.50, 0.41, 0.25],
                plate: [0.56, 0.47, 0.30],
                helmet: [0.23, 0.27, 0.12],
                net: [0.70, 0.62, 0.42],
                strap: [0.42, 0.34, 0.20],
                glove: [0.62, 0.52, 0.34],
                glove_dark: [0.47, 0.38, 0.23],
                boot: [0.56, 0.44, 0.27],
                sole: [0.16, 0.13, 0.10],
                metal: [0.20, 0.20, 0.21],
            },
            Team::Nightwatch => Palette {
                skin: skin_c,
                skin_dark,
                uniform: [0.11, 0.17, 0.31],
                trousers: [0.10, 0.15, 0.27],
                vest: [0.09, 0.09, 0.10],
                vest_dark: [0.05, 0.05, 0.06],
                plate: [0.15, 0.15, 0.17],
                helmet: [0.08, 0.08, 0.09],
                net: [0.08, 0.08, 0.09],
                strap: [0.04, 0.04, 0.05],
                glove: [0.07, 0.07, 0.08],
                glove_dark: [0.04, 0.04, 0.05],
                boot: [0.06, 0.06, 0.07],
                sole: [0.02, 0.02, 0.02],
                metal: [0.25, 0.26, 0.28],
            },
        }
    }
}

/// A soldier, built once per (team, skin).
pub struct Rig {
    pub team: Team,
    pub skin: u8,
    palette: Palette,
    pelvis: Template,
    torso: Template,
    head: Template,
    upper_arm: Template,
    forearm: Template,
    glove: [Template; 2],
    thigh: [Template; 2],
    shin: Template,
    boot: Template,
}

impl Rig {
    pub fn new(team: Team, skin: u8) -> Rig {
        let skin = skin.min(3);
        let p = Palette::new(team, skin);
        Rig {
            team,
            skin,
            palette: p,
            pelvis: build_pelvis(&p, team),
            torso: build_torso(&p, team),
            head: build_head(&p, team),
            upper_arm: build_upper_arm(&p, team),
            forearm: build_forearm(&p, team),
            glove: [glove(&p, false, 0.), glove(&p, true, 0.)],
            thigh: [build_thigh(&p, team, 1.), build_thigh(&p, team, -1.)],
            shin: build_shin(&p, team),
            boot: build_boot(&p),
        }
    }

    /// Vertex count per part, for budgeting: (name, vertices, copies per soldier).
    pub fn part_sizes(&self) -> Vec<(&'static str, usize, usize)> {
        vec![
            ("pelvis", self.pelvis.verts.len(), 1),
            ("torso", self.torso.verts.len(), 1),
            ("head", self.head.verts.len(), 1),
            ("upper_arm", self.upper_arm.verts.len(), 2),
            ("forearm", self.forearm.verts.len(), 2),
            ("glove", self.glove[0].verts.len(), 2),
            ("thigh", self.thigh[0].verts.len(), 2),
            ("shin", self.shin.verts.len(), 2),
            ("boot", self.boot.verts.len(), 2),
        ]
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// Total vertex count of all parts (what one soldier adds to a batch).
    pub fn vertex_count(&self) -> usize {
        self.pelvis.verts.len()
            + self.torso.verts.len()
            + self.head.verts.len()
            + 2 * (self.upper_arm.verts.len() + self.forearm.verts.len())
            + self.thigh[0].verts.len()
            + self.thigh[1].verts.len()
            + 2 * (self.shin.verts.len() + self.boot.verts.len())
            + self.glove[0].verts.len()
            + self.glove[1].verts.len()
    }

    /// Every part with its world transform.
    pub fn parts(&self, feet: Vec3, yaw: f32, pose: &Pose) -> Vec<(&Template, Mat4)> {
        let s = self.solve(pose);
        let w = world(feet, yaw);
        let mut v: Vec<(&Template, Mat4)> = Vec::with_capacity(16);
        v.push((&self.pelvis, w * s.pelvis));
        v.push((&self.torso, w * s.torso));
        v.push((&self.head, w * s.head));
        for i in 0..2 {
            v.push((&self.upper_arm, w * s.uarm[i]));
            v.push((&self.forearm, w * s.farm[i]));
            v.push((&self.glove[i], w * s.hand[i]));
            v.push((&self.thigh[i], w * s.thigh[i]));
            v.push((&self.shin, w * s.shin[i]));
            v.push((&self.boot, w * s.boot[i]));
        }
        v
    }

    /// Adds all parts to the batch.
    pub fn draw(&self, batch: &mut Batch, feet: Vec3, yaw: f32, pose: &Pose, tint: Tint) {
        for (t, m) in self.parts(feet, yaw, pose) {
            batch.add(t, m, tint);
        }
    }

    /// Where a weapon model goes: its grip (origin) in the right hand, -Z out of the muzzle, in world space.
    pub fn weapon_mount(&self, feet: Vec3, yaw: f32, pose: &Pose) -> Mat4 {
        world(feet, yaw) * self.solve(pose).mount
    }
}

// ---------------------------------------------------------------------------------------------------------
// Small maths
// ---------------------------------------------------------------------------------------------------------

fn world(feet: Vec3, yaw: f32) -> Mat4 {
    Mat4::from_translation(feet) * Mat4::from_rotation_y(-yaw)
}

fn mul(c: Rgb, k: f32) -> Rgb {
    [c[0] * k, c[1] * k, c[2] * k]
}

pub(crate) fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

fn fin(x: f32, default: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        default
    }
}

/// Piecewise smooth interpolation through `(t, value)` keys.
pub(crate) fn keys(t: f32, k: &[(f32, Vec3)]) -> Vec3 {
    if t <= k[0].0 {
        return k[0].1;
    }
    for w in k.windows(2) {
        if t <= w[1].0 {
            let u = smooth(w[0].0, w[1].0, t);
            return w[0].1.lerp(w[1].1, u);
        }
    }
    k[k.len() - 1].1
}

/// Matrix with origin `o` whose local -Y points from `o` to `end` (how every limb template hangs).
pub(crate) fn limb(o: Vec3, end: Vec3) -> Mat4 {
    let y = (o - end).normalize_or_zero();
    let y = if y == Vec3::ZERO { Vec3::Y } else { y };
    let mut x = y.cross(Vec3::Z);
    if x.length_squared() < 1e-4 {
        x = y.cross(Vec3::X);
    }
    let x = x.normalize();
    let z = x.cross(y);
    Mat4::from_cols(x.extend(0.), y.extend(0.), z.extend(0.), o.extend(1.))
}

/// Frame with the given rotation columns.
fn basis(x: Vec3, y: Vec3, z: Vec3) -> Mat4 {
    Mat4::from_cols(x.extend(0.), y.extend(0.), z.extend(0.), Vec4::W)
}

pub(crate) fn at_palm(palm: Vec3, rot: Mat4) -> Mat4 {
    Mat4::from_translation(palm) * rot
}

/// Orientation of a hand that hangs relaxed (used for the glove's canonical frame).
pub(crate) fn free_hand_rot() -> Mat4 {
    Mat4::from_rotation_x(0.25) * basis(Vec3::X, -Vec3::Z, Vec3::Y)
}

/// Orientation of the support hand cupping a handguard from below, fingers wrapping round its right side.
pub(crate) fn cup_rot() -> Mat4 {
    Mat4::from_rotation_y(0.5) * basis(Vec3::Y, -Vec3::Z, -Vec3::X)
}

/// Wrist position relative to the palm centre, in the glove's own frame (mirrored for the left glove).
pub(crate) fn wrist_offset(left: bool) -> Vec3 {
    vec3(if left { -0.030 } else { 0.030 }, 0.028, 0.078)
}

/// Two-bone IK: the elbow for a limb from `s` to `w`, bending towards `pole`.
fn elbow(s: Vec3, w: Vec3, l1: f32, l2: f32, pole: Vec3) -> Vec3 {
    let v = w - s;
    let d = v.length().clamp(0.12, l1 + l2 - 0.004);
    let dir = if v.length() > 1e-5 { v / v.length() } else { -Vec3::Y };
    let a = (l1 * l1 - l2 * l2 + d * d) / (2. * d);
    let h = (l1 * l1 - a * a).max(0.).sqrt();
    let mut pp = pole - dir * dir.dot(pole);
    if pp.length_squared() < 1e-6 {
        pp = -Vec3::Y - dir * dir.dot(-Vec3::Y);
        if pp.length_squared() < 1e-6 {
            pp = Vec3::X;
        }
    }
    s + dir * a + pp.normalize() * h
}

// ---------------------------------------------------------------------------------------------------------
// Dimensions
// ---------------------------------------------------------------------------------------------------------

const THIGH: f32 = 0.45;
const SHIN: f32 = 0.41;
const ANKLE: f32 = 0.100;
const HIP_STAND: f32 = ANKLE + THIGH + SHIN; // 0.96
const HIP_X: f32 = 0.11;
const WAIST: f32 = 0.06; // torso pivot above the hip centre
const SHOULDER_X: f32 = 0.20;
const SHOULDER_Y: f32 = 0.455;
const NECK_Y: f32 = 0.55;
const UARM: f32 = 0.31;
const FARM: f32 = 0.26;
/// Slightly bent knees when standing: the leg reach stops this short of fully straight.
const REACH: f32 = THIGH + SHIN - 0.05;

// ---------------------------------------------------------------------------------------------------------
// Solving the pose
// ---------------------------------------------------------------------------------------------------------

struct Skel {
    pelvis: Mat4,
    torso: Mat4,
    head: Mat4,
    uarm: [Mat4; 2],
    farm: [Mat4; 2],
    hand: [Mat4; 2],
    thigh: [Mat4; 2],
    shin: [Mat4; 2],
    boot: [Mat4; 2],
    mount: Mat4,
}

/// Where the support hand goes on a weapon (weapon-local), and how it holds.
#[derive(Clone, Copy, PartialEq)]
enum Support {
    Cup,
    Wrap,
}

fn support_of(hold: Hold) -> Option<(Vec3, Support)> {
    match hold {
        Hold::Pistol => Some((vec3(-0.022, -0.030, -0.022), Support::Wrap)),
        Hold::Smg => Some((vec3(0., -0.035, -0.20), Support::Cup)),
        Hold::Rifle => Some((vec3(0., -0.04, -0.36), Support::Cup)),
        Hold::Sniper => Some((vec3(0., -0.04, -0.42), Support::Cup)),
        Hold::Launcher => Some((vec3(0., 0.05, -0.32), Support::Cup)),
        _ => None,
    }
}

pub(crate) fn magwell_of(hold: Hold) -> Vec3 {
    match hold {
        Hold::Pistol => vec3(0., -0.085, 0.0),
        Hold::Launcher => vec3(0., -0.15, -0.05),
        _ => vec3(0., -0.11, -0.10),
    }
}

impl Rig {
    fn solve(&self, pose: &Pose) -> Skel {
        let dead = fin(pose.dead, 0.).clamp(0., 1.);
        let alive = 1. - smooth(0., 0.15, dead);
        let kneel = if dead > 0. { 0.9 * (std::f32::consts::PI * dead.powf(0.6)).sin().max(0.) } else { 0. };
        let crouch = fin(pose.crouch, 0.).clamp(0., 1.).max(kneel);
        let speed = fin(pose.speed, 0.).clamp(0., 12.) * alive;
        let phase = fin(pose.walk_phase, 0.);
        let pitch = fin(pose.pitch, 0.).clamp(-1.4, 1.4) * alive;
        let aim = fin(pose.aim, 0.).clamp(0., 1.);
        let recoil = fin(pose.recoil, 0.).clamp(0., 1.) * alive;
        let reload = fin(pose.reload, 0.).clamp(0., 1.) * alive;
        let swing = fin(pose.swing, 0.).clamp(0., 1.) * alive;
        let throwing = fin(pose.throwing, 0.).clamp(0., 1.) * alive;
        let hold = if dead > 0.05 { Hold::Unarmed } else { pose.hold };
        let air = pose.airborne && dead == 0.;
        let c = crouch;

        // ---- legs: stride targets, then the hip height that lets every leg reach ----
        let moving = smooth(0.15, 1.2, speed) * if air { 0. } else { 1. };
        let stride = moving * (0.10 + 0.045 * speed).min(0.36) * (1. - 0.45 * c);
        let lift_h = moving * (0.03 + 0.018 * speed).min(0.14) * (1. - 0.4 * c);
        let hz = 0.12 * c;
        let hh_base = HIP_STAND + (0.385 - HIP_STAND) * c;
        let mut foot = [Vec3::ZERO; 2];
        let mut toes = [0.0f32; 2];
        let mut hh = hh_base;
        for (i, f) in foot.iter_mut().enumerate() {
            let ph = phase + i as f32 * std::f32::consts::PI;
            let (mut fwd, mut lift) = (-stride * ph.cos(), lift_h * ph.sin().max(0.));
            if air {
                fwd = if i == 0 { 0.14 } else { -0.10 };
                lift = 0.16 + 0.06 * i as f32;
            }
            let side = if i == 0 { 1. } else { -1. };
            let x = side * (HIP_X + 0.015 + 0.03 * c);
            // toe down as the foot leaves the ground (the ankle rises so the toe stays above it), flat in stance
            let toe = if air { 0.35 } else { (-0.34 * ph.cos() * (stride / 0.3).min(1.)).clamp(-0.22, 0.34) };
            toes[i] = toe;
            *f = vec3(x, ANKLE + lift + 0.17 * toe.max(0.).sin() + 0.10 * (-toe).max(0.).sin(), fwd);
            if !air {
                let dz = fwd - hz;
                let reach = REACH;
                hh = hh.min(ANKLE + (reach * reach - dz * dz).max(0.01).sqrt());
            }
        }
        if air {
            hh = hh_base - 0.03;
        }

        let two_handed = matches!(hold, Hold::Pistol | Hold::Smg | Hold::Rifle | Hold::Sniper | Hold::Launcher);
        // the hips and shoulders counter-rotate in a stride (less with a weapon in both hands)
        let gait = moving * (speed / 3.).min(1.) * if two_handed { 0.3 } else { 1. };
        let hip_yaw = 0.07 * gait * phase.cos();
        let pelvis =
            Mat4::from_translation(vec3(0., hh, hz)) * Mat4::from_rotation_y(hip_yaw) * Mat4::from_rotation_x(-0.2 * c);

        // ---- torso: lean, twist, per-action motion ----
        let mut lean = 0.05 * (1. - c) + 0.55 * c + 0.07 * (speed / 6.).min(1.) - 0.25 * pitch + 0.10 * aim;
        // shoulders turn against the hips; a bladed stance behind the weapon when aiming
        let mut twist = -0.16 * gait * phase.cos();
        if two_handed {
            twist -= (0.10 + 0.20 * aim) * alive;
        }
        if throwing > 0. {
            lean += -0.30 * smooth(0., 0.3, throwing) * (1. - smooth(0.3, 0.6, throwing))
                + 0.4 * smooth(0.5, 0.75, throwing) * (1. - smooth(0.8, 1., throwing));
            twist = 0.55 * smooth(0., 0.35, throwing) * (1. - smooth(0.35, 0.6, throwing))
                - 0.35 * smooth(0.5, 0.7, throwing) * (1. - smooth(0.75, 1., throwing));
        }
        if swing > 0. {
            lean += -0.2 * smooth(0., 0.3, swing) * (1. - smooth(0.3, 0.5, swing))
                + 0.45 * smooth(0.3, 0.55, swing) * (1. - smooth(0.7, 1., swing));
            twist = 0.6 * smooth(0., 0.3, swing) * (1. - smooth(0.3, 0.5, swing))
                - 0.45 * smooth(0.35, 0.55, swing) * (1. - smooth(0.7, 1., swing));
        }
        lean += 0.25 * dead * (1. - smooth(0.3, 0.8, dead)) - 0.1 * smooth(0.6, 1., dead);
        let torso = pelvis
            * Mat4::from_translation(vec3(0., WAIST, 0.))
            * Mat4::from_rotation_y(twist)
            * Mat4::from_rotation_x(-lean);
        let head_up = (pitch + lean + 0.2 * c).clamp(-1.0, 1.0) * alive + 0.3 * (1. - alive);
        // head onto the weapon: cheek down towards the stock, turned back to face the target
        let cheek = if two_handed { aim * alive } else { 0. };
        let head = torso
            * Mat4::from_translation(vec3(0.03 * cheek, NECK_Y - 0.03 * cheek, -0.02 * cheek))
            * Mat4::from_rotation_y(-twist * 0.85)
            * Mat4::from_rotation_z(-0.13 * cheek)
            * Mat4::from_rotation_x(head_up);

        // ---- legs: IK in the sagittal plane ----
        let mut thigh = [Mat4::IDENTITY; 2];
        let mut shin = [Mat4::IDENTITY; 2];
        let mut boot = [Mat4::IDENTITY; 2];
        for i in 0..2 {
            let side = if i == 0 { 1. } else { -1. };
            let hip = pelvis.transform_point3(vec3(side * HIP_X, 0., 0.));
            let v = foot[i] - hip;
            let fwd = -v.z;
            let down = -v.y;
            let d = (fwd * fwd + down * down).sqrt().clamp(0.15, THIGH + SHIN - 0.003);
            let line = fwd.atan2(down);
            let a = ((THIGH * THIGH + d * d - SHIN * SHIN) / (2. * THIGH * d)).clamp(-1., 1.).acos();
            let b = ((SHIN * SHIN + d * d - THIGH * THIGH) / (2. * SHIN * d)).clamp(-1., 1.).acos();
            let th = line + a;
            let knee_flex = a + b;
            let hip_m = Mat4::from_translation(hip) * Mat4::from_rotation_x(th);
            let knee_m = hip_m * Mat4::from_translation(vec3(0., -THIGH, 0.)) * Mat4::from_rotation_x(-knee_flex);
            thigh[i] = hip_m;
            shin[i] = knee_m;
            let toe = toes[i];
            let ankle = knee_m * Mat4::from_translation(vec3(0., -SHIN, 0.));
            let abs_pitch = th - knee_flex; // pitch of the shin in character space
            boot[i] = ankle * Mat4::from_rotation_x(-abs_pitch - toe);
        }

        // ---- arms ----
        let sh_local = |i: usize| vec3(if i == 0 { SHOULDER_X } else { -SHOULDER_X }, SHOULDER_Y, 0.);
        let sh = [torso.transform_point3(sh_local(0)), torso.transform_point3(sh_local(1))];
        let mut wrist = [None::<(Vec3, Mat4)>; 2]; // IK: wrist point and palm frame
        let mut mount = Mat4::IDENTITY;
        let mut have_mount = false;

        if two_handed || matches!(hold, Hold::Grenade | Hold::Melee) {
            let bob = 0.008 * moving * (2. * phase).sin();
            let droop = -0.10 * (1. - aim) - 0.35 * smooth(4.5, 6.5, speed) * (1. - aim);
            let mut wp = pitch + droop + 0.07 * recoil;
            let mut roll = 0.;
            let right_frame: Mat4;
            match hold {
                Hold::Grenade => {
                    let rest = vec3(-0.03, -0.30, -0.27);
                    let p = if throwing > 0. {
                        keys(
                            throwing,
                            &[
                                (0., rest),
                                (0.3, vec3(0.06, 0.12, 0.20)),
                                (0.45, vec3(0.10, 0.20, 0.26)),
                                (0.62, vec3(0.02, 0.22, -0.42)),
                                (1., rest),
                            ],
                        )
                    } else {
                        rest
                    };
                    right_frame = torso * Mat4::from_translation(sh_local(0) + p);
                }
                Hold::Melee => {
                    let th = if swing > 0. {
                        keys(
                            swing,
                            &[
                                (0., vec3(-0.45, 0., 0.)),
                                (0.3, vec3(1.7, 0., 0.)),
                                (0.55, vec3(-0.9, 0., 0.)),
                                (1., vec3(-0.45, 0., 0.)),
                            ],
                        )
                        .x
                    } else {
                        -0.45
                    };
                    let local = sh_local(0)
                        + vec3(-0.03, 0., 0.)
                        + Mat4::from_rotation_x(th).transform_vector3(vec3(0., 0., -0.45));
                    right_frame = torso * Mat4::from_translation(local) * Mat4::from_rotation_x(th);
                }
                _ => {
                    let (hip_off, aim_off) = match hold {
                        Hold::Pistol => (vec3(-0.15, -0.32, -0.30), vec3(-0.17, 0.0, -0.46)),
                        Hold::Smg => (vec3(-0.12, -0.30, -0.22), vec3(-0.12, 0.02, -0.28)),
                        Hold::Rifle => (vec3(-0.12, -0.30, -0.22), vec3(-0.11, 0.03, -0.28)),
                        Hold::Sniper => (vec3(-0.12, -0.31, -0.22), vec3(-0.10, 0.03, -0.28)),
                        _ => (vec3(-0.05, -0.17, -0.22), vec3(-0.03, -0.11, -0.25)), // launcher on the shoulder
                    };
                    let mut off = hip_off.lerp(aim_off, aim);
                    off.y += bob;
                    off.z += 0.06 * recoil;
                    if hold == Hold::Launcher {
                        wp = pitch + 0.05 * recoil;
                    }
                    if reload > 0. {
                        let bump = (std::f32::consts::PI * reload).sin();
                        wp -= 0.25 * bump * (1. - 0.6 * aim);
                        roll = 0.45 * bump;
                        off.y -= 0.04 * bump;
                    }
                    // the aim frame sits on the right shoulder and follows the look pitch, not the body lean
                    right_frame = Mat4::from_translation(sh[0])
                        * Mat4::from_rotation_x(wp)
                        * Mat4::from_translation(off)
                        * Mat4::from_rotation_z(roll);
                }
            }
            mount = right_frame;
            have_mount = true;
            let grip_pos = right_frame.transform_point3(Vec3::ZERO);
            wrist[0] = Some((grip_pos + right_frame.transform_vector3(wrist_offset(false)), right_frame));

            // support hand
            if let Some((local, style)) = support_of(hold) {
                let rot = match style {
                    Support::Cup => cup_rot(),
                    Support::Wrap => Mat4::IDENTITY,
                };
                let base = right_frame;
                let mut p = base.transform_point3(local);
                if reload > 0. {
                    let mw = base.transform_point3(magwell_of(hold));
                    let pouch = torso.transform_point3(vec3(-0.15, 0.20, -0.17));
                    let rest = p;
                    p = keys(
                        reload,
                        &[
                            (0., rest),
                            (0.18, mw),
                            (0.42, pouch),
                            (0.66, mw + base.transform_vector3(vec3(0., 0.03, 0.))),
                            (0.80, mw),
                            (1., rest),
                        ],
                    );
                }
                let frame = at_palm(p, base * rot);
                let w = p + (base * rot).transform_vector3(wrist_offset(true));
                wrist[1] = Some((w, frame));
            }
        }

        let mut uarm = [Mat4::IDENTITY; 2];
        let mut farm = [Mat4::IDENTITY; 2];
        let mut hand = [Mat4::IDENTITY; 2];
        for i in 0..2 {
            let side = if i == 0 { 1. } else { -1. };
            if let Some((w, frame)) = wrist[i] {
                let pole = vec3(0.55 * side, -1., 0.35);
                let e = elbow(sh[i], w, UARM, FARM, pole);
                uarm[i] = limb(sh[i], e);
                farm[i] = limb(e, w);
                let rot = Mat4::from_cols(frame.x_axis, frame.y_axis, frame.z_axis, Vec4::W);
                let off = rot.transform_vector3(wrist_offset(i == 1));
                hand[i] = at_palm(w - off, rot);
            } else {
                // free arm, forward kinematics
                let ph = phase + i as f32 * std::f32::consts::PI;
                let amp = moving * (0.15 + 0.11 * speed).min(0.75) * (1. - 0.5 * c);
                let mut a = -amp * ph.cos(); // the arm swings against the same-side leg
                let mut flex = 0.25 + moving * (0.04 * speed + 0.55 * smooth(2.5, 6., speed)) + 0.3 * a.max(0.);
                let mut out = 0.13;
                if air {
                    a = 0.5;
                    flex = 0.6;
                    out = 0.45;
                }
                if hold == Hold::Grenade || hold == Hold::Melee {
                    // the free hand guards the chest
                    a = 0.55;
                    flex = 1.55;
                    out = -0.1;
                    if hold == Hold::Grenade && throwing > 0.25 && throwing < 0.7 {
                        let k = smooth(0.25, 0.4, throwing) * (1. - smooth(0.55, 0.7, throwing));
                        a = 0.55 + 0.6 * k;
                        flex = 1.55 - 1.2 * k;
                        out = -0.1 + 0.7 * k;
                    }
                }
                if dead > 0. {
                    a = 0.25 * dead * (1. - dead);
                    flex = 0.25 + 0.5 * dead * (1. - dead) - 0.2 * dead;
                    out = 0.10 + 0.30 * dead;
                }
                let s = torso
                    * Mat4::from_translation(sh_local(i))
                    * Mat4::from_rotation_z(side * out)
                    * Mat4::from_rotation_x(a);
                let el = s * Mat4::from_translation(vec3(0., -UARM, 0.)) * Mat4::from_rotation_x(flex);
                uarm[i] = s;
                farm[i] = el;
                let w = el.transform_point3(vec3(0., -FARM, 0.));
                let rot = Mat4::from_cols(el.x_axis, el.y_axis, el.z_axis, Vec4::W) * free_hand_rot();
                let off = rot.transform_vector3(wrist_offset(i == 1));
                hand[i] = at_palm(w - off, rot);
            }
        }
        if !have_mount {
            mount = hand[0];
        }

        // ---- the whole body: collapse on the ground ----
        let mut sk = Skel { pelvis, torso, head, uarm, farm, hand, thigh, shin, boot, mount };
        if dead > 0. {
            let ang = smooth(0.25, 1.0, dead) * std::f32::consts::FRAC_PI_2;
            let root = Mat4::from_translation(vec3(0., 0.27 * ang.sin(), 0.)) * Mat4::from_rotation_x(ang);
            sk.pelvis = root * sk.pelvis;
            sk.torso = root * sk.torso;
            sk.head = root * sk.head;
            sk.mount = root * sk.mount;
            for i in 0..2 {
                sk.uarm[i] = root * sk.uarm[i];
                sk.farm[i] = root * sk.farm[i];
                sk.hand[i] = root * sk.hand[i];
                sk.thigh[i] = root * sk.thigh[i];
                sk.shin[i] = root * sk.shin[i];
                sk.boot[i] = root * sk.boot[i];
            }
        }
        sk
    }
}

// ---------------------------------------------------------------------------------------------------------
// Building the parts
// ---------------------------------------------------------------------------------------------------------

/// A box turned by `rot` about its own centre.
fn obox(t: &mut Template, center: Vec3, rot: Quat, half: Vec3, c: Rgb) {
    let mut b = Template::new();
    b.box_(Vec3::ZERO, half, c, 0.);
    t.append(&b.transformed(Mat4::from_rotation_translation(rot, center)));
}

/// A flat strap (width `hw` along X, thickness `ht`) running from `a` to `b`.
fn strap(t: &mut Template, a: Vec3, b: Vec3, hw: f32, ht: f32, c: Rgb) {
    let d = b - a;
    let len = d.length();
    if len < 1e-5 {
        return;
    }
    obox(t, (a + b) * 0.5, Quat::from_rotation_arc(Vec3::Z, d / len), vec3(hw, ht, len * 0.5 + 0.004), c);
}

fn bx(t: &mut Template, cx: f32, cy: f32, cz: f32, hx: f32, hy: f32, hz: f32, c: Rgb) {
    t.box_(vec3(cx, cy, cz), vec3(hx, hy, hz), c, 0.);
}

/// Mirrored pair of boxes (x and -x).
#[allow(clippy::too_many_arguments)]
fn bx2(t: &mut Template, cx: f32, cy: f32, cz: f32, hx: f32, hy: f32, hz: f32, c: Rgb) {
    bx(t, cx, cy, cz, hx, hy, hz, c);
    bx(t, -cx, cy, cz, hx, hy, hz, c);
}

fn ball(t: &mut Template, cx: f32, cy: f32, cz: f32, rx: f32, ry: f32, rz: f32, c: Rgb, segs: usize) {
    t.ball(vec3(cx, cy, cz), vec3(rx, ry, rz), c, 0., segs, (segs / 2).max(4));
}

/// A smooth lofted surface: elliptical sections `[y, cx, cz, rx, rz]` (height, centre, half-widths), with
/// normals taken from the surface itself. Rings may be listed top-down or bottom-up. `caps` closes both ends.
fn loft(t: &mut Template, rings: &[[f32; 5]], c: Rgb, sides: usize, caps: bool) {
    let mut rs: Vec<[f32; 5]> = rings.to_vec();
    if rs[0][0] > rs[rs.len() - 1][0] {
        rs.reverse();
    }
    let base = t.verts.len();
    let w = sides + 1;
    let pt = |j: usize, i: usize| {
        let r = rs[j];
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        vec3(r[1] + r[3] * a.cos(), r[0], r[2] + r[4] * a.sin())
    };
    for j in 0..rs.len() {
        for i in 0..=sides {
            let a = i as f32 / sides as f32 * std::f32::consts::TAU;
            let r = rs[j];
            let da = vec3(-r[3] * a.sin(), 0., r[4] * a.cos());
            let dy = pt((j + 1).min(rs.len() - 1), i) - pt(j.saturating_sub(1), i);
            let mut n = dy.cross(da).normalize_or_zero();
            if n == Vec3::ZERO {
                n = if j == 0 { -Vec3::Y } else { Vec3::Y };
            }
            t.verts.push(vertex(pt(j, i), n, c));
        }
    }
    for j in 0..rs.len() - 1 {
        for i in 0..sides {
            let a = (base + j * w + i) as u16;
            let w = w as u16;
            t.idx.extend_from_slice(&[a + w, a, a + w + 1, a + w + 1, a, a + 1]);
        }
    }
    if caps {
        for (j, up) in [(0usize, false), (rs.len() - 1, true)] {
            let r = rs[j];
            if r[3] < 0.002 {
                continue;
            }
            let nrm = if up { Vec3::Y } else { -Vec3::Y };
            let hub = t.verts.len() as u16;
            t.verts.push(vertex(vec3(r[1], r[0], r[2]), nrm, c));
            for i in 0..=sides {
                t.verts.push(vertex(pt(j, i), nrm, c));
            }
            for i in 0..sides as u16 {
                if up {
                    t.idx.extend_from_slice(&[hub, hub + 2 + i, hub + 1 + i]);
                } else {
                    t.idx.extend_from_slice(&[hub, hub + 1 + i, hub + 2 + i]);
                }
            }
        }
    }
}

fn vertex(p: Vec3, n: Vec3, c: Rgb) -> vesper3d::viewer::kit::Vert {
    vesper3d::viewer::kit::Vert { p, n, c, e: 0., a: 1. }
}

/// A loft laid along the character's forward axis: ring `y` becomes distance forward (towards -Z), `cz` the height.
fn loft_fwd(t: &mut Template, rings: &[[f32; 5]], c: Rgb, sides: usize) {
    let mut s = Template::new();
    loft(&mut s, rings, c, sides, true);
    t.append(&s.transformed(basis(Vec3::X, -Vec3::Z, Vec3::Y)));
}

fn build_pelvis(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    // hips and seat
    loft(
        &mut t,
        &[
            [-0.15, 0., 0.0, 0.12, 0.085],
            [-0.10, 0., 0.006, 0.172, 0.112],
            [-0.03, 0., 0.008, 0.190, 0.122],
            [0.04, 0., 0.0, 0.182, 0.115],
            [0.075, 0., 0.0, 0.174, 0.110],
        ],
        p.trousers,
        16,
        false,
    );
    // belt, buckle, pouches, holster
    loft(&mut t, &[[0.022, 0., 0., 0.191, 0.124], [0.074, 0., 0., 0.191, 0.124]], p.strap, 16, false);
    bx(&mut t, 0., 0.048, -0.124, 0.027, 0.023, 0.007, p.metal);
    bx2(&mut t, 0.098, 0.040, -0.128, 0.036, 0.046, 0.024, p.vest);
    bx2(&mut t, 0.098, 0.082, -0.128, 0.038, 0.010, 0.026, p.vest_dark);
    bx(&mut t, 0., 0.036, 0.130, 0.075, 0.042, 0.03, p.vest);
    bx(&mut t, 0., 0.080, 0.130, 0.077, 0.009, 0.032, p.vest_dark);
    match team {
        Team::Ironclad => {
            // tan canteen on the right hip, holster pouch on the left
            loft(
                &mut t,
                &[
                    [-0.115, 0.205, 0.07, 0.030, 0.05],
                    [-0.03, 0.205, 0.07, 0.036, 0.062],
                    [0.045, 0.205, 0.07, 0.030, 0.052],
                ],
                p.vest,
                10,
                true,
            );
            bx(&mut t, 0.205, 0.058, 0.07, 0.020, 0.015, 0.02, p.metal);
            bx(&mut t, 0.200, -0.03, 0.07, 0.039, 0.012, 0.066, p.vest_dark);
            bx(&mut t, -0.197, -0.02, 0.01, 0.026, 0.085, 0.05, p.vest_dark);
            bx(&mut t, -0.197, 0.07, 0.01, 0.028, 0.012, 0.052, p.strap);
        }
        Team::Nightwatch => {
            // left utility pouch, radio pouch
            bx(&mut t, -0.197, 0.0, 0.03, 0.026, 0.065, 0.055, p.vest_dark);
            bx(&mut t, -0.197, 0.07, 0.03, 0.028, 0.012, 0.057, p.plate);
            bx(&mut t, 0.197, -0.005, 0.07, 0.024, 0.05, 0.045, p.vest_dark);
        }
    }
    t
}

fn build_torso(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    // the shirt: waist, V-taper to the chest, shoulder line, trapezius slope into the neck
    loft(
        &mut t,
        &[
            [-0.06, 0., 0., 0.172, 0.112],
            [0.05, 0., 0.0, 0.168, 0.110],
            [0.15, 0., -0.004, 0.186, 0.118],
            [0.25, 0., -0.008, 0.215, 0.134],
            [0.33, 0., -0.010, 0.230, 0.141],
            [0.41, 0., -0.006, 0.236, 0.130],
            [0.45, 0., 0.0, 0.205, 0.112],
            [0.480, 0., 0.0, 0.130, 0.086],
            [0.505, 0., 0.0, 0.078, 0.070],
            [0.512, 0., 0.0, 0.066, 0.064],
        ],
        p.uniform,
        18,
        false,
    );
    // plate carrier body and cummerbund
    loft(
        &mut t,
        &[[0.03, 0., 0., 0.184, 0.122], [0.20, 0., -0.003, 0.208, 0.140], [0.37, 0., -0.008, 0.246, 0.155]],
        p.vest,
        18,
        false,
    );
    loft(&mut t, &[[0.03, 0., 0., 0.187, 0.126], [0.14, 0., -0.002, 0.200, 0.134]], p.vest_dark, 18, false);
    // front and back plates (bevelled)
    let plate = |t: &mut Template, z: f32, h: f32, c: Rgb| {
        bx(t, 0., 0.245, z, 0.152, h, 0.020, c);
        bx(t, 0., 0.245 + h, z * 0.99, 0.125, 0.012, 0.018, c);
        bx(t, 0., 0.245 - h - 0.002, z * 0.99, 0.12, 0.014, 0.018, c);
    };
    plate(&mut t, -0.164, 0.125, p.plate);
    plate(&mut t, 0.164, 0.125, mul(p.plate, 0.9));
    // shoulder straps: front, over the trapezius, back
    for s in [1., -1.] {
        let x = 0.125 * s;
        let pts = [
            vec3(x, 0.325, -0.155),
            vec3(x, 0.455, -0.082),
            vec3(x, 0.490, 0.0),
            vec3(x, 0.455, 0.082),
            vec3(x, 0.325, 0.156),
        ];
        for k in 0..4 {
            strap(&mut t, pts[k], pts[k + 1], 0.034, 0.008, p.vest);
        }
        // shoulder pad
        bx(&mut t, s * 0.17, 0.452, 0., 0.03, 0.01, 0.075, p.vest_dark);
    }
    match team {
        Team::Ironclad => {
            // load-bearing vest: four double magazine pouches, chest pockets, webbing, back plate, pack, bedroll
            for k in 0..4 {
                let x = -0.108 + 0.072 * k as f32;
                bx(&mut t, x, 0.175, -0.172, 0.031, 0.062, 0.026, p.vest);
                bx(&mut t, x, 0.232, -0.176, 0.034, 0.013, 0.028, p.vest_dark);
                bx(&mut t, x, 0.212, -0.204, 0.007, 0.012, 0.004, p.metal);
            }
            bx(&mut t, 0., 0.32, -0.170, 0.16, 0.012, 0.02, p.vest_dark); // webbing row
            bx2(&mut t, 0.11, 0.37, -0.176, 0.04, 0.032, 0.016, p.vest_dark); // chest pockets
            bx(&mut t, 0.0, 0.09, -0.152, 0.10, 0.035, 0.022, p.vest); // utility pouch
            bx(&mut t, 0.0, 0.16, 0.20, 0.115, 0.10, 0.05, p.vest); // small pack
            bx(&mut t, 0.0, 0.265, 0.205, 0.11, 0.015, 0.052, p.vest_dark);
            bx(&mut t, 0.0, 0.16, 0.255, 0.08, 0.07, 0.006, p.vest_dark);
            // bedroll across the top
            let mut roll = Template::new();
            loft(&mut roll, &[[-0.19, 0., 0., 0.052, 0.052], [0.19, 0., 0., 0.052, 0.052]], p.strap, 10, true);
            t.append(&roll.transformed(
                Mat4::from_translation(vec3(0., 0.375, 0.2)) * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ));
            bx2(&mut t, 0.1, 0.375, 0.2, 0.012, 0.056, 0.056, p.vest_dark);
        }
        Team::Nightwatch => {
            // plate carrier: pouches on the cummerbund, admin panel, radio, hydration / assault pack
            for k in 0..4 {
                let x = -0.108 + 0.072 * k as f32;
                bx(&mut t, x, 0.145, -0.172, 0.030, 0.052, 0.024, p.vest_dark);
                bx(&mut t, x, 0.192, -0.175, 0.032, 0.010, 0.026, p.plate);
            }
            bx(&mut t, 0., 0.055, -0.150, 0.075, 0.035, 0.022, p.vest_dark); // utility pouch
            bx(&mut t, 0., 0.40, -0.172, 0.08, 0.02, 0.016, p.vest_dark); // admin panel
            bx(&mut t, -0.15, 0.47, -0.02, 0.026, 0.045, 0.032, p.vest_dark); // radio
            bx(&mut t, -0.15, 0.56, -0.02, 0.004, 0.06, 0.004, p.metal);
            bx(&mut t, 0., 0.30, 0.215, 0.125, 0.15, 0.058, p.plate); // assault pack
            bx(&mut t, 0., 0.30, 0.276, 0.10, 0.12, 0.006, p.vest_dark);
            bx(&mut t, 0., 0.455, 0.215, 0.10, 0.02, 0.06, p.vest_dark); // lid
            bx2(&mut t, 0.14, 0.30, 0.215, 0.012, 0.12, 0.05, p.vest_dark);
            bx2(&mut t, 0.07, 0.40, 0.268, 0.008, 0.03, 0.006, p.metal);
        }
    }
    t
}

fn build_head(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    // pivot is the top of the neck (under the jaw); the skull centre is 0.115 above it
    let neck_c = if team == Team::Nightwatch { p.vest_dark } else { p.skin_dark };
    loft(
        &mut t,
        &[[-0.085, 0., 0.006, 0.062, 0.060], [0.0, 0., 0.0, 0.050, 0.050], [0.05, 0., -0.01, 0.050, 0.052]],
        neck_c,
        12,
        false,
    );
    let sk = 0.115;
    // skull, cheeks, jaw wedge and chin
    ball(&mut t, 0., sk, 0.006, 0.078, 0.100, 0.093, p.skin, 16);
    ball(&mut t, 0., sk - 0.052, -0.016, 0.066, 0.056, 0.074, p.skin, 14);
    ball(&mut t, 0., sk - 0.084, -0.052, 0.034, 0.024, 0.030, p.skin, 10);
    // brow, nose, mouth, eyes, ears
    ball(&mut t, 0., sk + 0.030, -0.080, 0.064, 0.014, 0.020, mul(p.skin, 0.88), 10);
    obox(&mut t, vec3(0., sk - 0.016, -0.094), Quat::from_rotation_x(0.25), vec3(0.010, 0.024, 0.012), p.skin);
    ball(&mut t, 0., sk - 0.040, -0.104, 0.014, 0.010, 0.012, p.skin, 6);
    bx(&mut t, 0., sk - 0.066, -0.082, 0.024, 0.004, 0.006, mul(p.skin, 0.6));
    bx2(&mut t, 0.032, sk + 0.010, -0.083, 0.011, 0.006, 0.005, [0.08, 0.08, 0.10]);
    ball(&mut t, 0.079, sk - 0.008, 0.012, 0.011, 0.026, 0.019, p.skin_dark, 8);
    ball(&mut t, -0.079, sk - 0.008, 0.012, 0.011, 0.026, 0.019, p.skin_dark, 8);

    // ---- helmet: an elliptical dome of smooth rings, rim, nape cover, ear flaps, chin strap, mount ----
    let hb = 0.158; // rim height above the pivot
    let (rx, rz, cz, hh) = (0.108, 0.128, 0.010, 0.110);
    let h = p.helmet;
    let dome = |k: f32| -> [f32; 5] {
        // k in 0..=1 round the quarter ellipse
        let a = k * std::f32::consts::FRAC_PI_2;
        let s = (a.cos()).max(0.0);
        [hb + hh * a.sin(), 0., cz, rx * s, rz * s]
    };
    let ks = [0.0, 0.12, 0.26, 0.42, 0.58, 0.74, 0.88, 1.0];
    let shell: Vec<[f32; 5]> = ks.iter().map(|&k| dome(k)).collect();
    loft(&mut t, &shell, h, 18, true);
    // rim and nape cover, ear flaps
    loft(
        &mut t,
        &[[hb - 0.012, 0., cz, rx + 0.006, rz + 0.006], [hb + 0.014, 0., cz, rx + 0.006, rz + 0.006]],
        mul(h, 0.8),
        18,
        false,
    );
    bx(&mut t, 0., hb - 0.030, 0.118, 0.085, 0.032, 0.012, mul(h, 0.9));
    bx2(&mut t, 0.106, hb - 0.046, 0.045, 0.006, 0.024, 0.036, mul(h, 0.9));
    // chin strap and cheek straps
    let st = p.strap;
    bx2(&mut t, 0.081, sk - 0.02, -0.02, 0.004, 0.06, 0.007, st);
    bx(&mut t, 0., sk - 0.092, -0.056, 0.045, 0.006, 0.010, st);
    bx2(&mut t, 0.045, sk - 0.090, -0.040, 0.006, 0.006, 0.026, st);
    // front mount plate
    bx(&mut t, 0., hb + 0.05, -0.128, 0.032, 0.026, 0.008, p.metal);

    match team {
        Team::Ironclad => {
            // tan net band round the shell and arches over the crown; goggles pushed up on the front
            loft(
                &mut t,
                &[[hb + 0.018, 0., cz, rx + 0.004, rz + 0.004], [hb + 0.04, 0., cz, rx * 0.985, rz * 0.985]],
                p.net,
                18,
                false,
            );
            for (dx, dz) in [(1., 0.), (0., 1.), (0.7, 0.7), (0.7, -0.7)] {
                arch(&mut t, (hb, rx, rz, cz, hh), dx, dz, p.net);
            }
            bx(&mut t, 0., hb + 0.055, -0.133, 0.052, 0.008, 0.005, p.strap);
            bx2(&mut t, 0.028, hb + 0.050, -0.142, 0.021, 0.015, 0.008, [0.10, 0.10, 0.11]);
            bx2(&mut t, 0.028, hb + 0.050, -0.149, 0.016, 0.011, 0.004, [0.85, 0.62, 0.15]);
        }
        Team::Nightwatch => {
            // neck gaiter over the lower face, dark glasses, NVG mount and rails
            loft(
                &mut t,
                &[
                    [-0.045, 0., -0.004, 0.057, 0.061],
                    [0.0, 0., -0.012, 0.067, 0.078],
                    [0.050, 0., -0.030, 0.066, 0.074],
                    [0.082, 0., -0.036, 0.052, 0.062],
                ],
                p.vest_dark,
                14,
                false,
            );
            bx(&mut t, 0., sk + 0.010, -0.090, 0.060, 0.012, 0.005, [0.03, 0.03, 0.04]);
            bx2(&mut t, 0.030, sk + 0.010, -0.094, 0.023, 0.012, 0.003, [0.12, 0.16, 0.22]);
            bx(&mut t, 0., hb + 0.07, -0.122, 0.028, 0.02, 0.022, p.plate); // NVG mount
            bx(&mut t, 0., hb + 0.07, -0.145, 0.018, 0.012, 0.008, [0.04, 0.05, 0.05]);
            bx2(&mut t, 0.108, hb + 0.012, 0.0, 0.007, 0.012, 0.06, p.plate); // rails
        }
    }
    t
}

/// A net strap over the crown: short boxes following the dome, in the vertical plane given by (dx, dz).
fn arch(t: &mut Template, dome: (f32, f32, f32, f32, f32), dx: f32, dz: f32, c: Rgb) {
    let (hb, rx, rz, cz, hh) = dome;
    let dir = vec2(dx, dz).normalize();
    let n = 7;
    let pt = |k: usize| {
        let a = k as f32 / n as f32 * std::f32::consts::PI;
        let ring = vec2(a.cos() * (rx + 0.004) * dir.x, a.cos() * (rz + 0.004) * dir.y);
        vec3(ring.x, hb + 0.03 + a.sin() * (hh + 0.002 - 0.03).max(0.05) * 1.0, cz + ring.y)
    };
    for k in 0..n {
        let (a, b) = (pt(k), pt(k + 1));
        strap(t, a, b, 0.008, 0.004, c);
    }
}

fn build_upper_arm(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    let u = p.uniform;
    let pad = if team == Team::Ironclad { p.vest } else { p.vest_dark };
    ball(&mut t, 0., -0.004, 0., 0.069, 0.070, 0.066, u, 12); // deltoid
    loft(
        &mut t,
        &[
            [-0.02, 0., 0., 0.060, 0.060],
            [-0.09, 0., -0.003, 0.061, 0.065],
            [-0.17, 0., 0., 0.053, 0.054],
            [-UARM, 0., 0., 0.043, 0.043],
        ],
        u,
        12,
        false,
    );
    // rolled sleeve fold and a patch
    loft(&mut t, &[[-0.165, 0., 0., 0.055, 0.056], [-0.19, 0., 0., 0.054, 0.055]], mul(u, 0.82), 12, false);
    ball(&mut t, 0., -UARM, 0.004, 0.046, 0.046, 0.046, u, 10); // elbow
    bx(&mut t, 0.063, -0.08, 0., 0.004, 0.025, 0.022, pad);
    t
}

fn build_forearm(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    let u = p.uniform;
    let pad = if team == Team::Ironclad { p.vest } else { p.vest_dark };
    loft(
        &mut t,
        &[
            [0.0, 0., 0., 0.044, 0.044],
            [-0.05, 0., 0., 0.047, 0.047],
            [-0.13, 0., 0., 0.042, 0.040],
            [-0.21, 0., 0., 0.034, 0.033],
            [-FARM + 0.01, 0., 0., 0.031, 0.031],
        ],
        u,
        12,
        false,
    );
    ball(&mut t, 0., 0., 0.032, 0.036, 0.036, 0.032, pad, 8); // elbow pad
                                                              // the glove's gauntlet over the cuff
    t.cone(vec3(0., -FARM - 0.02, 0.), 0.034, 0.038, 0.075, p.glove, 0., 12);
    t.cone(vec3(0., -FARM + 0.04, 0.), 0.040, 0.040, 0.012, p.glove_dark, 0., 12);
    t
}

fn build_thigh(p: &Palette, team: Team, side: f32) -> Template {
    let mut t = Template::new();
    let tr = p.trousers;
    ball(&mut t, 0., 0., 0., 0.098, 0.098, 0.100, tr, 12); // hip joint
    loft(
        &mut t,
        &[
            [-0.03, 0., 0., 0.099, 0.102],
            [-0.12, 0., 0.0, 0.097, 0.100],
            [-0.26, 0., -0.002, 0.085, 0.088],
            [-0.37, 0., -0.004, 0.073, 0.075],
            [-THIGH, 0., 0., 0.065, 0.065],
        ],
        tr,
        14,
        false,
    );
    // cargo pocket on the outer side with a flap, and a seam fold
    let pk = mul(tr, 0.86);
    bx(&mut t, 0.097 * side, -0.215, -0.012, 0.016, 0.065, 0.050, pk);
    bx(&mut t, 0.105 * side, -0.165, -0.012, 0.011, 0.014, 0.052, mul(tr, 0.7));
    if team == Team::Nightwatch && side > 0. {
        // drop-leg holster
        bx(&mut t, 0.115, -0.19, 0.0, 0.016, 0.095, 0.052, p.vest_dark);
        bx(&mut t, 0.115, -0.30, 0.0, 0.020, 0.012, 0.056, p.plate);
        bx(&mut t, 0.115, -0.10, 0.0, 0.020, 0.012, 0.056, p.strap);
    }
    t
}

fn build_shin(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    let tr = p.trousers;
    ball(&mut t, 0., 0., 0., 0.069, 0.069, 0.069, tr, 10); // knee
    loft(
        &mut t,
        &[
            [-0.02, 0., 0., 0.068, 0.069],
            [-0.12, 0., 0.012, 0.064, 0.074],
            [-0.21, 0., 0.010, 0.058, 0.065],
            [-0.28, 0., 0.0, 0.049, 0.051],
            [-0.32, 0., 0.0, 0.050, 0.050],
        ],
        tr,
        14,
        false,
    );
    // knee pad
    let pad = if team == Team::Ironclad { p.vest } else { p.vest_dark };
    ball(&mut t, 0., -0.005, -0.062, 0.054, 0.064, 0.030, pad, 10);
    ball(&mut t, 0., -0.005, -0.084, 0.039, 0.047, 0.010, mul(pad, 0.8), 8);
    // bloused cuff over the boot top
    loft(
        &mut t,
        &[
            [-0.25, 0., 0., 0.051, 0.051],
            [-0.305, 0., 0., 0.068, 0.070],
            [-0.348, 0., 0., 0.067, 0.069],
            [-0.352, 0., 0., 0.057, 0.059],
        ],
        mul(tr, 0.93),
        14,
        false,
    );
    t
}

fn build_boot(p: &Palette) -> Template {
    let mut t = Template::new();
    // the ankle joint is the origin; the sole's underside is 0.10 below it, toe towards -Z
    loft(
        &mut t,
        &[[-0.06, 0., 0.0, 0.056, 0.060], [0.0, 0., 0.0, 0.054, 0.058], [0.10, 0., 0.0, 0.058, 0.060]],
        p.boot,
        12,
        false,
    );
    // foot: heel, arch, ball of the foot, toe box (stations from the heel forward, then along -Z)
    let foot: [[f32; 5]; 7] = [
        [-0.092, 0., -0.052, 0.040, 0.036],
        [-0.065, 0., -0.050, 0.052, 0.042],
        [0.0, 0., -0.052, 0.054, 0.040],
        [0.07, 0., -0.058, 0.057, 0.034],
        [0.14, 0., -0.062, 0.057, 0.030],
        [0.20, 0., -0.066, 0.046, 0.026],
        [0.222, 0., -0.068, 0.020, 0.016],
    ];
    loft_fwd(&mut t, &foot, p.boot, 12);
    // toe cap
    let cap = mul(p.boot, 0.8);
    ball(&mut t, 0., -0.062, -0.17, 0.052, 0.030, 0.050, cap, 10);
    // thick sole with a heel block
    let sole: [[f32; 5]; 6] = [
        [-0.094, 0., -0.085, 0.040, 0.014],
        [-0.075, 0., -0.085, 0.056, 0.015],
        [0.0, 0., -0.085, 0.060, 0.015],
        [0.12, 0., -0.085, 0.062, 0.015],
        [0.205, 0., -0.085, 0.052, 0.014],
        [0.230, 0., -0.085, 0.024, 0.010],
    ];
    loft_fwd(&mut t, &sole, p.sole, 10);
    bx(&mut t, 0., -0.078, 0.060, 0.056, 0.020, 0.030, mul(p.sole, 1.2));
    // laces, tongue and a pull tab
    bx(&mut t, 0., 0.0, -0.050, 0.026, 0.040, 0.012, mul(p.boot, 0.75));
    bx(&mut t, 0., 0.10, 0.045, 0.014, 0.014, 0.008, mul(p.boot, 0.7));
    t
}

/// A glove in its own frame: palm centre at the origin, gripping an upright object (a pistol grip) with the
/// palm on the +X side, the four fingers wrapping round the front (-Z) to the -X side, the thumb along the
/// -X side; the back of the hand runs towards +Z/+Y (the wrist, see [`wrist_offset`]). `left` mirrors it.
/// `open` > 0.5 stretches the fingers out forward (a released throw).
pub(crate) fn glove(p: &Palette, left: bool, open: f32) -> Template {
    let sx = if left { -1. } else { 1. };
    let mut t = Template::new();
    let c = p.glove;
    let cd = p.glove_dark;
    let mut b = |cx: f32, cy: f32, cz: f32, hx: f32, hy: f32, hz: f32, col: Rgb| {
        t.box_(vec3(cx * sx, cy, cz), vec3(hx, hy, hz), col, 0.);
    };
    // palm plate, heel behind the grip, back of the hand towards the wrist
    b(0.030, 0.0, 0.004, 0.012, 0.040, 0.038, c);
    b(0.014, 0.0, 0.040, 0.030, 0.036, 0.012, c);
    b(0.030, 0.014, 0.058, 0.024, 0.034, 0.030, c);
    b(0.043, 0.0, 0.004, 0.003, 0.038, 0.034, cd); // knuckle / back-of-hand pad edge
                                                   // fingers
    for k in 0..4 {
        let y = 0.027 - 0.018 * k as f32;
        b(0.029, y, -0.046, 0.0115, 0.0085, 0.013, c);
        if open > 0.5 {
            b(0.029, y, -0.090, 0.0105, 0.0078, 0.032, c);
        } else {
            b(0.0, y, -0.060, 0.030, 0.0085, 0.0105, c);
            b(-0.034, y, -0.040, 0.0105, 0.0078, 0.022, cd);
        }
    }
    // thumb
    if open > 0.5 {
        b(-0.010, 0.050, -0.010, 0.011, 0.011, 0.034, c);
    } else {
        b(0.0, 0.048, 0.024, 0.022, 0.011, 0.012, c);
        b(-0.030, 0.048, -0.010, 0.011, 0.011, 0.032, cd);
    }
    t
}

/// A sleeve: a tapered tube hanging from its origin (the elbow end) along -Y for `len`; its far end is the
/// wrist, radius `r_end`, growing to `r_start` at the origin. The glove's gauntlet covers the wrist end.
pub(crate) fn sleeve(p: &Palette, len: f32, r_end: f32, r_start: f32) -> Template {
    let mut t = Template::new();
    t.cone(vec3(0., -len + 0.05, 0.), r_end, r_start, len - 0.05, p.uniform, 0., 10);
    t.cone(vec3(0., -len - 0.02, 0.), r_end + 0.003, r_end + 0.006, 0.075, p.glove, 0., 10);
    t.cone(vec3(0., -len + 0.045, 0.), r_end + 0.007, r_end + 0.007, 0.012, p.glove_dark, 0., 10);
    t
}

/// Placement of a [`sleeve`] whose far end is at `wrist` and which runs back along `dir` (unit, away from the wrist).
pub(crate) fn sleeve_mat(wrist: Vec3, dir: Vec3, len: f32) -> Mat4 {
    limb(wrist + dir * len, wrist)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(rig: &Rig, pose: &Pose) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(1e9);
        let mut hi = Vec3::splat(-1e9);
        for (t, m) in rig.parts(Vec3::ZERO, 0., pose) {
            for v in &t.verts {
                let p = m.transform_point3(v.p);
                assert!(p.is_finite(), "non-finite vertex");
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        (lo, hi)
    }

    fn walk() -> Pose {
        Pose { speed: 3., walk_phase: 1.3, ..Pose::default() }
    }

    #[test]
    fn rigs_build_for_both_teams_and_all_skins() {
        for team in Team::ALL {
            for skin in 0..=3u8 {
                let rig = Rig::new(team, skin);
                assert!(rig.vertex_count() > 500);
                assert!(rig.vertex_count() <= 9000, "{} vertices", rig.vertex_count());
            }
        }
        // out-of-range skin is clamped rather than panicking
        let _ = Rig::new(Team::Ironclad, 9);
    }

    #[test]
    fn draw_adds_geometry_inside_a_soldier_box() {
        for team in Team::ALL {
            let rig = Rig::new(team, 1);
            let mut batch = Batch::new();
            rig.draw(&mut batch, vec3(5., 0., 3.), 0.7, &Pose::default(), Tint::NONE);
            assert!(batch.vertex_count() >= rig.vertex_count());
            let (lo, hi) = bounds(&rig, &Pose::default());
            assert!(lo.y >= -0.01, "feet below ground: {}", lo.y);
            assert!(hi.y <= 1.86 && hi.y > 1.74, "standing height {}", hi.y);
            assert!(lo.x > -0.33 && hi.x < 0.33 && lo.z > -0.33 && hi.z < 0.33, "footprint {lo} {hi}");
        }
    }

    #[test]
    fn the_body_is_stocky_not_spindly() {
        // shoulders about 0.5 m across, a thigh and a torso that fill the game's hit volumes
        for team in Team::ALL {
            let rig = Rig::new(team, 0);
            let s = rig.solve(&Pose::default());
            let span = |parts: &[(&Template, Mat4)], y0: f32, y1: f32| {
                let (mut lo, mut hi) = (1e9f32, -1e9f32);
                for (t, m) in parts {
                    for v in &t.verts {
                        let p = m.transform_point3(v.p);
                        if p.y >= y0 && p.y <= y1 {
                            lo = lo.min(p.x);
                            hi = hi.max(p.x);
                        }
                    }
                }
                hi - lo
            };
            let shoulders = span(&rig.parts(Vec3::ZERO, 0., &Pose::default()), 1.38, 1.50);
            assert!(shoulders > 0.46 && shoulders < 0.62, "shoulder width {shoulders}");
            let thigh = span(&[(&rig.thigh[0], s.thigh[0])], 0.85, 0.95);
            assert!(thigh > 0.16, "thigh width {thigh}");
        }
    }

    #[test]
    fn walking_and_running_keep_feet_on_the_ground() {
        let rig = Rig::new(Team::Nightwatch, 0);
        for speed in [1., 3., 6., 9.] {
            for k in 0..16 {
                let pose = Pose { speed, walk_phase: k as f32 * 0.4, hold: Hold::Rifle, ..Pose::default() };
                let (lo, hi) = bounds(&rig, &pose);
                assert!(lo.y >= -0.015, "speed {speed} phase {k}: feet at {}", lo.y);
                assert!(hi.y < 1.9);
            }
        }
    }

    #[test]
    fn crouching_lowers_the_head() {
        for team in Team::ALL {
            let rig = Rig::new(team, 2);
            let (lo, hi) = bounds(&rig, &Pose { crouch: 1., ..Pose::default() });
            assert!(hi.y > 1.05 && hi.y < 1.22, "crouched top {}", hi.y);
            assert!(lo.y >= -0.015);
        }
    }

    #[test]
    fn dead_pose_lies_on_the_ground() {
        for team in Team::ALL {
            let rig = Rig::new(team, 3);
            for hold in Hold::ALL {
                let (lo, hi) = bounds(&rig, &Pose { dead: 1., hold, speed: 4., ..Pose::default() });
                assert!(hi.y < 0.6, "dead top {} ({hold:?})", hi.y);
                assert!(lo.y > -0.04, "dead bottom {}", lo.y);
            }
            for d in [0.1, 0.3, 0.5, 0.7, 0.9] {
                let (lo, _) = bounds(&rig, &Pose { dead: d, ..Pose::default() });
                assert!(lo.y > -0.06, "falling at {d}: bottom {}", lo.y);
            }
        }
    }

    #[test]
    fn extreme_poses_are_finite() {
        let rig = Rig::new(Team::Ironclad, 0);
        for hold in Hold::ALL {
            for k in 0..64u32 {
                let f = |shift: u32| ((k >> shift) & 1) as f32;
                let pose = Pose {
                    speed: 12. * f(0),
                    walk_phase: 100. * f(1),
                    crouch: f(2),
                    pitch: if k & 8 != 0 { 1.5 } else { -1.5 },
                    aim: f(4),
                    hold,
                    recoil: f(5),
                    reload: f(0).max(f(3) * 0.5),
                    swing: f(1) * 0.5,
                    throwing: f(2) * 0.6,
                    dead: if k == 63 { 1. } else { 0. },
                    airborne: k & 16 != 0,
                };
                let (lo, hi) = bounds(&rig, &pose);
                assert!(lo.is_finite() && hi.is_finite());
                assert!(rig.weapon_mount(Vec3::ZERO, 1., &pose).is_finite());
            }
        }
        let nan = Pose { speed: f32::NAN, pitch: f32::INFINITY, crouch: f32::NAN, dead: f32::NAN, ..Pose::default() };
        let (lo, hi) = bounds(&rig, &nan);
        assert!(lo.is_finite() && hi.is_finite());
    }

    #[test]
    fn weapon_mount_sits_at_the_right_hand_in_front_of_the_chest() {
        let rig = Rig::new(Team::Ironclad, 0);
        for hold in [Hold::Pistol, Hold::Smg, Hold::Rifle, Hold::Sniper, Hold::Launcher, Hold::Grenade, Hold::Melee] {
            let pose = Pose { hold, aim: 1., ..walk() };
            let m = rig.weapon_mount(vec3(1., 0., 2.), 0., &pose);
            let p = m.transform_point3(Vec3::ZERO);
            assert!(p.y > 1.0 && p.y < 1.75, "{hold:?} grip height {}", p.y);
            assert!(p.x > 1. - 0.1 && p.x < 1. + 0.3, "{hold:?} x {}", p.x);
            // the muzzle direction points forward (towards -Z at yaw 0)
            let fwd = m.transform_vector3(-Vec3::Z);
            assert!(fwd.z < -0.5, "{hold:?} points {fwd}");
        }
    }

    #[test]
    fn yaw_turns_the_mount_towards_plus_x() {
        let rig = Rig::new(Team::Ironclad, 0);
        let pose = Pose { hold: Hold::Rifle, aim: 1., ..Pose::default() };
        let m = rig.weapon_mount(Vec3::ZERO, std::f32::consts::FRAC_PI_2, &pose);
        assert!(m.transform_vector3(-Vec3::Z).x > 0.9);
    }

    #[test]
    fn head_pitch_follows_look_pitch() {
        let rig = Rig::new(Team::Ironclad, 0);
        let up = bounds(&rig, &Pose { pitch: 1.0, ..Pose::default() }).1;
        let down = bounds(&rig, &Pose { pitch: -1.0, ..Pose::default() }).1;
        assert!(up.y > down.y);
    }
}
