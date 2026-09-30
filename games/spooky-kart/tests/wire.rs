//! The wire format: every message round-trips, the biggest snapshot fits its budget, and no malformed
//! datagram can panic the decoder (the server decodes whatever the internet sends it).
use spooky_kart::kart::{Driver, KartInput};
use spooky_kart::sim::{Event, Hazard, HazardKind, Phase};
use spooky_kart::wire::*;
use spooky_kart::{Character, Perk, Sim, ALL};
use vesper3d::math::V;

fn token() -> Token {
    [0x1122_3344_5566_7788, 0x99aa_bbcc_ddee_ff00]
}

fn snapshot_of(sim: &Sim, events: Vec<(u32, Event)>, hazards: usize) -> Snapshot {
    Snapshot {
        server_tick: 777,
        applied_seq: 770,
        your_kart: 3,
        phase: Phase::Racing,
        race_tick: 500,
        first_finish: Some(490),
        finished_count: 2,
        karts: sim
            .karts
            .iter()
            .map(|k| {
                // A bot's private skill and the server's statistics are not sent.
                let mut kart = k.clone();
                kart.skill = 1.;
                kart.stats = Default::default();
                KartWire { character: k.character.index() as u8, human: k.driver == Driver::Human, kart }
            })
            .collect(),
        hazards: (0..hazards)
            .map(|i| Hazard {
                pos: V(i as f32, 0., -(i as f32)),
                radius: 2.4,
                ttl: 300,
                owner: i % 8,
                kind: HazardKind::Bandage,
            })
            .collect(),
        events,
    }
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

#[test]
fn every_client_message_round_trips() {
    let msgs = vec![
        ClientMsg::Hello {
            key: "hunter2".into(),
            name: "Kevin".into(),
            character: 3,
            nonce: [7, 9],
            fingerprint: 0xdead_beef,
        },
        ClientMsg::Select { token: token(), character: 5 },
        ClientMsg::Ready { token: token(), ready: true },
        ClientMsg::Input {
            token: token(),
            ack_tick: 1234,
            frames: vec![
                (10, KartInput { throttle: 1., steer: -1., drift: true, perk: false }),
                (11, KartInput { throttle: -1., steer: 0., drift: false, perk: true }),
            ],
        },
        ClientMsg::Ping { token: token(), stamp: 42, rtt_ms: 31 },
        ClientMsg::Leave { token: token() },
    ];
    for m in msgs {
        assert_eq!(ClientMsg::decode(&m.encode()).unwrap(), m);
    }
}

#[test]
fn analog_input_survives_the_wire_to_within_a_hair() {
    let sent = KartInput { throttle: 0.5, steer: -0.33, drift: false, perk: false };
    let m = ClientMsg::Input { token: token(), ack_tick: 0, frames: vec![(1, sent)] };
    let ClientMsg::Input { frames, .. } = ClientMsg::decode(&m.encode()).unwrap() else { panic!() };
    assert!((frames[0].1.throttle - 0.5).abs() < 0.01 && (frames[0].1.steer + 0.33).abs() < 0.01);
}

#[test]
fn every_server_message_round_trips() {
    let sim = Sim::new(3);
    let msgs = vec![
        ServerMsg::Welcome { token: token(), slot: 4, fingerprint: 99 },
        ServerMsg::Rejected { reason: "Server is full".into() },
        ServerMsg::Lobby(LobbyState {
            stage: 0,
            seconds_left: 5,
            racers: 8,
            entries: vec![
                LobbyEntry { slot: 0, character: 1, ready: true, name: "Ann".into() },
                LobbyEntry { slot: 3, character: 6, ready: false, name: "Bo".into() },
            ],
        }),
        ServerMsg::Snapshot(snapshot_of(&sim, every_event(), 3)),
        ServerMsg::Pong { stamp: 5 },
    ];
    for m in msgs {
        assert_eq!(ServerMsg::decode(&m.encode()).unwrap(), m);
    }
}

#[test]
fn the_largest_snapshot_fits_the_datagram_budget() {
    let sim = Sim::new(3);
    let worst_events: Vec<(u32, Event)> =
        (0..MAX_EVENTS as u32).map(|t| (t, Event::LapDone { kart: 7, lap: 2, ticks: 99999 })).collect();
    let bytes = ServerMsg::Snapshot(snapshot_of(&sim, worst_events, MAX_HAZARDS)).encode();
    assert!(bytes.len() <= MAX_DATAGRAM, "a full snapshot is {} bytes, budget {MAX_DATAGRAM}", bytes.len());
    println!("worst-case snapshot: {} of {MAX_DATAGRAM} bytes", bytes.len());
    let typical = ServerMsg::Snapshot(snapshot_of(&sim, vec![], 2)).encode();
    println!("typical snapshot: {} bytes", typical.len());
    assert!(typical.len() < 700);
}

#[test]
fn a_snapshot_rebuilds_karts_that_can_keep_driving() {
    let mut sim = Sim::new(3);
    for _ in 0..400 {
        sim.step(&Default::default());
    }
    let s = snapshot_of(&sim, vec![], 0);
    let ServerMsg::Snapshot(back) = ServerMsg::decode(&ServerMsg::Snapshot(s).encode()).unwrap() else { panic!() };
    for (a, b) in sim.karts.iter().zip(&back.karts) {
        assert_eq!(a.character, b.kart.character);
        assert_eq!(a.pos, b.kart.pos);
        assert_eq!(a.vel, b.kart.vel);
        assert_eq!(a.progress, b.kart.progress);
        assert_eq!(a.lap, b.kart.lap);
        assert_eq!(a.cooldown, b.kart.cooldown);
    }
}

#[test]
fn truncating_any_valid_datagram_is_an_error_not_a_panic() {
    let sim = Sim::new(3);
    let samples: Vec<Vec<u8>> = vec![
        ServerMsg::Snapshot(snapshot_of(&sim, every_event(), 4)).encode(),
        ServerMsg::Lobby(LobbyState {
            stage: 1,
            seconds_left: 0,
            racers: 8,
            entries: vec![LobbyEntry { slot: 0, character: 0, ready: true, name: "x".into() }],
        })
        .encode(),
        ClientMsg::Input { token: token(), ack_tick: 1, frames: vec![(1, KartInput::default()); 4] }.encode(),
        ClientMsg::Hello { key: "k".into(), name: "n".into(), character: 0, nonce: [1, 2], fingerprint: 1 }.encode(),
    ];
    for bytes in samples {
        for len in 0..bytes.len() {
            let cut = &bytes[..len];
            let _ = ServerMsg::decode(cut);
            let _ = ClientMsg::decode(cut);
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(
            ServerMsg::decode(&longer).is_err() && ClientMsg::decode(&longer).is_err(),
            "trailing bytes are refused"
        );
    }
}

#[test]
fn random_garbage_never_panics_the_decoder() {
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..20_000 {
        let len = (next() % 200) as usize;
        let mut bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        // Half the time make it look like ours, so the decoder gets past the header.
        if next() % 2 == 0 && bytes.len() >= 4 {
            bytes[..2].copy_from_slice(&MAGIC);
            bytes[2] = PROTOCOL;
            bytes[3] = [1, 2, 3, 4, 5, 6, 101, 102, 103, 104, 105][(next() % 11) as usize];
        }
        let _ = ClientMsg::decode(&bytes);
        let _ = ServerMsg::decode(&bytes);
    }
}

#[test]
fn foreign_wrong_version_and_oversized_datagrams_are_refused() {
    let good = ClientMsg::Ping { token: token(), stamp: 1, rtt_ms: 0 }.encode();
    let mut wrong_magic = good.clone();
    wrong_magic[0] = b'X';
    let mut wrong_version = good.clone();
    wrong_version[2] = PROTOCOL + 1;
    assert!(ClientMsg::decode(&wrong_magic).is_err());
    assert!(ClientMsg::decode(&wrong_version).is_err());
    assert!(ClientMsg::decode(&vec![0u8; MAX_DATAGRAM + 1]).is_err());
    assert!(ClientMsg::decode(&[]).is_err());
}

#[test]
fn non_finite_numbers_and_out_of_range_karts_are_refused() {
    let sim = Sim::new(3);
    let bytes = ServerMsg::Snapshot(snapshot_of(&sim, vec![], 0)).encode();
    // Poison the first kart's x position (after the header, ids and snapshot fixed fields).
    let mut nan = bytes.clone();
    let at = 4 + 4 + 4 + 1 + 1 + 4 + 4 + 4 + 4 + 1 + 2;
    nan[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(ServerMsg::decode(&nan).is_err());
    let mut far = bytes.clone();
    far[at..at + 4].copy_from_slice(&1.0e9f32.to_le_bytes());
    assert!(ServerMsg::decode(&far).is_err());
    assert!(ServerMsg::decode(&bytes).is_ok());
    let _ = (ALL, Character::Ghost);
}
