pub mod protocol;
pub mod server;

use serde::{Deserialize, Serialize};
use vesper3d::{
    math::V,
    viewer::{
        arena::{LaunchVolume, PickupKind, ProjectileSpec, TimedPickup},
        controller::Collider,
    },
};

pub const DEFAULT_SERVER: &str = "127.0.0.1:4200";
pub const DEFAULT_KEY: &str = "riftwake-party";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponKind {
    Scattergun,
    Nailstorm,
    Grenade,
    Rocket,
    Arc,
    Rail,
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponDef {
    pub kind: WeaponKind,
    pub name: &'static str,
    pub damage: f32,
    pub cooldown: u32,
    pub projectile: Option<ProjectileSpec>,
    pub color: [f32; 3],
}

pub fn armory() -> [WeaponDef; 6] {
    [
        WeaponDef {
            kind: WeaponKind::Scattergun,
            name: "RIPPER SG",
            damage: 11.0,
            cooldown: 48,
            projectile: None,
            color: [1.0, 0.55, 0.15],
        },
        WeaponDef {
            kind: WeaponKind::Nailstorm,
            name: "NAILSTORM",
            damage: 9.0,
            cooldown: 5,
            projectile: None,
            color: [0.2, 0.95, 0.75],
        },
        WeaponDef {
            kind: WeaponKind::Grenade,
            name: "GRAV MORTAR",
            damage: 100.0,
            cooldown: 42,
            projectile: Some(ProjectileSpec {
                speed: 18.0,
                direct_damage: 100.0,
                splash_damage: 90.0,
                splash_radius: 4.5,
                lifetime_ticks: 150,
            }),
            color: [0.65, 1.0, 0.2],
        },
        WeaponDef {
            kind: WeaponKind::Rocket,
            name: "RIFT ROCKET",
            damage: 100.0,
            cooldown: 44,
            projectile: Some(ProjectileSpec {
                speed: 27.0,
                direct_damage: 100.0,
                splash_damage: 100.0,
                splash_radius: 4.8,
                lifetime_ticks: 120,
            }),
            color: [1.0, 0.25, 0.45],
        },
        WeaponDef {
            kind: WeaponKind::Arc,
            name: "ARC BEAM",
            damage: 8.0,
            cooldown: 4,
            projectile: None,
            color: [0.3, 0.75, 1.0],
        },
        WeaponDef {
            kind: WeaponKind::Rail,
            name: "VOID RAIL",
            damage: 90.0,
            cooldown: 78,
            projectile: None,
            color: [0.8, 0.25, 1.0],
        },
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct ArenaBox {
    pub center: V,
    pub size: V,
    pub color: [f32; 3],
    pub emissive: bool,
}

impl ArenaBox {
    pub fn collider(self) -> Collider {
        Collider {
            min: self.center - self.size * 0.5,
            max: self.center + self.size * 0.5,
        }
    }
}

/// Hand-authored three-level duel arena: a readable loop, crossing sightline,
/// risky central power position, jump-pad shortcuts and enough cover to break rails.
pub fn arena_boxes() -> Vec<ArenaBox> {
    let stone = [0.12, 0.15, 0.22];
    let metal = [0.18, 0.23, 0.31];
    let cyan = [0.04, 0.68, 0.86];
    let magenta = [0.78, 0.08, 0.38];
    vec![
        ArenaBox {
            center: V(0., -0.5, 0.),
            size: V(44., 1., 36.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(0., 5.0, -18.),
            size: V(44., 11., 1.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(0., 5.0, 18.),
            size: V(44., 11., 1.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(-22., 5.0, 0.),
            size: V(1., 11., 36.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(22., 5.0, 0.),
            size: V(1., 11., 36.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(0., 2.0, 0.),
            size: V(7., 4., 7.),
            color: [0.10, 0.11, 0.18],
            emissive: false,
        },
        ArenaBox {
            center: V(0., 4.25, 0.),
            size: V(10., 0.5, 10.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(-14., 2.0, -8.),
            size: V(12., 0.6, 7.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(14., 2.0, 8.),
            size: V(12., 0.6, 7.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(-16., 4.4, 9.),
            size: V(7., 0.6, 12.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(16., 4.4, -9.),
            size: V(7., 0.6, 12.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(-7.5, 1.4, 11.),
            size: V(1.2, 2.8, 7.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(7.5, 1.4, -11.),
            size: V(1.2, 2.8, 7.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(-11., 1.6, 0.),
            size: V(1.4, 3.2, 5.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(11., 1.6, 0.),
            size: V(1.4, 3.2, 5.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(-20.8, 1.2, 0.),
            size: V(0.25, 2.4, 28.),
            color: cyan,
            emissive: true,
        },
        ArenaBox {
            center: V(20.8, 1.2, 0.),
            size: V(0.25, 2.4, 28.),
            color: magenta,
            emissive: true,
        },
        ArenaBox {
            center: V(0., 4.75, 0.),
            size: V(7., 0.18, 7.),
            color: [0.68, 0.12, 0.95],
            emissive: true,
        },
        ArenaBox {
            center: V(-17., 0.08, 13.),
            size: V(3., 0.16, 3.),
            color: cyan,
            emissive: true,
        },
        ArenaBox {
            center: V(17., 0.08, -13.),
            size: V(3., 0.16, 3.),
            color: magenta,
            emissive: true,
        },
    ]
}

pub fn colliders() -> Vec<Collider> {
    arena_boxes()
        .into_iter()
        .filter(|b| !b.emissive)
        .map(ArenaBox::collider)
        .collect()
}

pub fn launch_pads() -> [LaunchVolume; 2] {
    [
        LaunchVolume {
            min: V(-18.5, -0.1, 11.5),
            max: V(-15.5, 0.3, 14.5),
            velocity: V(3.8, 12.8, -2.0),
        },
        LaunchVolume {
            min: V(15.5, -0.1, -14.5),
            max: V(18.5, 0.3, -11.5),
            velocity: V(-3.8, 12.8, 2.0),
        },
    ]
}

pub fn spawns() -> [(V, f32); 8] {
    [
        (V(-18., 0., -12.), 1.1),
        (V(18., 0., 12.), -2.0),
        (V(-15., 2.3, -8.), 0.7),
        (V(15., 2.3, 8.), -2.4),
        (V(-16., 4.7, 9.), 1.8),
        (V(16., 4.7, -9.), -1.3),
        (V(0., 0., 14.), std::f32::consts::PI),
        (V(0., 0., -14.), 0.),
    ]
}

pub fn pickups() -> Vec<TimedPickup> {
    vec![
        TimedPickup::new("mega", PickupKind::Health, V(0., 5.15, 0.), 100, 35 * 60).unwrap(),
        TimedPickup::new(
            "armor-west",
            PickupKind::Armor,
            V(-16., 4.9, 9.),
            50,
            25 * 60,
        )
        .unwrap(),
        TimedPickup::new(
            "armor-east",
            PickupKind::Armor,
            V(16., 4.9, -9.),
            50,
            25 * 60,
        )
        .unwrap(),
        TimedPickup::new(
            "health-north",
            PickupKind::Health,
            V(0., 0.5, -14.),
            25,
            15 * 60,
        )
        .unwrap(),
        TimedPickup::new(
            "health-south",
            PickupKind::Health,
            V(0., 0.5, 14.),
            25,
            15 * 60,
        )
        .unwrap(),
    ]
}
