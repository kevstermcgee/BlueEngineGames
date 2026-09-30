//! The race is a pure library, so it is tested without a window: same seed and inputs give the same race,
//! and every rule is checked by driving `Sim` the way the window and the server do.
use spooky_kart::character::{ALL, MAX_RACERS};
use spooky_kart::sim::{COUNTDOWN_TICKS, LAPS};
use spooky_kart::track::{forward, HALF_WIDTH, SHOULDER};
use spooky_kart::{Character, Driver, Event, Inputs, KartInput, Perk, Phase, Sim, Track};
use vesper3d::math::V;
use vesper3d::viewer::devkit::{assert_deterministic, run_inputs, snapshot, Simulation};

fn idle() -> Inputs {
    Inputs::default()
}

fn gas() -> Inputs {
    let mut i = Inputs::default();
    i.0[0] = KartInput { throttle: 1., ..Default::default() };
    i
}

/// A scripted human in slot 0: gas, weaving, drifting now and then, honking the perk.
fn scripted() -> Vec<Inputs> {
    (0..1500)
        .map(|t| {
            let mut i = Inputs::default();
            i.0[0] = KartInput {
                throttle: 1.,
                steer: if (t / 120) % 2 == 0 { 0.2 } else { -0.2 },
                drift: t % 400 > 300,
                perk: t % 500 == 499,
            };
            i
        })
        .collect()
}

/// Skip the countdown.
fn started(seed: u64, grid: &[(Character, Driver)]) -> Sim {
    let mut sim = Sim::with_grid(seed, grid);
    for _ in 0..COUNTDOWN_TICKS {
        sim.step(&idle());
    }
    assert!(sim.racing());
    sim.drain_events();
    sim
}

/// Put kart `slot` on the centreline `s` metres from the line, facing along the road, at `speed`.
fn place(sim: &mut Sim, slot: usize, s: f32, lateral: f32, speed: f32) {
    let track = Track::haunted_hollow();
    let tangent = track.tangent_at(s);
    let right = V(-tangent.2, 0., tangent.0);
    let k = &mut sim.karts[slot];
    k.pos = track.point_at(s) + right * lateral;
    k.yaw = spooky_kart::track::yaw_of(tangent);
    k.vel = tangent * speed;
    let near = track.nearest_global(k.pos);
    k.hint = near.index;
    k.s = near.s;
    k.lateral = near.lateral;
    k.progress = track.arc_delta(0., near.s);
    k.best_progress = k.progress;
}

fn two(a: Character, b: Character) -> [(Character, Driver); 2] {
    [(a, Driver::Human), (b, Driver::Human)]
}

#[test]
fn the_track_is_a_closed_loop_with_room_between_its_parts() {
    let track = Track::haunted_hollow();
    assert!((900. ..1700.).contains(&track.length), "a lap is {} m", track.length);
    let samples = track.samples();
    // Two parts of the road far apart along it must be far apart in space, or the walls would overlap.
    let clear = 2. * (HALF_WIDTH + SHOULDER) + 2.;
    let mut closest = f32::MAX;
    for (i, a) in samples.iter().enumerate() {
        for (j, b) in samples.iter().enumerate().skip(i + 1) {
            let arc = (j - i) as f32 / samples.len() as f32 * track.length;
            let arc = arc.min(track.length - arc);
            if arc > 80. {
                closest = closest.min((*a - *b).length());
            }
        }
    }
    assert!(closest >= clear, "two stretches of road come within {closest:.1} m; need {clear:.1}");
}

