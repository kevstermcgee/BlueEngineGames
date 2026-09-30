//! The table is a pure library, so it is tested without a window: same seed and inputs replay identically, and
//! each rule is checked by driving `Sim` the way the window does.
use clockwork_pinball::physics::{Material, Physics};
use clockwork_pinball::*;
use vesper3d::math::V;
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Simulation};

fn idle() -> Input {
    Input::default()
}

/// Serve the ball and launch it at `power` (0..1); returns the sim in play.
fn launched(seed: u64, power: f32) -> Sim {
    let mut sim = Sim::new(seed);
    let hold = (power / (1.1 / 60.)).ceil() as usize + 1;
    for _ in 0..hold {
        sim.step(&Input { plunger: true, ..idle() });
    }
    sim.step(&idle());
    assert_eq!(sim.phase, Phase::Playing, "releasing the plunger launches");
    sim
}

fn run(sim: &mut Sim, ticks: usize, input: Input) {
    for _ in 0..ticks {
        sim.step(&input);
    }
}

/// Inputs recorded from the autoplayer on its own table, so they really play the game (plunge, flip, drain).
fn scripted() -> Vec<Input> {
    let mut sim = Sim::new(7);
    (0..1800)
        .map(|t| {
            let mut input = autoplay(&sim);
            // Now and then a human flips too early or late, so the recording is not perfect play.
            input.left ^= t % 97 < 4;
            input.right ^= t % 89 < 5;
            sim.step(&input);
            input
        })
        .collect()
}

#[test]
fn the_same_seed_and_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(7), &scripted());
}

#[test]
fn the_ball_starts_on_the_plunger_and_does_nothing_until_launched() {
    let mut sim = Sim::new(1);
    let start = sim.ball_pos();
    run(&mut sim, 120, idle());
    assert!((sim.ball_pos() - start).length() < 0.1, "at rest on the plunger stop: {:?}", sim.ball_pos());
    assert_eq!(sim.phase, Phase::Waiting);
    assert!(sim.in_lane());
}

#[test]
fn a_weak_pull_falls_back_and_a_full_pull_reaches_the_playfield() {
    let mut weak = launched(2, 0.05);
    run(&mut weak, 300, idle());
    assert!(weak.ball_pos().2 > 0., "a weak plunge rolls back down the lane: {:?}", weak.ball_pos());
    let mut full = launched(2, 1.0);
    let mut reached = false;
    for _ in 0..360 {
        full.step(&idle());
        if full.ball_pos().0 < RIGHT - 0.3 && full.ball_pos().2 < -1. {
            reached = true;
            break;
        }
    }
    assert!(reached, "a full plunge gets round the top and into play: {:?}", full.ball_pos());
}

#[test]
fn the_incline_pulls_a_loose_ball_down_the_table() {
    let mut sim = launched(3, 1.0);
    sim.phys.set_position(sim.ball, V(1.4, 0.15, -1.0));
    sim.phys.set_linvel(sim.ball, V(0., 0., 0.));
    let z = sim.ball_pos().2;
    run(&mut sim, 45, idle());
    assert!(sim.ball_pos().2 > z + 0.5, "rolled down-table from {z} to {}", sim.ball_pos().2);
}

#[test]
fn a_fast_ball_cannot_tunnel_through_a_wall_or_the_floor() {
    // 0.15 m ball, 60 m/s is 1 m per tick: without continuous collision detection it would pass a 0.2 m wall.
    for (i, (dx, dz)) in [(-1., 0.), (0., -1.), (-0.7, -0.7), (0.4, -1.)].iter().enumerate() {
        let mut sim = launched(10 + i as u64, 1.0);
        sim.phys.set_position(sim.ball, V(-0.5, 0.15, -1.0));
        sim.phys.set_linvel(sim.ball, V(*dx, 0., *dz) * 60.);
        for _ in 0..240 {
            sim.step(&idle());
            let p = sim.ball_pos();
            if sim.phase != Phase::Playing {
                break;
            }
            assert!(
                p.0 > LEFT - 0.05 && p.0 < 3.1 && p.2 > TOP - 0.3 && p.1 > -0.05,
                "escaped at {p:?} (heading {dx},{dz})"
            );
        }
    }
}

#[test]
fn a_flipper_swung_into_the_ball_launches_it_up_the_table() {
    let mut sim = launched(4, 1.0);
    // Rest the ball on the raised-ready right flipper, near its tip.
    sim.phys.set_position(sim.ball, V(0.05, 0.15, 3.7));
    sim.phys.set_linvel(sim.ball, V(0., 0., 0.));
    run(&mut sim, 25, idle());
    let before = sim.ball_pos();
    let mut best = 0.0f32;
    for _ in 0..20 {
        sim.step(&Input { right: true, ..idle() });
        best = best.max(-sim.phys.linvel(sim.ball).2);
    }
    assert!(best > 4., "the flip sends the ball up-table at {best:.1} m/s (it sat at {before:?})");
}

#[test]
fn a_bumper_kicks_the_ball_away_and_scores() {
    let mut sim = launched(5, 1.0);
    let (bx, bz) = BUMPERS[2];
    sim.phys.set_position(sim.ball, V(bx + 1.2, 0.15, bz));
    sim.phys.set_linvel(sim.ball, V(-3., 0., 0.));
    let score = sim.score;
    let mut kicked = None;
    for _ in 0..120 {
        sim.step(&idle());
        for e in sim.drain_events() {
            if let Event::Bumper { points, .. } = e {
                kicked = Some(points);
            }
        }
        if kicked.is_some() {
            break;
        }
    }
    assert_eq!(kicked, Some(100));
    assert_eq!(sim.score, score + 100);
    assert!(sim.ball_speed() >= 6.5, "kicked away at {:.1} m/s", sim.ball_speed());
}

