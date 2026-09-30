//! Each rule is checked by driving `Sim` the way the window does.
use tumble_maze::*;
use vesper3d::math::V;
use vesper3d::viewer::devkit::{assert_deterministic, run_inputs, snapshot, Snapshot};

fn run(sim: &mut Sim, input: Input, ticks: u32) {
    for _ in 0..ticks {
        sim.step(&input);
    }
}

fn dist(a: V, b: V) -> f32 {
    ((a.0 - b.0).powi(2) + (a.2 - b.2).powi(2)).sqrt()
}

fn wobble() -> Vec<Input> {
    (0..600).map(|t| Input { x: ((t / 40) % 3) as f32 - 1., z: if (t / 70) % 2 == 0 { 0.6 } else { -0.4 } }).collect()
}

#[test]
fn the_same_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(1), &wobble());
    assert_deterministic(|| Sim::at(2), &wobble());
}

#[test]
fn every_level_can_be_solved_by_search() {
    for (i, level) in LEVELS.iter().enumerate() {
        let grid = Grid::parse(level.map);
        let route = grid.route(grid.start);
        assert!(!route.is_empty(), "level {i} ({}) has no route to the goal", level.name);
        for star in &grid.stars {
            let mut g2 = Grid::parse(level.map);
            g2.goal = *star;
            assert!(!g2.route(g2.start).is_empty(), "level {i}: a star at {star:?} cannot be reached");
        }
    }
}

#[test]
fn tilting_accelerates_the_marble_downhill() {
    let mut sim = Sim::new(1);
    let start = sim.ball_pos();
    run(&mut sim, Input { x: 1., z: 0. }, 30);
    let v = sim.phys.linvel(sim.ball);
    assert!(v.0 > 0.3, "tilting toward +x should roll it that way, vx = {}", v.0);
    assert!(sim.ball_pos().0 > start.0);
    assert!(v.2.abs() < 0.1, "no sideways drift on a straight tilt: {}", v.2);
}

#[test]
fn a_rolling_ball_follows_the_incline_physics() {
    // A ball rolling without slipping down an incline of angle a accelerates at 5/7 g sin a (less damping).
    let mut sim = Sim::new(1);
    let (x0, t) = (sim.ball_pos().0, 0.5);
    sim.tilt = (MAX_TILT, 0.);
    sim.phys.set_gravity(Sim::gravity_for(sim.tilt));
    run(&mut sim, Input { x: 1., z: 0. }, (t / vesper3d::viewer::devkit::TICK) as u32);
    let d = sim.ball_pos().0 - x0;
    let ideal = 0.5 * (5. / 7.) * 9.81 * MAX_TILT.sin() * t * t;
    assert!(d > 0.5 * ideal && d < 1.15 * ideal, "rolled {d:.3} m, ideal {ideal:.3} m");
}

#[test]
fn a_flat_board_lets_the_marble_coast_to_rest() {
    let mut sim = Sim::new(1);
    run(&mut sim, Input { x: 1., z: 0. }, 40);
    run(&mut sim, Input::default(), 600);
    assert!(sim.phys.speed(sim.ball) < 0.05, "friction and damping should stop it: {}", sim.phys.speed(sim.ball));
}

#[test]
fn walls_hold_the_marble_in_even_at_full_tilt() {
    let mut sim = Sim::new(1);
    run(&mut sim, Input { x: -1., z: -1. }, 1200);
    run(&mut sim, Input { x: 1., z: 1. }, 1200);
    let p = sim.ball_pos();
    assert!(p.0.abs() < sim.grid.w as f32 / 2. && p.2.abs() < sim.grid.h as f32 / 2. && p.1 > 0., "escaped to {p:?}");
    assert_eq!(sim.falls, 0);
}

