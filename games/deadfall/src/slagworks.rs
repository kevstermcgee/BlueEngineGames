//! "Slagworks": an overgrown steelworks, about 120 m by 90 m inside its perimeter wall.
//!
//! Team 0 (Ironclad) starts at the south end (+z), team 1 (Nightwatch) at the north end (-z). Three routes
//! lead between the bases:
//!
//! ```text
//!   x: -60      -46   -34 -20 -15        15   23        44  54 60
//! z=-45 +--------------------------------------------------------+
//!       | CT1 scrap | NORTH BASE (spawns -z)        | substation |
//!       |           |  gates                          |            |
//!       | goods shed|  rail yard | office | SMELTER  | yard  |tower| pipe
//!  z=0  | (dock)    |  (cars)    | 2 floor| HALL     |containers| alley
//!       | warehouse |  rail yard | office | (30x22)  |       |     |
//!       | SW yard   |            |        |           | boiler| CT2 |
//! z=+45 +--------------------------------------------------------+
//!          WEST route (rail yard + dock), CENTRE (smelter hall / office), EAST (containers + pipe alley)
//! ```
//!
//! Everything is axis-aligned boxes. The south half of several areas is written once with positive z and
//! mirrored (`flip = -1`) for the north so both teams get the same fight; the one-of-a-kind buildings
//! (hall, office) straddle z = 0, and the warehouse (south-west) and loading dock shed (north-west) are two
//! different buildings on mirrored footprints.
#![allow(clippy::too_many_arguments)]
use crate::level::{AmbientKind, AmbientSpot, Builder, DecorKind, Level, LightSpot, Loot, Material, Spawn};
use crate::weapons::WeaponId;
use vesper3d::math::V;

use DecorKind as D;
use Material as M;

/// Height of a ground patch above the dirt base, per layer, so coplanar patches never z-fight.
const L_GRASS: f32 = 0.012;
const L_GRAVEL: f32 = 0.018;
const L_ASPH: f32 = 0.024;
const L_CONC: f32 = 0.030;
const L_FLOOR: f32 = 0.036;

const HALF_X: f32 = 60.;
const HALF_Z: f32 = 45.;

/// A wall opening: `a..b` along the wall, from `lo` to `hi` above the wall's base.
#[derive(Clone, Copy)]
struct G {
    a: f32,
    b: f32,
    lo: f32,
    hi: f32,
}
fn door(a: f32, b: f32) -> G {
    G { a, b, lo: 0., hi: 2.4 }
}
/// A wide opening (vehicle gate, roller door) of the given clear height.
fn gate(a: f32, b: f32, hi: f32) -> G {
    G { a, b, lo: 0., hi }
}
/// A window with a 1.1 m sill (duck behind it), 1.2 m tall.
fn win(a: f32, b: f32) -> G {
    G { a, b, lo: 1.1, hi: 2.3 }
}

#[derive(Clone, Copy, PartialEq)]
enum Dir {
    PX,
    NX,
    PZ,
    NZ,
}

struct Map {
    b: Builder,
    /// +1 writes in south coordinates, -1 mirrors z.
    flip: f32,
    seed: u32,
    scatter: Vec<(DecorKind, [f32; 4], u32, f32, f32)>,
}

