//! Slapstick's wire layouts: every shape round-trips, the largest possible snapshot fits the datagram
//! budget, and nothing malformed can panic a decoder (the server decodes whatever the internet sends it).
use slapstick::{AirHockeyGame, AirHockeySnapshot, Event, Input, Phase};
use vesper3d::math::V;
use vesper3d::viewer::net::codec::{Reader, Writer};
use vesper3d::viewer::netplay::wire::{decode_server, encode_server, ServerMsg, SnapshotMsg, MAX_DATAGRAM};
use vesper3d::viewer::netplay::NetGame;

fn fixture_snapshot(phase: Phase) -> AirHockeySnapshot {
    AirHockeySnapshot {
        phase,
        paddles: [V(0.2, 0., -0.9), V(-0.15, 0., 1.0)],
        puck: (V(0.05, 0., 0.3), V(-3.5, 0., 7.2)),
        score: [4, 6],
    }
}

fn every_phase() -> Vec<Phase> {
    vec![Phase::Serve { ticks_left: 37, server: 1 }, Phase::Playing, Phase::GameOver { winner: 0 }]
}

fn every_event() -> Vec<Event> {
    vec![
        Event::Serve { server: 0 },
        Event::Serve { server: 1 },
        Event::Goal { scorer: 1, score: [3, 4] },
        Event::PaddleHit { paddle: 0, speed: 5.5 },
        Event::PaddleHit { paddle: 1, speed: 0.1 },
        Event::WallHit { speed: 2.25 },
        Event::GameOver { winner: 0 },
        Event::GameOver { winner: 1 },
    ]
}

#[test]
fn every_input_shape_round_trips() {
    for (target_x, target_z) in [(0.5, -0.33), (-1., 1.), (0., 0.), (1., -1.)] {
        let sent = Input { target_x, target_z };
        let mut w = Writer::new();
        AirHockeyGame::write_input(&sent, &mut w);
        let bytes = w.finish();
        let back = AirHockeyGame::read_input(&mut Reader::new(&bytes)).unwrap();
        assert!((back.target_x - target_x).abs() < 1e-5);
        assert!((back.target_z - target_z).abs() < 1e-5);
    }
}

#[test]
fn every_event_shape_round_trips() {
    for e in every_event() {
        let mut w = Writer::new();
        AirHockeyGame::write_event(&e, &mut w);
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        assert_eq!(AirHockeyGame::read_event(&mut r).unwrap(), e);
        r.done().unwrap();
    }
}

#[test]
fn every_snapshot_shape_round_trips() {
    for phase in every_phase() {
        let s = fixture_snapshot(phase);
        let mut w = Writer::new();
        AirHockeyGame::write_snapshot(&s, &mut w);
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        let back = AirHockeyGame::read_snapshot(&mut r).unwrap();
        r.done().unwrap();
        assert_eq!(back, s, "phase {phase:?} did not round-trip");
    }
}

#[test]
fn the_largest_possible_snapshot_fits_the_datagram_budget() {
    let snapshot = fixture_snapshot(Phase::Serve { ticks_left: u32::MAX, server: 1 });
    let worst_events: Vec<(u32, Event)> =
        (0..30).map(|t| (t, Event::PaddleHit { paddle: (t % 2) as usize, speed: 12.3 })).collect();
    let msg = ServerMsg::<AirHockeySnapshot, Event>::Snapshot(SnapshotMsg {
        server_tick: 999_999,
        applied_seq: 999_990,
        you: 0,
        snapshot,
        events: worst_events,
    });
    let worst = encode_server::<AirHockeyGame>(&msg);
    println!("worst-case snapshot: {} of {MAX_DATAGRAM} bytes", worst.len());
    assert!(worst.len() <= MAX_DATAGRAM, "{} bytes", worst.len());
    let typical = encode_server::<AirHockeyGame>(&ServerMsg::Snapshot(SnapshotMsg {
        server_tick: 777,
        applied_seq: 770,
        you: 0,
        snapshot: fixture_snapshot(Phase::Playing),
        events: vec![],
    }));
    println!("typical snapshot: {} bytes", typical.len());
    assert!(typical.len() < 200, "{} bytes", typical.len());
    assert!(decode_server::<AirHockeyGame>(&typical).is_ok());
}

#[test]
fn truncated_and_garbage_messages_never_panic() {
    let s = fixture_snapshot(Phase::Playing);
    let mut w = Writer::new();
    AirHockeyGame::write_snapshot(&s, &mut w);
    let bytes = w.finish();
    for len in 0..bytes.len() {
        let _ = AirHockeyGame::read_snapshot(&mut Reader::new(&bytes[..len]));
    }
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    for _ in 0..20_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let n = (x % 300) as usize;
        let junk: Vec<u8> = (0..n).map(|i| (x >> (i % 56)) as u8).collect();
        let _ = AirHockeyGame::read_snapshot(&mut Reader::new(&junk));
        let _ = AirHockeyGame::read_event(&mut Reader::new(&junk));
        let _ = AirHockeyGame::read_input(&mut Reader::new(&junk));
    }
}

#[test]
fn non_finite_or_absurd_positions_are_refused() {
    let s = fixture_snapshot(Phase::Playing);
    let mut w = Writer::new();
    AirHockeyGame::write_snapshot(&s, &mut w);
    let bytes = w.finish();
    // Header: phase tag (1) + u32 ticks (4) + server/winner slot (1) = 6 bytes, then paddle 0's x.
    let at = 6;
    let mut nan = bytes.clone();
    nan[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(AirHockeyGame::read_snapshot(&mut Reader::new(&nan)).is_err());
    let mut far = bytes.clone();
    far[at..at + 4].copy_from_slice(&1.0e8f32.to_le_bytes());
    assert!(AirHockeyGame::read_snapshot(&mut Reader::new(&far)).is_err());
    assert!(AirHockeyGame::read_snapshot(&mut Reader::new(&bytes)).is_ok());
}

#[test]
fn an_out_of_range_server_or_winner_slot_is_refused() {
    // Hand-write just the phase header the real codec would: tag, ticks, slot. An out-of-range slot
    // must be refused before anything else is even read.
    for (tag, slot) in [(0u8, 2u8), (2u8, 5u8)] {
        let mut w = Writer::new();
        w.u8(tag);
        w.u32(7);
        w.u8(slot);
        assert!(AirHockeyGame::read_snapshot(&mut Reader::new(&w.finish())).is_err());
    }
}
