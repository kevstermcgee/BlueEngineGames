//! The agent env is a layer over the race, so these tests prove it adds nothing (a race through the env is
//! the race the game plays), that trajectories replay exactly, and that the observation is what it claims.
use spooky_kart::bot::drive_as;
use spooky_kart::character::ALL;
use spooky_kart::env::{validate, Action, Env, EnvConfig, EnvError, Event, Obs, OBS_LEN};
use spooky_kart::policy::{self, Lookahead, ObsOnly, ReferenceBot};
use spooky_kart::recorder::{replay, Line, Recorder, RecorderOptions, Trajectory};
use spooky_kart::sim::COUNTDOWN_TICKS;
use spooky_kart::{Difficulty, Driver, Inputs, KartInput, Sim};

fn config(seed: u64, slots: &[usize], difficulty: Difficulty) -> EnvConfig {
    EnvConfig { seed, agent_slots: slots.to_vec(), difficulty, ..EnvConfig::default() }
}

/// The race exactly as the game and the other tests run it, with no env anywhere: agent slots are humans
/// whose input is the reference bot's, the countdown is stepped idle.
fn plain_race(seed: u64, slots: &[usize], difficulty: Difficulty, bot: Difficulty) -> Sim {
    let grid: Vec<_> = ALL
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, if slots.contains(&i) { Driver::Human } else { Driver::Bot }))
        .collect();
    let mut sim = Sim::with_difficulty(seed, &grid, difficulty);
    for _ in 0..COUNTDOWN_TICKS {
        sim.step(&Inputs::default());
    }
    while !sim.is_over() {
        let mut inputs = Inputs::default();
        for &s in slots {
            inputs.0[s] = drive_as(&sim, s, bot);
        }
        sim.step(&inputs);
    }
    sim
}

fn run_reference(cfg: EnvConfig, bot: Difficulty) -> Env {
    let mut env = Env::new(cfg).unwrap();
    let mut policy = ReferenceBot { difficulty: bot };
    policy::rollout(&mut env, &mut policy, |_, _, _, _| {}).unwrap();
    env
}

#[test]
fn a_race_through_the_env_is_the_race_the_game_plays() {
    use spooky_kart::Difficulty::*;
    use vesper3d::viewer::devkit::Simulation;
    for (seed, slots, rivals) in [(3u64, vec![5usize], Medium), (9, vec![0], Hard), (4, vec![1, 6], Easy)] {
        let env = run_reference(config(seed, &slots, rivals), Medium);
        let plain = plain_race(seed, &slots, rivals, Medium);
        assert!(env.sim().is_over() && plain.is_over());
        assert_eq!(env.sim().tick, plain.tick, "seed {seed}: different length");
        assert_eq!(env.state_hash(), plain.state_hash(), "seed {seed}: the env changed the race");
        assert_eq!(env.report(), plain.report(), "seed {seed}: the RaceReport differs");
    }
}

#[test]
fn the_same_seed_and_actions_give_the_same_episode_and_reset_repeats_it() {
    let cfg = config(7, &[2], Difficulty::Medium);
    let play = |env: &mut Env| {
        let mut policy = policy::by_name("random", Difficulty::Medium, 5).unwrap();
        let mut obs = env.observe();
        let mut trace = Vec::new();
        for _ in 0..800 {
            let a = policy.act(env, &obs);
            let r = env.step(&a).unwrap();
            trace.push((r.obs.iter().map(Obs::to_vec).collect::<Vec<_>>(), r.reward.clone(), env.state_hash()));
            obs = r.obs;
        }
        trace
    };
    let mut a = Env::new(cfg.clone()).unwrap();
    let first = a.observe()[0].to_vec();
    let ta = play(&mut a);
    let tb = play(&mut Env::new(cfg).unwrap());
    assert_eq!(ta, tb, "two envs diverged");
    assert_eq!(a.reset(7)[0].to_vec(), first, "reset(seed) did not restore the first observation");
    assert_eq!(play(&mut a), ta, "reset(seed) did not replay the same episode");
    let seven = a.state_hash();
    a.reset(8);
    assert_ne!(a.state_hash(), seven, "a different seed must be a different race");
}