impl Map {
    fn new() -> Self {
        Self { b: Builder::new("Slagworks", HALF_X, HALF_Z), flip: 1., seed: 0x9e37_79b9, scatter: Vec::new() }
    }
    fn zz(&self, z: f32) -> f32 {
        z * self.flip
    }
    fn rnd(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
    /// A box from x/z corners (z mirrored when flipped), its bottom `y0` and `h` tall.
    fn bx(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y0: f32, h: f32, mat: Material) {
        let (a, b) = (self.zz(z0), self.zz(z1));
        self.b.solid(x0.min(x1), a.min(b), x0.max(x1), a.max(b), y0, h, mat);
    }
    /// A thin ground patch (material on the dirt), `top` metres high.
    fn patch(&mut self, mat: Material, x0: f32, z0: f32, x1: f32, z1: f32, top: f32) {
        self.bx(x0, z0, x1, z1, 0., top, mat);
    }
    fn wall_x(&mut self, x0: f32, x1: f32, z: f32, t: f32, y0: f32, h: f32, mat: Material, gaps: &[G]) {
        let mut g = gaps.to_vec();
        g.sort_by(|p, q| p.a.total_cmp(&q.a));
        let (za, zb) = (z - t / 2., z + t / 2.);
        let mut cur = x0;
        for gp in g {
            if gp.a > cur {
                self.bx(cur, za, gp.a, zb, y0, h, mat);
            }
            if gp.lo > 0. {
                self.bx(gp.a, za, gp.b, zb, y0, gp.lo, mat);
            }
            if gp.hi < h {
                self.bx(gp.a, za, gp.b, zb, y0 + gp.hi, h - gp.hi, mat);
            }
            cur = gp.b;
        }
        if x1 > cur {
            self.bx(cur, za, x1, zb, y0, h, mat);
        }
    }
    fn wall_z(&mut self, x: f32, z0: f32, z1: f32, t: f32, y0: f32, h: f32, mat: Material, gaps: &[G]) {
        let mut g = gaps.to_vec();
        g.sort_by(|p, q| p.a.total_cmp(&q.a));
        let (xa, xb) = (x - t / 2., x + t / 2.);
        let mut cur = z0;
        for gp in g {
            if gp.a > cur {
                self.bx(xa, cur, xb, gp.a, y0, h, mat);
            }
            if gp.lo > 0. {
                self.bx(xa, gp.a, xb, gp.b, y0, gp.lo, mat);
            }
            if gp.hi < h {
                self.bx(xa, gp.a, xb, gp.b, y0 + gp.hi, h - gp.hi, mat);
            }
            cur = gp.b;
        }
        if z1 > cur {
            self.bx(xa, cur, xb, z1, y0, h, mat);
        }
    }
    /// Four walls around the outer footprint `x0..x1`, `z0..z1` (z0 < z1 in written coordinates).
    #[allow(clippy::too_many_arguments)]
    fn shell(
        &mut self,
        (x0, z0, x1, z1): (f32, f32, f32, f32),
        t: f32,
        y0: f32,
        h: f32,
        mat: Material,
        n: &[G],
        s: &[G],
        w: &[G],
        e: &[G],
    ) {
        self.wall_x(x0, x1, z0 + t / 2., t, y0, h, mat, n);
        self.wall_x(x0, x1, z1 - t / 2., t, y0, h, mat, s);
        self.wall_z(x0 + t / 2., z0 + t, z1 - t, t, y0, h, mat, w);
        self.wall_z(x1 - t / 2., z0 + t, z1 - t, t, y0, h, mat, e);
    }
    fn roof(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, mat: Material) {
        self.bx(x0, z0, x1, z1, y, 0.4, mat);
    }
    /// Steps of at most 0.22 m rise and 0.3 m run, starting at the lower edge (`x`,`z` = centre of that
    /// edge), ascending towards `dir` up to `height` above `y0`. Returns the coordinate where they end.
    #[allow(clippy::too_many_arguments)]
    fn stairs(&mut self, x: f32, z: f32, dir: Dir, width: f32, height: f32, y0: f32, mat: Material) -> f32 {
        let n = (height / 0.22).ceil() as usize;
        let rise = height / n as f32;
        let run = 0.3;
        let hw = width / 2.;
        for i in 0..n {
            let (a, b) = (i as f32 * run, (i + 1) as f32 * run);
            let h = (i + 1) as f32 * rise;
            match dir {
                Dir::PX => self.bx(x + a, z - hw, x + b, z + hw, y0, h, mat),
                Dir::NX => self.bx(x - b, z - hw, x - a, z + hw, y0, h, mat),
                Dir::PZ => self.bx(x - hw, z + a, x + hw, z + b, y0, h, mat),
                Dir::NZ => self.bx(x - hw, z - b, x + hw, z - a, y0, h, mat),
            }
        }
        let len = n as f32 * run;
        match dir {
            Dir::PX => x + len,
            Dir::NX => x - len,
            Dir::PZ => z + len,
            Dir::NZ => z - len,
        }
    }
    /// A raised walkway or floor whose walking surface is at `top`.
    fn plat(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, top: f32, thick: f32, mat: Material) {
        self.bx(x0, z0, x1, z1, top - thick, thick, mat);
    }
    /// A thin railing (duck cover) along a line, standing on `y0`.
    fn rail(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y0: f32) {
        self.bx(x0 - 0.04, z0 - 0.04, x1 + 0.04, z1 + 0.04, y0, 1.1, M::Metal);
    }
    /// A chain-link fence panel run (thin, tall metal block: drawn as wire by the renderer).
    fn fence(&mut self, x0: f32, z0: f32, x1: f32, z1: f32) {
        self.bx(x0 - 0.03, z0 - 0.03, x1 + 0.03, z1 + 0.03, 0., 2.2, M::Metal);
    }
    /// A shipping container with its min corner at `x`,`z`; `len` along x when `along_x`.
    fn cont(&mut self, x: f32, z: f32, len: f32, along_x: bool, stack: u32, mat: Material) {
        let (dx, dz) = if along_x { (len, 2.44) } else { (2.44, len) };
        self.bx(x, z, x + dx, z + dz, 0., 2.6 * stack as f32, mat);
    }
    fn desk(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y: f32) {
        self.bx(x0, z0, x1, z1, y, 0.76, M::Wood);
    }
    fn locker(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y: f32) {
        self.bx(x0, z0, x1, z1, y, 1.9, M::Metal);
    }
    fn dec(&mut self, kind: DecorKind, x: f32, y: f32, z: f32, yaw: f32, scale: f32) {
        let yaw = if self.flip < 0. { std::f32::consts::PI - yaw } else { yaw };
        let z = self.zz(z);
        self.b.decor(kind, V(x, y, z), yaw, scale);
    }
    fn light(&mut self, x: f32, y: f32, z: f32, radius: f32, rgb: [f32; 3], intensity: f32) {
        let z = self.zz(z);
        self.b.level.lights.push(LightSpot { pos: V(x, y, z), radius, rgb, intensity });
    }
    fn lamp(&mut self, x: f32, z: f32) {
        self.dec(D::LampPost, x, 0., z, 0., 1.);
        self.light(x, 4.6, z, 11., [1., 0.85, 0.6], 0.7);
    }
    fn hum(&mut self, kind: AmbientKind, x: f32, y: f32, z: f32, radius: f32) {
        let z = self.zz(z);
        self.b.level.ambient.push(AmbientSpot { pos: V(x, y, z), kind, radius });
    }
    fn loot(&mut self, w: WeaponId, x: f32, y: f32, z: f32) {
        let z = self.zz(z);
        let respawn_s = if is_rare(w) { 90. } else { 45. };
        self.b.level.loot.push(Loot { pos: V(x, y, z), weapon: w, respawn_s });
    }
    /// A tree: a solid trunk and the drawn crown.
    fn tree(&mut self, x: f32, z: f32, pine: bool, scale: f32) {
        let r = if pine { 0.2 } else { 0.26 } * scale;
        self.bx(x - r, z - r, x + r, z + r, 0., 3.2 * scale, M::Wood);
        self.dec(if pine { D::Pine } else { D::Tree }, x, 0., z, 0., scale);
    }
    /// A barrel you cannot walk through.
    fn barrel(&mut self, x: f32, z: f32, oil: bool) {
        self.bx(x - 0.3, z - 0.3, x + 0.3, z + 0.3, 0., 0.95, M::RustMetal);
        self.dec(if oil { D::OilDrum } else { D::Barrel }, x, 0., z, 0., 1.);
    }
    /// A crate block (solid).
    fn crate_(&mut self, x: f32, z: f32, s: f32) {
        self.bx(x - s / 2., z - s / 2., x + s / 2., z + s / 2., 0., s, M::Wood);
    }
    fn weeds(&mut self, kind: DecorKind, rect: [f32; 4], n: u32, lo: f32, hi: f32) {
        let (z0, z1) = (self.zz(rect[1]), self.zz(rect[3]));
        self.scatter.push((kind, [rect[0], z0.min(z1), rect[2], z0.max(z1)], n, lo, hi));
    }

    // ------------------------------------------------------------------------------------------------
    fn ground(&mut self) {
        self.b.block(V(-70., -1., -55.), V(70., 0., 55.), M::Dirt);
        // Outer wall: panels of two materials, 7 m high, nothing climbable.
        let mats = [M::Brick, M::ConcreteDark];
        for (i, (x0, x1)) in
            [(-60.6, -40.), (-40., -20.), (-20., 0.), (0., 20.), (20., 40.), (40., 60.6)].into_iter().enumerate()
        {
            let m = mats[i % 2];
            self.b.solid(x0, -45.6, x1, -45., 0., 7., m);
            self.b.solid(x0, 45., x1, 45.6, 0., 7., mats[(i + 1) % 2]);
        }
        for (i, (z0, z1)) in [(-45., -22.5), (-22.5, 0.), (0., 22.5), (22.5, 45.)].into_iter().enumerate() {
            self.b.solid(-60.6, z0, -60., z1, 0., 7., mats[i % 2]);
            self.b.solid(60., z0, 60.6, z1, 0., 7., mats[(i + 1) % 2]);
        }
    }

    // --- bases ---------------------------------------------------------------------------------------
    fn base(&mut self) {
        let t = 0.4;
        self.patch(M::Concrete, -21.4, 32., 21.4, 45., L_CONC);
        self.patch(M::Asphalt, -19., 18., 19., 32., L_ASPH);
        let h = 3.4;
        self.wall_x(
            -21.4,
            21.4,
            32.2,
            t,
            0.,
            h,
            M::Concrete,
            &[gate(-17., -13., h), gate(-2.5, 2.5, h), gate(13., 17., h)],
        );
        self.wall_z(-21.2, 32.4, 45., t, 0., h, M::Concrete, &[gate(36., 40., h)]);
        self.wall_z(21.2, 32.4, 45., t, 0., h, M::Concrete, &[gate(36., 40., h)]);
        // Hazard cap along the front wall top is drawn by the renderer; here the cover behind the gates.
        for (x0, x1) in [(-16., -14.), (-1., 1.), (14., 16.)] {
            self.bx(x0, 34.6, x1, 35.2, 0., 1.1, M::Concrete);
        }
        self.bx(-8., 38., -5., 38.7, 0., 1.1, M::Concrete);
        self.bx(5., 38., 8., 38.7, 0., 1.1, M::Concrete);
        // Guard hut.
        self.shell((14., 39., 19.6, 44.4), 0.3, 0., 2.8, M::Brick, &[win(15., 16.6)], &[], &[door(40., 41.6)], &[]);
        self.roof(13.9, 38.9, 19.7, 44.5, 2.8, M::Metal);
        self.bx(18.6, 40.5, 19.3, 43.6, 0., 0.5, M::Wood);
        self.dec(D::PottedPlant, 14.7, 0., 43.7, 0., 0.8);
        self.loot(19, 17., 0., 42.4);
        self.loot(26, -18.6, 0., 42.6);
        self.loot(5, -17.6, 0., 38.6);
        self.loot(2, 9.6, 0., 36.4);
        for i in 0..10 {
            let x = -11.25 + 2.5 * i as f32;
            let z = self.zz(42.5);
            let (dx, dz) = (-x, -z);
            let yaw = dx.atan2(-dz);
            self.b.level.spawns[if self.flip > 0. { 0 } else { 1 }].push(Spawn { pos: V(x, 0., z), yaw });
        }
        // Dressing.
        self.lamp(-15., 33.);
        self.lamp(15., 33.);
        self.lamp(0., 44.);
        self.dec(D::Sign, 0., 0., 32.9, 0., 1.);
        self.dec(D::Sandbags, -10., 0., 36.4, 0., 2.6);
        self.dec(D::Sandbags, 10., 0., 36.4, 0., 2.6);
        self.barrel(-19.5, 44., true);
        self.barrel(-18.8, 44.2, false);
        self.dec(D::Crate, -12., 0., 44.3, 0.3, 1.);
        self.dec(D::Pallet, 12., 0., 44.3, 0., 1.);
        self.dec(D::Tyres, 20., 0., 35., 0., 1.);
        self.dec(D::Cable, -4., 0.03, 40.5, 0., 6.);
        self.light(0., 3.2, 40., 14., [1., 0.88, 0.7], 0.5);
        self.weeds(D::Weeds, [-20., 33., 20., 44.5], 18, 0.6, 1.1);
        self.weeds(D::GrassTuft, [-20., 33., 20., 44.5], 14, 0.6, 1.1);
        self.hum(AmbientKind::WindGap, -15., 2., 32., 14.);
        self.hum(AmbientKind::MachineHum, 0., 3., 44., 12.);
    }

    // --- forecourts: where the three routes diverge ------------------------------------------------------
    fn forecourt(&mut self) {
        self.wall_x(-9., -4., 25.5, 0.5, 0., 2.6, M::Concrete, &[]);
        self.wall_x(4., 9., 25.5, 0.5, 0., 2.6, M::Concrete, &[]);
        // Aprons in front of the hall with planted beds.
        self.patch(M::Concrete, -15., 11., 15., 18., L_CONC);
        self.patch(M::Grass, -15.4, 13., -9., 17., L_GRASS);
        self.patch(M::Grass, 9., 13., 15.4, 17., L_GRASS);
        self.tree(-12., 15., false, 1.0);
        self.tree(12., 15.2, true, 1.0);
        self.dec(D::Bush, -10., 0., 14., 0., 1.1);
        self.dec(D::Bush, 10.5, 0., 16.5, 0., 0.9);
        // Overgrown corners of the forecourt.
        self.patch(M::Grass, -32., 17., -23., 22., L_GRASS);
        self.patch(M::Gravel, -33., 23., -22., 28., L_GRAVEL);
        self.patch(M::Grass, 22.5, 24., 25.4, 30., L_GRASS);
        self.tree(-27., 19., false, 1.1);
        self.dec(D::Bush, -30., 0., 21., 0., 1.2);
        self.dec(D::Bush, -24., 0., 18., 0., 1.0);
        self.dec(D::Bush, 24., 0., 27., 0., 1.0);
        self.crate_(0., 22.5, 1.3);
        self.crate_(1.3, 22.6, 0.9);
        // Screens in front of the gates and containers on the flanks.
        self.cont(-19.6, 27., 6.1, true, 1, M::ContainerYellow);
        self.cont(13.5, 27., 6.1, true, 1, M::ContainerGreen);
        self.cont(-38., 24., 12.2, true, 1, M::ContainerRed);
        self.cont(25.8, 24., 12.2, true, 1, M::ContainerBlue);
        // Chicane of jersey barriers on the two side approaches.
        self.bx(-27., 30.5, -24., 31.1, 0., 1.1, M::Concrete);
        self.bx(-46., 29., -43., 29.6, 0., 1.1, M::Concrete);
        self.loot(9, -9., 0., 22.4);
        self.loot(10, 9., 0., 22.4);
        self.lamp(-10., 29.);
        self.lamp(10., 29.);
        self.lamp(-30., 22.);
        self.lamp(31., 22.);
        self.dec(D::Sandbags, -6.5, 0., 25.5, 0., 2.4);
        self.dec(D::Sandbags, 6.5, 0., 25.5, 0., 2.4);
        self.barrel(-18., 25., false);
        self.barrel(-17.4, 25.6, true);
        self.barrel(18.5, 25.4, true);
        self.dec(D::Puddle, -2., L_ASPH, 28., 0., 1.4);
        self.dec(D::Puddle, 6., L_ASPH, 20., 0.4, 1.0);
        self.dec(D::Rubble, 12., 0., 21., 0., 1.);
        self.weeds(D::Weeds, [-20., 18., 20., 32.], 14, 0.6, 1.2);
        self.weeds(D::GrassTuft, [-44., 20., -20., 32.], 18, 0.6, 1.2);
        self.weeds(D::GrassTuft, [20., 20., 44., 32.], 18, 0.6, 1.2);
        self.weeds(D::Bush, [-46., 18., -22., 31.], 4, 0.8, 1.3);
        self.weeds(D::Bush, [22., 18., 44., 31.], 4, 0.8, 1.3);
        self.hum(AmbientKind::WindGap, 0., 2., 26., 12.);
    }

    // --- the Smelter Hall ----------------------------------------------------------------------------
    fn smelter_hall(&mut self) {
        let (x0, x1, z0, z1) = (-15., 15., -11., 11.);
        self.patch(M::ConcreteDark, x0, z0, x1, z1, L_FLOOR);
        let gn = [door(-7.5, -5.1), gate(-3., 3., 5.), door(5.1, 7.5)];
        let gs = [door(-7.5, -5.1), gate(-3., 3., 5.), door(5.1, 7.5)];
        let gw = [door(-3.6, -1.2), door(1.2, 3.6)];
        let ge = [door(-3.6, -1.2), door(1.2, 3.6)];
        self.shell((x0, z0, x1, z1), 0.4, 0., 6., M::Brick, &gn, &gs, &gw, &ge);
        // Window band (glass) and corrugated upper wall.
        let t = 0.4;
        self.bx(x0, z0, x1, z0 + t, 6., 1.6, M::Glass);
        self.bx(x0, z1 - t, x1, z1, 6., 1.6, M::Glass);
        self.bx(x0, z0 + t, x0 + t, z1 - t, 6., 1.6, M::Glass);
        self.bx(x1 - t, z0 + t, x1, z1 - t, 6., 1.6, M::Glass);
        self.shell((x0, z0, x1, z1), 0.4, 7.6, 1.4, M::Metal, &[], &[], &[], &[]);
        self.roof(x0 - 0.2, z0 - 0.2, x1 + 0.2, z1 + 0.2, 9., M::Metal);
        // Furnace banks with their glowing mouths (drawn as decor on the front).
        for (fx0, fx1) in [(-13.5, -7.), (7., 13.5)] {
            for (fz0, fz1) in [(-8.6, -5.6), (5.6, 8.6)] {
                self.bx(fx0, fz0, fx1, fz1, 0., 3.4, M::RustMetal);
                self.bx(fx0 - 0.15, fz0 - 0.15, fx1 + 0.15, fz1 + 0.15, 0., 0.5, M::Hazard);
            }
        }
        // Ladle cradles: two plinths and the ladle.
        for sx in [-1., 1.] {
            let c = 10. * sx;
            self.bx(c - 1.6, -1.6, c + 1.6, 1.6, 0., 0.4, M::ConcreteDark);
            self.bx(c - 1.1, -1.1, c + 1.1, 1.1, 0.4, 1.7, M::RustMetal);
            self.bx(c - 1.6, -0.25, c + 1.6, 0.25, 0.4, 2.3, M::Metal);
        }
        // Columns.
        for (cx, cz) in [(-5., -4.5), (5., -4.5), (-5., 4.5), (5., 4.5)] {
            self.bx(cx - 0.3, cz - 0.3, cx + 0.3, cz + 0.3, 0., 9., M::ConcreteDark);
        }
        // Catwalks along both long walls with stairs at each end, and a bridge across the middle.
        let top = 3.3;
        for s in [1., -1.] {
            let (za, zb) = if s > 0. { (9., 10.6) } else { (-10.6, -9.) };
            self.plat(-9., za, 9., zb, top, 0.3, M::Metal);
            self.stairs(-13.5, (za + zb) / 2., Dir::PX, 1.6, top, 0., M::Metal);
            self.stairs(13.5, (za + zb) / 2., Dir::NX, 1.6, top, 0., M::Metal);
            let edge = if s > 0. { za } else { zb };
            self.rail(-9., edge, -1.2, edge, top);
            self.rail(1.2, edge, 9., edge, top);
        }
        self.plat(-1., -9., 1., 9., top, 0.3, M::Metal);
        self.rail(-1., -9., -1., 9., top);
        self.rail(1., -9., 1., 9., top);
        self.loot(24, 0., top, 0.);
        self.loot(29, -7.5, top, 9.8);
        self.loot(29, 7.5, top, -9.8);
        self.loot(22, 0., 0., 0.);
        self.loot(33, -10.5, 0., 3.6);
        self.loot(33, -10.5, 0., -3.6);
        // Glow, steam and hum.
        for (x, z) in [(-10., -7.), (10., -7.), (-10., 7.), (10., 7.)] {
            self.light(x, 1.5, z, 10., [1., 0.5, 0.18], 1.2);
        }
        self.light(0., 7.5, 0., 14., [1., 0.85, 0.65], 0.45);
        self.hum(AmbientKind::MachineHum, 0., 2., 0., 20.);
        self.hum(AmbientKind::Steam, -10., 3., 0., 8.);
        self.hum(AmbientKind::MetalCreak, 0., 5., 0., 10.);
        // Dressing.
        for (x, z) in [(-10., -7.), (10., -7.), (-10., 7.), (10., 7.)] {
            self.dec(D::Vent, x, 3.4, z, 0., 1.);
        }
        self.dec(D::PipeRun, -14.3, 5.5, -8., std::f32::consts::FRAC_PI_2, 16.);
        self.dec(D::PipeRun, 14.3, 5.5, -8., std::f32::consts::FRAC_PI_2, 16.);
        self.dec(D::Crane, 0., 7.5, 0., 0., 1.);
        self.dec(D::Cable, -3., 0.04, -2., 0.2, 7.);
        self.dec(D::Rubble, -12., 0.04, 0.5, 0., 1.);
        self.dec(D::Rubble, 12.5, 0.04, -1.3, 0., 0.8);
        self.dec(D::OilDrum, -12.8, 0.04, -3.5, 0., 1.);
        self.dec(D::Puddle, 3., 0.04, 3., 0., 1.);
        self.dec(D::Puddle, -3., 0.04, -6., 0.8, 1.3);
        self.dec(D::Weeds, 14.3, 0.04, 9.8, 0., 1.1);
        self.dec(D::Weeds, -14.2, 0.04, -9.5, 0., 1.);
        self.dec(D::Weeds, 2.5, 0.04, 10.2, 0., 0.9);
    }

    // --- the Control Office: two floors ----------------------------------------------------------------
    fn control_office(&mut self) {
        let (x0, x1, z0, z1) = (-32., -20., -7., 7.);
        let t = 0.35;
        self.patch(M::Concrete, -34.5, -8.5, -17., 8.5, L_CONC);
        self.patch(M::Plaster, x0, z0, x1, z1, L_FLOOR);
        // Ground floor 0..3.0.
        let n1 = [win(-31., -29.6), door(-27.7, -26.3), win(-24.5, -22.)];
        let s1 = [win(-31., -29.6), door(-27.7, -26.3), win(-24.5, -22.)];
        let w1 = [win(-6., -4.4), win(-3.6, -2.4), door(-0.8, 0.8), win(2.4, 3.6), win(4.4, 6.)];
        let e1 = [win(-6., -4.4), win(-3.6, -2.4), door(-0.8, 0.8), win(2.4, 3.6), win(4.4, 6.)];
        self.shell((x0, z0, x1, z1), t, 0., 3., M::Brick, &n1, &s1, &w1, &e1);
        // Floor slab 3.0..3.3 with the stairwell left open (x -32..-28, z -7..-2).
        self.bx(x0, -2., x1, z1, 3., 0.3, M::Concrete);
        self.bx(-28., z0, x1, -2., 3., 0.3, M::Concrete);
        // Upper floor 3.3..6.3.
        let n2 = [win(-31., -29.6), win(-27.7, -26.3), win(-24.5, -22.)];
        let s2 = [win(-31., -29.6), win(-27.7, -26.3), win(-24.5, -22.)];
        let w2 = [win(-6., -4.4), win(-3.6, -2.4), win(-0.8, 0.8), win(2.4, 3.6), win(4.4, 6.)];
        let e2 = [door(-4.6, -3.2), win(-2.4, -0.8), win(0.8, 2.4), win(3.4, 4.6), win(5., 6.2)];
        self.shell((x0, z0, x1, z1), t, 3.3, 3., M::Plaster, &n2, &s2, &w2, &e2);
        self.roof(x0 - 0.2, z0 - 0.2, x1 + 0.2, z1 + 0.2, 6.3, M::Metal);
        // Partitions (both floors): a + shaped corridor, four quadrant rooms.
        for (y0, up) in [(0., false), (3.3, true)] {
            let h = 3.;
            // Corridor N-S walls at x = -28 and -26; doors into the rooms.
            let stair_door: &[G] = if up { &[] } else { &[door(-6.4, -5.2)] };
            self.wall_z(-28., z0 + t, -1., 0.3, y0, h, M::Plaster, stair_door);
            self.wall_z(-28., 1., z1 - t, 0.3, y0, h, M::Plaster, &[door(3., 4.4)]);
            self.wall_z(-26., z0 + t, -1., 0.3, y0, h, M::Plaster, &[door(-4., -2.6)]);
            self.wall_z(-26., 1., z1 - t, 0.3, y0, h, M::Plaster, &[door(3., 4.4)]);
            // East-west corridor walls at z = -1 and 1 (door from the stairwell on the upper floor).
            self.wall_x(x0 + t, -28., -1., 0.3, y0, h, M::Plaster, &[door(-31., -29.6)]);
            self.wall_x(-26., x1 - t, -1., 0.3, y0, h, M::Plaster, &[]);
            self.wall_x(x0 + t, -28., 1., 0.3, y0, h, M::Plaster, &[]);
            self.wall_x(-26., x1 - t, 1., 0.3, y0, h, M::Plaster, &[]);
            // Furniture.
            let y = y0 + if up { 0. } else { 0.03 };
            self.desk(-23., -5.8, -21.0, -4.9, y);
            self.desk(-23., -3.2, -21.0, -2.3, y);
            self.desk(-23.6, 3.2, -21.0, 4.0, y);
            self.desk(-25.2, 5.4, -23., 6.2, y);
            self.locker(-31.6, 3.0, -31.1, 3.6, y);
            self.locker(-31.6, 3.7, -31.1, 4.3, y);
            self.locker(-31.6, 4.4, -31.1, 5.0, y);
            self.locker(-31.6, 5.1, -31.1, 5.7, y);
            self.dec(D::PottedPlant, -21.6, y, 6.2, 0., 1.);
            self.dec(D::PottedPlant, -25.4, y, -1.6, 0., 0.9);
            self.dec(D::PottedPlant, -31.4, y, 0.5, 0., 0.8);
            self.light(-23.5, y0 + 2.6, -3.5, 7., [1., 0.93, 0.78], 0.55);
            self.light(-23.5, y0 + 2.6, 3.8, 7., [1., 0.93, 0.78], 0.55);
        }
        // Stairs inside the north-west stairwell, ascending +z, and the railing round the void.
        let end = self.stairs(-30.6, -6.5, Dir::PZ, 1.5, 3.3, 0., M::Concrete);
        debug_assert!((end + 2.0).abs() < 0.6, "{end}");
        self.rail(-29.85, -6.5, -29.85, -2.05, 3.3);
        self.rail(-29.85, -2.05, -28.15, -2.05, 3.3);
        // Outer stairs up to the balcony that joins the upper floor.
        self.plat(-20., -5.6, -18.4, 1.9, 3.3, 0.3, M::Metal);
        self.stairs(-19.2, 6.4, Dir::NZ, 1.6, 3.3, 0., M::Metal);
        self.rail(-18.4, -5.6, -18.4, 1.9, 3.3);
        self.rail(-19.2, -5.6, -18.4, -5.6, 3.3);
        // Loot: the marksman's rifle on the upper dispatch desk, the DMR on the control desk.
        self.loot(17, -22., 3.3 + 0.76, 3.6);
        self.loot(15, -22., 0.03 + 0.76, -4.4);
        self.lamp(-19., 8.);
        self.lamp(-33., -8.);
        self.dec(D::Sign, -24., 0., 7.25, 0., 1.);
        self.dec(D::Vent, -28., 6.7, 0., 0., 1.2);
        self.dec(D::Vent, -24., 6.7, -3., 0., 1.);
        self.dec(D::Bush, -21., 0., 8., 0., 1.);
        self.dec(D::Bush, -33.8, 0., 7.8, 0., 1.1);
        self.dec(D::Weeds, -20.4, 0., -7.6, 0., 1.);
        self.hum(AmbientKind::MachineHum, -26., 1., 0., 8.);
        self.hum(AmbientKind::WindGap, -17.5, 2., 0., 7.);
    }

    // --- the Warehouse (south-west, written with positive z) ---------------------------------------------
    fn warehouse(&mut self) {
        let (x0, x1, z0, z1) = (-58.4, -46., 12., 28.);
        self.patch(M::Concrete, -58.4, 11., -45., 29., L_CONC);
        let n = [door(-54., -51.6)];
        let s = [door(-52., -49.6)];
        let w: [G; 0] = [];
        let e = [door(15., 17.4), gate(21.5, 26., 3.6)];
        self.shell((x0, z0, x1, z1), 0.4, 0., 5.6, M::Metal, &n, &s, &w, &e);
        self.roof(x0 - 0.2, z0 - 0.2, x1 + 0.2, z1 + 0.2, 5.6, M::RustMetal);
        // Racks: three rows of shelving with a forklift-sized gap in each.
        for (rx0, rx1) in [(-57.6, -56.7), (-54.3, -53.4), (-51., -50.1)] {
            self.bx(rx0, 14., rx1, 18.6, 0., 3., M::Metal);
            self.bx(rx0, 21., rx1, 26.4, 0., 3., M::Metal);
        }
        // Pallets and crates as cover.
        self.crate_(-47.6, 14.3, 1.2);
        self.crate_(-47.6, 15.6, 0.9);
        self.crate_(-48.2, 19.4, 1.1);
        self.crate_(-52.3, 19.8, 1.1);
        self.crate_(-55.8, 19.8, 1.2);
        self.crate_(-47.3, 27.0, 1.0);
        self.crate_(-53., 26.8, 1.0);
        self.dec(D::Pallet, -49., 0., 22., 0.5, 1.);
        self.dec(D::Pallet, -52.3, 0., 23., 0., 1.);
        self.dec(D::Pallet, -55.5, 0., 23.2, 0.3, 1.);
        self.dec(D::Tyres, -46.9, 0., 12.9, 0., 1.);
        self.dec(D::Puddle, -53., 0.04, 20., 0., 1.2);
        self.dec(D::Weeds, -57.8, 0.04, 27.5, 0., 1.2);
        self.dec(D::Weeds, -51., 0.04, 12.6, 0., 1.);
        self.lamp(-44.5, 12.);
        self.light(-52., 4.5, 16., 11., [1., 0.9, 0.7], 0.5);
        self.light(-52., 4.5, 24., 11., [1., 0.9, 0.7], 0.5);
        self.loot(6, -56.2, 0.03, 13.4);
        self.loot(21, -47.6, 0.03, 25.2);
        self.hum(AmbientKind::Dripping, -54., 4., 22., 10.);
        self.hum(AmbientKind::MetalCreak, -52., 5., 16., 10.);
    }

    // --- the Loading Dock (north-west; written in mirrored coordinates) --------------------------------
    fn loading_dock(&mut self) {
        // The shed floor is raised 1.2 m; the platform in front is level with it; stairs at both ends.
        let (x0, x1, z0, z1) = (-58.4, -49., 12., 28.);
        self.patch(M::Concrete, -58.4, 11., -44.6, 29., L_CONC);
        let e = [G { a: 14.6, b: 18.6, lo: 1.2, hi: 4.2 }, G { a: 21., b: 25., lo: 1.2, hi: 4.2 }];
        self.shell((x0, z0, x1, z1), 0.4, 0., 5.2, M::Brick, &[], &[], &[], &e);
        self.bx(x0 + 0.4, z0 + 0.4, x1 - 0.4, z1 - 0.4, 0., 1.2, M::Concrete);
        self.roof(x0 - 0.2, z0 - 0.2, x1 + 0.2, z1 + 0.2, 5.2, M::RustMetal);
        // Platform, hazard edge, canopy on posts, stairs at both ends.
        self.bx(-49., 13., -46.3, 27., 0., 1.2, M::Concrete);
        self.bx(-46.3, 13., -46., 27., 0., 1.26, M::Hazard);
        for z in [13.2, 17.9, 22.6, 26.8] {
            self.bx(-46.4, z - 0.15, -46.1, z + 0.15, 1.2, 2.6, M::Metal);
        }
        self.bx(-49., 13., -45.6, 27., 3.8, 0.2, M::Metal);
        self.stairs(-47.6, 28.8, Dir::NZ, 2.4, 1.2, 0., M::Concrete);
        self.stairs(-47.6, 11.2, Dir::PZ, 2.4, 1.2, 0., M::Concrete);
        // Inside: racks, crates, a foreman's desk.
        self.bx(-57.6, 14., -56.7, 19., 1.2, 2.4, M::Metal);
        self.bx(-57.6, 21., -56.7, 26., 1.2, 2.4, M::Metal);
        self.bx(-54.6, 14., -53.7, 19.4, 1.2, 2.4, M::Metal);
        self.bx(-54.6, 21.4, -53.7, 26., 1.2, 2.4, M::Metal);
        self.crate_(-51., 15.6, 1.2);
        self.crate_(-50.6, 19.8, 1.0);
        self.crate_(-52., 25.2, 1.3);
        self.bx(-57.4, 12.8, -55.4, 13.6, 1.2, 0.76, M::Wood);
        self.dec(D::Pallet, -52., 1.2, 18.4, 0.2, 1.);
        self.dec(D::Pallet, -55.8, 1.2, 20.2, 0., 1.);
        self.dec(D::Puddle, -51.5, 1.2, 22.5, 0., 0.9);
        self.loot(6, -56.6, 1.2, 13.6);
        self.loot(21, -50., 1.2, 26.6);
        self.light(-53.6, 4., 17., 10., [1., 0.9, 0.7], 0.5);
        self.light(-53.6, 4., 23., 10., [1., 0.9, 0.7], 0.5);
        self.light(-47.5, 3.6, 20., 9., [1., 0.85, 0.6], 0.6);
        self.lamp(-44.5, 12.);
        self.dec(D::Sign, -46., 0., 11.4, 1.57, 1.);
        self.hum(AmbientKind::MetalCreak, -53., 4., 20., 10.);
        self.hum(AmbientKind::Dripping, -47., 3., 25., 6.);
    }

    /// The tank yard between the warehouse and the dock shed (both written with their own mirror).
    fn tank_yard(&mut self) {
        self.patch(M::Gravel, -58., -12., -47., 12., L_GRAVEL);
        for (x, z) in [(-53., -5.), (-53., 5.)] {
            self.bx(x - 1.5, z - 1.5, x + 1.5, z + 1.5, 0., 3.4, M::Metal);
            self.dec(D::Tank, x, 0., z, 0., 1.2);
        }
        self.bx(-57., -1., -55., 1., 0., 1.2, M::Concrete);
        self.bx(-50.5, -2., -49.5, 2., 0., 1.1, M::Concrete);
        self.dec(D::PipeRun, -53., 0.5, -3.4, 1.57, 6.8);
        self.dec(D::Sandbags, -48., 0., -9., 0., 2.4);
        self.dec(D::Sandbags, -48., 0., 9., 0., 2.4);
        self.tree(-57., -9.5, false, 1.2);
        self.tree(-56.5, 9., true, 1.2);
        self.tree(-49., 0., false, 0.9);
        self.dec(D::Bush, -55., 0., -11., 0., 1.2);
        self.dec(D::Bush, -50., 0., 10.5, 0., 1.1);
        self.dec(D::Barrel, -51., 0., -1., 0., 1.);
        self.dec(D::Puddle, -49., L_GRAVEL, 4., 0., 1.4);
        self.loot(26, -47.5, 0., 0.);
        self.weeds(D::Weeds, [-58., -12., -46.5, 12.], 16, 0.6, 1.2);
        self.weeds(D::GrassTuft, [-58., -12., -46.5, 12.], 18, 0.6, 1.2);
        self.hum(AmbientKind::Birds, -55., 5., 0., 12.);
    }

    // --- the Rail Yard: two tracks of box cars with crossing gaps; the long lane runs between them -----------
    fn rail_yard_half(&mut self) {
        self.patch(M::Gravel, -46., 0., -32., 30., L_GRAVEL);
        let cars1 = [(2., 11.), (15., 27.)];
        let cars2 = [(4.5, 13.), (16.5, 24.)];
        for (i, (a, b)) in cars1.into_iter().enumerate() {
            self.bx(-43.2, a, -40.2, b, 0., 3.4, if i == 0 { M::RustMetal } else { M::ContainerGreen });
        }
        for (i, (a, b)) in cars2.into_iter().enumerate() {
            self.bx(-37.2, a, -34.2, b, 0., 3.4, if i == 0 { M::ContainerRed } else { M::RustMetal });
        }
        // Buffer stop and a signal post at the yard ends.
        self.bx(-42.4, 28.4, -41., 29., 0., 0.9, M::Concrete);
        self.bx(-36.4, 28.4, -35., 29., 0., 0.9, M::Concrete);
        self.loot(12, -44.6, 0., 20.);
        self.loot(31, -33.2, 0., 12.);
        self.dec(D::Sign, -38.7, 0., 28.8, 0., 0.9);
        self.lamp(-44.8, 14.);
        self.light(-38.7, 5., 2., 12., [1., 0.85, 0.6], 0.5);
        self.dec(D::Barrel, -44.8, 0., 14., 0., 1.);
        self.dec(D::Crate, -38.9, 0., 11.6, 0., 1.);
        self.dec(D::Puddle, -38.6, L_GRAVEL, 20., 0., 1.4);
        self.dec(D::Puddle, -45., L_GRAVEL, 8., 0.2, 1.0);
        self.weeds(D::Weeds, [-46., 1., -32., 29.], 26, 0.6, 1.3);
        self.weeds(D::GrassTuft, [-46., 1., -32., 29.], 26, 0.6, 1.2);
        self.weeds(D::Bush, [-46., 6., -32., 29.], 3, 0.7, 1.2);
        self.hum(AmbientKind::MetalCreak, -41.7, 3., 20., 12.);
        self.hum(AmbientKind::Birds, -46., 6., 9., 12.);
    }

    // --- the Container Yard: stacks of 40 ft containers with 3 m alleys; tops reached by stairs -----------------
    fn container_half(&mut self) {
        self.patch(M::Concrete, 22.5, -22., 44.5, 22., L_CONC);
        let cols = [23., 31., 39.];
        // South segment (z 9.1..21.3) of each column.
        let south = [
            ([M::ContainerRed, M::ContainerBlue], 1),
            ([M::ContainerYellow, M::ContainerRed], 1),
            ([M::ContainerGreen, M::ContainerBlue], 2),
        ];
        for (x, (mats, stack)) in cols.into_iter().zip(south) {
            self.cont(x, 9.1, 12.2, false, stack, mats[0]);
            self.cont(x + 2.44, 9.1, 12.2, false, stack, mats[1]);
        }
        // Stairs to the west and centre tops, gangplank across the alley to the middle segment.
        self.stairs(28.6, 21.3, Dir::NZ, 1.4, 2.6, 0., M::Concrete);
        self.stairs(36.6, 21.3, Dir::NZ, 1.4, 2.6, 0., M::Concrete);
        self.bx(24., 6.3, 26., 8.9, 2.45, 0.15, M::Metal);
        self.bx(27.88, 13., 31., 14.2, 2.45, 0.15, M::Metal);
        self.loot(13, 25.5, 2.6, 15.);
        self.loot(14, 33.4, 2.6, 15.);
        // Dressing in the alleys.
        self.barrel(29.6, 12., false);
        self.barrel(30.3, 12.4, true);
        self.crate_(37.6, 19.8, 1.1);
        self.dec(D::Pallet, 29.4, 0., 17.6, 0.3, 1.);
        self.dec(D::Tyres, 45., 0., 12., 0., 1.);
        self.dec(D::Puddle, 33.4, L_CONC, 7.6, 0., 1.3);
        self.dec(D::Puddle, 29.4, L_CONC, 20.2, 0.6, 1.0);
        self.dec(D::Rubble, 37.4, 0., 15., 0., 0.9);
        self.lamp(29.4, 9.4);
        self.lamp(37.4, 9.4);
        self.dec(D::FloodLight, 33.4, 5.2, 13., 0., 1.);
        self.weeds(D::Weeds, [23., 9., 44., 22.], 20, 0.6, 1.2);
        self.weeds(D::GrassTuft, [23., 9., 44., 22.], 16, 0.6, 1.1);
        self.hum(AmbientKind::MetalCreak, 33., 3., 15., 12.);
    }
    fn container_centre(&mut self) {
        self.cont(23., -6.1, 12.2, false, 1, M::ContainerGreen);
        self.cont(25.44, -6.1, 12.2, false, 1, M::ContainerYellow);
        self.cont(31., -6.1, 12.2, false, 2, M::ContainerBlue);
        self.cont(33.44, -6.1, 12.2, false, 2, M::ContainerGreen);
        self.cont(39., -6.1, 12.2, false, 1, M::ContainerRed);
        self.cont(41.44, -6.1, 12.2, false, 1, M::ContainerYellow);
        // The top of the west middle stack: the big rifle, reached over the gangplanks.
        self.loot(18, 25.4, 2.6, 0.);
        self.dec(D::Weeds, 29.4, 0.03, 0.6, 0., 1.2);
        self.dec(D::Weeds, 37.4, 0.03, -0.6, 0., 1.0);
        self.dec(D::Puddle, 29.4, L_CONC, 0., 0., 1.4);
    }

    // --- the Pipe Alley and the tower plaza -------------------------------------------------------------------
    fn pipe_half(&mut self) {
        // East edge: a tall pipe bundle against the perimeter wall and a rack opposite it, 2.5 m apart.
        self.patch(M::Concrete, 44.5, -22., 60., 22., L_CONC);
        self.bx(58., 0., 60., 22., 0., 5., M::RustMetal);
        let segs = [(1.2, 9.6), (12., 22.)];
        for (a, b) in segs {
            self.bx(54., a, 55.5, b, 0., 3.4, M::RustMetal);
            self.bx(54.2, a, 55.3, b, 3.4, 0.4, M::Metal);
        }
        // Pump house on the plaza.
        self.shell((45.5, 9., 49.5, 13.), 0.3, 0., 2.8, M::Brick, &[], &[], &[door(10., 11.4)], &[win(10., 11.6)]);
        self.roof(45.4, 8.9, 49.6, 13.1, 2.8, M::Metal);
        self.loot(7, 56.75, 0., 15.);
        for z in [4., 8., 14., 19.] {
            self.dec(D::PipeRun, 54., 4.1 + (z as i32 % 3) as f32 * 0.25, z, 0., 6.);
        }
        self.dec(D::PipeRun, 55.5, 3.7, 1.2, std::f32::consts::FRAC_PI_2, 8.4);
        self.dec(D::PipeRun, 55.5, 4.0, 12., std::f32::consts::FRAC_PI_2, 10.);
        self.dec(D::Tank, 52., 0., 18., 0., 1.);
        self.dec(D::Vent, 59., 5., 10., 0., 1.);
        self.dec(D::Barrel, 56.3, 0., 21., 0., 1.);
        self.dec(D::Puddle, 56.7, L_CONC, 6., 0., 1.0);
        self.dec(D::Puddle, 56.7, L_CONC, 18., 0., 0.8);
        self.dec(D::Cable, 55.6, 0.04, 2., 1.57, 18.);
        self.lamp(53., 14.);
        self.light(56.7, 3.2, 8., 8., [1., 0.85, 0.6], 0.5);
        self.light(56.7, 3.2, 17., 8., [1., 0.85, 0.6], 0.5);
        self.weeds(D::Weeds, [44.5, 1., 59., 22.], 14, 0.6, 1.2);
        self.weeds(D::Bush, [44.5, 14., 53., 22.], 3, 0.8, 1.3);
        self.hum(AmbientKind::Dripping, 56.7, 3., 12., 12.);
        self.hum(AmbientKind::Steam, 58.5, 2., 8., 8.);
        self.hum(AmbientKind::WindGap, 56.7, 2., 1.5, 8.);
    }
    fn plaza(&mut self) {
        // Water tower: four legs, cross braces are decor, the tank is a block for bullets.
        for (x, z) in [(47.5, -2.), (50.5, -2.), (47.5, 2.), (50.5, 2.)] {
            self.bx(x - 0.25, z - 0.25, x + 0.25, z + 0.25, 0., 9., M::RustMetal);
        }
        self.bx(46.4, -2.8, 51.6, 2.8, 9., 4.4, M::Metal);
        self.dec(D::WaterTower, 49., 0., 0., 0., 1.);
        self.loot(25, 49., 0., 0.);
        self.loot(23, 56.75, 0., 0.);
        self.dec(D::Weeds, 49., 0.04, 1., 0., 1.4);
        self.dec(D::Bush, 46., 0., -3.4, 0., 1.);
        self.dec(D::Puddle, 49., L_CONC, -1., 0., 1.4);
        self.lamp(45., 0.);
        self.hum(AmbientKind::MetalCreak, 49., 8., 0., 12.);
    }

    // --- corner yards ------------------------------------------------------------------------------------------
    fn corner_yard(&mut self) {
        let north = self.flip < 0.;
        self.patch(M::Grass, -58., 38., -44., 44.5, L_GRASS);
        self.patch(M::Grass, -30., 36., -22., 44.5, L_GRASS);
        self.patch(M::Gravel, -44., 29.6, -22., 36., L_GRAVEL);
        self.cont(-39., 34., 12.2, true, 1, M::ContainerRed);
        self.cont(-34., 39.5, 6.1, true, 1, M::ContainerBlue);
        self.barrel(-24.5, 34., true);
        self.barrel(-23.9, 34.6, false);
        self.tree(-25.5, 42., false, 1.1);
        self.dec(D::Bush, -28., 0., 38., 0., 1.);
        self.dec(D::Rubble, -41., 0., 38., 0.4, 1.);
        self.dec(D::Tyres, -27., 0., 33., 0., 1.);
        self.loot(30, -27., 0., 40.4);
        self.loot(10, -46., 0., 42.2);
        self.hum(AmbientKind::Birds, -26., 5., 42., 14.);
        if north {
            // Scrapyard: a jib crane, heaps, tyres.
            self.dec(D::Crane, -31., 0., 33., 0.5, 1.);
            self.bx(-31.6, 32.4, -30.4, 33.6, 0., 2.4, M::RustMetal);
            self.bx(-44.4, 39., -41., 43., 0., 1.4, M::RustMetal);
            self.bx(-40.6, 40., -38.4, 42.4, 0., 1.0, M::Concrete);
            self.dec(D::Rubble, -42.7, 1.4, 41., 0., 1.6);
            self.dec(D::Tyres, -36., 0., 43.4, 0.5, 1.2);
            self.dec(D::Tyres, -36.4, 0., 38.6, 0.1, 1.0);
            self.weeds(D::Weeds, [-44., 30., -22., 44.], 12, 0.6, 1.2);
        } else {
            // Staging yard: a double stack, a second container, a stand of trees.
            self.cont(-56., 36., 12.2, true, 2, M::ContainerGreen);
            self.cont(-57., 40.5, 6.1, true, 1, M::ContainerYellow);
            self.tree(-56.5, 31., false, 1.2);
            self.tree(-48., 44., true, 1.1);
            self.tree(-50., 43.5, false, 1.);
            self.dec(D::Bush, -49., 0., 38.6, 0., 1.);
            self.dec(D::Sandbags, -44., 0., 30.2, 0., 2.4);
            self.weeds(D::Weeds, [-58., 29., -22., 44.], 18, 0.6, 1.3);
            self.weeds(D::Bush, [-58., 30., -44., 44.], 4, 0.8, 1.4);
        }
        self.weeds(D::GrassTuft, [-58., 29., -22., 44.], 24, 0.6, 1.2);
        self.lamp(-40., 31.);
    }
    /// South-east: the Boiler House; north-east: a fenced substation of the same size.
    fn se_corner(&mut self) {
        let boiler = self.flip > 0.;
        self.patch(M::Concrete, 22., 29., 57., 44.5, L_CONC);
        if boiler {
            let t = 0.4;
            let n = [door(28., 30.4), door(37., 39.4), win(32., 35.), win(34.6, 36.4)];
            self.shell(
                (26., 30., 41., 43.),
                t,
                0.,
                5.5,
                M::Brick,
                &n,
                &[],
                &[door(34., 36.4), win(38., 40.)],
                &[door(38., 40.4)],
            );
            self.roof(25.8, 29.8, 41.2, 43.2, 5.5, M::Metal);
            for c in [30., 34.2, 38.4] {
                self.bx(c - 1.3, 35., c + 1.3, 38.6, 0., 3.2, M::Metal);
                self.bx(c - 1.3, 35., c + 1.3, 35.3, 3.2, 0.3, M::Hazard);
                self.dec(D::Tank, c, 0., 36.8, 0., 1.);
            }
            self.bx(27., 40., 28.2, 41.2, 0., 1.2, M::Metal);
            self.bx(31.6, 30.4, 33., 32., 0., 1.0, M::Concrete);
            self.crate_(39.8, 32.4, 1.0);
            self.bx(41.1, 41., 43.5, 43.4, 0., 34., M::Brick);
            self.dec(D::Chimney, 42.3, 0., 42.2, 0., 1.);
            self.dec(D::PipeRun, 27., 4.6, 34.5, 0., 14.);
            self.dec(D::PipeRun, 27., 4.2, 39.4, 0., 14.);
            self.dec(D::PipeRun, 28.5, 3.8, 31.2, 1.57, 12.);
            self.dec(D::Vent, 30., 5.5, 37., 0., 1.);
            self.dec(D::Vent, 36., 5.5, 37., 0., 1.);
            self.dec(D::Puddle, 34., 0.04, 33.3, 0., 1.);
            self.light(30., 3., 37., 9., [1., 0.6, 0.3], 0.8);
            self.light(38., 3., 37., 9., [1., 0.6, 0.3], 0.8);
            self.hum(AmbientKind::Steam, 34., 3., 37., 12.);
            self.hum(AmbientKind::MachineHum, 34., 2., 37., 14.);
        } else {
            // Fenced yard with a gate on the west side and one towards the centre.
            let (x0, x1, z0, z1) = (26., 41., 30., 43.);
            self.fence(x0, z0, 28., z0);
            self.fence(30.4, z0, 37., z0);
            self.fence(39.4, z0, x1, z0);
            self.fence(x0, z1, x1, z1);
            self.fence(x1, z0, x1, z1);
            self.fence(x0, z0, x0, 34.);
            self.fence(x0, 36.4, x0, z1);
            for (a, b) in [(28.5, 30.9), (33., 35.4)] {
                for (c, d) in [(33.6, 35.8), (39., 41.2)] {
                    self.bx(a, c, b, d, 0., 2.6, M::ContainerGreen);
                    self.bx(a + 0.2, c + 0.2, b - 0.2, d - 0.2, 2.6, 0.5, M::Metal);
                }
            }
            self.shell(
                (36.5, 35.5, 40.5, 40.8),
                0.3,
                0.,
                2.8,
                M::Brick,
                &[],
                &[],
                &[door(37., 38.4)],
                &[win(37., 38.6)],
            );
            self.roof(36.4, 35.4, 40.6, 40.9, 2.8, M::Metal);
            self.dec(D::Cable, 30., 0.04, 36.5, 0., 12.);
            self.dec(D::Sign, 27.3, 0., 30.3, 0., 1.);
            self.dec(D::Vent, 38.5, 2.8, 38., 0., 1.);
            self.light(32., 3., 37.5, 9., [0.7, 0.85, 1.], 0.6);
            self.hum(AmbientKind::MachineHum, 32., 2., 37.5, 14.);
        }
        self.loot(8, 27., 0.03, 41.8);
        self.loot(32, 39.6, 0.03, 40.4);
        // Cooling tower footprint is added separately.
        self.lamp(24., 38.);
        self.weeds(D::Weeds, [22., 29., 26., 44.], 6, 0.6, 1.1);
    }
    /// Cooling tower: a plus-shaped solid approximating a 11 m circle, 26 m high.
    fn cooling_tower(&mut self, cx: f32, cz: f32) {
        self.b.solid(cx - 5.5, cz - 3.2, cx + 5.5, cz + 3.2, 0., 26., M::Concrete);
        self.b.solid(cx - 3.2, cz - 5.5, cx + 3.2, cz + 5.5, 0., 26., M::Concrete);
        self.b.decor(D::CoolingTower, V(cx, 0., cz), 0., 1.);
        self.b.level.ambient.push(AmbientSpot { pos: V(cx, 2., cz), kind: AmbientKind::Steam, radius: 14. });
    }

    // --- the centre street and the rail yard's middle ---------------------------------------------------------------
    fn streets(&mut self) {
        // Yard Street between the hall and the container yard: cover and a parked truck.
        self.patch(M::Asphalt, 15., -30., 22.4, 30., L_ASPH + 0.002);
        self.patch(M::Asphalt, -18., -30., -15., 30., L_ASPH + 0.002);
        self.bx(17.4, -16., 19.8, -10.5, 0., 3., M::ContainerBlue);
        self.bx(17.4, -10.5, 19.8, -8.9, 0., 2.2, M::Metal);
        self.bx(16., 14., 17., 17.5, 0., 1.1, M::Concrete);
        self.bx(20., -19., 21., -15.5, 0., 1.1, M::Concrete);
        self.bx(-19.6, 14., -17.6, 15., 0., 1.1, M::Concrete);
        self.bx(-19.6, -15., -17.6, -14., 0., 1.1, M::Concrete);
        self.dec(D::Cable, 18., 0.04, 0., 1.57, 10.);
        self.dec(D::Barrel, 21.4, 0., 10., 0., 1.);
        self.dec(D::Barrel, 21.4, 0., -6., 0., 1.);
        self.dec(D::Pallet, 16.4, 0., 6., 0.4, 1.);
        self.dec(D::Puddle, 19., L_ASPH, 3., 0., 1.4);
        self.dec(D::Puddle, -16.5, L_ASPH, -9., 0.3, 1.0);
        self.dec(D::Puddle, 18.5, L_ASPH, -22., 0., 1.6);
        self.lamp(16., 0.);
        self.lamp(-16.2, -12.);
        self.lamp(-16.2, 12.);
        self.lamp(21., 22.);
        self.lamp(21., -22.);
        self.dec(D::Crane, 19.5, 0., 4., 1.2, 1.);
        self.bx(19., 3.4, 20., 4.6, 0., 2.4, M::RustMetal);
        self.loot(27, -16.4, 0., 0.);
        self.weeds(D::Weeds, [15., -30., 22.4, 30.], 22, 0.6, 1.2);
        self.weeds(D::Weeds, [-19., -30., -14.6, 30.], 14, 0.6, 1.1);
        self.weeds(D::GrassTuft, [15., -30., 22.4, 30.], 14, 0.6, 1.1);
        self.hum(AmbientKind::WindGap, -16.5, 2., 0., 8.);
        self.hum(AmbientKind::Birds, 19., 6., 0., 12.);
        // The hand cannon waits in the middle of the rail yard lane.
        self.loot(3, -38.7, 0., 0.);
        self.dec(D::RailTrack, -41.7, L_GRAVEL, -29., std::f32::consts::FRAC_PI_2, 58.);
        self.dec(D::RailTrack, -35.7, L_GRAVEL, -29., std::f32::consts::FRAC_PI_2, 58.);
    }

    fn finish(mut self) -> Level {
        let requests = std::mem::take(&mut self.scatter);
        for (kind, rect, n, lo, hi) in requests {
            for _ in 0..n {
                let x = rect[0] + self.rnd() * (rect[2] - rect[0]);
                let z = rect[1] + self.rnd() * (rect[3] - rect[1]);
                let yaw = self.rnd() * std::f32::consts::TAU;
                let s = lo + self.rnd() * (hi - lo);
                if self.b.level.blocks.iter().any(|b| {
                    let inside_xz =
                        x > b.min.0 - 0.35 && x < b.max.0 + 0.35 && z > b.min.2 - 0.35 && z < b.max.2 + 0.35;
                    let body = b.max.1 > 0.1 && b.min.1 < 1.8;
                    let cover = b.min.1 >= 1.9;
                    inside_xz && (body || cover)
                }) {
                    continue;
                }
                self.b.decor(kind, V(x, 0.02, z), yaw, s);
            }
        }
        // Things that stand on the ground rest on whatever thin patch is under them.
        let blocks = self.b.level.blocks.clone();
        let ground = |x: f32, z: f32, y: f32| {
            blocks
                .iter()
                .filter(|b| x > b.min.0 && x < b.max.0 && z > b.min.2 && z < b.max.2 && b.max.1 <= y + 0.05)
                .map(|b| b.max.1)
                .fold(y, f32::max)
        };
        for l in &mut self.b.level.loot {
            l.pos.1 = ground(l.pos.0, l.pos.2, l.pos.1);
        }
        for team in &mut self.b.level.spawns {
            for s in team {
                s.pos.1 = ground(s.pos.0, s.pos.2, s.pos.1);
            }
        }
        self.b.finish()
    }
}

/// Strong or scarce weapons respawn slowly: hand cannon, sniper rifles, light machine guns, launchers.
pub fn is_rare(w: WeaponId) -> bool {
    matches!(w, 3 | 16..=18 | 22..=25)
}

/// Builds Slagworks.
pub fn build() -> Level {
    let mut m = Map::new();
    m.ground();
    for f in [1., -1.] {
        m.flip = f;
        m.base();
        m.forecourt();
        m.rail_yard_half();
        m.container_half();
        m.pipe_half();
        m.corner_yard();
        m.se_corner();
    }
    m.flip = 1.;
    m.smelter_hall();
    m.control_office();
    m.warehouse();
    m.flip = -1.;
    m.loading_dock();
    m.flip = 1.;
    m.container_centre();
    m.tank_yard();
    m.plaza();
    m.streets();
    m.cooling_tower(50.5, 36.5);
    m.cooling_tower(-50.5, -36.5);
    m.finish()
}

#[cfg(test)]
mod level_tests {
    use super::*;
    use crate::level::Block;
    use std::collections::{HashMap, HashSet, VecDeque};
    use vesper3d::viewer::controller::{Collider, CROUCH_HEIGHT, RADIUS, STANDING_HEIGHT};

