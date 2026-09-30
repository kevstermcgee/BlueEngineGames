//! The kart layouts on the wire: they round-trip, the biggest snapshot fits its datagram, and nothing
//! malformed can panic a decoder (the server decodes whatever the internet sends it). The envelope around them
//! (sessions, lobby, bundles) is tested in BlueEngine's `tests/netplay.rs`.
use spooky_kart::kart::{Driver, KartInput};
use spooky_kart::sim::{Event, Hazard, HazardKind, Phase};
use spooky_kart::{Character, KartGame, KartSnapshot, Perk, Sim};
use vesper3d::math::V;
use vesper3d::viewer::net::codec::{Reader, Writer};
use vesper3d::viewer::netplay::wire::{decode_server, encode_server, ServerMsg, SnapshotMsg, MAX_DATAGRAM};
use vesper3d::viewer::netplay::NetGame;

fn snapshot_of(sim: &Sim, hazards: usize) -> KartSnapshot {
    let mut s = KartGame::snapshot(sim, None);
    // A bot's private skill and the server's statistics are not sent.
    for k in &mut s.karts {
        k.skill = 1.;
        k.stats = Default::default();
    }
    s.hazards = (0..hazards)
        .map(|i| Hazard {
            pos: V(i as f32, 0., -(i as f32)),
            radius: 2.4,
            ttl: 300,
            owner: i % 8,
            kind: HazardKind::Bandage,
        })
        .collect();
    s
}

fn every_event() -> Vec<(u32, Event)> {
    vec![
        (1, Event::Count(3)),
        (2, Event::Go),
        (3, Event::LapDone { kart: 2, lap: 1, ticks: 3000 }),
        (4, Event::Finished { kart: 3, place: 1, ticks: 6000 }),
        (5, Event::DriftBoost { kart: 1, tier: 2 }),
        (6, Event::Bump { a: 0, b: 7, speed: 9.5 }),
        (7, Event::WallHit { kart: 4, speed: 12.25 }),
        (8, Event::PerkUsed { kart: 5, perk: Perk::Honk }),
        (9, Event::HazardHit { kart: 6, kind: HazardKind::Bone }),
        (10, Event::RaceOver),
    ]
}

fn encode(s: &KartSnapshot) -> Vec<u8> {
    let mut w = Writer::new();
    KartGame::write_snapshot(s, &mut w);
    w.finish()
}

#[test]
fn a_snapshot_round_trips_and_rebuilds_karts_that_can_keep_driving() {
    let mut sim = Sim::new(3);
    for _ in 0..400 {
        sim.step(&Default::default());
    }
    let s = snapshot_of(&sim, 3);
    let bytes = encode(&s);
    let mut r = Reader::new(&bytes);
    let back = KartGame::read_snapshot(&mut r).unwrap();
    r.done().unwrap();
    assert_eq!(back, s);
    for (a, b) in sim.karts.iter().zip(&back.karts) {
        assert_eq!((a.pos, a.vel, a.progress, a.lap, a.cooldown), (b.pos, b.vel, b.progress, b.lap, b.cooldown));
    }
}

#[test]
fn every_event_and_input_round_trips() {
    for (_, e) in every_event() {
        let mut w = Writer::new();
        KartGame::write_event(&e, &mut w);
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        assert_eq!(KartGame::read_event(&mut r).unwrap(), e);
        r.done().unwrap();
    }
    let sent = KartInput { throttle: 0.5, steer: -0.33, drift: true, perk: true };
    let mut w = Writer::new();
    KartGame::write_input(&sent, &mut w);
    let bytes = w.finish();
    let back = KartGame::read_input(&mut Reader::new(&bytes)).unwrap();
    assert!((back.throttle - 0.5).abs() < 0.01 && (back.steer + 0.33).abs() < 0.01 && back.drift && back.perk);
}

#[test]
fn the_largest_snapshot_fits_the_datagram_budget() {
    let sim = Sim::new(3);
    let worst_events: Vec<(u32, Event)> =
        (0..24).map(|t| (t, Event::LapDone { kart: 7, lap: 2, ticks: 99999 })).collect();
    let msg = |hazards, events| {
        ServerMsg::<KartSnapshot, Event>::Snapshot(SnapshotMsg {
            server_tick: 777,
            applied_seq: 770,
            you: 3,
            snapshot: snapshot_of(&sim, hazards),
            events,
        })
    };
    let worst = encode_server::<KartGame>(&msg(24, worst_events));
    println!("worst-case snapshot: {} of {MAX_DATAGRAM} bytes", worst.len());
    assert!(worst.len() <= MAX_DATAGRAM, "{} bytes", worst.len());
    let typical = encode_server::<KartGame>(&msg(2, vec![]));
    println!("typical snapshot: {} bytes", typical.len());
    assert!(typical.len() < 700);
    assert!(decode_server::<KartGame>(&typical).is_ok());
}

#[test]
fn truncated_and_garbage_snapshots_never_panic() {
    let sim = Sim::new(3);
    let bytes = encode(&snapshot_of(&sim, 4));
    for len in 0..bytes.len() {
        let _ = KartGame::read_snapshot(&mut Reader::new(&bytes[..len]));
    }
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..20_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let n = (x % 300) as usize;
        let junk: Vec<u8> = (0..n).map(|i| (x >> (i % 56)) as u8).collect();
        let _ = KartGame::read_snapshot(&mut Reader::new(&junk));
        let _ = KartGame::read_event(&mut Reader::new(&junk));
        let _ = KartGame::read_input(&mut Reader::new(&junk));
    }
}

#[test]
fn non_finite_numbers_and_out_of_range_karts_are_refused() {
    let sim = Sim::new(3);
    let bytes = encode(&snapshot_of(&sim, 0));
    // Header: phase (1 + 4), race tick, first finish, finished count (4 each), kart count (1), then the first
    // kart's character and human flag (2): its x position follows.
    let at = 5 + 4 + 4 + 4 + 1 + 2;
    let mut nan = bytes.clone();
    nan[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(KartGame::read_snapshot(&mut Reader::new(&nan)).is_err());
    let mut far = bytes.clone();
    far[at..at + 4].copy_from_slice(&1.0e9f32.to_le_bytes());
    assert!(KartGame::read_snapshot(&mut Reader::new(&far)).is_err());
    assert!(KartGame::read_snapshot(&mut Reader::new(&bytes)).is_ok());
    let _ = (Phase::Racing, Driver::Bot, Character::Ghost);
}