#[test]
fn a_recorded_trajectory_replays_to_the_same_hashes_and_report() {
    for (policy_name, frame_skip, options) in [
        ("bot", 1, RecorderOptions { hash_every: 1, record_obs: true }),
        ("bot", 4, RecorderOptions { hash_every: 7, record_obs: false }),
        ("lookahead", 2, RecorderOptions { hash_every: 1, record_obs: true }),
    ] {
        let cfg = EnvConfig { frame_skip, ..config(12, &[3], Difficulty::Medium) };
        let mut env = Env::new(cfg.clone()).unwrap();
        let mut policy = policy::by_name(policy_name, Difficulty::Medium, 12).unwrap();
        let mut buffer = Vec::new();
        let mut recorder = Recorder::new(&mut buffer, &env, options).unwrap();
        let mut last = None;
        let rollout = policy::rollout(&mut env, policy.as_mut(), |env, obs, actions, step| {
            recorder.record(env, obs, actions, step).unwrap();
            last = Some(step.clone());
        })
        .unwrap();
        recorder.finish(&env, &last.unwrap()).unwrap();
        assert!(rollout.terminated);

        let trajectory = Trajectory::read(buffer.as_slice()).unwrap();
        assert_eq!(trajectory.header.content_fingerprint, spooky_kart::content_fingerprint());
        assert_eq!(trajectory.header.seed, 12);
        assert_eq!(trajectory.header.agent_slots, vec![3]);
        assert_eq!(trajectory.header.obs_layout.iter().map(|l| l.1).sum::<usize>(), OBS_LEN);
        assert_eq!(trajectory.header.action_layout.iter().map(|l| l.1).sum::<usize>(), Action::LEN);
        assert!(trajectory.header.determinism.contains("per machine"));
        assert_eq!(trajectory.steps.len() as u64, rollout.steps);
        let footer = trajectory.footer.as_ref().unwrap();
        assert_eq!(footer.state_hash, rollout.state_hash);
        assert_eq!(footer.report, env.report());
        assert_eq!(trajectory.steps[0].obs.is_some(), options.record_obs);

        let result = replay(&trajectory);
        assert!(result.ok(), "{policy_name} frame_skip {frame_skip}: {:?}", result.first_divergence);
        assert_eq!(result.steps_replayed as u64, rollout.steps);
        let expected_hashes =
            (1..=rollout.steps).filter(|s| s % options.hash_every as u64 == 0 || *s == rollout.steps).count();
        assert_eq!(result.hashes_checked, expected_hashes);
        assert_eq!(result.obs_checked, if options.record_obs { trajectory.steps.len() } else { 0 });
    }
}

fn record_bot_race(seed: u64) -> Vec<u8> {
    let mut env = Env::new(config(seed, &[4], Difficulty::Medium)).unwrap();
    let mut policy = ReferenceBot { difficulty: Difficulty::Medium };
    let mut buffer = Vec::new();
    let mut recorder = Recorder::new(&mut buffer, &env, RecorderOptions::default()).unwrap();
    let mut last = None;
    policy::rollout(&mut env, &mut policy, |env, obs, actions, step| {
        recorder.record(env, obs, actions, step).unwrap();
        last = Some(step.clone());
    })
    .unwrap();
    recorder.finish(&env, &last.unwrap()).unwrap();
    buffer
}

fn rewrite(buffer: &[u8], edit: impl Fn(usize, &mut Line)) -> Trajectory {
    let text = String::from_utf8(buffer.to_vec()).unwrap();
    let mut out = String::new();
    for (n, line) in text.lines().enumerate() {
        let mut parsed: Line = serde_json::from_str(line).unwrap();
        edit(n, &mut parsed);
        out.push_str(&serde_json::to_string(&parsed).unwrap());
        out.push('\n');
    }
    Trajectory::read(out.as_bytes()).unwrap()
}

#[test]
fn a_replay_names_the_first_step_where_a_trajectory_stops_matching() {
    let buffer = record_bot_race(5);
    // Change one recorded action: the hash after that very step must differ.
    let tampered = rewrite(&buffer, |n, line| {
        if let (101, Line::Step(s)) = (n, line) {
            s.action[0][1] = if s.action[0][1] > 0. { -1. } else { 1. };
            s.action[0][0] = 1.;
            s.obs = None;
        }
    });
    let result = replay(&tampered);
    let d = result.first_divergence.clone().expect("a tampered action must diverge");
    assert_eq!((d.step, d.what), (100, "hash"), "{d}");
    assert!(!result.ok());

    // A recorded observation that is not what the env sees.
    let tampered = rewrite(&buffer, |n, line| {
        if let (51, Line::Step(s)) = (n, line) {
            s.obs.as_mut().unwrap()[0][0] += 0.5;
        }
    });
    let d = replay(&tampered).first_divergence.unwrap();
    assert_eq!((d.step, d.what), (50, "obs"), "{d}");

    // A footer that does not match.
    let tampered = rewrite(&buffer, |_, line| {
        if let Line::Footer(f) = line {
            f.state_hash ^= 1;
        }
    });
    let result = replay(&tampered);
    assert_eq!(result.first_divergence.unwrap().what, "footer_hash");

    // Content that is not this build's: refused before running anything.
    let tampered = rewrite(&buffer, |_, line| {
        if let Line::Header(h) = line {
            h.content_fingerprint ^= 1;
        }
    });
    let result = replay(&tampered);
    assert_eq!(result.first_divergence.unwrap().what, "fingerprint");
    assert_eq!(result.steps_replayed, 0);

    // Not a trajectory at all.
    assert!(Trajectory::read("not json\n".as_bytes()).is_err());
    assert!(Trajectory::read("".as_bytes()).is_err());
}

