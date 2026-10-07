//! Stable map IDs, shared cached geometry, and objective anchors.
use crate::level::{AmbientKind, AmbientSpot, Builder, DecorKind as D, Level, LightSpot, Loot, Material as M, Spawn};
use std::sync::OnceLock;
use vesper3d::math::V;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapId {
    #[default]
    Slagworks = 0,
    Switchyard = 1,
    Stormbreak = 2,
}
impl MapId {
    pub const ALL: [Self; 3] = [Self::Slagworks, Self::Switchyard, Self::Stormbreak];
    pub fn from_id(id: u8) -> Self {
        Self::ALL[id.min(2) as usize]
    }
    pub fn name(self) -> &'static str {
        ["Slagworks", "Switchyard", "Stormbreak"][self as usize]
    }
    pub fn build(self) -> Level {
        match self {
            Self::Slagworks => crate::slagworks::build(),
            Self::Switchyard => switchyard(),
            Self::Stormbreak => stormbreak(),
        }
    }
    pub fn level(self) -> &'static Level {
        static LEVELS: [OnceLock<Level>; 3] = [const { OnceLock::new() }; 3];
        LEVELS[self as usize].get_or_init(|| self.build())
    }
    pub fn bases(self) -> [V; 2] {
        let z = match self {
            Self::Slagworks => 40.,
            Self::Switchyard => 23.,
            Self::Stormbreak => 27.,
        };
        [V(0., 0., z), V(0., 0., -z)]
    }
    pub fn sites(self) -> [V; 2] {
        match self {
            Self::Slagworks => [V(-15., 0., 8.), V(15., 0., -8.)],
            Self::Switchyard => [V(-20., 0., 0.), V(20., 0., 0.)],
            Self::Stormbreak => [V(-24., 0., 0.), V(24., 0., 0.)],
        }
    }
}
fn arena(name: &'static str, x: f32, z: f32, surface: M) -> Builder {
    let mut b = Builder::new(name, x, z);
    b.solid(-x, -z, x, z, -1., 1., surface);
    for f in [-1., 1.] {
        b.solid(-x - 0.5, f * z - 0.25, x + 0.5, f * z + 0.25, 0., 4., M::ConcreteDark);
        b.solid(f * x - 0.25, -z, f * x + 0.25, z, 0., 4., M::ConcreteDark);
    }
    for team in 0..2 {
        let f = if team == 0 { 1. } else { -1. };
        for i in 0..8 {
            b.level.spawns[team].push(Spawn {
                pos: V(-10.5 + i as f32 * 3., 0., f * (z - 3.)),
                yaw: if team == 0 { 0. } else { std::f32::consts::PI },
            });
        }
        // Three exits around offset blast walls; opponents cannot shoot straight into the spawn row.
        for x in [-10., 0., 10.] {
            b.solid(x - 2., f * (z - 8.) - 0.35, x + 2., f * (z - 8.) + 0.35, 0., 2.3, M::Concrete);
        }
        b.decor(D::Sign, V(0., 0., f * (z - 0.8)), 0., 1.);
        for (x, key) in [(-15., "hornet"), (15., "breach8"), (-7., "k47"), (7., "frag")] {
            b.level.loot.push(Loot {
                pos: V(x, 0., f * (z - 10.)),
                weapon: crate::weapons::id_of(key).unwrap(),
                respawn_s: 25.,
            });
        }
    }
    b
}
fn stairs(b: &mut Builder, x: f32, z: f32, dir: f32, height: f32) -> f32 {
    let n = (height / 0.20).ceil() as usize;
    for i in 0..n {
        let a = z + dir * i as f32 * 0.4;
        let c = z + dir * (i + 1) as f32 * 0.4;
        b.solid(x - 0.8, a, x + 0.8, c, 0., height * (i + 1) as f32 / n as f32, M::ConcreteDark);
    }
    z + dir * n as f32 * 0.4
}
fn freight_crate(b: &mut Builder, x: f32, z: f32, h: f32) {
    b.solid(x - 0.65, z - 0.65, x + 0.65, z + 0.65, 0., h, M::Wood);
    b.decor(D::Pallet, V(x, 0.02, z), 0., 1.4);
}
fn switchyard() -> Level {
    let mut b = arena("Switchyard", 32., 27., M::Gravel);
    // Open freight shed: four wide doors, solid end walls, central low machinery, a roof over the duel lane.
    for x in [-8., 8.] {
        for z in [-5., 5.] {
            b.solid(x - 0.35, z - 0.35, x + 0.35, z + 0.35, 0., 5., M::RustMetal);
        }
    }
    b.solid(-8.4, -5.4, 8.4, 5.4, 5., 0.25, M::Metal);
    for x in [-8., 8.] {
        b.solid(x - 0.25, -2., x + 0.25, 2., 0., 3.5, M::Brick);
    }
    b.solid(-2., -1., 2., 1., 0., 1.05, M::ConcreteDark);
    for f in [-1., 1.] {
        // Staggered wagons leave a middle cross lane and two outside flanks.
        b.solid(-26., f * 10. - 1.5, -14., f * 10. + 1.5, 0., 2.6, M::ContainerRed);
        b.solid(14., f * 14. - 1.5, 26., f * 14. + 1.5, 0., 2.6, M::ContainerBlue);
        for x in [-20., 20.] {
            if f > 0. {
                b.decor(D::RailTrack, V(x, 0.015, -25.), std::f32::consts::FRAC_PI_2, 50.);
            }
            b.decor(D::FloodLight, V(x, 0., f * 21.), 0., 1.);
        }
        b.decor(D::Crane, V(f * 29., 0., 0.), 0., 0.8);
        b.decor(D::Pallet, V(f * 12., 0., f * 3.), 0., 1.);
        b.level.lights.push(LightSpot { pos: V(0., 4.4, f * 3.), radius: 20., rgb: [1., 0.8, 0.5], intensity: 0.7 });
        b.level.ambient.push(AmbientSpot { pos: V(f * 29., 2., 0.), kind: AmbientKind::MetalCreak, radius: 24. });
        b.level.loot.push(Loot {
            pos: V(f * 20., 0., 0.),
            weapon: crate::weapons::id_of("ranger").unwrap(),
            respawn_s: 35.,
        });
    }
    // Two loading overlooks with broad steps, and ground routes around both ends.
    for f in [-1., 1.] {
        b.solid(f * 27. - 2., -3., f * 27. + 2., 3., 0., 1., M::ConcreteDark);
        stairs(&mut b, f * 27., 5., -1., 1.);
        b.solid(f * 27. - 1.5, -2.8, f * 27. + 1.5, -2.4, 1., 0.85, M::Metal);
        for z in [-17., 17.] {
            freight_crate(&mut b, f * 12., z, 1.1);
            freight_crate(&mut b, f * 25., z - f * 2., 1.35);
            b.decor(D::OilDrum, V(f * 11., 0., z - f * 2.), 0., 0.85);
            b.decor(D::Tyres, V(f * 28., 0., z + 2.), 0., 1.);
            b.solid(f * 27. - 2., z + 3., f * 27. + 2., z + 3.4, 0., 2.7, M::Brick);
            b.solid(f * 27. - 2., z + 3., f * 27. - 1.6, z + 6., 0., 2.7, M::Brick);
            b.solid(f * 27. - 2., z + 3., f * 27. + 2., z + 6., 2.7, 0.15, M::Metal);
            b.decor(D::Vent, V(f * 27., 2.85, z + 4.5), 0., 0.7);
        }
        for z in [-6., 6.] {
            freight_crate(&mut b, f * 12., z, 1.25);
            b.decor(D::Barrel, V(f * 13.7, 0., z), 0., 1.);
        }
        b.solid(-4., f * 13. - 0.4, 4., f * 13. + 0.4, 0., 1.1, M::Concrete);
        b.decor(D::Sandbags, V(-4., 0., f * 13.), 0., 8.);
        b.level.loot.push(Loot {
            pos: V(f * 27., 1., 0.),
            weapon: crate::weapons::id_of("smoke").unwrap(),
            respawn_s: 25.,
        });
        b.level.lights.push(LightSpot { pos: V(f * 27., 2.6, 0.), radius: 14., rgb: [1., 0.75, 0.45], intensity: 0.5 });
    }
    // Road markings and a pale apron give clear visual cues to the center and base exits.
    b.solid(-4., -23., 4., 23., 0., 0.012, M::Asphalt);
    for z in [-22., 22.] {
        b.solid(-18., z - 2., 18., z + 2., 0., 0.018, M::Concrete);
    }
    // Feet start on the raised apron, rather than penetrating its edge collider.
    for spawn in b.level.spawns.iter_mut().flatten() {
        spawn.pos.1 = 0.018;
    }
    b.finish()
}
fn stormbreak() -> Level {
    let mut b = arena("Stormbreak", 40., 31., M::Concrete);
    // Low sea walls contain movement while leaving the coastal horizon visible at eye height.
    for wall in b.level.blocks.iter_mut().skip(1) {
        wall.max.1 = 1.15;
    }
    // Relay station with an east/west through corridor and doors on both approaches.
    for z in [-7., 7.] {
        for (x0, x1) in [(-9., -2.), (2., 9.)] {
            b.solid(x0, z - 0.25, x1, z + 0.25, 0., 3.6, M::Plaster);
        }
    }
    for x in [-9., 9.] {
        for (z0, z1) in [(-7., -2.5), (2.5, 7.)] {
            b.solid(x - 0.25, z0, x + 0.25, z1, 0., 3.6, M::ConcreteDark);
        }
    }
    b.solid(-9.3, -7.3, 9.3, 7.3, 3.6, 0.25, M::Metal);
    b.solid(-1., -1., 1., 1., 0., 1.2, M::Metal);
    b.decor(D::RadarDish, V(0., 3.85, 0.), 0., 1.);
    for f in [-1., 1.] {
        // Terrace cover is offset: fast lower flanks remain clear and the central corridor is contested.
        b.solid(-33., f * 13. - 2., -21., f * 13. + 2., 0., 1.15, M::ConcreteDark);
        b.solid(21., f * 18. - 2., 33., f * 18. + 2., 0., 1.15, M::ConcreteDark);
        // Roofed generator terraces and windbreaks make the outside lanes readable.
        for (x, z) in [(-27., f * 13.), (27., f * 18.)] {
            for dx in [-5.5, 5.5] {
                for dz in [-1.7, 1.7] {
                    b.solid(x + dx - 0.12, z + dz - 0.12, x + dx + 0.12, z + dz + 0.12, 1.15, 2.15, M::RustMetal);
                }
            }
            b.solid(x - 6., z - 2.2, x + 6., z + 2.2, 3.3, 0.15, M::Metal);
            for dx in [-2.5, 2.5] {
                b.solid(x + dx - 1., z - 0.85, x + dx + 1., z + 0.85, 1.15, 1.25, M::Metal);
                b.decor(D::Vent, V(x + dx, 2.4, z), 0., 0.7);
            }
        }
        b.solid(f * 18. - 2., f * 21. - 0.25, f * 18. + 2., f * 21. + 0.25, 0., 1.3, M::ConcreteDark);
        b.decor(D::Sandbags, V(f * 18. - 2., 0., f * 21.), 0., 4.);
        b.solid(f * 14. - 1., f * 10. - 2., f * 14. + 1., f * 10. + 2., 0., 2.4, M::Metal);
        for x in [-35., 35.] {
            b.decor(D::LampPost, V(x, 0., f * 23.), 0., 1.);
            b.decor(D::Fence, V(x, 0., f * 8.), 0., 1.);
        }
        b.decor(D::Tank, V(f * 29., 0., f * 25.), 0., 0.6);
        b.decor(D::PipeRun, V(f * 12., 0., 0.), std::f32::consts::FRAC_PI_2, 1.);
        b.level.loot.push(Loot {
            pos: V(f * 24., 0., 0.),
            weapon: crate::weapons::id_of("ranger").unwrap(),
            respawn_s: 30.,
        });
        b.level.ambient.push(AmbientSpot { pos: V(f * 39., 1., 0.), kind: AmbientKind::WindGap, radius: 40. });
    }
    // Climbable relay roof, two opposed stair approaches, and clear lines to either objective site.
    for f in [-1., 1.] {
        stairs(&mut b, f * 12., f * 15., -f, 3.85);
        b.solid(f * 10.5 - 2., f * 7. - 0.8, f * 10.5 + 2., f * 7. + 0.8, 3.65, 0.2, M::Metal);
        b.solid(f * 12. - 0.15, f * 7. - 0.8, f * 12. + 0.15, f * 7. + 0.8, 0., 3.65, M::ConcreteDark);
        for z in [-3., 3.] {
            b.solid(f * 27. - 1., z - 0.4, f * 27. + 1., z + 0.4, 0., 1.1, M::ConcreteDark);
            b.decor(D::Sandbags, V(f * 26., 0., z), 0., 2.);
        }
        b.solid(f * 30. - 1.5, -1.5, f * 30. + 1.5, 1.5, 0., 2.4, M::ContainerBlue);
        b.decor(D::FloodLight, V(f * 31., 0., f * 20.), 0., 1.);
        for z in [-20., 20.] {
            b.decor(D::OilDrum, V(f * 20., 0., z), 0., 1.);
            freight_crate(&mut b, f * 18., z + f * 2., 1.15);
        }
        b.decor(D::Cable, V(f * 9., 0.04, -7.), std::f32::consts::FRAC_PI_2, 14.);
        b.level.loot.push(Loot {
            pos: V(f * 5., 3.85, 0.),
            weapon: crate::weapons::id_of("dmr20").unwrap(),
            respawn_s: 35.,
        });
        b.level.lights.push(LightSpot { pos: V(f * 4., 3.3, 0.), radius: 15., rgb: [0.6, 0.8, 1.], intensity: 0.65 });
    }
    // Water is visible beyond the fenced perimeter; never a movement collider.
    for f in [-1., 1.] {
        b.solid(f * 27. - 9., -21., f * 27. + 9., 21., 0., 0.008, M::Asphalt);
        for z in [-8., 8.] {
            freight_crate(&mut b, f * 34., z, 1.2);
            b.decor(D::OilDrum, V(f * 32., 0., z), 0., 1.);
        }
    }
    b.solid(-52., -43., 52., 43., -1.6, 0.2, M::Water);
    b.finish()
}
