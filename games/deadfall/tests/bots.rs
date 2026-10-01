//! Twelve bots in a match by themselves: they find each other, fight, die, come back and keep the score.
use deadfall::sim::{EndRule, Match, Settings};
use std::time::Instant;
use vesper3d::math::V;

#[test]
fn twelve_bots_fight_a_full_minute_without_standing_around() {
    let settings = Settings { bots: true, end: EndRule::Kills { target: 999 }, bot_skill: 1 };
    let (mut m, slots) = Match::new(11, &[], settings);
    assert!(slots.is_empty());
    assert_eq!(m.players.len(), 12);
    let start: Vec<V> = m.players.iter().map(|p| p.feet()).collect();
    let mut travelled = vec![0f32; 12];
    let mut last = start.clone();
    let clock = Instant::now();
    let inputs = vec![None; 12];
    for _ in 0..60 * 120 {
        m.step(&inputs);
        for (i, p) in m.players.iter().enumerate() {
            travelled[i] += (p.feet() - last[i]).length().min(5.);
            last[i] = p.feet();
        }
        m.events.clear();
    }
    let per_tick = clock.elapsed().as_secs_f64() / (60. * 120.);
    let kills: u32 = m.players.iter().map(|p| p.kills as u32).sum();
    let deaths: u32 = m.players.iter().map(|p| p.deaths as u32).sum();
    println!("two minutes: {kills} kills, {deaths} deaths, scores {:?}, {:.3} ms per tick", m.scores, per_tick * 1000.);
    for (i, d) in travelled.iter().enumerate() {
        assert!(*d > 40., "bot {i} barely moved: {d} m in two minutes");
    }
    assert!(kills >= 5, "bots should kill each other: {kills}");
    assert_eq!(m.scores[0] + m.scores[1], kills as u16);
    assert!(per_tick < 0.004, "a tick took {:.2} ms", per_tick * 1000.);
    assert!(m.players.iter().any(|p| p.inv.primary.is_some()), "somebody picked up a weapon");
}

#[test]
fn bots_collect_weapons_and_survive_a_long_soak_on_several_seeds() {
    for seed in 1..=4u64 {
        let settings = Settings { bots: true, end: EndRule::Time { minutes: 30 }, bot_skill: (seed % 3) as u8 };
        let (mut m, _) = Match::new(seed, &[], settings);
        let inputs = vec![None; 12];
        let mut armed = [false; 12];
        let mut grenades = 0;
        for _ in 0..60 * 240 {
            m.step(&inputs);
            for p in &m.players {
                armed[p.slot] |= p.inv.primary.is_some();
            }
            grenades += m.events.iter().filter(|e| matches!(e, deadfall::sim::Event::Throw { .. })).count();
            m.events.clear();
        }
        let kills: u32 = m.players.iter().map(|p| p.kills as u32).sum();
        assert!(kills >= 20, "seed {seed}: only {kills} kills in four minutes");
        assert!(armed.iter().filter(|a| **a).count() >= 8, "seed {seed}: bots should pick up primaries, {armed:?}");
        println!("seed {seed}: {kills} kills, {} armed, {grenades} grenades thrown", armed.iter().filter(|a| **a).count());
        for p in &m.players {
            assert!(p.ctrl.position.1 > -5. && p.ctrl.position.0.abs() < 70. && p.ctrl.position.2.abs() < 50., "bot left the map: {:?}", p.ctrl.position);
        }
    }
}

#[test]
fn a_bot_with_a_grenade_throws_it_at_an_enemy_in_range() {
    use deadfall::sim::{world_for, Event};
    let world = world_for(deadfall::level::placeholder());
    let (mut m, _) = Match::new_in(world, 5, &[], Settings { bots: true, end: EndRule::Kills { target: 999 }, bot_skill: 2 });
    // Only bot 0 (Ironclad) and bot 6 (Nightwatch) matter: park everyone else far away behind the crate and freeze them.
    for p in &mut m.players {
        p.protect_until = u32::MAX;
    }
    let frag = deadfall::weapons::id_of("frag").unwrap();
    m.players[0].inv.grenades = [frag, 0];
    m.teleport(0, V(-20., 0., 22.), 0., 0.);
    m.teleport(6, V(-20., 0., 8.), 3.14, 0.);
    for i in 1..12 {
        if i != 6 {
            m.teleport(i, V(25. + i as f32, 0., 25.), 0., 0.);
        }
    }
    let inputs = vec![None; 12];
    let mut thrown = false;
    for _ in 0..60 * 90 {
        m.step(&inputs);
        thrown |= m.events.iter().any(|e| matches!(e, Event::Throw { thrower: 0, .. }));
        m.events.clear();
        if thrown {
            break;
        }
    }
    assert!(thrown, "the bot never threw its grenade");
}