#[test]
fn decimal_text_returns_every_f32_action_bit_for_bit() {
    // The replay promise rests on actions surviving JSON; scan a lot of floats, including awkward ones.
    let mut x: u64 = 0x1234_5678_9abc_def1;
    let mut bad = 0;
    for i in 0..400_000u32 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let v = if i % 4 == 0 {
            f32::from_bits((x >> 32) as u32)
        } else {
            ((x >> 40) as f32 / (1u64 << 24) as f32) * 2. - 1.
        };
        if !v.is_finite() {
            continue;
        }
        let back: f32 = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        if back.to_bits() != v.to_bits() {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "{bad} floats changed through JSON");
}

#[test]
fn validate_passes_for_several_configurations() {
    for cfg in [
        EnvConfig::default(),
        config(5, &[5], Difficulty::Hard),
        EnvConfig { frame_skip: 3, ..config(2, &[2, 6], Difficulty::Easy) },
    ] {
        let report = validate(&cfg);
        assert!(report.passed(), "{report}");
        assert_eq!(report.checks.len(), 9);
    }
}

#[test]
fn the_obs_only_baseline_finishes_close_to_the_privileged_bot() {
    for (seed, slot) in [(1u64, 0usize), (2, 4), (3, 7)] {
        let finish = |name: &str| {
            let mut env = Env::new(config(seed, &[slot], Difficulty::Medium)).unwrap();
            let mut p = policy::by_name(name, Difficulty::Medium, seed).unwrap();
            assert_eq!(p.privileged(), name == "bot");
            let r = policy::rollout(&mut env, p.as_mut(), |_, _, _, _| {}).unwrap();
            assert!(r.terminated && !r.truncated, "{name} seed {seed}");
            (r.finish_seconds[0].expect("it finished"), r.places[0], r.total_reward[0])
        };
        let (bot, _, bot_reward) = finish("bot");
        let (look, place, look_reward) = finish("lookahead");
        assert!((look - bot).abs() < 6., "seed {seed}: the obs-only baseline took {look:.1} s, the bot {bot:.1} s");
        assert!((1..=8).contains(&place));
        assert!(look_reward > 15. && bot_reward > 15., "rewards {look_reward} {bot_reward}");
    }
    // The baseline is a type that can only see an Obs.
    let _only_sees_obs: ObsOnly<Lookahead> = ObsOnly(Lookahead::default());
}

#[test]
fn an_idle_or_random_agent_is_last_and_the_race_still_ends() {
    for name in ["idle", "random"] {
        let mut env = Env::new(config(6, &[3], Difficulty::Medium)).unwrap();
        let mut p = policy::by_name(name, Difficulty::Medium, 6).unwrap();
        let r = policy::rollout(&mut env, p.as_mut(), |_, _, _, _| {}).unwrap();
        assert!(r.terminated, "{name}: the race should end by the game's own rule");
        assert_eq!(r.places, vec![8]);
        assert!(r.finish_seconds[0].is_none());
        assert!(r.total_reward[0].is_finite());
    }
}

#[test]
fn stepping_after_the_end_or_with_bad_actions_is_refused() {
    let mut env = Env::new(EnvConfig { max_ticks: 10, ..EnvConfig::default() }).unwrap();
    let idle = [KartInput::default()];
    assert!(matches!(env.step(&[]), Err(EnvError::ActionCount { expected: 1, got: 0 })));
    let nan = [KartInput { steer: f32::NAN, ..Default::default() }];
    assert!(matches!(env.step(&nan), Err(EnvError::NonFinite { agent: 0, field: "steer" })));
    assert_eq!(env.steps(), 0, "a refused action must not advance the race");
    for _ in 0..10 {
        env.step(&idle).unwrap();
    }
    assert!(env.is_done());
    assert!(matches!(env.step(&idle), Err(EnvError::EpisodeOver)));
    env.reset(1);
    env.step(&idle).unwrap();
}

