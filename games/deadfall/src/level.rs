//! The map as data: solid blocks, spawns, weapon spots, decoration. Pure and deterministic, so the server, the
//! bots and the tests use exactly what the client draws and collides with.
//!
//! Conventions (the engine's): metres, +Y up, yaw 0 faces -Z and positive yaw turns towards +X. Every solid is an
//! axis-aligned box ([`Block`]); the engine's `Controller` collides with them as `Collider`s and steps over
//! anything up to its step height, so stairs are stacks of low boxes and ramps are not supported.
use crate::weapons::WeaponId;
use vesper3d::math::V;
use vesper3d::viewer::controller::Collider;

/// What a block is made of: picks its colour in the renderer and its footstep and impact sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    Concrete,
    ConcreteDark,
    Asphalt,
    Gravel,
    Dirt,
    Grass,
    Brick,
    Plaster,
    Metal,
    RustMetal,
    Wood,
    Glass,
    /// Shipping containers, four colours.
    ContainerRed,
    ContainerBlue,
    ContainerGreen,
    ContainerYellow,
    /// Hazard-striped yellow and black.
    Hazard,
    /// Water (shallow, not solid: decoration only).
    Water,
}

/// A solid, visible box. `solid: false` makes it decoration that neither blocks movement nor bullets (glass panes
/// are solid to movement but let bullets through is NOT supported: keep it simple, glass blocks both).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub min: V,
    pub max: V,
    pub material: Material,
}

/// A place a player can appear, facing `yaw`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spawn {
    /// Feet position.
    pub pos: V,
    pub yaw: f32,
}

/// Where a weapon lies on the ground at the start of a match.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loot {
    /// Feet-level position (the weapon is drawn a little above it).
    pub pos: V,
    pub weapon: WeaponId,
    /// Seconds after pickup before another copy appears (0 = never; carried weapons are dropped, not respawned).
    pub respawn_s: f32,
}

/// Non-solid scenery the renderer builds from its kind (plants, lamps, barrels, pipes ...). Solid scenery is
/// ALSO a [`Block`] so it is collided with; `Decor` is only what is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decor {
    pub kind: DecorKind,
    pub pos: V,
    pub yaw: f32,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecorKind {
    Tree,
    Pine,
    Bush,
    GrassTuft,
    Weeds,
    PottedPlant,
    Barrel,
    OilDrum,
    PipeRun,
    LampPost,
    FloodLight,
    Crate,
    Pallet,
    Tyres,
    Sign,
    Fence,
    Vent,
    Chimney,
    WaterTower,
    CoolingTower,
    Crane,
    RailTrack,
    Tank,
    Puddle,
    Rubble,
    Sandbags,
    Cable,
}

/// A fixed light in the world (lamps, furnace glow). The renderer may use the four nearest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSpot {
    pub pos: V,
    pub radius: f32,
    pub rgb: [f32; 3],
    pub intensity: f32,
}

/// A place the world makes natural noise from (machinery hum, dripping, wind through a gap, birds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmbientKind {
    MachineHum,
    Dripping,
    WindGap,
    Birds,
    MetalCreak,
    Steam,
    Crickets,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientSpot {
    pub pos: V,
    pub kind: AmbientKind,
    pub radius: f32,
}

/// The whole map.
#[derive(Clone, Debug)]
pub struct Level {
    pub name: &'static str,
    /// Playable area (x/z half extents around the origin) and the floor-fall limit.
    pub half_x: f32,
    pub half_z: f32,
    pub blocks: Vec<Block>,
    /// Spawns per team (index 0 and 1), at least 8 each, all inside the team's base.
    pub spawns: [Vec<Spawn>; 2],
    pub loot: Vec<Loot>,
    pub decor: Vec<Decor>,
    pub lights: Vec<LightSpot>,
    pub ambient: Vec<AmbientSpot>,
}

impl Level {
    /// The engine colliders for every solid block, in block order.
    pub fn colliders(&self) -> Vec<Collider> {
        self.blocks.iter().map(|b| Collider { min: b.min, max: b.max }).collect()
    }

