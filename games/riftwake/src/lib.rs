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
    let stone = [0.34, 0.34, 0.31];
    let metal = [0.64, 0.62, 0.54];
    let trim = [0.22, 0.30, 0.34];
    let cyan = [0.24, 0.36, 0.44];
    let magenta = [0.63, 0.47, 0.19];
    let mut boxes = vec![
        // Ground plane and enclosing walls establish one intentional combat volume.
        ArenaBox {
            center: V(0., -0.5, 0.),
            size: V(44., 1., 36.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(0., 3.0, -18.),
            size: V(44., 6., 1.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(0., 3.0, 18.),
            size: V(44., 6., 1.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(-22., 3.0, 0.),
            size: V(1., 6., 36.),
            color: metal,
            emissive: false,
        },
        ArenaBox {
            center: V(22., 3.0, 0.),
            size: V(1., 6., 36.),
            color: metal,
            emissive: false,
        },
        // Central reactor tower and its contested mega-health deck.
        ArenaBox {
            center: V(0., 2.0, 0.),
            size: V(7., 4., 7.),
            color: [0.62, 0.59, 0.49],
            emissive: false,
        },
        ArenaBox {
            center: V(0., 4.25, 0.),
            size: V(10., 0.5, 10.),
            color: metal,
            emissive: false,
        },
        // Mid decks: broad enough to fight on, supported instead of floating.
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
            center: V(-18.5, 1.0, -8.),
            size: V(1.0, 2.0, 5.5),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(-10.0, 1.0, -8.),
            size: V(1.0, 2.0, 5.5),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(18.5, 1.0, 8.),
            size: V(1.0, 2.0, 5.5),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(10.0, 1.0, 8.),
            size: V(1.0, 2.0, 5.5),
            color: stone,
            emissive: false,
        },
        // Upper galleries and structural piers.
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
            center: V(-18.5, 2.2, 9.),
            size: V(1.2, 4.4, 9.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(-13.5, 2.2, 9.),
            size: V(1.2, 4.4, 9.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(18.5, 2.2, -9.),
            size: V(1.2, 4.4, 9.),
            color: stone,
            emissive: false,
        },
        ArenaBox {
            center: V(13.5, 2.2, -9.),
            size: V(1.2, 4.4, 9.),
            color: stone,
            emissive: false,
        },
        // Bridges connect both upper galleries to the central reactor deck.
        ArenaBox {
            center: V(-8.75, 4.42, 4.0),
            size: V(7.5, 0.5, 2.8),
            color: trim,
            emissive: false,
        },
        ArenaBox {
            center: V(8.75, 4.42, -4.0),
            size: V(7.5, 0.5, 2.8),
            color: trim,
            emissive: false,
        },
        // North/south gate frames turn the long axis into recognizable rooms.
        ArenaBox {
            center: V(-4.2, 1.6, -14.5),
            size: V(1.0, 3.2, 1.2),
            color: trim,
            emissive: false,
        },
        ArenaBox {
            center: V(4.2, 1.6, -14.5),
            size: V(1.0, 3.2, 1.2),
            color: trim,
            emissive: false,
        },
        ArenaBox {
            center: V(0.0, 3.5, -14.5),
            size: V(9.4, 0.6, 1.2),
            color: trim,
            emissive: false,
        },
        ArenaBox {
            center: V(-4.2, 1.6, 14.5),
            size: V(1.0, 3.2, 1.2),
            color: trim,
            emissive: false,
        },
        ArenaBox {
            center: V(4.2, 1.6, 14.5),
            size: V(1.0, 3.2, 1.2),
            color: trim,
            emissive: false,
        },
        ArenaBox {
            center: V(0.0, 3.5, 14.5),
            size: V(9.4, 0.6, 1.2),
            color: trim,
            emissive: false,
        },
        // Non-colliding neon language: team sides, bridges, reactor and launch pads.
        ArenaBox {
            center: V(-21.35, 1.4, 0.),
            size: V(0.18, 2.8, 30.),
            color: cyan,
            emissive: true,
        },
        ArenaBox {
            center: V(21.35, 1.4, 0.),
            size: V(0.18, 2.8, 30.),
            color: magenta,
            emissive: true,
        },
        ArenaBox {
            center: V(0., 4.508, 0.),
            size: V(7., 0.012, 7.),
            color: [0.33, 0.35, 0.32],
            emissive: true,
        },
        ArenaBox {
            center: V(-7.5, 0.08, 14.),
            size: V(3., 0.16, 3.),
            color: cyan,
            emissive: true,
        },
        ArenaBox {
            center: V(7.5, 0.08, -14.),
            size: V(3., 0.16, 3.),
            color: magenta,
            emissive: true,
        },
        ArenaBox {
            center: V(-8.75, 4.678, 2.55),
            size: V(7.5, 0.012, 0.12),
            color: cyan,
            emissive: true,
        },
        ArenaBox {
            center: V(8.75, 4.678, -2.55),
            size: V(7.5, 0.012, 0.12),
            color: magenta,
            emissive: true,
        },
    ];

    // Six-step approaches make both mid decks traversable without a jump pad.
    staircase_x(
        &mut boxes,
        Staircase::new(V(-4.8, 0.0, -8.0), -1.0, 12, 0.19, 0.35, 3.0, metal),
    );
    staircase_x(
        &mut boxes,
        Staircase::new(V(4.8, 0.0, 8.0), 1.0, 12, 0.19, 0.35, 3.0, metal),
    );
    // Eleven-step gallery stairs form clear, grounded routes to the top circuit.
    staircase_x(
        &mut boxes,
        Staircase::new(V(-6.2, 0.0, 7.0), -1.0, 22, 0.21, 0.3, 3.0, trim),
    );
    staircase_x(
        &mut boxes,
        Staircase::new(V(6.2, 0.0, -7.0), 1.0, 22, 0.21, 0.3, 3.0, trim),
    );
    // Mirrored reactor stairs prevent the mega platform from being pad-only.
    staircase_x(
        &mut boxes,
        Staircase::new(V(-10.3, 0.0, 0.0), 1.0, 20, 0.225, 0.3, 2.8, stone),
    );
    staircase_x(
        &mut boxes,
        Staircase::new(V(10.3, 0.0, 0.0), -1.0, 20, 0.225, 0.3, 2.8, stone),
    );
    boxes
}

struct Staircase {
    start: V,
    direction: f32,
    steps: usize,
    rise: f32,
    run: f32,
    width: f32,
    color: [f32; 3],
}

impl Staircase {
    fn new(
        start: V,
        direction: f32,
        steps: usize,
        rise: f32,
        run: f32,
        width: f32,
        color: [f32; 3],
    ) -> Self {
        Self {
            start,
            direction,
            steps,
            rise,
            run,
            width,
            color,
        }
    }
}

fn staircase_x(boxes: &mut Vec<ArenaBox>, stair: Staircase) {
    for index in 0..stair.steps {
        let top = (index + 1) as f32 * stair.rise;
        boxes.push(ArenaBox {
            center: V(
                stair.start.0 + stair.direction * index as f32 * stair.run,
                top * 0.5,
                stair.start.2,
            ),
            size: V(stair.run + 0.03, top, stair.width),
            color: stair.color,
            emissive: false,
        });
    }
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
            min: V(-9.0, -0.1, 12.5),
            max: V(-6.0, 0.3, 15.5),
            velocity: V(-11.0, 15.5, -6.0),
        },
        LaunchVolume {
            min: V(6.0, -0.1, -15.5),
            max: V(9.0, 0.3, -12.5),
            velocity: V(11.0, 15.5, 6.0),
        },
    ]
}