#[test]
fn events_are_reported_and_a_perk_press_happens_once_per_step() {
    // The Mummy's bandage trail (slot 2) on the grid: press the perk with frame_skip 5 and see one use.
    let cfg = EnvConfig { frame_skip: 5, ..config(1, &[2], Difficulty::Medium) };
    let mut env = Env::new(cfg).unwrap();
    let press = [KartInput { throttle: 1., perk: true, ..Default::default() }];
    let r = env.step(&press).unwrap();
    let uses = r.events.iter().filter(|e| matches!(e, Event::PerkUsed { kart: 2, .. })).count();
    assert_eq!(uses, 1, "{:?}", r.events);
    assert_eq!(r.info.ticks, 5);
    assert!(r.obs[0].own.perk_cooldown > 0.9 && !r.obs[0].own.perk_ready);
    // A whole race: the agent gets exactly one Finished event, and the step it comes in pays the bonus.
    let mut env = Env::new(config(8, &[1], Difficulty::Medium)).unwrap();
    let mut bot = ReferenceBot { difficulty: Difficulty::Medium };
    let (mut finishes, mut bonus_step) = (0, 0.);
    policy::rollout(&mut env, &mut bot, |_, _, _, step| {
        if step.events.iter().any(|e| matches!(e, Event::Finished { kart: 1, .. })) {
            finishes += 1;
            bonus_step = step.reward[0];
        }
    })
    .unwrap();
    assert_eq!(finishes, 1);
    assert!(bonus_step >= 2., "the finishing step paid {bonus_step}");
    assert!(env.info().finished[0]);
    env.reset(8);
    assert!(!env.info().finished[0]);
}

/// Put the agent's kart `lateral` metres right of the centreline `s` metres along, turned `turn` radians
/// right of the road direction, at 15 m/s. Mirrors what the race tests do to stage a kart.
fn stage(env: &mut Env, s: f32, lateral: f32, turn: f32) {
    use spooky_kart::track::{yaw_of, Track};
    use vesper3d::math::V;
    let track = Track::haunted_hollow();
    let tangent = track.tangent_at(s);
    let right = V(-tangent.2, 0., tangent.0);
    let slot = env.agent_slots()[0];
    let k = &mut env.sim_mut().karts[slot];
    k.pos = track.point_at(s) + right * lateral;
    k.yaw = yaw_of(tangent) + turn;
    k.vel = spooky_kart::track::forward(k.yaw) * 15.;
    let near = track.nearest_global(k.pos);
    k.hint = near.index;
    k.s = near.s;
    k.lateral = near.lateral;
    k.progress = track.arc_delta(0., near.s);
    k.best_progress = k.progress;
}

#[test]
fn the_observation_signs_follow_one_convention_right_is_positive() {
    use spooky_kart::env::OBS_LAYOUT;
    let offset = |name: &str| OBS_LAYOUT.iter().take_while(|l| l.0 != name).map(|l| l.1).sum::<usize>();
    let mut env = Env::new(config(1, &[5], Difficulty::Medium)).unwrap();
    // Right of the centre and pointing right of the road: the road is to the kart's left.
    stage(&mut env, 300., 4., 0.3);
    let o = &env.observe()[0];
    assert!((o.own.lateral - 4.).abs() < 0.2, "lateral {}", o.own.lateral);
    assert!((o.own.heading_error - 0.3).abs() < 0.05, "heading error {}", o.own.heading_error);
    assert!(o.lookahead[0].bearing < -0.1, "the road ahead should be to the left, bearing {}", o.lookahead[0].bearing);
    let v = o.to_vec();
    assert!(v[offset("lateral")] > 0.4 && v[offset("heading_error")] > 0.05 && v[offset("lookahead_bearing")] < -0.03);
    assert!((v[offset("speed")] - 0.5).abs() < 0.02, "15 m/s is half of the speed scale");
    // The obs-only baseline steers left (negative) for it; mirrored, it steers right.
    let mut look = policy::Lookahead::default();
    use policy::ObsPolicy;
    assert!(look.act(o).steer < -0.2);
    stage(&mut env, 300., -4., -0.3);
    let o = &env.observe()[0];
    assert!(o.own.lateral < -3. && o.own.heading_error < -0.2 && look.act(o).steer > 0.2);
    // Off the road is flagged only after the kart's next update has seen it.
    stage(&mut env, 300., 0., 0.);
    assert!(!env.observe()[0].own.offroad);
    // Another kart straight ahead and to the right shows up with the right sign.
    let slot = env.agent_slots()[0];
    let other = if slot == 0 { 1 } else { 0 };
    stage(&mut env, 300., 0., 0.);
    let (me, tangent) = (env.sim().karts[slot].pos, env.sim().karts[slot].yaw);
    let ahead_right = me + spooky_kart::track::forward(tangent) * 12. + spooky_kart::track::right(tangent) * 4.;
    env.sim_mut().karts[other].pos = ahead_right;
    let seen = env.observe()[0].others[0].expect("the kart is within range");
    assert!((seen.distance - (12f32 * 12. + 16.).sqrt()).abs() < 0.01);
    assert!(seen.bearing > 0. && seen.bearing < 0.5, "bearing {}", seen.bearing);
}
