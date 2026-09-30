use physics_lab::*;
use vesper3d::{
    math::V,
    viewer::devkit::{assert_deterministic, snapshot::assert_resumes_as_promised},
};
#[test]
fn replay_and_save() {
    let inputs: Vec<_> =
        (0..600).map(|t| Input { action: if t % 100 == 0 { DROP | LIGHT } else { 0 }, ..Default::default() }).collect();
    assert_deterministic(|| Sim::new(7), &inputs);
    assert_resumes_as_promised(|| Sim::new(7), &inputs, 200);
}
#[test]
fn material_drop_and_buoyancy() {
    let mut s = Sim::new(7);
    s.balls.clear();
    for material in [0, 2] {
        s.balls.push(Ball { p: V(6., 0.5, -2.), v: V::ZERO, material, heat: 20., fuel: 1. });
    }
    for _ in 0..300 {
        s.step(&Input::default());
    }
    assert!(s.balls[0].p.1 > 0.8);
    assert!(s.balls[1].p.1 < 0.4);
}
#[test]
fn fire_heats_and_water_cools() {
    let mut s = Sim::new(0);
    s.water = false;
    s.balls = vec![Ball { p: V(-2., 0.3, -3.), v: V::ZERO, material: 4, heat: 20., fuel: 1. }];
    for _ in 0..120 {
        s.step(&Input::default());
    }
    assert!(s.balls[0].heat > 100.);
    assert!(s.balls[0].fuel < 1.);
    s.fire = false;
    for _ in 0..10 {
        s.drops.push(Drop { p: s.balls[0].p, v: V::ZERO, age: 0 });
    }
    s.step(&Input::default());
    assert!(s.balls[0].heat < 100.);
    assert!(s.extinguished > 0);
}
#[test]
fn toggles_and_bounds() {
    let mut s = Sim::new(0);
    s.step(&Input { action: WATER | FIRE | GRAVITY | WIND | MATERIAL | LIGHT, ..Default::default() });
    assert!(!s.water && !s.fire && s.low_gravity && s.wind);
    assert_eq!((s.selected, s.light), (1, 1));
    for _ in 0..1000 {
        s.step(&Input { action: DROP, ..Default::default() });
    }
    assert_eq!(s.balls.len(), 90);
    assert!(s.drops.len() <= 360);
}
