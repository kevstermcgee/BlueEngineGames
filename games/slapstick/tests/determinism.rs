//! The rules are a plain library, so they are tested without a window: same seed and inputs must give
//! the same run, and save/load goes through the public `Simulation`/`Snapshot` API exactly as the window
//! and `netgame.rs` use it.
use slapstick::{
    Event, Input, Inputs, Phase, Puck, Sim, GOAL_HALF_WIDTH, MATCH_TIME_LIMIT_TICKS, PADDLE_RADIUS, PUCK_MAX_SPEED,
    PUCK_RADIUS, TABLE_HALF_LENGTH, TABLE_HALF_WIDTH, WIN_GOALS,
};
use vesper3d::math::V;
use vesper3d::viewer::devkit::{assert_deterministic, snapshot, Rng, SaveError, SaveSlots, Simulation, QUICK_SLOT};

fn wiggly_script(n: usize) -> Vec<Inputs> {
    (0..n)
        .map(|t| {
            let f = t as f32;
            Inputs([
                Input { target_x: (f * 0.031).sin(), target_z: (f * 0.021).cos() },
                Input { target_x: (f * 0.026).cos(), target_z: (f * 0.017).sin() },
            ])
        })
        .collect()
}

#[test]
fn the_same_seed_and_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(101), &wiggly_script(600));
}

#[test]
fn a_save_from_any_tick_resumes_as_the_game_promises_in_a_brand_new_match() {
    // `Sim::POLICY` is `SavePolicy::Exact` (no rigid-body world: hand-rolled vector math), so this is
    // the strongest and simplest save contract (see AGENTS.md's "No rapier3d, deliberately").
    snapshot::assert_resumes_as_promised(|| Sim::new(102), &wiggly_script(400), 25);
}

#[test]
fn a_bad_save_is_refused_and_the_running_match_is_left_alone() {
    let mut sim = Sim::new(103);
    let script = wiggly_script(150);
    for input in &script {
        sim.step(input);
    }
    let bytes = snapshot::save(&sim, "good").unwrap();
    for input in &script[..40] {
        sim.step(input);
    }
    let before = sim.state_hash();
    let mut damaged = bytes.clone();
    let mid = damaged.len() / 2;
    damaged[mid] ^= 0x55;
    assert!(snapshot::restore(&mut sim, &damaged).is_err(), "a flipped bit is always caught");
    assert!(snapshot::restore(&mut sim, &bytes[..bytes.len() / 2]).is_err(), "a truncated save is refused");
    assert_eq!(sim.state_hash(), before, "a refused load changes nothing");
    snapshot::restore(&mut sim, &bytes).unwrap();
    assert_eq!(sim.tick, 150);
}

#[test]
fn quick_save_and_quick_load_go_through_a_slot_with_a_backup() {
    let dir = std::env::temp_dir().join(format!("slapstick-det-slots-{}", std::process::id()));
    let slots = SaveSlots::new(&dir);
    let mut sim = Sim::new(104);
    assert!(matches!(snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT), Err(SaveError::NotFound(_))));
    let script = wiggly_script(90);
    for input in &script {
        sim.step(input);
    }
    snapshot::save_to_slot(&sim, &slots, QUICK_SLOT, "first").unwrap();
    let first_hash = sim.state_hash();
    for input in &script {
        sim.step(input);
    }
    snapshot::save_to_slot(&sim, &slots, QUICK_SLOT, "second").unwrap();
    let second_hash = sim.state_hash();
    for input in &script[..30] {
        sim.step(input);
    }
    let (header, _) = snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT).unwrap();
    assert_eq!((header.label.as_str(), sim.state_hash()), ("second", second_hash));
    assert_ne!(first_hash, second_hash);
    let path = slots.path(QUICK_SLOT).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x44;
    std::fs::write(&path, bytes).unwrap();
    let (header, _) = snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT).unwrap();
    assert_eq!(header.label, "first", "a damaged primary falls back to the backup");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_puck_never_leaves_the_table_bounds() {
    for seed in 0..16u64 {
        let mut sim = Sim::new(seed);
        let mut rng = Rng::new(seed ^ 0x51A7_ABCD);
        for _ in 0..3000 {
            let inputs = Inputs([
                Input { target_x: rng.range(-1., 1.), target_z: rng.range(-1., 1.) },
                Input { target_x: rng.range(-1., 1.), target_z: rng.range(-1., 1.) },
            ]);
            sim.step(&inputs);
            let slack = PUCK_RADIUS + 1e-3;
            assert!(sim.puck.pos.0.abs() <= TABLE_HALF_WIDTH + slack, "seed {seed}: {:?}", sim.puck.pos);
            assert!(sim.puck.pos.2.abs() <= TABLE_HALF_LENGTH + slack, "seed {seed}: {:?}", sim.puck.pos);
        }
    }
}

#[test]
fn the_puck_never_exceeds_its_speed_cap() {
    for seed in 0..16u64 {
        let mut sim = Sim::new(seed);
        let mut rng = Rng::new(seed ^ 0x9E37_1234);
        for _ in 0..3000 {
            let inputs = Inputs([
                Input { target_x: rng.range(-1., 1.), target_z: rng.range(-1., 1.) },
                Input { target_x: rng.range(-1., 1.), target_z: rng.range(-1., 1.) },
            ]);
            sim.step(&inputs);
            assert!(sim.puck.vel.length() <= PUCK_MAX_SPEED + 1e-3, "seed {seed}: {}", sim.puck.vel.length());
        }
    }
}