pub fn spawns() -> [(V, f32); 8] {
    [
        V(-18., 0., -13.),
        V(18., 0., 13.),
        V(-15., 2.3, -8.),
        V(15., 2.3, 8.),
        V(-16., 4.7, 10.),
        V(16., 4.7, -10.),
        V(0., 0., 14.),
        V(0., 0., -14.),
    ]
    .map(|feet| (feet, (-feet.0).atan2(feet.2)))
}

pub fn pickups() -> Vec<TimedPickup> {
    vec![
        arena_pickup("mega", PickupKind::Health, V(0., 5.4, 0.), 100, 35 * 60),
        arena_pickup(
            "armor-west",
            PickupKind::Armor,
            V(-16., 5.6, 10.),
            50,
            25 * 60,
        ),
        arena_pickup(
            "armor-east",
            PickupKind::Armor,
            V(16., 5.6, -10.),
            50,
            25 * 60,
        ),
        arena_pickup(
            "health-north",
            PickupKind::Health,
            V(0., 0.9, -14.),
            25,
            15 * 60,
        ),
        arena_pickup(
            "health-south",
            PickupKind::Health,
            V(0., 0.9, 14.),
            25,
            15 * 60,
        ),
    ]
}

fn arena_pickup(id: &str, kind: PickupKind, position: V, amount: u16, ticks: u32) -> TimedPickup {
    let mut pickup = TimedPickup::new(id, kind, position, amount, ticks).unwrap();
    pickup.radius = 1.15;
    pickup
}

