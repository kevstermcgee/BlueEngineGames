//! The tower is a pure library, so it is tested without a window: same seed and inputs replay identically, and each
//! rule is checked by driving `Sim` the way the window does.
use vesper3d::math::V;
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Simulation};
use wobble_tower::physics::{Material, Physics};
use wobble_tower::*;

fn idle() -> Input {
    Input::default()
}

fn run(sim: &mut Sim, ticks: usize) {
    for _ in 0..ticks {
        sim.step(&idle());
    }
}

/// Wait until the crane is over `x` with a crate on the hook, then drop it.
fn drop_over(sim: &mut Sim, x: f32) {
    for _ in 0..60 * 30 {
        if sim.carried.is_some() && (sim.carriage_x - x).abs() < 0.04 {
            sim.step(&Input { drop: true });
            sim.step(&idle());
            return;
        }
        sim.step(&idle());
    }
    panic!("the crane never got over x = {x}");
}

fn play(sim: &mut Sim, ticks: usize) {
    for _ in 0..ticks {
        let input = autoplay(sim);
        sim.step(&input);
    }
}

/// Inputs recorded from the autoplayer, so the determinism and save tests really play (drop, stack, wind).
fn recorded(seed: u64, ticks: usize) -> Vec<Input> {
    let mut sim = Sim::new(seed);
    (0..ticks)
        .map(|_| {
            let input = autoplay(&sim);
            sim.step(&input);
            input
        })
        .collect()
}

#[test]
fn the_same_seed_and_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(7), &recorded(7, 1500));
}

#[test]
fn different_seeds_hang_different_crates() {
    let a = Sim::new(1).carried;
    let b = (2..40).map(|s| Sim::new(s).carried).find(|c| *c != a);
    assert!(b.is_some(), "the crates come from the seed");
}

#[test]
fn a_crate_dropped_on_the_platform_lands_rests_and_counts() {
    let mut sim = Sim::new(3);
    drop_over(&mut sim, 0.0);
    let half = sim.crates[0].half;
    run(&mut sim, 60 * 4);
    let p = sim.phys.position(physics::Body(sim.crates[0].body));
    assert!((p.1 - half.1).abs() < 0.03, "it rests on the platform: y = {} (half height {})", p.1, half.1);
    assert_eq!(sim.crates[0].state, CrateState::Settled);
    assert_eq!(sim.stacked, 1);
    assert!(sim.score >= 100);
    assert!(sim.phys.is_sleeping(physics::Body(sim.crates[0].body)), "a settled crate falls asleep");
    let events = sim.drain_events();
    assert!(events.iter().any(|e| matches!(e, Event::Drop { .. })));
    assert!(events.iter().any(|e| matches!(e, Event::Land { .. })));
    assert!(events.iter().any(|e| matches!(e, Event::Settled { .. })));
}

#[test]
fn a_modest_neat_tower_stands_through_the_wind() {
    let mut sim = Sim::new(5);
    let mut ticks = 0;
    while sim.stacked < 5 && ticks < 60 * 90 {
        let input = autoplay(&sim);
        sim.step(&input);
        ticks += 1;
    }
    assert!(sim.stacked >= 5, "the autoplayer stacked only {} crates in {} s", sim.stacked, ticks / 60);
    assert!(sim.height > 3., "the tower has some height: {:.1} m", sim.height);
    let lives = sim.lives;
    for _ in 0..60 * 10 {
        sim.step(&idle());
    }
    assert_eq!(sim.lives, lives, "five crates stand through ten seconds of wind");
    assert!(sim.perfects >= 1, "some drops were perfect");
}

#[test]
fn the_wind_eventually_beats_a_tall_tower_so_a_game_always_ends() {
    for seed in [5, 6, 7] {
        let mut sim = Sim::new(seed);
        let mut best = 0.0f32;
        for _ in 0..60 * 60 * 5 {
            let input = autoplay(&sim);
            sim.step(&input);
            best = best.max(sim.height);
            if sim.phase == Phase::Over {
                break;
            }
        }
        assert_eq!(sim.phase, Phase::Over, "seed {seed}: still standing after five minutes at {best:.1} m");
        assert!(best > 3., "seed {seed}: a fair player builds more than {best:.1} m before the end");
    }
}

#[test]
fn a_crate_over_the_edge_falls_into_the_void_and_costs_a_life() {
    let mut sim = Sim::new(6);
    drop_over(&mut sim, PLATFORM_HALF + 1.3);
    run(&mut sim, 60 * 4);
    assert_eq!(sim.lives, LIVES - 1);
    assert_eq!(sim.crates[0].state, CrateState::Fell);
    assert!(!sim.phys.alive(physics::Body(sim.crates[0].body)), "its body is removed");
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::Fell { .. })));
}

#[test]
fn a_crate_whose_middle_hangs_past_the_edge_of_the_one_below_tips_off() {
    let mut sim = Sim::new(8);
    drop_over(&mut sim, 0.0);
    run(&mut sim, 60 * 3);
    let below = sim.crates[0];
    let below_top = sim.phys.position(physics::Body(below.body)).1 + below.half.1;
    // The next crate, dropped with its centre well beyond the lower crate's edge.
    while sim.carried.is_none() {
        sim.step(&idle());
    }
    let hx = sim.carried.unwrap().0 .0;
    let target = below.half.0 + 0.35 * hx;
    drop_over(&mut sim, target);
    run(&mut sim, 60 * 6);
    let second = sim.crates[1];
    let final_y =
        if second.state == CrateState::Fell { f32::MIN } else { sim.phys.position(physics::Body(second.body)).1 };
    assert!(final_y < below_top, "it did not stay on top: y = {final_y} (below's top at {below_top})");
}

