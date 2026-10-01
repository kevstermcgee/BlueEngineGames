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
    thigh: Template,
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
            upper_arm: build_upper_arm(&p),
            forearm: build_forearm(&p),
            glove: [glove(&p, false, 0.), glove(&p, true, 0.)],
            thigh: build_thigh(&p),
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
            ("thigh", self.thigh.verts.len(), 2),
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
            + 2 * (self.upper_arm.verts.len() + self.forearm.verts.len() + self.thigh.verts.len())
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
            v.push((&self.thigh, w * s.thigh[i]));
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

const THIGH: f32 = 0.44;
const SHIN: f32 = 0.40;
const ANKLE: f32 = 0.100;
const HIP_STAND: f32 = ANKLE + THIGH + SHIN; // 0.935
const HIP_X: f32 = 0.10;
const WAIST: f32 = 0.06; // torso pivot above the hip centre
const SHOULDER_X: f32 = 0.18;
const SHOULDER_Y: f32 = 0.43;
const NECK_Y: f32 = 0.49;
const UARM: f32 = 0.29;
const FARM: f32 = 0.26;

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
            *f = vec3(x, ANKLE + lift + 0.17 * toe.max(0.).sin(), fwd);
            if !air {
                let dz = fwd - hz;
                let reach = 0.822f32;
                hh = hh.min(ANKLE + (reach * reach - dz * dz).max(0.01).sqrt());
            }
        }
        if air {
            hh = hh_base - 0.03;
        }

        let pelvis = Mat4::from_translation(vec3(0., hh, hz)) * Mat4::from_rotation_x(-0.2 * c);

        // ---- torso: lean, twist, per-action motion ----
        let mut lean = 0.55 * c + 0.06 * (speed / 6.).min(1.) - 0.25 * pitch + 0.10 * aim;
        let mut twist = 0.;
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
        let head = torso * Mat4::from_translation(vec3(0., NECK_Y, 0.)) * Mat4::from_rotation_x(head_up);

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

        let two_handed = matches!(hold, Hold::Pistol | Hold::Smg | Hold::Rifle | Hold::Sniper | Hold::Launcher);
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
                        Hold::Pistol => (vec3(-0.15, -0.32, -0.30), vec3(-0.17, -0.10, -0.46)),
                        Hold::Smg => (vec3(-0.12, -0.30, -0.22), vec3(-0.14, -0.06, -0.27)),
                        Hold::Rifle => (vec3(-0.12, -0.30, -0.22), vec3(-0.12, -0.05, -0.27)),
                        Hold::Sniper => (vec3(-0.12, -0.31, -0.22), vec3(-0.11, -0.04, -0.27)),
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
                let mut a = amp * ph.cos();
                let mut flex = 0.25 + moving * 0.04 * speed + 0.3 * a.max(0.);
                let mut out = 0.10;
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

