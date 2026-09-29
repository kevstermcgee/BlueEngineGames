use skyhook_sprint::{Event, Input, Sim};
use vesper3d::math::V;
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Simulation};

fn script() -> Vec<Input> {
    (0..360).map(|t| Input { forward: 1., right: if t % 120 < 60 { 0.15 } else { -0.15 }, jump: t == 20, ..Default::default() }).collect()
}

#[test]
fn replay_is_deterministic() { assert_deterministic(|| Sim::new(7), &script()); }

#[test]
fn pads_apply_an_airborne_forward_impulse() {
    let mut sim = Sim::new(1);
    let feet = sim.player.feet_height();
    sim.bumpers[0].pos = V(sim.player.position.0, feet, sim.player.position.2);
    sim.step(&Input::default());
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::Bumped { .. })));
    assert!(sim.player.vertical_velocity() > 6. && sim.player.push_velocity().2 < -6.);
}

#[test]
fn gates_must_be_collected_in_order() {
    let mut sim = Sim::new(2);
    sim.player.position = V(0., 1.5, -0.6);
    sim.step(&Input::default());
    assert_eq!(sim.score, 0);
    sim.player.position = V(0., 1., 3.8);
    sim.step(&Input::default());
    assert_eq!(sim.score, 1);
}

#[test]
fn all_three_gates_win() {
    let mut sim = Sim::new(3);
    for _ in 0..3 {
        let center = V(sim.player.position.0, sim.player.feet_height() + 0.8, sim.player.position.2);
        sim.orbs[0] = center;
        sim.step(&Input::default());
    }
    assert!(sim.over);
    assert!(sim.drain_events().contains(&Event::Won));
}

#[test]
fn save_resume_is_exact() { snapshot::assert_resumes_exactly(|| Sim::new(9), &script(), 90); }

#[test]
fn finished_runs_ignore_input() {
    let mut sim = Sim::new(4);
    sim.over = true;
    let tick = sim.tick;
    Simulation::step(&mut sim, &Input { forward: 1., ..Default::default() });
    assert_eq!(sim.tick, tick);
}
