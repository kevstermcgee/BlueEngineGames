//! The rules are a pure library, so they are tested without a window: same seed and inputs must give the
//! same run, and save/load goes through the public `Simulation`/`Snapshot` API exactly as the window uses it.
use dead_air::{Event, Input, Outcome, Sim, SHIFT_LENGTH};
use vesper3d::viewer::devkit::{
    assert_deterministic, snapshot, Playback, SaveError, SaveSlots, Simulation, QUICK_SLOT, TICK,
};

/// A scripted "operator": walks forward through the corridor, weaving and glancing side to side.
fn scripted() -> Vec<Input> {
    (0..600)
        .map(|t| Input {
            forward: 1.,
            right: if (t / 90) % 2 == 0 { 0.3 } else { -0.3 },
            sneak: t % 200 < 40,
            look: [if t % 120 < 30 { 0.01 } else { 0. }, 0.],
            flashlight_toggle: t == 10,
            fire: false,
            interact: false,
        })
        .collect()
}

#[test]
fn the_same_seed_and_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(7), &scripted());
}

#[test]
fn recorded_inputs_round_trip_through_json_playback() {
    let json = serde_json::to_vec(&scripted()).unwrap();
    let mut play = Playback::<Input>::from_json(&json).unwrap();
    assert_eq!(play.len(), 600);
    assert_eq!(play.next_input(), scripted()[0]);
}

#[test]
fn running_out_the_clock_ends_the_shift_in_defeat_through_the_public_api() {
    let mut sim = Sim::new(3);
    let idle = Input::default();
    // A generous margin past the ideal tick count: summing TICK (not exactly representable in f32)
    // thousands of times drifts a little, so the exact crossing tick is not worth pinning down here.
    let ticks = (SHIFT_LENGTH / TICK) as u32 + 300;
    for _ in 0..ticks {
        sim.step(&idle);
    }
    assert_eq!(sim.outcome, Some(Outcome::RanOutOfTime));
    let tick = sim.tick;
    sim.step(&idle);
    assert_eq!(sim.tick, tick, "a finished shift ignores further input");
}

#[test]
fn a_save_from_any_tick_resumes_as_the_game_promises_in_a_brand_new_game() {
    // `Sim::POLICY` decides what this demands: `Exact` means a resumed run is bit-identical to the
    // uninterrupted one (docs/SAVE_STATE.md, "Which contract a physics game can keep").
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &scripted(), 25);
}

#[test]
fn a_bad_save_is_refused_and_the_running_game_is_left_alone() {
    let mut sim = Sim::new(11);
    for input in scripted().iter().take(100) {
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
    assert_eq!(sim.tick, 100);
}

#[test]
fn quick_save_and_quick_load_go_through_a_slot_with_a_backup() {
    let dir = std::env::temp_dir().join(format!("dead_air-slots-{}", std::process::id()));
    let slots = SaveSlots::new(&dir);
    let mut sim = Sim::new(13);
    assert!(matches!(snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT), Err(SaveError::NotFound(_))));
    for input in scripted().iter().take(60) {
        sim.step(input);
    }
    snapshot::save_to_slot(&sim, &slots, QUICK_SLOT, "first").unwrap();
    let saved = sim.state_hash();
    for input in scripted().iter().take(60) {
        sim.step(input);
    }
    let (header, _) = snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT).unwrap();
    assert_eq!((header.label.as_str(), sim.state_hash()), ("first", saved));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn flashlight_toggles_and_events_flow_through_the_public_step_api() {
    let mut sim = Sim::new(1);
    let mut saw_on = false;
    for input in scripted() {
        sim.step(&input);
        if sim.drain_events().contains(&Event::FlashlightOn) {
            saw_on = true;
        }
    }
    assert!(saw_on, "the scripted run toggles the flashlight on at tick 10");
}
