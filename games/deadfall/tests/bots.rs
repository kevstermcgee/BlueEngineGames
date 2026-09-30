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