    /// Distance along the ray to the nearest block, if it is within `max`.
    pub fn raycast(&self, origin: V, dir: V, max: f32) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        for (i, b) in self.blocks.iter().enumerate() {
            if b.material == Material::Water {
                continue;
            }
            if let Some(t) = ray_box(origin, dir, b.min, b.max, best.map_or(max, |(d, _)| d)) {
                best = Some((t, i));
            }
        }
        best
    }

    /// Like [`Level::raycast`], plus the outward normal of the face that was hit.
    pub fn raycast_normal(&self, origin: V, dir: V, max: f32) -> Option<(f32, usize, V)> {
        let (t, i) = self.raycast(origin, dir, max)?;
        let b = &self.blocks[i];
        let p = origin + dir * t;
        // The face the hit point lies on (the one it is closest to).
        let faces = [
            ((p.0 - b.min.0).abs(), V(-1., 0., 0.)),
            ((p.0 - b.max.0).abs(), V(1., 0., 0.)),
            ((p.1 - b.min.1).abs(), V(0., -1., 0.)),
            ((p.1 - b.max.1).abs(), V(0., 1., 0.)),
            ((p.2 - b.min.2).abs(), V(0., 0., -1.)),
            ((p.2 - b.max.2).abs(), V(0., 0., 1.)),
        ];
        let n = faces.iter().fold((f32::MAX, V(0., 1., 0.)), |best, f| if f.0 < best.0 { *f } else { best }).1;
        Some((t, i, n))
    }

    /// Whether the straight segment between two points is clear of every block.
    pub fn line_of_sight(&self, a: V, b: V) -> bool {
        let d = b - a;
        let len = d.length();
        if len < 1e-4 {
            return true;
        }
        self.raycast(a, d * (1. / len), len - 0.01).is_none()
    }
}

/// Slab test: entry distance of a ray into an axis-aligned box, within `0..=max`.
pub fn ray_box(o: V, d: V, min: V, max: V, limit: f32) -> Option<f32> {
    let mut t0 = 0f32;
    let mut t1 = limit;
    for (o, d, lo, hi) in [(o.0, d.0, min.0, max.0), (o.1, d.1, min.1, max.1), (o.2, d.2, min.2, max.2)] {
        if d.abs() < 1e-9 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let (mut a, mut b) = ((lo - o) / d, (hi - o) / d);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            t0 = t0.max(a);
            t1 = t1.min(b);
            if t0 > t1 {
                return None;
            }
        }
    }
    Some(t0)
}

/// Convenience for writing maps: boxes from a corner and a size.
pub struct Builder {
    pub level: Level,
}

impl Builder {
    pub fn new(name: &'static str, half_x: f32, half_z: f32) -> Self {
        Self {
            level: Level {
                name,
                half_x,
                half_z,
                blocks: Vec::new(),
                spawns: [Vec::new(), Vec::new()],
                loot: Vec::new(),
                decor: Vec::new(),
                lights: Vec::new(),
                ambient: Vec::new(),
            },
        }
    }
    /// A block from two opposite corners (any order).
    pub fn block(&mut self, a: V, b: V, material: Material) -> &mut Self {
        let (min, max) = (V(a.0.min(b.0), a.1.min(b.1), a.2.min(b.2)), V(a.0.max(b.0), a.1.max(b.1), a.2.max(b.2)));
        self.level.blocks.push(Block { min, max, material });
        self
    }
    /// A block standing on the ground (y from `y0`) with the footprint `x0..x1`, `z0..z1` and `height`.
    pub fn solid(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y0: f32, height: f32, material: Material) -> &mut Self {
        self.block(V(x0, y0, z0), V(x1, y0 + height, z1), material)
    }
    pub fn decor(&mut self, kind: DecorKind, pos: V, yaw: f32, scale: f32) -> &mut Self {
        self.level.decor.push(Decor { kind, pos, yaw, scale });
        self
    }
    pub fn finish(self) -> Level {
        self.level
    }
}

/// A stand-in map until the real one is written: a walled square with two spawn rows.
pub fn placeholder() -> Level {
    let mut b = Builder::new("Placeholder", 30., 30.);
    b.solid(-30., -30., 30., 30., -1., 1., Material::Asphalt);
    b.solid(-30., -31., 30., -30., 0., 6., Material::Concrete);
    b.solid(-30., 30., 30., 31., 0., 6., Material::Concrete);
    b.solid(-31., -30., -30., 30., 0., 6., Material::Concrete);
    b.solid(30., -30., 31., 30., 0., 6., Material::Concrete);
    b.solid(-3., -3., 3., 3., 0., 2.5, Material::ContainerRed);
    for i in 0..8 {
        b.level.spawns[0].push(Spawn { pos: V(-14. + i as f32 * 4., 0., 26.), yaw: 0. });
        b.level.spawns[1].push(Spawn { pos: V(-14. + i as f32 * 4., 0., -26.), yaw: std::f32::consts::PI });
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_hits_the_near_face_and_misses_past_the_end() {
        let level = placeholder();
        let (t, _) = level.raycast(V(0., 1., 10.), V(0., 0., -1.), 50.).unwrap();
        assert!((t - 7.).abs() < 1e-4, "{t}");
        assert!(level.raycast(V(0., 1., 10.), V(0., 0., -1.), 5.).is_none());
    }
    #[test]
    fn line_of_sight_is_blocked_by_a_block() {
        let level = placeholder();
        assert!(!level.line_of_sight(V(0., 1., 10.), V(0., 1., -10.)));
        assert!(level.line_of_sight(V(10., 1., 10.), V(10., 1., -10.)));
    }
}