/// Facade details are outside movement lanes; gameplay collision remains arena_boxes.
pub fn arena_details() -> Vec<ArenaBox> {
    let mut out = Vec::new();
    let mut add = |center, size, color| {
        out.push(ArenaBox {
            center,
            size,
            color,
            emissive: true,
        })
    };
    let dark = [0.16, 0.20, 0.21];
    let glass = [0.18, 0.28, 0.32];
    let cream = [0.70, 0.67, 0.57];
    for z in [-17.48, 17.48] {
        for x in [-17., -8., 2., 12.] {
            add(V(x, 4.6, z), V(6.2, 1.7, 0.04), dark);
            add(V(x, 4.6, z - z.signum() * 0.025), V(5.9, 1.45, 0.03), glass);
            for offset in [-2., 0., 2.] {
                add(
                    V(x + offset, 4.6, z - z.signum() * 0.05),
                    V(0.065, 1.55, 0.03),
                    cream,
                );
            }
        }
        for x in [-5., 5.] {
            add(V(x, 1.6, z), V(3.6, 3.2, 0.05), dark);
            for y in 0..13 {
                add(
                    V(x, 0.15 + y as f32 * 0.23, z - z.signum() * 0.04),
                    V(3.5, 0.02, 0.025),
                    cream,
                );
            }
        }
        add(V(0., 3.35, z - z.signum() * 0.10), V(43., 0.12, 0.12), dark);
    }
    // Distant utility buildings and roof equipment complete the site silhouette.
    for (x, z) in [(-28., -9.), (28., 9.)] {
        add(V(x, 5., z), V(9., 10., 20.), cream);
        add(V(x, 10.25, z), V(9.5, 0.5, 20.5), dark);
        add(V(x, 11.2, z), V(4., 1.4, 5.), [0.35, 0.37, 0.34]);
    }
    // Service vents and a blue identification band on the central machine housing.
    for x in [-3.51, 3.51] {
        for i in 0..8 {
            add(V(x, 1. + i as f32 * 0.20, 0.), V(0.025, 0.055, 3.8), dark);
        }
        add(V(x, 3.3, 0.), V(0.025, 0.55, 6.8), [0.23, 0.34, 0.40]);
    }
    out
}

#[cfg(test)]
mod arena_tests {
    use super::*;
    use vesper3d::viewer::arena::{ArenaBody, ArenaInput, ArenaMovementConfig};

    #[test]
    fn ordinary_stair_route_reaches_the_west_mid_deck() {
        let walls = colliders();
        let body = run_route(V(-3.8, 0.0, -8.0), -std::f32::consts::FRAC_PI_2, 55, &walls);
        assert!(
            body.feet().0 < -12.0,
            "stair route stopped at {:?}",
            body.feet()
        );
        assert!(
            (body.feet().1 - 2.3).abs() < 0.02,
            "stair route missed deck: {:?}",
            body.feet()
        );
    }

    #[test]
    fn upper_gallery_and_reactor_have_grounded_routes() {
        let walls = colliders();
        let gallery = run_route(V(-5.2, 0.0, 7.0), -std::f32::consts::FRAC_PI_2, 60, &walls);
        assert!(
            (gallery.feet().1 - 4.7).abs() < 0.02,
            "gallery route failed: {:?}",
            gallery.feet()
        );

        let reactor = run_route(V(-11.3, 0.0, 0.0), std::f32::consts::FRAC_PI_2, 55, &walls);
        assert!(
            (reactor.feet().1 - 4.5).abs() < 0.02,
            "reactor route failed: {:?}",
            reactor.feet()
        );
    }

    #[test]
    fn launch_pads_have_clear_vertical_departures() {
        let walls = colliders();
        for pad in launch_pads() {
            let center = V(
                (pad.min.0 + pad.max.0) * 0.5,
                pad.max.1,
                (pad.min.2 + pad.max.2) * 0.5,
            );
            assert!(
                !walls.iter().any(|wall| wall.min.1 > pad.max.1
                    && center.0 >= wall.min.0
                    && center.0 <= wall.max.0
                    && center.2 >= wall.min.2
                    && center.2 <= wall.max.2),
                "launch pad is covered by solid geometry at {center:?}"
            );
        }
    }

    #[test]
    fn every_pickup_is_collectible_from_player_eye_height() {
        for mut pickup in pickups() {
            let player_eye = pickup.position + V(0.0, 0.66, 0.0);
            assert_eq!(
                pickup.collect(player_eye),
                Some(pickup.amount),
                "{} is out of reach",
                pickup.id
            );
        }
    }

    #[test]
    fn traversal_never_leaves_body_embedded_in_walls_or_steps() {
        let walls = colliders();
        for start in [
            V(-18., 0., -12.),
            V(18., 0., 12.),
            V(-11.3, 0., 0.),
            V(-5.2, 0., 7.),
        ] {
            let mut body = ArenaBody::spawn(start, 0., Default::default()).unwrap();
            for tick in 0..900 {
                body.yaw = (tick / 90) as f32 * 0.73;
                body.step(
                    ArenaInput {
                        forward: 1.,
                        right: if tick % 180 < 90 { 0.4 } else { -0.4 },
                        jump: tick % 73 == 0,
                    },
                    1. / 60.,
                    &walls,
                );
                let feet = body.feet();
                assert!(
                    !walls.iter().any(|c| c.max.1 > feet.1 + 0.002
                        && c.min.1 < feet.1 + body.config.height - 0.002
                        && c.overlaps_xz(body.position, body.config.radius - 0.002)),
                    "embedded at tick {tick}: {:?}",
                    body.position
                );
            }
        }
    }

    fn run_route(feet: V, yaw: f32, ticks: usize, walls: &[Collider]) -> ArenaBody {
        let mut body = ArenaBody::spawn(feet, yaw, ArenaMovementConfig::default()).unwrap();
        for _ in 0..ticks {
            body.step(
                ArenaInput {
                    forward: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
                walls,
            );
        }
        body
    }
}