#[test]
fn the_grid_sits_on_the_road_behind_the_line_facing_forward() {
    let sim = Sim::new(1);
    assert_eq!(sim.karts.len(), MAX_RACERS);
    for k in &sim.karts {
        assert!(k.lateral.abs() < HALF_WIDTH, "{} starts on the grass", k.character.name());
        assert!(k.progress < 0. && k.progress > -60., "{} progress {}", k.character.name(), k.progress);
        let along = forward(k.yaw).dot(sim.track().tangent_at(k.s));
        assert!(along > 0.99, "{} faces the wrong way", k.character.name());
    }
    let mut spots: Vec<(i32, i32)> = sim.karts.iter().map(|k| ((k.pos.0 * 2.) as i32, (k.pos.2 * 2.) as i32)).collect();
    spots.sort();
    spots.dedup();
    assert_eq!(spots.len(), MAX_RACERS, "two karts share a spot");
}

#[test]
fn the_same_seed_and_inputs_replay_identically() {
    assert_deterministic(|| Sim::new(7), &scripted());
}

#[test]
fn different_seeds_play_differently() {
    let a = run_inputs(&mut Sim::new(1), &scripted());
    let b = run_inputs(&mut Sim::new(2), &scripted());
    assert_eq!(a.first_divergence(&b), Some(0), "bot skill is drawn from the seed");
}

#[test]
fn the_countdown_holds_the_karts_then_the_lights_go_green() {
    let mut sim = Sim::new(3);
    let mut events = Vec::new();
    for t in 0..COUNTDOWN_TICKS {
        sim.step(&gas());
        events.extend(sim.drain_events());
        if t + 1 < COUNTDOWN_TICKS {
            assert!(sim.karts[0].vel.length() < 0.01, "moved during the countdown");
        }
    }
    let counts: Vec<u32> =
        events.iter().filter_map(|e| if let Event::Count(n) = e { Some(*n) } else { None }).collect();
    assert_eq!(counts, vec![3, 2, 1]);
    assert!(events.contains(&Event::Go));
    assert_eq!(sim.phase, Phase::Racing);
    for _ in 0..120 {
        sim.step(&gas());
    }
    assert!(sim.karts[0].forward_speed() > 10., "gas moves the kart once racing");
}

#[test]
fn steering_turns_only_when_moving_and_grass_slows() {
    let mut sim = started(4, &two(Character::Mummy, Character::Mummy));
    let yaw = sim.karts[0].yaw;
    let mut turn = idle();
    turn.0[0].steer = 1.;
    for _ in 0..30 {
        sim.step(&turn);
    }
    assert!((sim.karts[0].yaw - yaw).abs() < 1e-3, "a stopped kart cannot steer");
    // On the road versus on the grass, same gas, same time.
    let mut sim = started(4, &two(Character::Mummy, Character::Mummy));
    place(&mut sim, 0, 100., 0., 0.);
    place(&mut sim, 1, 100., HALF_WIDTH + 2.5, 0.);
    let mut both = idle();
    both.0[0].throttle = 1.;
    both.0[1].throttle = 1.;
    for _ in 0..300 {
        sim.step(&both);
    }
    assert!(sim.karts[1].stats.offroad_ticks > 0, "the grass was noticed");
    assert!(sim.karts[0].best_progress - sim.karts[0].progress < 1.);
    assert!(sim.karts[0].progress > sim.karts[1].progress, "grass is slower than road");
}

#[test]
fn the_wall_stops_a_kart_and_reports_the_hit() {
    let mut sim = started(5, &two(Character::Frankenstein, Character::Mummy));
    place(&mut sim, 0, 100., HALF_WIDTH + 2., 0.);
    // Face straight out of the road.
    let out = V(-sim.track().tangent_at(100.).2, 0., sim.track().tangent_at(100.).0);
    sim.karts[0].yaw = spooky_kart::track::yaw_of(out);
    sim.karts[0].vel = out * 20.;
    let mut hit = false;
    for _ in 0..120 {
        sim.step(&gas());
        hit |= sim.drain_events().iter().any(|e| matches!(e, Event::WallHit { kart: 0, .. }));
        let k = &sim.karts[0];
        let distance = (k.pos - sim.track().nearest_global(k.pos).center).length();
        assert!(distance <= sim.track().wall() + 0.01, "left the world: {distance}");
    }
    assert!(hit, "hitting the wall at speed is an event");
}