#[test]
fn the_three_rollover_lanes_raise_the_multiplier_and_reset() {
    let mut sim = launched(6, 1.0);
    for (x, z) in LANES {
        sim.phys.set_position(sim.ball, V(x, 0.15, z + 0.05));
        sim.phys.set_linvel(sim.ball, V(0., 0., 0.));
        sim.step(&idle());
    }
    assert_eq!(sim.multiplier, 2, "completing the lanes raises the multiplier");
    assert_eq!(sim.lanes, [false; 3], "and lights out again");
    assert!(sim.score >= 3 * 500 + 2000);
    let events = sim.drain_events();
    assert!(events.contains(&Event::Multiplier(2)));
}

#[test]
fn draining_costs_a_ball_serves_the_next_and_the_last_ends_the_game() {
    let mut sim = launched(7, 1.0);
    for remaining in (0..3).rev() {
        sim.phys.set_position(sim.ball, V(-0.25, 0.15, DRAIN_Z - 0.2));
        sim.phys.set_linvel(sim.ball, V(0., 0., 5.));
        run(&mut sim, 40, idle());
        assert_eq!(sim.balls_left, remaining);
        if remaining == 0 {
            assert_eq!(sim.phase, Phase::GameOver);
            break;
        }
        run(&mut sim, 100, idle());
        assert_eq!(sim.phase, Phase::Waiting, "the next ball waits on the plunger");
        assert!(sim.in_lane());
        // Launch it for the next drain.
        run(&mut sim, 70, Input { plunger: true, ..idle() });
        sim.step(&idle());
    }
    let tick = sim.tick;
    sim.step(&idle());
    assert_eq!(sim.tick, tick, "a finished game ignores input");
}

#[test]
fn the_physics_layer_stacks_boxes_and_reports_contacts() {
    let mut p = Physics::new(V(0., -9.81, 0.));
    let mat = Material { friction: 0.8, restitution: 0.0, ..Default::default() };
    let floor = p.fixed_box(V(0., -0.5, 0.), V(5., 0.5, 5.), 0., mat);
    let a = p.dynamic_box(V(0., 0.5, 0.), V(0.5, 0.5, 0.5), 0., mat);
    let b = p.dynamic_box(V(0., 1.6, 0.), V(0.5, 0.5, 0.5), 0., mat);
    for _ in 0..240 {
        p.step();
    }
    assert!((p.position(b).1 - 1.5).abs() < 0.05, "a box rests on a box: {:?}", p.position(b));
    assert!(p.touching(a).contains(&floor) && p.touching(a).contains(&b));
    assert!(p.is_sleeping(a) && p.is_sleeping(b), "a settled stack falls asleep");
    p.remove(b);
    assert!(!p.alive(b));
}

#[test]
fn saves_are_a_pure_function_of_the_file_and_resume_close_to_the_original() {
    // Pinball is chaotic, so the promise is the physics-continuation one: a load replays identically, and a
    // resumed run stays within a measured distance of the uninterrupted one for a short while.
    let inputs: Vec<Input> = scripted().into_iter().take(700).collect();
    snapshot::assert_loads_replay_identically(|| Sim::new(7), &inputs, 25);
}

#[test]
fn a_bad_save_is_refused_and_the_running_game_is_left_alone() {
    let mut sim = Sim::new(11);
    for input in scripted().iter().take(120) {
        sim.step(input);
    }
    let bytes = snapshot::save(&sim, "good").unwrap();
    for input in scripted().iter().take(50) {
        sim.step(input);
    }
    let before = sim.state_hash();
    let mut damaged = bytes.clone();
    let mid = damaged.len() / 2;
    damaged[mid] ^= 0x20;
    assert!(snapshot::restore(&mut sim, &damaged).is_err(), "a flipped bit is always caught");
    assert!(snapshot::restore(&mut sim, &bytes[..bytes.len() / 2]).is_err());
    assert_eq!(sim.state_hash(), before, "a refused load changes nothing");
    snapshot::restore(&mut sim, &bytes).unwrap();
    assert_eq!(sim.tick, 120);
}

#[test]
fn a_simple_player_keeps_the_ball_alive_and_keeps_scoring() {
    // (A weak first plunge once left the ball parked on the plunger for good: the game must serve it again.)
    let mut sim = Sim::new(21);
    let mut at = Vec::new();
    for t in 1..=60 * 60 * 4 {
        let input = autoplay(&sim);
        sim.step(&input);
        if t % (60 * 60) == 0 {
            at.push((sim.score, sim.balls_left));
        }
    }
    println!("autoplay score and balls left each minute: {at:?}");
    assert!(at[3].0 > at[1].0 && at[1].0 > at[0].0, "it keeps scoring, so the table has no dead end: {at:?}");
    assert!(at[0].0 >= 1500, "a player who just flips at the ball scores quickly: {at:?}");
}

#[test]
fn a_resumed_run_stays_near_the_uninterrupted_one_for_a_short_while() {
    // Pinball is chaotic, so this is the measured promise, not an exact one: within a couple of seconds of a load
    // the ball is within a few metres of where it would have been (the table is 11 m long).
    let inputs: Vec<Input> = scripted().into_iter().take(130).collect();
    let worst = snapshot::assert_resumes_within(
        || Sim::new(7),
        &inputs,
        25,
        50.0,
        |a, b| (a.ball_pos() - b.ball_pos()).length(),
    );
    println!("worst resume drift: {worst:.3} m");
}
