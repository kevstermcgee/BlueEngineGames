//! Fixed-step, rendering-free environmental test bench. Spheres use an explicit contact model.
use serde::{Deserialize, Serialize};
use vesper3d::{
    math::V,
    viewer::{
        controller::{Collider, Controller, ControllerState, Movement},
        devkit::{SavePolicy, Simulation, Snapshot, TICK},
    },
};
#[derive(Clone, Copy, Default)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub look: [f32; 2],
    pub jump: bool,
    pub action: u32,
}
pub const WATER: u32 = 1;
pub const FIRE: u32 = 2;
pub const DROP: u32 = 4;
pub const LIGHT: u32 = 8;
pub const GRAVITY: u32 = 16;
pub const WIND: u32 = 32;
pub const MATERIAL: u32 = 64;
#[derive(Clone, Copy)]
pub struct Material {
    pub name: &'static str,
    pub color: [f32; 3],
    pub density: f32,
    pub bounce: f32,
    pub friction: f32,
    pub burn: bool,
}
pub const MATERIALS: [Material; 6] = [
    Material { name: "Cork", color: [0.65, 0.42, 0.19], density: 240., bounce: 0.25, friction: 0.7, burn: true },
    Material { name: "Rubber", color: [0.85, 0.16, 0.24], density: 1100., bounce: 0.85, friction: 0.9, burn: true },
    Material { name: "Steel", color: [0.56, 0.66, 0.76], density: 7800., bounce: 0.35, friction: 0.2, burn: false },
    Material { name: "Glass", color: [0.35, 0.87, 0.95], density: 2500., bounce: 0.15, friction: 0.15, burn: false },
    Material { name: "Wood", color: [0.48, 0.25, 0.10], density: 650., bounce: 0.3, friction: 0.6, burn: true },
    Material { name: "Ice", color: [0.65, 0.87, 1.], density: 920., bounce: 0.1, friction: 0.03, burn: false },
];
#[derive(Clone, Serialize, Deserialize)]
pub struct Ball {
    pub p: V,
    pub v: V,
    pub material: usize,
    pub heat: f32,
    pub fuel: f32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Drop {
    pub p: V,
    pub v: V,
    pub age: u32,
}
pub struct Sim {
    pub tick: u64,
    pub player: Controller,
    pub balls: Vec<Ball>,
    pub drops: Vec<Drop>,
    pub water: bool,
    pub fire: bool,
    pub light: usize,
    pub low_gravity: bool,
    pub wind: bool,
    pub selected: usize,
    pub extinguished: u64,
}
#[derive(Serialize, Deserialize)]
pub struct State {
    tick: u64,
    player: ControllerState,
    balls: Vec<Ball>,
    drops: Vec<Drop>,
    water: bool,
    fire: bool,
    light: usize,
    low_gravity: bool,
    wind: bool,
    selected: usize,
    extinguished: u64,
}
pub fn obstacles() -> Vec<Collider> {
    let mut boxes = vec![
        Collider { min: V(-5.2, 0., -8.12), max: V(5.2, 5.1, -7.88) },
        Collider { min: V(-3., 0., -4.), max: V(-1., 0.24, -2.) },
        Collider { min: V(-4.12, 0., -3.12), max: V(-3.88, 4.62, -2.88) },
    ];
    for x in [4., 9.] {
        boxes.push(Collider { min: V(x - 0.05, 0., -5.), max: V(x + 0.05, 1.3, 0.) });
    }
    for z in [-5., 0.] {
        boxes.push(Collider { min: V(4., 0., z - 0.05), max: V(9., 1.3, z + 0.05) });
    }
    for i in 0..6 {
        let x = -8. + i as f32 * 1.25;
        boxes.push(Collider { min: V(x - 0.4, 0., 2.6), max: V(x + 0.4, 0.3, 3.4) });
    }
    boxes
}
fn contact(b: &mut Ball, c: &Collider) {
    let q = V(b.p.0.clamp(c.min.0, c.max.0), b.p.1.clamp(c.min.1, c.max.1), b.p.2.clamp(c.min.2, c.max.2));
    let d = b.p - q;
    let l = d.length();
    let (n, depth) = if l > 0.00001 {
        (d / l, 0.3 - l)
    } else {
        let candidates = [
            (b.p.0 - c.min.0, V(-1., 0., 0.)),
            (c.max.0 - b.p.0, V(1., 0., 0.)),
            (b.p.1 - c.min.1, V(0., -1., 0.)),
            (c.max.1 - b.p.1, V(0., 1., 0.)),
            (b.p.2 - c.min.2, V(0., 0., -1.)),
            (c.max.2 - b.p.2, V(0., 0., 1.)),
        ];
        let (dist, n) = candidates.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
        (n, dist + 0.3)
    };
    if depth > 0. {
        b.p = b.p + n * depth;
        let speed = b.v.dot(n);
        if speed < 0. {
            b.v = b.v - n * ((1. + MATERIALS[b.material].bounce) * speed);
        }
    }
}
impl Sim {
    pub fn new(_: u64) -> Self {
        let player = Controller::for_profile(Default::default(), V(0., 0., 10.), 0.).unwrap();
        let mut s = Self {
            tick: 0,
            player,
            balls: vec![],
            drops: vec![],
            water: true,
            fire: true,
            light: 0,
            low_gravity: false,
            wind: false,
            selected: 0,
            extinguished: 0,
        };
        s.drop_test();
        s
    }
    pub fn drop_test(&mut self) {
        for i in 0..6 {
            self.balls.push(Ball { p: V(-8. + i as f32 * 1.25, 4., 1.), v: V::ZERO, material: i, heat: 20., fuel: 1. });
        }
    }
    pub fn step(&mut self, i: &Input) {
        self.tick += 1;
        self.player.look(i.look[0], i.look[1], 1., false);
        self.player.update(
            Movement { forward: i.forward, right: i.right, jump: i.jump, ..Default::default() },
            TICK,
            &obstacles(),
        );
        self.player.position.0 = self.player.position.0.clamp(-11.5, 11.5);
        self.player.position.2 = self.player.position.2.clamp(-7.5, 11.5);
        if i.action & WATER != 0 {
            self.water = !self.water
        }
        if i.action & FIRE != 0 {
            self.fire = !self.fire
        }
        if i.action & LIGHT != 0 {
            self.light = (self.light + 1) % 4
        }
        if i.action & GRAVITY != 0 {
            self.low_gravity = !self.low_gravity
        }
        if i.action & WIND != 0 {
            self.wind = !self.wind
        }
        if i.action & MATERIAL != 0 {
            self.selected = (self.selected + 1) % 6
        }
        if i.action & DROP != 0 && self.balls.len() < 90 {
            let d = V(self.player.yaw.sin(), self.player.pitch.sin(), -self.player.yaw.cos());
            self.balls.push(Ball {
                p: self.player.position + d * 1.2,
                v: d * 7. + V(0., 3., 0.),
                material: self.selected,
                heat: 20.,
                fuel: 1.,
            });
        }
        let boxes = obstacles();
        let gravity = if self.low_gravity { 2. } else { 9.81 };
        if self.water && self.drops.len() < 360 {
            for n in 0..3 {
                self.drops.push(Drop { p: V(-4. + n as f32 * 0.10, 4.5, -3.), v: V(1.8, 0., 0.), age: 0 });
            }
        }
        for d in &mut self.drops {
            d.age += 1;
            d.v.1 -= gravity * TICK;
            d.p = d.p + d.v * TICK;
        }
        for b in &mut self.balls {
            let m = MATERIALS[b.material];
            b.v.1 -= gravity * TICK;
            if self.wind {
                b.v.0 += 2. * TICK;
            }
            // Water tank at x=4..9,z=-5..0, level=1.2. Archimedes with linear sphere submersion approximation.
            if b.p.0 > 4. && b.p.0 < 9. && b.p.2 > -5. && b.p.2 < 0. {
                let fraction = ((1.2 - b.p.1 + 0.3) / 0.6).clamp(0., 1.);
                b.v.1 += gravity * 1000. / m.density * fraction * TICK;
                b.v = b.v * (1. - 2. * fraction * TICK);
                b.heat += (20. - b.heat) * fraction * 0.1;
            }
            b.p = b.p + b.v * TICK;
            for c in &boxes {
                contact(b, c);
            }
            if b.p.1 < 0.3 {
                b.p.1 = 0.3;
                if b.v.1 < 0. {
                    b.v.1 = -b.v.1 * m.bounce;
                }
                b.v.0 *= 1. - m.friction * TICK * 4.;
                b.v.2 *= 1. - m.friction * TICK * 4.;
            }
            for axis in [0, 2] {
                let (lo, hi) = if axis == 0 { (-11.5, 11.5) } else { (-7.5, 11.5) };
                let x = b.p.axis(axis);
                if x < lo || x > hi {
                    if axis == 0 {
                        b.p.0 = x.clamp(lo, hi);
                        b.v.0 = -b.v.0 * m.bounce;
                    } else {
                        b.p.2 = x.clamp(lo, hi);
                        b.v.2 = -b.v.2 * m.bounce;
                    }
                }
            }
            if self.fire && (b.p - V(-2., 0.3, -3.)).length() < 1.5 {
                b.heat += 100. * TICK;
            }
            for d in &self.drops {
                if (d.p - b.p).length() < 0.45 {
                    if b.heat > 100. {
                        self.extinguished += 1;
                    }
                    b.heat = (b.heat - 35.).max(20.);
                }
            }
            if m.burn && b.heat > 100. && b.fuel > 0. {
                b.fuel = (b.fuel - 0.08 * TICK).max(0.);
                b.heat += 20. * TICK;
            } else {
                b.heat += (20. - b.heat) * 0.1 * TICK;
            }
        }
        // Equal radius sphere contact with density-based masses; no angular dynamics in this test model.
        for a in 0..self.balls.len() {
            for c in a + 1..self.balls.len() {
                let (left, right) = self.balls.split_at_mut(c);
                let a = &mut left[a];
                let b = &mut right[0];
                let delta = b.p - a.p;
                let len = delta.length();
                if len < 0.6 && len > 0.0001 {
                    let n = delta / len;
                    let ia = 1. / MATERIALS[a.material].density;
                    let ib = 1. / MATERIALS[b.material].density;
                    let total = ia + ib;
                    a.p = a.p - n * ((0.6 - len) * ia / total);
                    b.p = b.p + n * ((0.6 - len) * ib / total);
                    let speed = (b.v - a.v).dot(n);
                    if speed < 0. {
                        let j = -(1. + MATERIALS[a.material].bounce.min(MATERIALS[b.material].bounce)) * speed / total;
                        a.v = a.v - n * (j * ia);
                        b.v = b.v + n * (j * ib);
                    }
                }
            }
        }
        self.drops.retain(|d| d.age < 240 && d.p.1 > 0.);
    }
}
impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, i: &Input) {
        Sim::step(self, i)
    }
    fn state_hash(&self) -> u64 {
        let bytes = serde_json::to_vec(&self.capture()).unwrap();
        bytes.iter().fold(14695981039346656037, |h, b| (h ^ u64::from(*b)).wrapping_mul(1099511628211))
    }
}
impl Snapshot for Sim {
    const KIND: &'static str = "physics-lab";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = State;
    fn capture(&self) -> State {
        State {
            tick: self.tick,
            player: self.player.network_state(),
            balls: self.balls.clone(),
            drops: self.drops.clone(),
            water: self.water,
            fire: self.fire,
            light: self.light,
            low_gravity: self.low_gravity,
            wind: self.wind,
            selected: self.selected,
            extinguished: self.extinguished,
        }
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        let finite = |v: V| v.0.is_finite() && v.1.is_finite() && v.2.is_finite();
        if !finite(s.player.position)
            || !finite(s.player.velocity)
            || !finite(s.player.push)
            || ![s.player.yaw, s.player.pitch, s.player.feet, s.player.body_height, s.player.vertical_velocity]
                .iter()
                .all(|v| v.is_finite())
            || s.drops.iter().any(|d| !finite(d.p) || !finite(d.v) || d.age > 240)
            || s.balls.len() > 90
            || s.drops.len() > 360
            || s.selected >= 6
            || s.light >= 4
            || s.balls.iter().any(|b| {
                b.material >= 6
                    || ![b.p.0, b.p.1, b.p.2, b.v.0, b.v.1, b.v.2, b.heat, b.fuel].iter().all(|x| x.is_finite())
                    || b.fuel < 0.
                    || b.fuel > 1.
            })
        {
            return Err("Invalid laboratory state".into());
        }
        self.tick = s.tick;
        self.player.restore_network_state(&s.player);
        self.balls = s.balls;
        self.drops = s.drops;
        self.water = s.water;
        self.fire = s.fire;
        self.light = s.light;
        self.low_gravity = s.low_gravity;
        self.wind = s.wind;
        self.selected = s.selected;
        self.extinguished = s.extinguished;
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}
