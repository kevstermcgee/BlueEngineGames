//! Prop Hunt's wire layouts: they round-trip, the redaction table actually redacts (not just a shape
//! check — this is the property the whole game depends on), the largest snapshot fits its datagram, and
//! nothing malformed can panic a decoder (the server decodes whatever the internet sends it). The
//! envelope around them (sessions, lobby, bundles) is tested in BlueEngine's `tests/netplay.rs`.
use prop_hunt::{
    Event, Input, Inputs, Outcome, PropHuntGame, PropHuntSnapshot, Sim, HIDE_PHASE_TICKS, MAX_HIDERS, SEEKER_SLOT,
};
use vesper3d::math::V;
use vesper3d::viewer::net::codec::{Reader, Writer};
use vesper3d::viewer::netplay::wire::{decode_server, encode_server, ServerMsg, SnapshotMsg, MAX_DATAGRAM};
use vesper3d::viewer::netplay::NetGame;

/// A round with everybody auto-placed (so every hider has a real disguise and position to possibly leak)
/// and one hider hand-tagged, so both halves of the "already tagged -> visible" rule have a case.
fn fixture() -> Sim {
    let mut sim = Sim::new(99);
    for _ in 0..=HIDE_PHASE_TICKS {
        sim.step(&Inputs::default());
    }
    sim.hiders[2].tagged = true;
    sim.hiders[2].tagged_tick = Some(123);
    sim
}

/// Only x/z travel on the wire (every body stands on the flat floor at y = 0): a real `Sim` position has
/// to be flattened like this before comparing it against anything a snapshot carries.
fn flat(v: V) -> V {
    V(v.0, 0., v.2)
}

fn every_event() -> Vec<Event> {
    vec![
        Event::HidePhaseEnded,
        Event::SeekPhaseBegan,
        Event::InspectStarted,
        Event::Tagged { hider: 3 },
        Event::RoundOver(Outcome::HidersWin),
        Event::RoundOver(Outcome::SeekerWins),
    ]
}

#[test]
fn every_event_and_input_shape_round_trips() {
    for e in every_event() {
        let mut w = Writer::new();
        PropHuntGame::write_event(&e, &mut w);
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        assert_eq!(PropHuntGame::read_event(&mut r).unwrap(), e);
        r.done().unwrap();
    }
    let sent = Input {
        forward: 0.5,
        right: -0.33,
        sprint: true,
        look: [0.1, -0.2],
        choose_disguise: Some(3),
        confirm_placement: true,
        inspect: true,
    };
    let mut w = Writer::new();
    PropHuntGame::write_input(&sent, &mut w);
    let bytes = w.finish();
    let back = PropHuntGame::read_input(&mut Reader::new(&bytes)).unwrap();
    assert!((back.forward - 0.5).abs() < 0.01 && (back.right + 0.33).abs() < 0.01);
    assert!(back.sprint && back.confirm_placement && back.inspect);
    assert_eq!(back.choose_disguise, Some(3));
    assert!((back.look[0] - 0.1).abs() < 0.001 && (back.look[1] + 0.2).abs() < 0.001);

    let quiet = Input::default();
    let mut w = Writer::new();
    PropHuntGame::write_input(&quiet, &mut w);
    let back = PropHuntGame::read_input(&mut Reader::new(&w.finish())).unwrap();
    assert_eq!(back.choose_disguise, None);
    assert!(!back.sprint && !back.confirm_placement && !back.inspect);
}

#[test]
fn a_snapshot_round_trips_for_every_kind_of_viewer() {
    let sim = fixture();
    for participant in [None, Some(0usize), Some(2usize), Some(SEEKER_SLOT)] {
        let s = PropHuntGame::snapshot(&sim, participant);
        let mut w = Writer::new();
        PropHuntGame::write_snapshot(&s, &mut w);
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        let back = PropHuntGame::read_snapshot(&mut r).unwrap();
        r.done().unwrap();
        assert_eq!(back, s, "viewer {participant:?} did not round-trip");
    }
}

/// The single property this game's networking exists to prove: an untagged hider's own snapshot must
/// never carry another still-active hider's real position or disguise. Not a round-trip check — an
/// assertion against the *real* simulation state, so a redaction bug that round-trips consistently
/// (wrong on both ends alike) still fails this test.
#[test]
fn an_untagged_hiders_own_snapshot_never_contains_another_untagged_hiders_real_position_or_disguise() {
    let sim = fixture();
    assert!(!sim.hiders[0].tagged, "the fixture's hider 0 must be untagged for this test to mean anything");
    let seen = PropHuntGame::snapshot(&sim, Some(0));
    let mut found_a_real_other = false;
    for i in 0..MAX_HIDERS {
        if i == 0 || sim.hiders[i].tagged {
            continue; // self, and anyone already tagged, are allowed to be real per the redaction table
        }
        found_a_real_other |= sim.hiders[i].disguise.is_some();
        assert_eq!(seen.hiders[i].pos, V::ZERO, "hider {i}'s real position reached hider 0's snapshot");
        assert_eq!(seen.hiders[i].disguise, None, "hider {i}'s real disguise reached hider 0's snapshot");
        assert!(!seen.hiders[i].confirmed, "hider {i}'s confirmed flag reached hider 0's snapshot");
    }
    assert!(found_a_real_other, "the fixture has no other untagged, disguised hider: this test would be vacuous");
    // Hider 0 must still see itself, and the already-tagged hider 2, for real.
    assert_eq!(seen.hiders[0].pos, flat(sim.hiders[0].controller.position));
    assert_eq!(seen.hiders[0].disguise, sim.hiders[0].disguise);
    assert_eq!(seen.hiders[2].pos, flat(sim.hiders[2].controller.position));
    assert_eq!(seen.hiders[2].disguise, sim.hiders[2].disguise);
    assert!(seen.hiders[2].tagged);
}