    const CELL: f32 = 0.25;

    /// A walkability model that follows the engine `Controller`: a body of 0.23 radius and 1.8 height, steps up
    /// anything whose top is within 0.221 above the feet, falls when unsupported.
    struct Nav {
        cols: Vec<Collider>,
        grid: HashMap<(i32, i32), Vec<usize>>,
    }
    impl Nav {
        fn new(level: &Level, keep: impl Fn(usize, &Block) -> bool) -> Self {
            let mut cols = Vec::new();
            let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
            for (i, b) in level.blocks.iter().enumerate() {
                if !keep(i, b) {
                    continue;
                }
                let id = cols.len();
                cols.push(Collider { min: b.min, max: b.max });
                for gx in (b.min.0 / 4.).floor() as i32..=(b.max.0 / 4.).floor() as i32 {
                    for gz in (b.min.2 / 4.).floor() as i32..=(b.max.2 / 4.).floor() as i32 {
                        grid.entry((gx, gz)).or_default().push(id);
                    }
                }
            }
            Self { cols, grid }
        }
        fn near(&self, x: f32, z: f32) -> impl Iterator<Item = &Collider> + '_ {
            let (gx, gz) = ((x / 4.).floor() as i32, (z / 4.).floor() as i32);
            let mut ids: Vec<usize> = Vec::new();
            for dx in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = self.grid.get(&(gx + dx, gz + dz)) {
                        ids.extend(v);
                    }
                }
            }
            ids.sort_unstable();
            ids.dedup();
            ids.into_iter().map(move |i| &self.cols[i])
        }
        fn overlapping(&self, x: f32, z: f32, feet: f32, height: f32) -> Vec<&Collider> {
            let p = V(x, 0., z);
            self.near(x, z).filter(|c| c.overlaps_body(p, feet, height, RADIUS)).collect()
        }
        /// Height of the ground under a body at (x, z) when its feet are at `feet`.
        fn support(&self, x: f32, z: f32, feet: f32) -> f32 {
            let p = V(x, 0., z);
            self.near(x, z)
                .filter(|c| c.max.1 <= feet + 0.001 && c.overlaps_xz(p, RADIUS))
                .map(|c| c.max.1)
                .fold(0., f32::max)
        }
        /// Where a standing body at feet height `feet` ends up when it moves to (x, z), if it can.
        fn step_to(&self, x: f32, z: f32, feet: f32) -> Option<f32> {
            let hit = self.overlapping(x, z, feet, STANDING_HEIGHT);
            let mut f = feet;
            if !hit.is_empty() {
                if hit.iter().any(|c| c.max.1 - feet > 0.221) {
                    return None;
                }
                f = hit.iter().map(|c| c.max.1).fold(feet, f32::max);
                if !self.overlapping(x, z, f, STANDING_HEIGHT).is_empty() {
                    return None;
                }
            }
            let ground = self.support(x, z, f);
            if ground < f - 0.001 {
                f = ground;
                if !self.overlapping(x, z, f, STANDING_HEIGHT).is_empty() {
                    return None;
                }
            }
            Some(f)
        }
        /// Flood fill from `starts`; `stop` ends it early. Returns the visited (cell, feet) states.
        fn flood(
            &self,
            starts: &[V],
            limit: (f32, f32),
            stop: impl Fn(i32, i32, f32) -> bool,
        ) -> HashSet<(i32, i32, i32)> {
            let key = |x: f32, z: f32, f: f32| {
                ((x / CELL).round() as i32, (z / CELL).round() as i32, (f * 1000.).round() as i32)
            };
            let mut seen = HashSet::new();
            let mut queue = VecDeque::new();
            for s in starts {
                let k = key(s.0, s.2, s.1);
                if seen.insert(k) {
                    queue.push_back(k);
                }
            }
            while let Some((ix, iz, fm)) = queue.pop_front() {
                let (x, z, f) = (ix as f32 * CELL, iz as f32 * CELL, fm as f32 / 1000.);
                if stop(ix, iz, f) {
                    break;
                }
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                    let (nx, nz) = (x + dx as f32 * CELL, z + dz as f32 * CELL);
                    if nx.abs() > limit.0 || nz.abs() > limit.1 {
                        continue;
                    }
                    // A diagonal must also be possible along both axes (no squeezing through corners).
                    if dx != 0 && dz != 0 && (self.step_to(nx, z, f).is_none() || self.step_to(x, nz, f).is_none()) {
                        continue;
                    }
                    if let Some(nf) = self.step_to(nx, nz, f) {
                        let k = key(nx, nz, nf);
                        if seen.insert(k) {
                            queue.push_back(k);
                        }
                    }
                }
            }
            seen
        }
    }

    fn level() -> Level {
        build()
    }
    fn nav(level: &Level) -> Nav {
        Nav::new(level, |_, _| true)
    }
    fn starts(level: &Level, team: usize) -> Vec<V> {
        level.spawns[team].iter().map(|s| s.pos).collect()
    }
    fn reached(seen: &HashSet<(i32, i32, i32)>, p: V, dy: f32) -> bool {
        seen.iter().any(|&(ix, iz, fm)| {
            let (x, z, f) = (ix as f32 * CELL, iz as f32 * CELL, fm as f32 / 1000.);
            (x - p.0).hypot(z - p.2) <= 1.0 && (f - p.1).abs() <= dy
        })
    }
    fn value(w: WeaponId) -> f32 {
        match w {
            1 | 2 | 4 | 26..=29 | 30..=33 => 1.,
            5..=8 | 19..=21 => 2.,
            9..=13 => 3.,
            14 | 15 => 4.,
            3 => 5.,
            16..=18 | 22..=25 => 7.,
            _ => 0.,
        }
    }

    #[test]
    fn counts_are_sane() {
        let l = level();
        assert!(l.blocks.len() < 900, "{} blocks", l.blocks.len());
        assert!((40..=48).contains(&l.loot.len()), "{} loot", l.loot.len());
        assert!((150..=900).contains(&l.decor.len()), "{} decor", l.decor.len());
        assert!(l.lights.len() >= 25 && l.ambient.len() >= 25);
        assert!(l.blocks.iter().all(|b| b.min.0 < b.max.0 && b.min.1 < b.max.1 && b.min.2 < b.max.2));
        for s in &l.loot {
            assert!((1..=33).contains(&s.weapon));
            assert_eq!(s.respawn_s, if is_rare(s.weapon) { 90. } else { 45. });
        }
    }

    #[test]
    fn spawns_are_clear_and_face_the_map() {
        let l = level();
        let cols = l.colliders();
        for team in 0..2 {
            let sp = &l.spawns[team];
            assert!(sp.len() >= 8, "team {team} has {}", sp.len());
            for (i, s) in sp.iter().enumerate() {
                for (b, c) in l.blocks.iter().zip(&cols) {
                    assert!(
                        !c.overlaps_body(s.pos, s.pos.1, STANDING_HEIGHT, RADIUS),
                        "spawn {team}/{i} at {:?} overlaps {:?}",
                        s.pos,
                        b
                    );
                }
                assert!(!cols.iter().any(|c| c.overlaps_body(s.pos, s.pos.1, CROUCH_HEIGHT, RADIUS)));
                let base_ok =
                    if team == 0 { s.pos.2 > 32.5 && s.pos.2 < 45. } else { s.pos.2 < -32.5 && s.pos.2 > -45. };
                assert!(base_ok && s.pos.0.abs() < 21., "{:?}", s.pos);
                // Facing: forward is (sin yaw, -cos yaw); it must point within 40 degrees of the centre.
                let fwd = (s.yaw.sin(), -s.yaw.cos());
                let to = (-s.pos.0, -s.pos.2);
                let len = to.0.hypot(to.1);
                let cos = (fwd.0 * to.0 + fwd.1 * to.1) / len;
                assert!(cos > 0.76, "spawn {team}/{i} faces away: cos {cos}");
                // 2-3 m from its nearest neighbour.
                let nearest = sp
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, o)| (o.pos - s.pos).length())
                    .fold(f32::MAX, f32::min);
                assert!((2. ..=3.2).contains(&nearest), "spawn {team}/{i} neighbour {nearest}");
            }
        }
        assert_eq!(l.spawns[0].len(), l.spawns[1].len());
    }

    #[test]
    fn loot_is_not_buried_and_is_spread_sensibly() {
        let l = level();
        for s in &l.loot {
            for b in &l.blocks {
                let inside = s.pos.0 > b.min.0
                    && s.pos.0 < b.max.0
                    && s.pos.2 > b.min.2
                    && s.pos.2 < b.max.2
                    && s.pos.1 > b.min.1 + 0.001
                    && s.pos.1 < b.max.1 - 0.001;
                assert!(!inside, "loot {:?} is inside {:?}", s, b);
            }
        }
        // Both bases have weapons within 15 m of a spawn.
        for team in 0..2 {
            let near =
                l.loot.iter().filter(|s| l.spawns[team].iter().any(|sp| (sp.pos - s.pos).length() < 15.)).count();
            assert!(near >= 3, "team {team} has {near} weapons near its spawns");
        }
        // Scarce weapons sit towards the middle.
        let rare: Vec<_> = l.loot.iter().filter(|s| is_rare(s.weapon)).collect();
        assert!(rare.len() >= 6);
        assert!(rare.iter().all(|s| s.pos.2.abs() < 22.), "a rare weapon is out by a base");
        assert!(l.loot.iter().filter(|s| s.weapon == 18 || s.weapon == 17 || s.weapon == 16).count() >= 2);
    }

    #[test]
    fn the_bases_are_mirror_balanced() {
        let l = level();
        let sum = |f: &dyn Fn(f32) -> bool| l.loot.iter().filter(|s| f(s.pos.2)).map(|s| value(s.weapon)).sum::<f32>();
        let (south, north) = (sum(&|z| z > 6.), sum(&|z| z < -6.));
        assert!((south - north).abs() <= 0.06 * (south + north), "south {south} north {north}");
        let count = |f: &dyn Fn(f32) -> bool| l.loot.iter().filter(|s| f(s.pos.2)).count();
        assert_eq!(count(&|z| z > 6.), count(&|z| z < -6.));
        let mid: Vec<f32> = l.loot.iter().filter(|s| s.pos.2.abs() <= 6.).map(|s| value(s.weapon)).collect();
        let out: Vec<f32> = l.loot.iter().filter(|s| s.pos.2.abs() > 6.).map(|s| value(s.weapon)).collect();
        let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        assert!(avg(&mid) > avg(&out) + 1.5, "mid {} out {}", avg(&mid), avg(&out));
    }

    #[test]
    fn every_spawn_and_weapon_is_reachable_and_the_map_is_closed() {
        let l = level();
        let nav = nav(&l);
        let all: Vec<V> = starts(&l, 0).into_iter().chain(starts(&l, 1)).collect();
        for s in &all {
            assert!(nav.overlapping(s.0, s.2, s.1, STANDING_HEIGHT).is_empty());
        }
        let seen = nav.flood(&all[..1], (l.half_x + 3., l.half_z + 3.), |_, _, _| false);
        for s in &all {
            assert!(reached(&seen, *s, 0.3), "spawn at {s:?} is cut off");
        }
        for s in &l.loot {
            assert!(reached(&seen, s.pos, 0.95), "loot {:?} at {:?} is not reachable", s.weapon, s.pos);
        }
        // Closed: nothing reachable outside the playable rectangle.
        for &(ix, iz, _) in &seen {
            let (x, z) = (ix as f32 * CELL, iz as f32 * CELL);
            assert!(x.abs() <= l.half_x - RADIUS + 0.01 && z.abs() <= l.half_z - RADIUS + 0.01, "leak at {x},{z}");
        }
        eprintln!("reachable states: {}", seen.len());
    }

    fn connected_without(l: &Level, drop: impl Fn(&Block) -> bool) -> bool {
        let nav = Nav::new(l, |_, b| !drop(b));
        let target = l.spawns[1][0].pos;
        let (tx, tz) = ((target.0 / CELL).round() as i32, (target.2 / CELL).round() as i32);
        let seen = nav.flood(&starts(l, 0)[..1], (l.half_x + 3., l.half_z + 3.), |ix, iz, _| ix == tx && iz == tz);
        seen.iter().any(|&(ix, iz, _)| ix == tx && iz == tz)
    }

    #[test]
    fn the_bases_are_joined_by_three_independent_routes() {
        let l = level();
        assert!(connected_without(&l, |_| false));
        // Take out a whole lane at a time: the other two still connect the bases.
        let in_hall = |b: &Block| b.max.1 > 0.1 && b.min.0 > -16. && b.max.0 < 16. && b.min.2 > -12. && b.max.2 < 12.;
        assert!(connected_without(&l, in_hall), "centre removed");
        let block_lane = |lo: f32, hi: f32| {
            let mut m = l.clone();
            m.blocks.push(Block { min: V(lo, 0., -8.), max: V(hi, 30., 8.), material: Material::Concrete });
            m
        };
        // Sealing one lane's crossing at z=0 leaves the bases joined through the other two.
        assert!(connected_without(&block_lane(-60., -21.5), |_| false), "west sealed");
        assert!(connected_without(&block_lane(21.5, 60.), |_| false), "east sealed");
        assert!(connected_without(&block_lane(-15., 15.), |_| false), "hall sealed");
        // Two lanes sealed at once still leave the third.
        let two = |a: (f32, f32), b: (f32, f32)| {
            let mut m = block_lane(a.0, a.1);
            m.blocks.push(Block { min: V(b.0, 0., -8.), max: V(b.1, 30., 8.), material: Material::Concrete });
            m
        };
        assert!(connected_without(&two((-60., -21.5), (21.5, 60.)), |_| false), "only the middle left");
        assert!(connected_without(&two((-60., -15.), (15., 60.)), |_| false), "only the hall left");
        assert!(connected_without(&block_lane(-21.5, 60.), |_| false), "only the west lane left");
        assert!(connected_without(&block_lane(-60., 21.5), |_| false), "only the east lane left");
        assert!(!connected_without(&block_lane(-60., 60.), |_| false), "sanity: a full wall cuts the map");
    }

    #[test]
    fn a_fifty_metre_sightline_exists_between_reachable_points() {
        let l = level();
        let nav = nav(&l);
        let seen = nav.flood(&starts(&l, 0)[..1], (l.half_x + 3., l.half_z + 3.), |_, _, _| false);
        // For each x column, the two most distant reachable ground points along z.
        let mut cols: HashMap<i32, (i32, i32)> = HashMap::new();
        for &(ix, iz, fm) in &seen {
            if fm < 600 {
                let e = cols.entry(ix).or_insert((iz, iz));
                e.0 = e.0.min(iz);
                e.1 = e.1.max(iz);
            }
        }
        let mut best = 0f32;
        for (ix, (a, b)) in cols {
            let (x, za, zb) = (ix as f32 * CELL, a as f32 * CELL, b as f32 * CELL);
            let len = zb - za;
            if len >= 50. && l.line_of_sight(V(x, 1.68, za), V(x, 1.68, zb)) {
                best = best.max(len);
            }
        }
        assert!(best >= 50., "longest clear z lane is {best} m");
        eprintln!("longest sightline {best} m");
    }

    #[test]
    fn raycasts_behave_inside_the_map() {
        let l = level();
        // From the hall's centre a ray north hits the hall wall or something beyond only after the gate.
        let hit = l.raycast(V(0., 1.68, 0.), V(1., 0., 0.), 100.).unwrap();
        assert!(hit.0 > 5. && hit.0 < 16., "{hit:?}");
        // The perimeter stops every ray.
        for dir in [V(0., 0., 1.), V(0., 0., -1.), V(1., 0., 0.), V(-1., 0., 0.)] {
            assert!(l.raycast(V(-39., 1.68, -2.), dir, 200.).is_some());
        }
        // A window sill (1.1 m) hides a crouched eye (0.98 m) but not a standing one (1.68 m).
        let eye = |y: f32| l.line_of_sight(V(-23., y, -3.), V(-16., y, -3.));
        assert!(!eye(0.98), "the sill should cover a crouched player");
        assert!(eye(1.68), "a standing player should see out of the window");
    }

    #[test]
    fn walls_are_thick_enough_and_the_perimeter_is_tall() {
        let l = level();
        for b in &l.blocks {
            // Perimeter wall: outside the playable box.
            let outside = b.max.0 > l.half_x && b.min.0 >= l.half_x - 0.01
                || b.min.0 < -l.half_x && b.max.0 <= -l.half_x + 0.01
                || b.min.2 >= l.half_z - 0.01 && b.max.2 > l.half_z
                || b.max.2 <= -l.half_z + 0.01 && b.min.2 < -l.half_z;
            if outside && b.min.1 >= 0. {
                assert!(b.max.1 >= 6., "perimeter block {:?} too low", b);
            }
        }
    }

    /// Writes a top-down ASCII plan to `$SLAG_ASCII` (run with `--ignored`).
    #[test]
    #[ignore]
    fn dump_ascii_plan() {
        let Ok(path) = std::env::var("SLAG_ASCII") else { return };
        let l = level();
        let mut out = String::new();
        let nav = nav(&l);
        let seen = nav.flood(&starts(&l, 0)[..1], (l.half_x + 3., l.half_z + 3.), |_, _, _| false);
        let ground: HashSet<(i32, i32)> = seen.iter().filter(|k| k.2 < 600).map(|k| (k.0, k.1)).collect();
        for row in -95..=95 {
            let z = row as f32 * 0.5;
            let mut line = String::new();
            for col in -124..=124 {
                let x = col as f32 * 0.5;
                let p = V(x, 0., z);
                let mut ch = ' ';
                let top = l
                    .blocks
                    .iter()
                    .filter(|b| b.max.1 > 0.1 && x >= b.min.0 && x <= b.max.0 && z >= b.min.2 && z <= b.max.2)
                    .map(|b| (b.min.1, b.max.1))
                    .fold(None::<(f32, f32)>, |a, b| Some(a.map_or(b, |a| (a.0.min(b.0), a.1.max(b.1)))));
                if let Some((lo, hi)) = top {
                    ch = if hi > 6. {
                        '#'
                    } else if hi > 2.5 {
                        'M'
                    } else if hi > 1.0 && lo < 0.5 {
                        'm'
                    } else if lo > 1.9 {
                        '^'
                    } else {
                        '-'
                    };
                } else if ground.contains(&((x * 4.).round() as i32, (z * 4.).round() as i32)) {
                    ch = '.';
                }
                for t in 0..2 {
                    if l.spawns[t].iter().any(|s| (s.pos - p).length() < 0.4) {
                        ch = if t == 0 { 'a' } else { 'b' };
                    }
                }
                if l.loot.iter().any(|s| (s.pos.0 - x).abs() < 0.3 && (s.pos.2 - z).abs() < 0.3) {
                    ch = 'L';
                }
                line.push(ch);
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        std::fs::write(path, out).unwrap();
    }
}