#[test]
fn drifting_through_a_corner_then_letting_go_pays_a_boost() {
    let mut sim = started(6, &two(Character::Vampire, Character::Ghost));
    place(&mut sim, 0, 100., 0., 25.);
    let mut drift = idle();
    drift.0[0] = KartInput { throttle: 1., steer: 0.6, drift: true, perk: false };
    let mut boosted = false;
    for _ in 0..200 {
        sim.step(&drift);
        // Stay on the road so the test is about the drift, not the wall.
        place_lateral_reset(&mut sim);
    }
    assert!(sim.karts[0].drifting, "the drift held");
    let mut release = idle();
    release.0[0] = KartInput { throttle: 1., steer: 0.6, drift: false, perk: false };
    sim.step(&release);
    for e in sim.drain_events() {
        if let Event::DriftBoost { kart: 0, tier } = e {
            boosted = tier >= 1;
        }
    }
    assert!(boosted, "a long drift pays a boost");
    assert!(sim.karts[0].boost_ticks > 0);
}

/// Keep kart 0 on the road and at speed during a drift test.
fn place_lateral_reset(sim: &mut Sim) {
    let k = &mut sim.karts[0];
    if k.lateral.abs() > 6. {
        let near = Track::haunted_hollow().nearest_global(k.pos);
        k.pos = near.center;
        k.lateral = 0.;
    }
    if k.vel.length() < 20. {
        k.vel = forward(k.yaw) * 22.;
    }
}

#[test]
fn a_bandage_trail_slows_everyone_but_its_owner() {
    let mut sim = started(7, &two(Character::Mummy, Character::Clown));
    place(&mut sim, 0, 200., 0., 20.);
    place(&mut sim, 1, 175., 0., 0.);
    let mut drop = idle();
    drop.0[0].perk = true;
    sim.step(&drop);
    assert_eq!(sim.hazards.len(), 4);
    assert!(sim.drain_events().iter().any(|e| matches!(e, Event::PerkUsed { kart: 0, perk: Perk::BandageTrail })));
    assert!(sim.karts[0].cooldown > 0);
    // Park the Clown on the strip.
    let strip = sim.hazards[1].pos;
    sim.karts[1].pos = strip;
    sim.step(&idle());
    assert!(sim.karts[1].slow_ticks > 0, "the bandage slows a kart on it");
    assert_eq!(sim.karts[0].slow_ticks, 0, "its owner is unaffected");
    // The perk needs time to recharge.
    sim.step(&drop);
    assert_eq!(sim.hazards.len(), 4, "still cooling down");
}

#[test]
fn bones_spin_out_a_kart_once_and_the_skeleton_shrugs_them_off() {
    let grid =
        [(Character::Skeleton, Driver::Human), (Character::Clown, Driver::Human), (Character::Ghost, Driver::Human)];
    let mut sim = started(8, &grid);
    place(&mut sim, 0, 200., 0., 20.);
    place(&mut sim, 1, 150., 0., 0.);
    place(&mut sim, 2, 120., 0., 0.);
    let mut drop = idle();
    drop.0[0].perk = true;
    sim.step(&drop);
    let bone = sim.hazards[0].pos;
    sim.karts[1].pos = bone;
    sim.step(&idle());
    assert!(sim.karts[1].spin_ticks > 0, "a bone spins the Clown out");
    assert!(sim.hazards.iter().all(|h| h.pos != bone), "the bone is used up");
    // The Skeleton drives over its own second bone and a fresh one from anyone else without a scratch.
    sim.hazards.push(spooky_kart::Hazard {
        pos: sim.karts[0].pos,
        radius: 2.,
        ttl: 100,
        owner: 1,
        kind: spooky_kart::HazardKind::Bone,
    });
    sim.step(&idle());
    assert_eq!(sim.karts[0].spin_ticks, 0, "the Skeleton is immune");
}