#[test]
fn the_seeker_and_any_open_viewer_see_every_hiders_real_disguise_tagged_or_not() {
    let sim = fixture();
    for viewer in [Some(SEEKER_SLOT), Some(2usize), None] {
        let seen = PropHuntGame::snapshot(&sim, viewer);
        for i in 0..MAX_HIDERS {
            assert_eq!(seen.hiders[i].disguise, sim.hiders[i].disguise, "viewer {viewer:?}, hider {i}");
            assert_eq!(seen.hiders[i].pos, flat(sim.hiders[i].controller.position), "viewer {viewer:?}, hider {i}");
            assert_eq!(seen.hiders[i].tagged, sim.hiders[i].tagged, "viewer {viewer:?}, hider {i}");
        }
    }
}

#[test]
fn the_seeker_always_sees_their_own_real_position() {
    let sim = fixture();
    let seen = PropHuntGame::snapshot(&sim, Some(SEEKER_SLOT));
    assert_eq!(seen.seeker.pos, flat(sim.seeker.position));
}

#[test]
fn the_largest_possible_snapshot_fits_the_datagram_budget() {
    let sim = fixture();
    let worst_events: Vec<(u32, Event)> =
        (0..30).map(|t| (t, Event::Tagged { hider: t as usize % MAX_HIDERS })).collect();
    let msg = ServerMsg::<PropHuntSnapshot, Event>::Snapshot(SnapshotMsg {
        server_tick: 777,
        applied_seq: 770,
        you: 0,
        snapshot: PropHuntGame::snapshot(&sim, None), // the open view: every field populated, nothing blanked
        events: worst_events,
    });
    let worst = encode_server::<PropHuntGame>(&msg);
    println!("worst-case snapshot: {} of {MAX_DATAGRAM} bytes", worst.len());
    assert!(worst.len() <= MAX_DATAGRAM, "{} bytes", worst.len());
    let typical = encode_server::<PropHuntGame>(&ServerMsg::Snapshot(SnapshotMsg {
        server_tick: 777,
        applied_seq: 770,
        you: 0,
        snapshot: PropHuntGame::snapshot(&sim, Some(0)),
        events: vec![],
    }));
    println!("typical snapshot: {} bytes", typical.len());
    assert!(typical.len() < 400);
    assert!(decode_server::<PropHuntGame>(&typical).is_ok());
}

#[test]
fn truncated_and_garbage_messages_never_panic() {
    let sim = fixture();
    let s = PropHuntGame::snapshot(&sim, Some(0));
    let mut w = Writer::new();
    PropHuntGame::write_snapshot(&s, &mut w);
    let bytes = w.finish();
    for len in 0..bytes.len() {
        let _ = PropHuntGame::read_snapshot(&mut Reader::new(&bytes[..len]));
    }
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..20_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let n = (x % 300) as usize;
        let junk: Vec<u8> = (0..n).map(|i| (x >> (i % 56)) as u8).collect();
        let _ = PropHuntGame::read_snapshot(&mut Reader::new(&junk));
        let _ = PropHuntGame::read_event(&mut Reader::new(&junk));
        let _ = PropHuntGame::read_input(&mut Reader::new(&junk));
    }
}

#[test]
fn non_finite_or_absurd_positions_are_refused() {
    let sim = fixture();
    let s = PropHuntGame::snapshot(&sim, None);
    let mut w = Writer::new();
    PropHuntGame::write_snapshot(&s, &mut w);
    let bytes = w.finish();
    // Header: phase tag + u32 (5 bytes), then the first hider's x position.
    let at = 5;
    let mut nan = bytes.clone();
    nan[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(PropHuntGame::read_snapshot(&mut Reader::new(&nan)).is_err());
    let mut far = bytes.clone();
    far[at..at + 4].copy_from_slice(&1.0e8f32.to_le_bytes());
    assert!(PropHuntGame::read_snapshot(&mut Reader::new(&far)).is_err());
    assert!(PropHuntGame::read_snapshot(&mut Reader::new(&bytes)).is_ok());
}

#[test]
fn an_out_of_range_disguise_index_is_refused() {
    let sim = fixture();
    let s = PropHuntGame::snapshot(&sim, None);
    let mut w = Writer::new();
    PropHuntGame::write_snapshot(&s, &mut w);
    let bytes = w.finish();
    // Header (5) + pos.x, pos.z, yaw (12) lands on the first hider's disguise byte.
    let at = 5 + 12;
    let mut bad = bytes.clone();
    bad[at] = 200; // not 255 (None) and not a valid DisguiseKind index
    assert!(PropHuntGame::read_snapshot(&mut Reader::new(&bad)).is_err());
}