#[test]
fn a_hole_swallows_the_marble_and_costs_time() {
    let mut sim = Sim::at(1);
    let hole = sim.grid.centre(sim.grid.holes[0]);
    sim.phys.set_position(sim.ball, V(hole.0 - 0.6, BALL_R + 0.01, hole.2));
    sim.phys.set_linvel(sim.ball, V(2., 0., 0.));
    let before = sim.clock;
    run(&mut sim, Input::default(), 60);
    assert_eq!(sim.falls, 1);
    assert!(sim.clock >= before + 60 + (FALL_PENALTY / vesper3d::viewer::devkit::TICK) as u32 - 1);
    let start = sim.grid.centre(sim.grid.start);
    assert!(dist(sim.ball_pos(), start) < 0.3, "back at the start");
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::Fell { .. })));
}

#[test]
fn a_fast_marble_does_not_tunnel_through_a_wall() {
    let mut sim = Sim::new(1);
    // From the start corner, fire at the outer wall at 40 m/s.
    sim.phys.set_linvel(sim.ball, V(-40., 0., 0.));
    run(&mut sim, Input::default(), 120);
    let p = sim.ball_pos();
    assert!(p.0 > -(sim.grid.w as f32) / 2. && p.1 > 0., "tunnelled out to {p:?}");
}

#[test]
fn stars_are_collected_once() {
    let mut sim = Sim::new(1);
    let star = sim.grid.centre(sim.grid.stars[0]);
    sim.phys.set_position(sim.ball, V(star.0, BALL_R + 0.01, star.2));
    run(&mut sim, Input::default(), 2);
    assert_eq!(sim.stars(), 1);
    run(&mut sim, Input::default(), 30);
    assert_eq!(sim.stars(), 1);
    let stars: Vec<_> = sim.drain_events().into_iter().filter(|e| matches!(e, Event::Star { .. })).collect();
    assert_eq!(stars.len(), 1);
}

#[test]
fn the_bot_beats_every_level_by_rolling() {
    let mut sim = Sim::new(1);
    for level in 0..LEVELS.len() {
        assert_eq!(sim.level, level);
        let mut ticks = 0;
        while matches!(sim.phase, Phase::Playing) {
            let input = autoplay(&sim);
            sim.step(&input);
            ticks += 1;
            assert!(
                ticks < 60 * 240,
                "level {level} ({}) took over four minutes; bot at {:?}",
                LEVELS[level].name,
                sim.ball_pos()
            );
        }
        eprintln!("level {level}: {:.1}s, {} falls, score {}", sim.seconds(), sim.falls, sim.score);
        while matches!(sim.phase, Phase::Cleared(_)) {
            sim.step(&Input::default());
        }
    }
    assert_eq!(sim.phase, Phase::Done);
    assert!(sim.score >= 3000);
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::AllDone { .. })));
}

#[test]
fn the_sweeping_bars_move_and_shove() {
    let mut sim = Sim::at(2);
    let a = sim.phys.position(sim.movers[0]);
    run(&mut sim, Input::default(), 75);
    let b = sim.phys.position(sim.movers[0]);
    assert!((a.0 - b.0).abs() > 1.0, "the first bar should have travelled: {a:?} -> {b:?}");
}

#[test]
fn saves_resume_within_a_fair_margin() {
    let worst = snapshot::assert_resumes_within(
        || Sim::new(1),
        &wobble()[..300],
        25,
        0.75,
        |a, b| (a.ball_pos() - b.ball_pos()).length(),
    );
    println!("worst resume drift: {worst:.3} m");
}

#[test]
fn a_loaded_game_replays_the_same_way_twice() {
    snapshot::assert_loads_replay_identically(|| Sim::at(2), &wobble()[..300], 60);
}

#[test]
fn bad_saves_are_refused() {
    let mut sim = Sim::new(1);
    let mut s = sim.capture();
    s.level = 9;
    assert!(sim.restore(s).is_err());
    let mut s = sim.capture();
    s.tilt = (5., 0.);
    assert!(sim.restore(s).is_err());
    let mut s = sim.capture();
    s.bodies.pop();
    assert!(sim.restore(s).is_err());
    let _ = run_inputs(&mut sim, &wobble()[..10]);
}
