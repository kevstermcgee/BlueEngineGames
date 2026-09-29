//! Pulse Nova determinism and gameplay rule tests.
//!
//! Pure simulation tests: same seed and inputs must replay identically bit-for-bit.
use vesper3d::math::V;
use vesper3d::viewer::devkit::{
    assert_deterministic, run_inputs, snapshot, Playback, SaveError, SaveSlots, Simulation, QUICK_SLOT,
};
use pulse_nova::{Event, Input, Sim};

/// Scripted player input: runs around, aims, fires pulse blaster, leaps and dashes.
fn scripted() -> Vec<Input> {
    (0..480)
        .map(|t| Input {
            forward: if t % 180 < 120 { 1. } else { -0.5 },
            right: if (t / 60) % 2 == 0 { 0.5 } else { -0.5 },
            look: [if t % 90 < 45 { 0.015 } else { -0.01 }, if t % 120 < 60 { 0.005 } else { -0.005 }],
            jump: t % 80 == 40,
            fire: t % 20 == 0,
            dash: t % 120 == 60,
        })
        .collect()
}

#[test]
fn the_same_seed_and_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(7), &scripted());
}

#[test]
fn different_seeds_play_differently() {
    let a = run_inputs(&mut Sim::new(1), &scripted());
    let b = run_inputs(&mut Sim::new(2), &scripted());
    assert_ne!(a.first_divergence(&b), None, "enemy spawns diverge with different seeds");
}

#[test]
fn recorded_inputs_round_trip_through_json_playback() {
    let json = serde_json::to_vec(&scripted()).unwrap();
    let mut play = Playback::<Input>::from_json(&json).unwrap();
    assert_eq!(play.len(), 480);
    assert_eq!(play.next_input(), scripted()[0]);
}

#[test]
fn firing_destroys_enemy_and_triggers_chain_reaction() {
    let mut sim = Sim::new(42);
    // Wait until wave 1 spawns
    for _ in 0..35 {
        sim.step(&Input::default());
    }
    assert!(!sim.enemies.is_empty(), "enemies should have spawned");

    // Line up an enemy right in front of the player at center height (y = 0.8)
    sim.player.yaw = 0.;
    sim.player.pitch = 0.;
    let p_pos = sim.player.position;
    sim.enemies[0].pos = V(p_pos.0, 0.8, p_pos.2 - 3.0);
    sim.enemies[0].health = 20.0; // 1 shot kill

    // Fire
    let fire_input = Input { fire: true, ..Default::default() };
    sim.step(&fire_input);

    // Step a few ticks for bullet travel
    for _ in 0..10 {
        sim.step(&Input::default());
    }

    let events = sim.drain_events();
    assert!(
        events.iter().any(|e| matches!(e, Event::EnemyHit { fatal: true, .. })),
        "enemy should be hit fatally"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::EnemyExploded { .. })),
        "fatal hit should trigger explosion"
    );
    assert!(sim.score > 0, "score should increase");
    assert!(sim.combo >= 1, "combo should increment");
}

#[test]
fn jump_pad_launches_player_skyward() {
    let mut sim = Sim::new(99);
    let pad_pos = sim.jump_pads[0].pos;
    sim.player.position = V(pad_pos.0, pad_pos.1 + 0.1, pad_pos.2);
    sim.step(&Input::default());

    let events = sim.drain_events();
    assert!(
        events.iter().any(|e| matches!(e, Event::JumpPadUsed { .. })),
        "jump pad should trigger event"
    );
    assert!(!sim.player.is_grounded(), "player should leave the ground");
}

#[test]
fn a_save_from_any_tick_resumes_exactly_in_a_brand_new_game() {
    snapshot::assert_resumes_exactly(|| Sim::new(7), &scripted(), 30);
    snapshot::assert_resumes_exactly(|| Sim::new(19), &scripted(), 15);
}

#[test]
fn a_bad_save_is_refused_and_the_running_game_is_left_alone() {
    let mut sim = Sim::new(11);
    for input in scripted().iter().take(80) {
        sim.step(input);
    }
    let bytes = snapshot::save(&sim, "good").unwrap();
    for input in scripted().iter().take(40) {
        sim.step(input);
    }
    let before = sim.state_hash();
    let mut damaged = bytes.clone();
    let mid = damaged.len() / 2;
    damaged[mid] ^= 0x20;
    assert!(snapshot::restore(&mut sim, &damaged).is_err(), "flipped bit is rejected");
    assert_eq!(sim.state_hash(), before, "refused load does not mutate running state");
    snapshot::restore(&mut sim, &bytes).unwrap();
    assert_eq!(sim.tick, 80);
}

#[test]
fn quick_save_and_quick_load_go_through_a_slot_with_a_backup() {
    let dir = std::env::temp_dir().join(format!("pulse-nova-slots-{}", std::process::id()));
    let slots = SaveSlots::new(&dir);
    let mut sim = Sim::new(13);
    assert!(matches!(snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT), Err(SaveError::NotFound(_))));
    for input in scripted().iter().take(50) {
        sim.step(input);
    }
    snapshot::save_to_slot(&sim, &slots, QUICK_SLOT, "checkpoint").unwrap();
    let saved = sim.state_hash();
    for input in scripted().iter().take(50) {
        sim.step(input);
    }
    let (header, _) = snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT).unwrap();
    assert_eq!((header.label.as_str(), sim.state_hash()), ("checkpoint", saved));
    let _ = std::fs::remove_dir_all(&dir);
}