#[test]
fn a_paddle_never_crosses_the_center_line_into_the_opponents_half() {
    let mut sim = Sim::new(201);
    let inputs = Inputs([Input { target_x: 1., target_z: 1. }, Input { target_x: 1., target_z: 1. }]);
    for _ in 0..1000 {
        sim.step(&inputs);
        assert!(sim.paddles[0].pos.2 <= 1e-6, "paddle 0 crossed into the opponent's half: {}", sim.paddles[0].pos.2);
        assert!(sim.paddles[1].pos.2 >= -1e-6, "paddle 1 crossed into the opponent's half: {}", sim.paddles[1].pos.2);
    }
}

#[test]
fn a_goal_is_detected_the_exact_tick_the_puck_crosses_the_goal_line() {
    let mut sim = Sim::new(202);
    sim.phase = Phase::Playing;
    sim.puck = Puck { pos: V(0., 0., -(TABLE_HALF_LENGTH - 0.15)), vel: V(0., 0., -20.) };
    let before = sim.puck.pos.2;
    let goal_line = TABLE_HALF_LENGTH + PUCK_RADIUS;
    let mut scored_tick = None;
    for t in 1..=5u32 {
        sim.step(&Inputs::default());
        if sim.score[1] == 1 {
            scored_tick = Some(t);
            break;
        }
    }
    assert!(before > -goal_line, "sanity: the puck started inside the table");
    assert_eq!(scored_tick, Some(1), "the goal fires on the very first tick the puck crosses the line");
    assert!(matches!(sim.phase, Phase::Serve { server: 0, .. }));
}

#[test]
fn scoring_outside_the_goal_mouth_bounces_off_the_end_wall_instead() {
    let mut sim = Sim::new(203);
    sim.phase = Phase::Playing;
    let wide_x = GOAL_HALF_WIDTH + PADDLE_RADIUS;
    sim.puck = Puck { pos: V(wide_x, 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
    for _ in 0..3 {
        sim.step(&Inputs::default());
    }
    assert_eq!(sim.score, [0, 0], "a shot wide of the goal mouth must not score");
    assert!(sim.puck.pos.2 <= TABLE_HALF_LENGTH - PUCK_RADIUS + 1e-4);
    assert!(sim.puck.vel.2 < 0., "it bounced back off the solid part of the end wall");
}

#[test]
fn the_conceding_player_serves_next() {
    let mut sim = Sim::new(204);
    sim.phase = Phase::Playing;
    sim.puck = Puck { pos: V(0., 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
    for _ in 0..5 {
        sim.step(&Inputs::default());
        if sim.score[0] == 1 {
            break;
        }
    }
    assert_eq!(sim.score[0], 1, "player 0 scored on player 1's goal");
    assert!(
        matches!(sim.phase, Phase::Serve { server: 1, .. }),
        "player 1, who conceded, serves next: {:?}",
        sim.phase
    );
}

#[test]
fn the_match_ends_the_instant_a_player_reaches_win_goals() {
    let mut sim = Sim::new(205);
    sim.score = [WIN_GOALS - 1, 3];
    sim.phase = Phase::Playing;
    sim.puck = Puck { pos: V(0., 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
    let mut events = Vec::new();
    for _ in 0..5 {
        sim.step(&Inputs::default());
        events.extend(sim.drain_events());
        if sim.is_over() {
            break;
        }
    }
    assert!(sim.is_over());
    assert!(matches!(sim.phase, Phase::GameOver { winner: 0 }));
    assert!(events.contains(&Event::GameOver { winner: 0 }));
}

#[test]
fn a_leading_player_wins_immediately_at_the_time_limit() {
    let mut sim = Sim::new(206);
    sim.phase = Phase::Playing;
    sim.score = [4, 1];
    sim.tick = MATCH_TIME_LIMIT_TICKS - 1;
    sim.step(&Inputs::default());
    assert!(sim.is_over());
    assert!(matches!(sim.phase, Phase::GameOver { winner: 0 }));
}

#[test]
fn a_tied_match_continues_past_the_time_limit_in_sudden_death() {
    let mut sim = Sim::new(207);
    sim.phase = Phase::Playing;
    sim.score = [3, 3];
    sim.tick = MATCH_TIME_LIMIT_TICKS - 1;
    sim.step(&Inputs::default());
    assert!(!sim.is_over(), "a tie does not end the match at the time limit");
    assert!(sim.sudden_death(), "sudden death begins once the time limit passes while tied");
    sim.puck = Puck { pos: V(0., 0., TABLE_HALF_LENGTH - 0.2), vel: V(0., 0., 20.) };
    for _ in 0..5 {
        sim.step(&Inputs::default());
        if sim.is_over() {
            break;
        }
    }
    assert!(sim.is_over(), "any goal at all ends a sudden-death match");
    assert_eq!(sim.score, [4, 3], "neither player had reached WIN_GOALS");
}

#[test]
fn releasing_a_participant_hands_their_paddle_to_the_bot_ai() {
    let mut sim = Sim::new(208);
    sim.paddles[0].ai = true;
    sim.phase = Phase::Playing;
    sim.puck = Puck { pos: V(0.3, 0., -0.8), vel: V(0., 0., 0.) };
    let before = sim.paddles[0].pos;
    for _ in 0..30 {
        sim.step(&Inputs::default()); // no human input for slot 0 at all
    }
    assert_ne!(sim.paddles[0].pos, before, "a released paddle still moves, driven by the bot AI");
}