#[test]
fn a_ghost_slips_through_karts_hazards_and_the_grass() {
    let mut sim = started(9, &two(Character::Ghost, Character::Frankenstein));
    place(&mut sim, 0, 200., 0., 0.);
    place(&mut sim, 1, 200., 0.5, 0.);
    let mut solid = sim.karts[0].pos;
    sim.step(&idle());
    assert!((sim.karts[0].pos - solid).length() > 0.05, "two karts on one spot push apart");
    place(&mut sim, 0, 200., 0., 0.);
    place(&mut sim, 1, 200., 0.5, 0.);
    let mut phase = idle();
    phase.0[0].perk = true;
    sim.step(&phase);
    assert!(sim.karts[0].phased());
    solid = sim.karts[0].pos;
    sim.step(&idle());
    assert!((sim.karts[0].pos - solid).length() < 1e-3, "a phased Ghost is not pushed");
    sim.karts[0].lateral = HALF_WIDTH + 3.;
    sim.step(&idle());
    assert!(!sim.karts[0].offroad, "no grass penalty while phased");
}

#[test]
fn crows_slow_the_nearest_kart_ahead_and_only_that_one() {
    let grid =
        [(Character::Scarecrow, Driver::Human), (Character::Clown, Driver::Human), (Character::Zombie, Driver::Human)];
    let mut sim = started(10, &grid);
    place(&mut sim, 0, 200., 0., 20.);
    place(&mut sim, 1, 215., 0., 20.); // 15 m ahead
    place(&mut sim, 2, 190., 0., 20.); // behind
    let mut caw = idle();
    caw.0[0].perk = true;
    sim.step(&caw);
    assert!(sim.karts[1].slow_ticks > 0, "the kart ahead is slowed");
    assert_eq!(sim.karts[2].slow_ticks, 0, "a kart behind is left alone");
}

#[test]
fn a_honk_shoves_close_karts_but_not_far_ones() {
    let grid =
        [(Character::Clown, Driver::Human), (Character::Mummy, Driver::Human), (Character::Skeleton, Driver::Human)];
    let mut sim = started(11, &grid);
    place(&mut sim, 0, 300., 0., 0.);
    place(&mut sim, 1, 300., 4., 0.);
    place(&mut sim, 2, 330., 0., 0.);
    let mut honk = idle();
    honk.0[0].perk = true;
    sim.step(&honk);
    assert!(sim.karts[1].vel.length() > 5., "a close kart is shoved");
    assert!(sim.karts[2].vel.length() < 0.5, "a far kart is not");
}

#[test]
fn the_monsters_ram_launches_and_slows_a_lighter_kart() {
    let mut sim = started(12, &two(Character::Frankenstein, Character::Skeleton));
    place(&mut sim, 0, 400., 0., 28.);
    place(&mut sim, 1, 400., 2.0, 0.);
    sim.karts[1].pos = sim.karts[0].pos + forward(sim.karts[0].yaw) * 2.0;
    sim.step(&gas());
    assert!(sim.karts[1].slow_ticks > 0, "the rammed kart is slowed");
    assert!(sim.karts[1].vel.length() > 10., "and launched");
    assert!(sim.karts[0].stats.collisions == 1 && sim.karts[1].stats.collisions == 1);
}

#[test]
fn completing_laps_is_counted_and_the_last_one_finishes_the_race() {
    let mut sim = started(13, &two(Character::Vampire, Character::Mummy));
    sim.karts[1].driver = Driver::Bot;
    let l = sim.track().length;
    let mut laps = Vec::new();
    for lap in 0..LAPS {
        place(&mut sim, 0, l - 8., 0., 22.);
        // make the kart's progress reflect the laps already run
        let base = (lap + 1) as f32 * l;
        sim.karts[0].progress += base;
        sim.karts[0].best_progress += base;
        sim.karts[0].lap = lap;
        for _ in 0..120 {
            sim.step(&gas());
            for e in sim.drain_events() {
                if let Event::LapDone { kart: 0, lap, .. } = e {
                    laps.push(lap);
                }
            }
            if !laps.is_empty() && laps.last() == Some(&(lap + 1)) {
                break;
            }
        }
    }
    assert_eq!(laps, (1..=LAPS).collect::<Vec<_>>());
    assert!(sim.karts[0].finished_tick.is_some());
    assert_eq!(sim.karts[0].place, 1);
}

