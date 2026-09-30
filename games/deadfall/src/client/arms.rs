//! First-person arms: sleeves, cuffs and gloved hands gripping a weapon, in WEAPON-LOCAL space (origin at
//! the firing hand's grip, -Z out of the muzzle), so they can be drawn with the same matrix as the weapon.
use super::anchors::WeaponAnchors;
use super::character::{
    at_palm, cup_rot, free_hand_rot, glove, keys, magwell_of, sleeve, sleeve_mat, smooth, wrist_offset, Hold, Palette,
};
use crate::team::Team;
use macroquad::prelude::*;
use vesper3d::viewer::kit::Template;

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
}

impl Default for ArmPose {
    fn default() -> Self {
        ArmPose { reload: 0., swing: 0., throwing: 0., draw: 1., ads: 0. }
    }
}

fn f(x: f32, d: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        d
    }
}

/// Forearms and gloved hands for the first-person view: the right hand grips at `anchors.grip`, the left at
/// `anchors.support` (see the module docs of the design brief for the `None` cases).
pub fn first_person_arms(team: Team, skin: u8, anchors: &WeaponAnchors, hold: Hold, pose: &ArmPose) -> Template {
    let pal = Palette::new(team, skin);
    let ads = f(pose.ads, 0.).clamp(0., 1.);
    let draw = f(pose.draw, 1.).clamp(0., 1.);
    let reload = f(pose.reload, 0.).clamp(0., 1.);
    let throwing = f(pose.throwing, 0.).clamp(0., 1.);
    let mut out = Template::new();
    let firearm = matches!(hold, Hold::Pistol | Hold::Smg | Hold::Rifle | Hold::Sniper | Hold::Launcher);
    let open = if hold == Hold::Grenade && throwing > 0.6 { 1. } else { 0. };
    let len = 0.62;

    // ---- right hand: on the grip ----
    let grip = anchors.grip;
    out.append(&glove(&pal, false, open).transformed(at_palm(grip, Mat4::IDENTITY)));
    let wrist = grip + wrist_offset(false);
    let drop = (1. - draw) * 0.7;
    let dir_r = vec3(0.12, -0.40, 1.0).lerp(vec3(0.25, -0.75, 0.8), ads) + vec3(0., -drop, 0.);
    out.append(&sleeve(&pal, len, 0.032, 0.046).transformed(sleeve_mat(wrist, dir_r.normalize(), len)));

    // ---- left hand ----
    let rest_pos = grip + vec3(-0.30, -0.26, 0.10);
    let (mut pos, mut rot, rest_rot) = match (anchors.support, hold) {
        (Some(s), _) => (s, cup_rot(), cup_rot()),
        (None, Hold::Pistol) => {
            let t = smooth(0., 0.3, ads);
            let wrap = grip + vec3(-0.022, -0.030, -0.022);
            (rest_pos.lerp(wrap, t), slerp(free_hand_rot(), Mat4::IDENTITY, t), Mat4::IDENTITY)
        }
        (None, _) => (rest_pos, free_hand_rot(), free_hand_rot()),
    };
    if reload > 0. && firearm {
        let base = pos;
        let mw = grip + magwell_of(hold);
        let away = grip + vec3(-0.06, -0.32, 0.30);
        pos =
            keys(reload, &[(0., base), (0.2, mw), (0.4, away), (0.66, mw), (0.8, mw + vec3(0., 0.03, 0.)), (1., base)]);
        let busy = smooth(0., 0.15, reload) * (1. - smooth(0.85, 1., reload));
        rot = slerp(rot, if anchors.support.is_some() { cup_rot() } else { Mat4::IDENTITY }, busy);
        let _ = rest_rot;
    }
    out.append(&glove(&pal, true, 0.).transformed(at_palm(pos, rot)));
    let wrist_l = pos + rot.transform_vector3(wrist_offset(true));
    let dir_l = vec3(-0.35, -0.45 - drop, 1.0).normalize();
    out.append(&sleeve(&pal, len, 0.032, 0.046).transformed(sleeve_mat(wrist_l, dir_l, len)));
    out
}

/// Blend between two rotation matrices (translation ignored).
fn slerp(a: Mat4, b: Mat4, t: f32) -> Mat4 {
    let qa = Quat::from_mat4(&Mat4::from_cols(a.x_axis, a.y_axis, a.z_axis, Vec4::W));
    let qb = Quat::from_mat4(&Mat4::from_cols(b.x_axis, b.y_axis, b.z_axis, Vec4::W));
    Mat4::from_quat(qa.slerp(qb, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchors(support: bool) -> WeaponAnchors {
        WeaponAnchors {
            grip: Vec3::ZERO,
            support: support.then(|| vec3(0., -0.02, -0.35)),
            sight: vec3(0., 0.08, 0.05),
            muzzle: vec3(0., 0.02, -0.85),
            eject: vec3(0.03, 0.04, -0.1),
        }
    }

    #[test]
    fn non_empty_and_finite_for_every_hold() {
        for team in Team::ALL {
            for hold in Hold::ALL {
                for support in [false, true] {
                    for pose in [
                        ArmPose::default(),
                        ArmPose { reload: 0.4, ads: 1., ..ArmPose::default() },
                        ArmPose { throwing: 0.9, draw: 0.2, swing: 0.5, ..ArmPose::default() },
                        ArmPose { reload: f32::NAN, ads: f32::INFINITY, ..ArmPose::default() },
                    ] {
                        let t = first_person_arms(team, 2, &anchors(support), hold, &pose);
                        assert!(t.verts.len() > 100, "{hold:?}");
                        assert!(t.verts.len() < 9000);
                        assert!(t.verts.iter().all(|v| v.p.is_finite()));
                    }
                }
            }
        }
    }

    #[test]
    fn sleeves_and_gloves_use_the_team_colours() {
        for team in Team::ALL {
            let pal = Palette::new(team, 0);
            let t = first_person_arms(team, 0, &anchors(true), Hold::Rifle, &ArmPose::default());
            assert!(t.verts.iter().any(|v| v.c == pal.uniform), "sleeve colour missing");
            assert!(t.verts.iter().any(|v| v.c == pal.glove), "glove colour missing");
        }
    }

    #[test]
    fn forearms_run_back_towards_the_camera_and_down() {
        let t = first_person_arms(Team::Ironclad, 0, &anchors(true), Hold::Rifle, &ArmPose::default());
        let max_z = t.verts.iter().map(|v| v.p.z).fold(f32::MIN, f32::max);
        let min_y = t.verts.iter().map(|v| v.p.y).fold(f32::MAX, f32::min);
        assert!(max_z > 0.5, "arms reach back to z={max_z}");
        assert!(min_y < -0.2, "arms drop to y={min_y}");
    }

    #[test]
    fn reload_moves_the_support_hand_down_and_back() {
        let a = anchors(true);
        let rest = first_person_arms(Team::Ironclad, 0, &a, Hold::Rifle, &ArmPose::default());
        let mid = first_person_arms(Team::Ironclad, 0, &a, Hold::Rifle, &ArmPose { reload: 0.4, ..ArmPose::default() });
        let low = |t: &Template| t.verts.iter().map(|v| v.p.y).fold(f32::MAX, f32::min);
        assert!(low(&mid) < low(&rest) - 0.05);
        let end = first_person_arms(Team::Ironclad, 0, &a, Hold::Rifle, &ArmPose { reload: 1.0, ..ArmPose::default() });
        assert!((low(&end) - low(&rest)).abs() < 0.02);
    }
}
