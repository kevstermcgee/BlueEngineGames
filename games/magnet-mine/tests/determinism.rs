use magnet_mine::{Event, Input, Sim};
use vesper3d::viewer::devkit::{assert_deterministic, snapshot};

fn script() -> Vec<Input> { (0..300).map(|t| Input { forward: 0.5, right: if t < 150 { 0.4 } else { -0.4 }, jump: t == 80 || t == 180, ..Default::default() }).collect() }

#[test]
fn replay_is_deterministic() { assert_deterministic(|| Sim::new(7), &script()); }

#[test]
fn space_flips_polarity_once_per_press() {
    let mut sim=Sim::new(1); assert_eq!(sim.polarity(),1.);
    sim.step(&Input{jump:true,..Default::default()}); assert_eq!(sim.polarity(),-1.);
    sim.step(&Input{jump:true,..Default::default()}); assert_eq!(sim.polarity(),-1.);
    sim.step(&Input::default()); sim.step(&Input{jump:true,..Default::default()}); assert_eq!(sim.polarity(),1.);
}

#[test]
fn attraction_accelerates_a_drone_toward_the_player() {
    let mut sim=Sim::new(2); let before=(sim.bumpers[0].pos-sim.player.position).length();
    for _ in 0..60 { sim.step(&Input::default()); }
    assert!((sim.bumpers[0].pos-sim.player.position).length() < before);
}

#[test]
fn docking_a_drone_scores() {
    let mut sim=Sim::new(3); sim.bumpers[0].pos=sim.orbs[0]; sim.step(&Input::default());
    assert_eq!(sim.score,1); assert!(sim.drain_events().iter().any(|e| matches!(e,Event::Collected{..})));
}

#[test]
fn save_resume_is_exact() { snapshot::assert_resumes_exactly(|| Sim::new(4), &script(), 120); }
