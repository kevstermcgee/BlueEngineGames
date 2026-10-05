//! The rules are a pure library, so they are tested without a window: same seed and inputs must give the
//! same run, and save/load goes through the public `Simulation`/`Snapshot` API exactly as the window and
//! `netgame.rs` use it.
use prop_hunt::{
    DisguiseKind, Event, Input, Inputs, Outcome, Phase, Sim, HIDE_PHASE_TICKS, INSPECT_HOLD_TICKS, MAX_HIDERS,
    SEEKER_SLOT, SEEK_PHASE_TICKS,
};
use vesper3d::math::V;
use vesper3d::viewer::devkit::{
    assert_deterministic, snapshot, Playback, SaveError, SaveSlots, Simulation, QUICK_SLOT,
};

/// A scripted hider in seat 0: walks toward the house, then spends the rest of the hide phase cycling
/// disguises and trying to confirm, so a real save can land mid-cycle.
fn scripted() -> Vec<Inputs> {
    (0..HIDE_PHASE_TICKS as usize + 600)
        .map(|t| {
            let mut inputs = Inputs::default();
            inputs.0[0] = Input {
                forward: if t < 400 { 1. } else { 0. },
                right: if (t / 90) % 2 == 0 { 0.2 } else { -0.2 },
                look: [if t % 120 < 30 { 0.01 } else { 0. }, 0.],
                choose_disguise: Some((t % DisguiseKind::ALL.len()) as u8),
                confirm_placement: t % 50 == 0,
                ..Default::default()
            };
            inputs.0[SEEKER_SLOT] = Input { forward: 1., right: 0., ..Default::default() };
            inputs
        })
        .collect()
}

#[test]
fn the_same_seed_and_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(7), &scripted());
}

#[test]
fn recorded_inputs_round_trip_through_json_playback() {
    let script = scripted();
    let json = serde_json::to_vec(&script).unwrap();
    let mut play = Playback::<Inputs>::from_json(&json).unwrap();
    assert_eq!(play.len(), script.len());
    assert_eq!(play.next_input(), script[0]);
}

#[test]
fn hide_phase_auto_placement_never_leaves_a_hider_undisguised_or_unconfirmed() {
    let mut sim = Sim::new(42);
    for _ in 0..=HIDE_PHASE_TICKS {
        sim.step(&Inputs::default());
    }
    assert!(matches!(sim.phase, Phase::Seeking(_)));
    for hider in &sim.hiders {
        assert!(hider.confirmed, "every hider is confirmed once the hide phase ends");
        assert!(hider.disguise.is_some(), "every hider has a disguise once the hide phase ends");
    }
}

#[test]
fn confirmed_hiders_never_overlap_a_wall_collider() {
    // Every legal spot is within CONFIRM_RADIUS of itself (trivially) and, by construction
    // (`layout::tests::every_disguise_spot_is_clear_of_every_wall`), clear of every wall; running many
    // seeds of auto-placement is a cheap extra check that the chosen spots are always actually used.
    let colliders = prop_hunt::layout::colliders();
    for seed in 0..20u64 {
        let mut sim = Sim::new(seed);
        for _ in 0..=HIDE_PHASE_TICKS {
            sim.step(&Inputs::default());
        }
        for hider in &sim.hiders {
            let pos = hider.controller.position;
            assert!(
                !colliders.iter().any(|c| c.overlaps_xz(V(pos.0, 0., pos.2), 0.05)),
                "seed {seed}: a confirmed hider overlaps a wall at {pos:?}"
            );
        }
    }
}

fn seeking_round(seed: u64) -> Sim {
    let mut sim = Sim::new(seed);
    for _ in 0..=HIDE_PHASE_TICKS {
        sim.step(&Inputs::default());
    }
    assert!(matches!(sim.phase, Phase::Seeking(_)));
    sim
}

fn stand_seeker_on(sim: &mut Sim, hider: usize) {
    let pos = sim.hiders[hider].controller.position;
    sim.seeker =
        vesper3d::viewer::controller::Controller::for_profile(Default::default(), V(pos.0, 0., pos.2), 0.).unwrap();
}

fn seeker_input(inspect: bool) -> Inputs {
    let mut inputs = Inputs::default();
    inputs.0[SEEKER_SLOT] = Input { inspect, ..Default::default() };
    inputs
}