/// A cone/frustum standing on `base`, squashed along Z by `zs` (elliptical section).
#[allow(clippy::too_many_arguments)]
fn econe(t: &mut Template, base: Vec3, rb: f32, rt: f32, h: f32, zs: f32, c: Rgb, sides: usize) {
    let mut b = Template::new();
    b.cone(Vec3::ZERO, rb, rt, h, c, 0., sides);
    t.append(&b.transformed(Mat4::from_translation(base) * Mat4::from_scale(vec3(1., 1., zs))));
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

fn build_pelvis(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    bx(&mut t, 0., -0.03, 0., 0.160, 0.085, 0.098, p.trousers);
    // belt with buckle and pouches
    bx(&mut t, 0., 0.058, 0., 0.166, 0.026, 0.106, p.strap);
    bx(&mut t, 0., 0.058, -0.108, 0.024, 0.02, 0.005, p.metal);
    bx2(&mut t, 0.088, 0.040, -0.118, 0.038, 0.045, 0.022, p.vest);
    bx2(&mut t, 0.088, 0.078, -0.118, 0.040, 0.008, 0.024, p.vest_dark);
    bx(&mut t, 0., 0.040, 0.122, 0.07, 0.04, 0.03, p.vest);
    bx(&mut t, -0.178, 0.0, 0.04, 0.024, 0.06, 0.05, p.vest_dark);
    match team {
        Team::Ironclad => {
            // canteen on the right hip
            let mut can = Template::new();
            can.cylinder(Vec3::ZERO, 0.045, 0.14, [0.30, 0.33, 0.16], 0., 10);
            t.append(&can.transformed(Mat4::from_translation(vec3(0.185, -0.08, 0.05))));
            bx(&mut t, 0.185, 0.07, 0.05, 0.04, 0.012, 0.045, p.vest_dark);
        }
        Team::Nightwatch => {
            // drop-leg holster panel
            bx(&mut t, 0.175, -0.10, 0.0, 0.016, 0.09, 0.05, p.vest_dark);
            bx(&mut t, 0.175, -0.19, 0.0, 0.02, 0.012, 0.055, p.plate);
        }
    }
    t
}

fn build_torso(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    // shirt: waist, chest, shoulders, collar
    bx(&mut t, 0., 0.10, 0., 0.142, 0.10, 0.092, p.uniform);
    bx(&mut t, 0., 0.33, 0., 0.158, 0.14, 0.096, p.uniform);
    bx(&mut t, 0., 0.435, 0., 0.172, 0.05, 0.090, p.uniform);
    bx(&mut t, 0., 0.485, 0., 0.065, 0.014, 0.06, p.uniform);
    // vest body
    bx(&mut t, 0., 0.305, 0., 0.166, 0.165, 0.105, p.vest);
    // shoulder straps over the shoulders
    bx2(&mut t, 0.098, 0.468, 0., 0.045, 0.012, 0.104, p.vest);
    bx2(&mut t, 0.098, 0.482, 0., 0.030, 0.004, 0.092, p.vest_dark);
    match team {
        Team::Ironclad => {
            // load-bearing vest: four double magazine pouches, flaps, webbing rows, back plate
            for k in 0..4 {
                let x = -0.105 + 0.07 * k as f32;
                bx(&mut t, x, 0.19, -0.128, 0.031, 0.06, 0.026, p.vest);
                bx(&mut t, x, 0.245, -0.131, 0.034, 0.012, 0.028, p.vest_dark);
                bx(&mut t, x, 0.225, -0.156, 0.007, 0.012, 0.004, p.metal);
            }
            bx(&mut t, 0., 0.36, -0.115, 0.13, 0.055, 0.015, p.vest_dark); // chest panel
            bx2(&mut t, 0.07, 0.36, -0.13, 0.04, 0.035, 0.014, p.vest); // chest pockets
            bx(&mut t, 0., 0.31, 0.118, 0.13, 0.14, 0.018, p.plate);
            bx2(&mut t, 0.105, 0.17, 0.135, 0.036, 0.05, 0.03, p.vest); // rear pouches
            bx(&mut t, 0., 0.44, 0.14, 0.11, 0.04, 0.045, p.strap); // rolled blanket on top
        }
        Team::Nightwatch => {
            // plate carrier: big plates, cummerbund pouches, radio and assault pack
            bx(&mut t, 0., 0.355, -0.114, 0.118, 0.128, 0.018, p.plate);
            bx(&mut t, 0., 0.355, -0.130, 0.095, 0.105, 0.004, p.vest_dark);
            for k in 0..3 {
                let x = -0.075 + 0.075 * k as f32;
                bx(&mut t, x, 0.165, -0.125, 0.032, 0.05, 0.02, p.vest_dark);
            }
            bx2(&mut t, 0.172, 0.22, -0.02, 0.02, 0.06, 0.05, p.vest_dark);
            bx(&mut t, -0.12, 0.47, -0.05, 0.025, 0.05, 0.03, p.vest_dark); // radio
            bx(&mut t, 0., 0.33, 0.14, 0.115, 0.15, 0.05, p.plate); // assault pack
            bx(&mut t, 0., 0.33, 0.194, 0.09, 0.11, 0.008, p.vest_dark);
            bx(&mut t, 0., 0.50, 0.14, 0.06, 0.014, 0.04, p.vest_dark);
        }
    }
    t
}

fn build_head(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    // pivot is the top of the neck; the skull centre is 0.115 above it
    let neck_c = if team == Team::Nightwatch { p.vest_dark } else { p.skin_dark };
    let mut neck = Template::new();
    neck.cylinder(vec3(0., -0.035, 0.), 0.046, 0.09, neck_c, 0., 10);
    t.append(&neck);
    let sk = 0.115;
    t.ball(vec3(0., sk, 0.004), vec3(0.078, 0.10, 0.092), p.skin, 0., 14, 10);
    // jaw and chin
    bx(&mut t, 0., sk - 0.078, -0.028, 0.055, 0.022, 0.052, p.skin);
    // nose, brow, eyes
    bx(&mut t, 0., sk - 0.012, -0.094, 0.012, 0.022, 0.012, p.skin);
    bx(&mut t, 0., sk + 0.034, -0.086, 0.060, 0.006, 0.006, mul(p.skin, 0.6));
    bx2(&mut t, 0.033, sk + 0.012, -0.086, 0.011, 0.006, 0.005, [0.08, 0.08, 0.10]);
    // ears
    t.ball(vec3(0.080, sk - 0.005, 0.012), vec3(0.012, 0.027, 0.02), p.skin_dark, 0., 8, 6);
    t.ball(vec3(-0.080, sk - 0.005, 0.012), vec3(0.012, 0.027, 0.02), p.skin_dark, 0., 8, 6);

    // ---- helmet: stacked frusta (an elliptical dome), rim, back skirt, ear guards, chin strap ----
    let hb = 0.175; // rim height above the pivot
    let zs = 1.14;
    let h = p.helmet;
    econe(&mut t, vec3(0., hb, 0.01), 0.128, 0.128, 0.022, zs, mul(h, 0.78), 14); // rim
    econe(&mut t, vec3(0., hb + 0.022, 0.01), 0.128, 0.120, 0.035, zs, h, 14);
    econe(&mut t, vec3(0., hb + 0.057, 0.01), 0.120, 0.100, 0.035, zs, h, 14);
    econe(&mut t, vec3(0., hb + 0.092, 0.01), 0.100, 0.066, 0.03, zs, h, 14);
    econe(&mut t, vec3(0., hb + 0.122, 0.01), 0.066, 0.030, 0.018, zs, h, 14);
    bx(&mut t, 0., hb - 0.028, 0.128, 0.098, 0.030, 0.014, mul(h, 0.9)); // neck skirt at the back
    bx2(&mut t, 0.122, hb - 0.03, 0.03, 0.006, 0.030, 0.05, mul(h, 0.9)); // ear guards
                                                                          // chin strap
    let strap = p.strap;
    bx2(&mut t, 0.079, sk - 0.02, -0.02, 0.005, 0.075, 0.009, strap);
    bx(&mut t, 0., sk - 0.095, -0.062, 0.05, 0.007, 0.012, strap);
    bx2(&mut t, 0.055, sk - 0.095, -0.04, 0.007, 0.007, 0.03, strap);

    match team {
        Team::Ironclad => {
            // tan net cover: a band round the shell and four arches over the crown, plus goggles on the front
            econe(&mut t, vec3(0., hb + 0.026, 0.01), 0.131, 0.124, 0.014, zs, p.net, 14);
            for (dx, dz) in [(1., 0.), (0., 1.), (0.7, 0.7), (0.7, -0.7)] {
                arch(&mut t, hb, dx, dz, p.net);
            }
            bx(&mut t, 0., hb + 0.05, -0.143, 0.065, 0.009, 0.006, p.strap); // goggle strap over the shell
            bx2(&mut t, 0.03, hb + 0.05, -0.147, 0.026, 0.019, 0.01, [0.10, 0.10, 0.11]);
            bx2(&mut t, 0.03, hb + 0.05, -0.154, 0.020, 0.014, 0.004, [0.85, 0.62, 0.15]);
        }
        Team::Nightwatch => {
            // neck gaiter over the lower face, slim dark glasses, helmet accessory rail
            bx(&mut t, 0., sk - 0.052, -0.058, 0.066, 0.040, 0.034, p.vest_dark);
            bx(&mut t, 0., sk - 0.028, -0.078, 0.052, 0.012, 0.016, p.vest_dark);
            bx(&mut t, 0., sk + 0.012, -0.092, 0.058, 0.012, 0.006, [0.03, 0.03, 0.04]);
            bx2(&mut t, 0.03, sk + 0.012, -0.096, 0.022, 0.011, 0.003, [0.12, 0.16, 0.22]);
            bx(&mut t, 0., hb + 0.075, -0.098, 0.03, 0.012, 0.02, p.plate); // NVG mount
            bx(&mut t, 0., hb + 0.05, -0.136, 0.05, 0.008, 0.005, p.plate);
        }
    }
    t
}

/// A net strap over the crown: short boxes following the dome, in the vertical plane given by (dx, dz).
fn arch(t: &mut Template, hb: f32, dx: f32, dz: f32, c: Rgb) {
    let dir = vec2(dx, dz).normalize();
    let n = 6;
    let pt = |k: usize| {
        let a = k as f32 / n as f32 * std::f32::consts::PI;
        let r = 0.130 + 0.004;
        let zs = 1.0 + 0.14 * dir.y.abs();
        // a point on the dome: radius 0.128 at the rim, height 0.14 above it
        let ring = (a.cos() * r) * vec2(dir.x, dir.y * zs);
        vec3(ring.x, hb + 0.022 + a.sin() * 0.122, 0.01 + ring.y)
    };
    for k in 0..n {
        let (a, b) = (pt(k), pt(k + 1));
        let mid = (a + b) * 0.5;
        let d = b - a;
        let len = d.length();
        let rot = Quat::from_rotation_arc(Vec3::Z, d / len);
        obox(t, mid, rot, vec3(0.008, 0.004, len * 0.5 + 0.002), c);
    }
}

fn build_upper_arm(p: &Palette) -> Template {
    let mut t = Template::new();
    t.ball(Vec3::ZERO, vec3(0.056, 0.056, 0.056), p.uniform, 0., 10, 8);
    t.cone(vec3(0., -UARM, 0.), 0.040, 0.046, UARM, p.uniform, 0., 10);
    t.ball(vec3(0., -UARM, 0.), vec3(0.04, 0.04, 0.04), p.uniform, 0., 8, 6);
    t
}

fn build_forearm(p: &Palette) -> Template {
    let mut t = Template::new();
    t.ball(Vec3::ZERO, vec3(0.041, 0.041, 0.041), p.uniform, 0., 8, 6);
    t.cone(vec3(0., -FARM + 0.04, 0.), 0.033, 0.041, FARM - 0.04, p.uniform, 0., 10);
    // the glove's gauntlet over the cuff
    t.cone(vec3(0., -FARM - 0.02, 0.), 0.034, 0.038, 0.075, p.glove, 0., 10);
    t.cone(vec3(0., -FARM + 0.04, 0.), 0.040, 0.040, 0.012, p.glove_dark, 0., 10);
    t
}

fn build_thigh(p: &Palette) -> Template {
    let mut t = Template::new();
    t.ball(Vec3::ZERO, vec3(0.08, 0.08, 0.08), p.trousers, 0., 10, 8);
    t.cone(vec3(0., -THIGH, 0.), 0.060, 0.078, THIGH, p.trousers, 0., 10);
    // cargo pocket
    bx(&mut t, 0.072, -0.22, -0.01, 0.014, 0.065, 0.048, mul(p.trousers, 0.85));
    bx(&mut t, -0.072, -0.22, -0.01, 0.014, 0.065, 0.048, mul(p.trousers, 0.85));
    t
}

fn build_shin(p: &Palette, team: Team) -> Template {
    let mut t = Template::new();
    t.ball(Vec3::ZERO, vec3(0.062, 0.062, 0.062), p.trousers, 0., 8, 6);
    t.cone(vec3(0., -SHIN, 0.), 0.052, 0.060, SHIN, p.trousers, 0., 10);
    // knee pad
    let pad = if team == Team::Ironclad { p.vest } else { p.vest_dark };
    bx(&mut t, 0., 0.0, -0.058, 0.056, 0.062, 0.022, pad);
    bx(&mut t, 0., 0.0, -0.078, 0.042, 0.048, 0.005, mul(pad, 0.8));
    // bloused cuff
    t.cone(vec3(0., -SHIN - 0.0, 0.), 0.062, 0.056, 0.05, p.trousers, 0., 10);
    t
}

fn build_boot(p: &Palette) -> Template {
    let mut t = Template::new();
    // shaft up the leg, foot forward (-Z); the ankle joint is the origin
    t.cone(vec3(0., -0.06, 0.), 0.064, 0.060, 0.19, p.boot, 0., 10);
    bx(&mut t, 0., -0.052, -0.06, 0.058, 0.038, 0.128, p.boot);
    t.ball(vec3(0., -0.052, -0.165), vec3(0.055, 0.04, 0.05), p.boot, 0., 8, 6); // toe cap
    bx(&mut t, 0., -0.085, -0.06, 0.061, 0.011, 0.14, p.sole);
    bx(&mut t, 0., -0.072, 0.048, 0.06, 0.016, 0.022, p.sole); // heel block
    bx(&mut t, 0., 0.03, -0.055, 0.03, 0.01, 0.01, mul(p.boot, 0.75)); // lacing
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
                assert!(rig.vertex_count() <= 6000, "{} vertices", rig.vertex_count());
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
            assert!(hi.y <= 1.9 && hi.y > 1.72, "standing height {}", hi.y);
            assert!(lo.x > -0.33 && hi.x < 0.33 && lo.z > -0.33 && hi.z < 0.33, "footprint {lo} {hi}");
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
            assert!(hi.y > 1.0 && hi.y < 1.25, "crouched top {}", hi.y);
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