#[test]
fn a_full_race_of_bots_finishes_with_every_place_taken_once() {
    let grid: Vec<_> = ALL.iter().map(|c| (*c, Driver::Bot)).collect();
    let mut sim = Sim::with_grid(21, &grid);
    let mut ticks = 0;
    while !sim.is_over() && ticks < 60 * 60 * 8 {
        sim.step(&idle());
        ticks += 1;
    }
    assert!(sim.is_over(), "the race never ended");
    let mut places: Vec<u32> = sim.karts.iter().map(|k| k.place).collect();
    places.sort();
    assert_eq!(places, (1..=MAX_RACERS as u32).collect::<Vec<_>>());
    let report = sim.report();
    assert!(report.race_seconds > 60. && report.race_seconds < 300., "a race took {:.0} s", report.race_seconds);
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("spooky-kart"));
}

/// Not an assertion of fairness, a measurement: who wins across seeds, printed so a human (or a later
/// session) can rebalance `character.rs`. Fails only if a kart is plainly broken.
#[test]
fn every_character_is_a_viable_racer_for_bots() {
    let races = 24u64;
    let mut wins = [0u32; MAX_RACERS];
    let mut place_sum = [0u32; MAX_RACERS];
    let mut finish_sum = [0f32; MAX_RACERS];
    let mut walls = [0u32; MAX_RACERS];
    for seed in 0..races {
        // Rotate the grid so no character always starts in front.
        let grid: Vec<_> = (0..MAX_RACERS).map(|i| (ALL[(i + seed as usize) % MAX_RACERS], Driver::Bot)).collect();
        let mut sim = Sim::with_grid(seed + 100, &grid);
        let mut ticks = 0;
        while !sim.is_over() && ticks < 60 * 60 * 8 {
            sim.step(&idle());
            ticks += 1;
        }
        assert!(sim.is_over());
        for r in sim.report().racers {
            let c = r.character.index();
            place_sum[c] += r.place;
            finish_sum[c] += r.finish_seconds.unwrap_or(600.);
            walls[c] += r.wall_hits;
            if r.place == 1 {
                wins[c] += 1;
            }
        }
    }
    println!("\n{:<24} {:>5} {:>10} {:>9} {:>10}", "character", "wins", "avg place", "avg time", "wall hits");
    for c in ALL {
        let i = c.index();
        println!(
            "{:<24} {:>5} {:>10.2} {:>8.1}s {:>10.1}",
            c.name(),
            wins[i],
            place_sum[i] as f32 / races as f32,
            finish_sum[i] / races as f32,
            walls[i] as f32 / races as f32
        );
    }
    for c in ALL {
        assert!(
            wins[c.index()] as u64 * 2 <= races,
            "{} wins {} of {races} races: retune character.rs",
            c.name(),
            wins[c.index()]
        );
    }
    let best = finish_sum.iter().cloned().fold(f32::MAX, f32::min) / races as f32;
    for c in ALL {
        let avg = finish_sum[c.index()] / races as f32;
        assert!(avg < best * 1.35, "{} is far too slow ({avg:.0} s against {best:.0} s)", c.name());
    }
}

#[test]
fn a_save_from_any_tick_resumes_exactly_in_a_brand_new_race() {
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &scripted(), 60);
}

#[test]
fn a_bad_save_is_refused_and_the_running_race_is_left_alone() {
    let mut sim = Sim::new(11);
    for input in scripted().iter().take(400) {
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
    assert_eq!(sim.tick, 400);
}