#[test]
fn inspect_hold_only_tags_after_continuous_proximity_and_resets_on_walk_away() {
    let mut sim = seeking_round(3);
    stand_seeker_on(&mut sim, 0);
    for _ in 0..(INSPECT_HOLD_TICKS - 1) {
        sim.step(&seeker_input(true));
    }
    assert!(!sim.hiders[0].tagged);
    sim.step(&seeker_input(false)); // walk away / release inspect
    assert_eq!(sim.inspect_progress, 0, "progress resets, it does not pause");
    let mut events = Vec::new();
    for _ in 0..INSPECT_HOLD_TICKS {
        sim.step(&seeker_input(true));
        events.extend(sim.drain_events());
    }
    assert!(sim.hiders[0].tagged);
    assert!(events.contains(&Event::Tagged { hider: 0 }));
}

#[test]
fn win_conditions_fire_at_the_exact_tick() {
    // Hiders win at the instant the clock reaches zero, never a tick early or late.
    let mut sim = seeking_round(5);
    for _ in 0..SEEK_PHASE_TICKS {
        sim.step(&Inputs::default());
        assert!(sim.outcome.is_none());
    }
    assert_eq!(sim.phase, Phase::Seeking(0));
    let events = {
        sim.step(&Inputs::default());
        sim.drain_events()
    };
    assert_eq!(sim.outcome, Some(Outcome::HidersWin));
    assert!(events.contains(&Event::RoundOver(Outcome::HidersWin)));

    // The seeker wins the instant the last hider is tagged, however much time is left.
    let mut sim = seeking_round(6);
    for i in 1..MAX_HIDERS {
        sim.hiders[i].tagged = true;
    }
    stand_seeker_on(&mut sim, 0);
    let mut tagged_tick = None;
    for t in 0..INSPECT_HOLD_TICKS {
        sim.step(&seeker_input(true));
        if sim.outcome.is_some() {
            tagged_tick = Some(t);
            break;
        }
    }
    assert_eq!(sim.outcome, Some(Outcome::SeekerWins));
    assert_eq!(tagged_tick, Some(INSPECT_HOLD_TICKS - 1), "tags on the exact hold-complete tick");
}

#[test]
fn a_finished_round_ignores_further_input() {
    let mut sim = seeking_round(9);
    for hider in &mut sim.hiders {
        hider.tagged = true;
    }
    sim.step(&Inputs::default());
    assert_eq!(sim.phase, Phase::RoundOver);
    let tick = sim.tick;
    let mut inputs = Inputs::default();
    inputs.0[SEEKER_SLOT] = Input { forward: 1., ..Default::default() };
    sim.step(&inputs);
    assert_eq!(sim.tick, tick, "a finished round does not advance");
}

#[test]
fn a_save_from_any_tick_resumes_as_the_game_promises_in_a_brand_new_round() {
    // `Sim::POLICY` decides what this demands: `Exact` means a resumed round is bit-identical to the
    // uninterrupted one (docs/SAVE_STATE.md, "Which contract a physics game can keep"). No rigid bodies
    // are involved, so this is the strongest and simplest contract.
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &scripted(), 30);
}

#[test]
fn a_bad_save_is_refused_and_the_running_round_is_left_alone() {
    let mut sim = Sim::new(11);
    let script = scripted();
    for input in script.iter().take(100) {
        sim.step(input);
    }
    let bytes = snapshot::save(&sim, "good").unwrap();
    for input in script.iter().take(50) {
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
    let dir = std::env::temp_dir().join(format!("prop-hunt-slots-{}", std::process::id()));
    let slots = SaveSlots::new(&dir);
    let mut sim = Sim::new(13);
    assert!(matches!(snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT), Err(SaveError::NotFound(_))));
    let script = scripted();
    for input in script.iter().take(60) {
        sim.step(input);
    }
    snapshot::save_to_slot(&sim, &slots, QUICK_SLOT, "first").unwrap();
    let saved = sim.state_hash();
    for input in script.iter().take(60) {
        sim.step(input);
    }
    let (header, _) = snapshot::load_from_slot(&mut sim, &slots, QUICK_SLOT).unwrap();
    assert_eq!((header.label.as_str(), sim.state_hash()), ("first", saved));
    let _ = std::fs::remove_dir_all(&dir);
}