#[test]
fn three_lost_crates_end_the_game_and_a_finished_game_ignores_input() {
    let mut sim = Sim::new(9);
    for _ in 0..3 {
        drop_over(&mut sim, -(PLATFORM_HALF + 1.4));
        run(&mut sim, 60 * 2);
    }
    assert_eq!(sim.lives, 0);
    assert_eq!(sim.phase, Phase::Over);
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::GameOver { .. })));
    let tick = sim.tick;
    sim.step(&Input { drop: true });
    assert_eq!(sim.tick, tick, "input is ignored");
}

#[test]
fn gusts_start_with_a_few_crates_and_grow_with_the_stack() {
    let mut sim = Sim::new(5);
    let mut gusts = Vec::new();
    for _ in 0..60 * 120 {
        let input = autoplay(&sim);
        sim.step(&input);
        for e in sim.drain_events() {
            if let Event::Gust { strength } = e {
                gusts.push((sim.stacked, strength.abs()));
            }
        }
        if sim.phase == Phase::Over {
            break;
        }
    }
    assert!(gusts.len() >= 3, "gusts come and go: {gusts:?}");
    assert!(gusts.iter().all(|(stacked, _)| *stacked >= 3), "none before three crates: {gusts:?}");
    let first = gusts.first().unwrap().1;
    let strongest = gusts.iter().map(|g| g.1).fold(0., f32::max);
    assert!(strongest > first, "later gusts are stronger: {gusts:?}");
    assert!(strongest <= 2.41);
}

#[test]
fn a_grippy_heavy_crate_holds_where_a_slippery_light_one_slides_in_the_same_push() {
    // Friction is what stands between a stack and the wind: the same 4 m/s^2 push moves the light crate (grip
    // about 3 m/s^2 with this floor) and not the heavy one (about 5).
    let mut p = Physics::new(V(0., -9.81, 0.));
    // Friction between two bodies is the AVERAGE of their two coefficients (rapier's default), so a grippy floor
    // would hold both crates: give the floor little grip of its own.
    let floor = Material { friction: 0.1, restitution: 0., ..Default::default() };
    p.fixed_box(V(0., -0.5, 0.), V(20., 0.5, 1.), 0., floor);
    let light = p.dynamic_box_planar(V(-5., 0.5, 0.), V(0.5, 0.5, 0.5), 0., Kind::Light.material());
    let heavy = p.dynamic_box_planar(V(5., 0.5, 0.), V(0.5, 0.5, 0.5), 0., Kind::Heavy.material());
    for _ in 0..60 {
        p.step();
    }
    for _ in 0..90 {
        for b in [light, heavy] {
            p.push(b, V(4. * p.mass(b), 0., 0.));
        }
        p.step();
    }
    let (l, h) = (p.position(light).0 + 5., p.position(heavy).0 - 5.);
    assert!(l > 1.0, "the light crate slid {l:.2} m");
    assert!(h.abs() < 0.05, "the heavy crate held: {h:.3} m");
}

#[test]
fn planar_bodies_stay_in_their_plane() {
    let mut sim = Sim::new(2);
    play(&mut sim, 60 * 40);
    for c in sim.crates.iter().filter(|c| c.state != CrateState::Fell) {
        let b = physics::Body(c.body);
        let p = sim.phys.position(b);
        let q = sim.phys.rotation(b);
        assert!(p.2.abs() < 1e-4, "z stays 0: {}", p.2);
        assert!(q[0].abs() < 1e-4 && q[1].abs() < 1e-4, "it only turns about z: {q:?}");
    }
}

#[test]
fn saves_load_identically_and_a_resumed_run_stays_close_for_a_while() {
    let inputs = recorded(7, 900);
    snapshot::assert_loads_replay_identically(|| Sim::new(7), &inputs, 30);
    let short: Vec<Input> = inputs.iter().take(450).cloned().collect();
    let worst = snapshot::assert_resumes_within(
        || Sim::new(7),
        &short,
        30,
        50.0,
        |a, b| {
            a.crates
                .iter()
                .zip(&b.crates)
                .filter(|(x, y)| x.state != CrateState::Fell && y.state != CrateState::Fell)
                .map(|(x, y)| {
                    (a.phys.position(physics::Body(x.body)) - b.phys.position(physics::Body(y.body))).length()
                })
                .fold(0., f32::max)
        },
    );
    println!("worst resume drift: {worst:.3} m");
}

#[test]
fn a_bad_save_is_refused_and_the_running_game_is_left_alone() {
    let mut sim = Sim::new(11);
    play(&mut sim, 60 * 20);
    let bytes = snapshot::save(&sim, "good").unwrap();
    play(&mut sim, 60);
    let before = sim.state_hash();
    let mut damaged = bytes.clone();
    let mid = damaged.len() / 2;
    damaged[mid] ^= 0x20;
    assert!(snapshot::restore(&mut sim, &damaged).is_err(), "a flipped bit is always caught");
    assert!(snapshot::restore(&mut sim, &bytes[..bytes.len() / 2]).is_err());
    assert_eq!(sim.state_hash(), before, "a refused load changes nothing");
    snapshot::restore(&mut sim, &bytes).unwrap();
    assert_eq!(sim.tick, 60 * 20);
}
