//! Prop Hunt as a [`NetGame`]: everything BlueEngine's netplay kit needs to host and join a round. The
//! kit owns the lobby, sessions, input streaming, snapshots and statistics; this file is what is
//! particular to Prop Hunt: the layouts of an input, a snapshot and an event, how seats become a seeker
//! and hiders, and — the whole reason this game exists — a snapshot that is genuinely different per
//! viewer, not just an identical broadcast dressed up as one.
//!
//! **The redaction table** (see `snapshot()` below), the single most important thing in this file:
//! - The seeker sees every hider's real position and disguise, tagged or not: a disguised hider must be
//!   indistinguishable from a real decoy prop, which only works if the seeker's own rendering cannot
//!   special-case "the real one".
//! - An untagged hider sees the seeker (hiders can hear/sense the hunt, a genre convention), its own
//!   position and disguise, and any *already-tagged* hider in full (nothing left to hide once caught) —
//!   but every other still-active hider is blanked: no position, no disguise. This is the anti-cheat
//!   property this game is actually testing: one live hider must never learn where another is hiding.
//! - A tagged hider, or any viewer with no seat at all, gets the open spectator view: everything real.
//!
//! Events cannot carry this distinction — `netplay::server` broadcasts the same event list to everyone
//! (`send_snapshots`/`step_match` in `netplay/server.rs`) — so no [`Event`] ever carries a live hider's
//! position or disguise; only `snapshot()` differs per viewer.
//!
//! Static geometry (walls, decoy props, legal spots) is never sent: both ends rebuild it from
//! `layout.rs`'s constants, exactly like Spooky Kart's track and Dead Air's station.
use crate::disguise::DisguiseKind;
use crate::layout;
use crate::sim::{apply_movement, Event, Input, Outcome, Phase, Sim, MAX_HIDERS, MAX_SEATS as SIM_SEATS, SEEKER_SLOT};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller};
use vesper3d::viewer::net::codec::{Reader, WireError, WireResult, Writer};
use vesper3d::viewer::netplay::{ClientView, NetGame, PredictionStats, Seat};

/// A prediction error larger than this snaps instead of being treated as a normal correction.
const SNAP_DISTANCE: f32 = 4.;

pub struct PropHuntGame;

/// One hider as a viewer sees them: blank (`HiderView::blank()`) when the redaction table hides them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HiderView {
    pub pos: V,
    pub yaw: f32,
    pub disguise: Option<DisguiseKind>,
    pub confirmed: bool,
    pub tagged: bool,
}

impl HiderView {
    fn blank() -> Self {
        Self { pos: V::ZERO, yaw: 0., disguise: None, confirmed: false, tagged: false }
    }

    fn real(sim: &Sim, i: usize) -> Self {
        let h = &sim.hiders[i];
        let p = h.controller.position;
        // Only x/z travel on the wire (every body stands on the flat floor at y = 0; see `write_hider`),
        // so this is built pre-zeroed to match exactly what `read_hider` reconstructs.
        Self {
            pos: V(p.0, 0., p.2),
            yaw: h.controller.yaw,
            disguise: h.disguise,
            confirmed: h.confirmed,
            tagged: h.tagged,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeekerView {
    pub pos: V,
    pub yaw: f32,
}

/// The round as one server tick shows it, redacted for whoever asked. Always `MAX_HIDERS` hiders long,
/// including slots nobody is seated in (an unfilled hider slot is simply a hider nobody ever moves or
/// confirms for themselves — `Sim` auto-places it like any other at the end of the hide phase).
#[derive(Clone, Debug, PartialEq)]
pub struct PropHuntSnapshot {
    pub phase: Phase,
    pub hiders: Vec<HiderView>,
    pub seeker: SeekerView,
    pub outcome: Option<Outcome>,
    /// How many consecutive ticks the seeker has held a continuous inspection (0 if none); harmless to
    /// broadcast to everyone since it never says *who* is being inspected (see [`Event::InspectStarted`]),
    /// but used in practice only for the seeker's own "inspecting..." progress bar.
    pub inspect_progress: u32,
}

/// Who `participant` is, for the purposes of the redaction table.
enum Viewer {
    Seeker,
    UntaggedHider(usize),
    /// A tagged hider, a true spectator (`participant` is `None`), or an out-of-range slot: the open view.
    Open,
}

fn viewer_of(sim: &Sim, participant: Option<usize>) -> Viewer {
    match participant {
        Some(SEEKER_SLOT) => Viewer::Seeker,
        Some(p) if p < MAX_HIDERS && !sim.hiders[p].tagged => Viewer::UntaggedHider(p),
        _ => Viewer::Open,
    }
}

impl PropHuntGame {
    fn write_hider(w: &mut Writer, h: &HiderView) {
        w.f32(h.pos.0);
        w.f32(h.pos.2);
        w.f32(h.yaw);
        w.u8(h.disguise.map_or(255, DisguiseKind::index));
        w.u8(h.confirmed as u8 | (h.tagged as u8) << 1);
    }

    fn read_hider(r: &mut Reader) -> WireResult<HiderView> {
        let (x, z) = (r.f32_within(500.)?, r.f32_within(500.)?);
        let yaw = r.f32()?;
        let disguise = match r.u8()? {
            255 => None,
            i => Some(DisguiseKind::from_index(i).ok_or(WireError("bad disguise index"))?),
        };
        let flags = r.u8()?;
        if flags > 3 {
            return Err(WireError("bad hider flags"));
        }
        Ok(HiderView { pos: V(x, 0., z), yaw, disguise, confirmed: flags & 1 != 0, tagged: flags & 2 != 0 })
    }

    fn write_phase(w: &mut Writer, phase: Phase) {
        match phase {
            Phase::Hiding(n) => {
                w.u8(0);
                w.u32(n);
            }
            Phase::Seeking(n) => {
                w.u8(1);
                w.u32(n);
            }
            Phase::RoundOver => {
                w.u8(2);
                w.u32(0);
            }
        }
    }

    fn read_phase(r: &mut Reader) -> WireResult<Phase> {
        Ok(match (r.u8()?, r.u32()?) {
            (0, n) => Phase::Hiding(n),
            (1, n) => Phase::Seeking(n),
            (2, _) => Phase::RoundOver,
            _ => return Err(WireError("bad phase")),
        })
    }

    fn write_outcome(w: &mut Writer, outcome: Option<Outcome>) {
        w.u8(match outcome {
            None => 0,
            Some(Outcome::HidersWin) => 1,
            Some(Outcome::SeekerWins) => 2,
        });
    }

    fn read_outcome(r: &mut Reader) -> WireResult<Option<Outcome>> {
        Ok(match r.u8()? {
            0 => None,
            1 => Some(Outcome::HidersWin),
            2 => Some(Outcome::SeekerWins),
            _ => return Err(WireError("bad outcome")),
        })
    }
}

impl NetGame for PropHuntGame {
    type Input = Input;
    type Match = Sim;
    type View = PropHuntView;
    type Snapshot = PropHuntSnapshot;
    type Event = Event;

    const NAME: &'static str = "prop-hunt";
    const MAX_SEATS: usize = SIM_SEATS;
    /// A lobby preference, not an assignment: "I'd like to hide" (0) or "I'd like to seek" (1). Several
    /// players may prefer the same role; exactly one seeker is still chosen in `start`.
    const CHOICES: u8 = 2;
    const UNIQUE_CHOICES: bool = false;

    fn fingerprint() -> u32 {
        crate::content_fingerprint()
    }

    fn write_input(i: &Input, w: &mut Writer) {
        w.u8((i.forward.clamp(-1., 1.) * 127.).round() as i8 as u8);
        w.u8((i.right.clamp(-1., 1.) * 127.).round() as i8 as u8);
        w.u8(i.sprint as u8 | (i.confirm_placement as u8) << 1 | (i.inspect as u8) << 2);
        w.u8(i.choose_disguise.unwrap_or(255));
        w.f32(i.look[0]);
        w.f32(i.look[1]);
    }

    fn read_input(r: &mut Reader) -> WireResult<Input> {
        let forward = r.u8()? as i8 as f32 / 127.;
        let right = r.u8()? as i8 as f32 / 127.;
        let bits = r.u8()?;
        if bits > 7 {
            return Err(WireError("bad input flags"));
        }
        let choose_disguise = match r.u8()? {
            255 => None,
            i if DisguiseKind::from_index(i).is_some() => Some(i),
            _ => return Err(WireError("bad disguise choice")),
        };
        let look = [r.f32_within(4.)?, r.f32_within(4.)?];
        Ok(Input {
            forward,
            right,
            sprint: bits & 1 != 0,
            look,
            choose_disguise,
            confirm_placement: bits & 2 != 0,
            inspect: bits & 4 != 0,
        })
    }

    fn write_snapshot(s: &PropHuntSnapshot, w: &mut Writer) {
        Self::write_phase(w, s.phase);
        for h in &s.hiders {
            Self::write_hider(w, h);
        }
        w.f32(s.seeker.pos.0);
        w.f32(s.seeker.pos.2);
        w.f32(s.seeker.yaw);
        Self::write_outcome(w, s.outcome);
        w.u8(s.inspect_progress.min(255) as u8);
    }

    fn read_snapshot(r: &mut Reader) -> WireResult<PropHuntSnapshot> {
        let phase = Self::read_phase(r)?;
        let hiders = (0..MAX_HIDERS).map(|_| Self::read_hider(r)).collect::<WireResult<Vec<_>>>()?;
        let (sx, sz) = (r.f32_within(500.)?, r.f32_within(500.)?);
        let syaw = r.f32()?;
        let outcome = Self::read_outcome(r)?;
        let inspect_progress = r.u8()? as u32;
        Ok(PropHuntSnapshot {
            phase,
            hiders,
            seeker: SeekerView { pos: V(sx, 0., sz), yaw: syaw },
            outcome,
            inspect_progress,
        })
    }

    fn write_event(e: &Event, w: &mut Writer) {
        match e {
            Event::HidePhaseEnded => w.u8(0),
            Event::SeekPhaseBegan => w.u8(1),
            Event::InspectStarted => w.u8(2),
            Event::Tagged { hider } => {
                w.u8(3);
                w.u8(*hider as u8);
            }
            Event::RoundOver(outcome) => {
                w.u8(4);
                Self::write_outcome(w, Some(*outcome));
            }
        }
    }

    fn read_event(r: &mut Reader) -> WireResult<Event> {
        Ok(match r.u8()? {
            0 => Event::HidePhaseEnded,
            1 => Event::SeekPhaseBegan,
            2 => Event::InspectStarted,
            3 => {
                let hider = r.u8()? as usize;
                if hider >= MAX_HIDERS {
                    return Err(WireError("bad hider index"));
                }
                Event::Tagged { hider }
            }
            4 => match Self::read_outcome(r)? {
                Some(outcome) => Event::RoundOver(outcome),
                None => return Err(WireError("round-over event with no outcome")),
            },
            _ => return Err(WireError("unknown event")),
        })
    }

    /// One seeker, seeded-random among whoever preferred it (or among everyone if nobody did); the
    /// seeker always drives [`SEEKER_SLOT`]. Everyone else drives a hider slot in seat order. No bots
    /// fill empty hider slots: an unseated hider slot is simply never moved or confirmed by a human, and
    /// `Sim` auto-places it like any other at the end of the hide phase.
    fn start(seed: u64, seats: &[Seat], _participants: usize) -> (Sim, Vec<usize>) {
        let mut rng = vesper3d::viewer::devkit::Rng::new(seed);
        let preferred: Vec<usize> = seats.iter().enumerate().filter(|(_, s)| s.choice == 1).map(|(i, _)| i).collect();
        let pool: Vec<usize> = if preferred.is_empty() { (0..seats.len()).collect() } else { preferred };
        let seeker_seat = rng.pick(&pool).copied();
        let mut assigned = Vec::with_capacity(seats.len());
        let mut next_hider = 0usize;
        for i in 0..seats.len() {
            if Some(i) == seeker_seat {
                assigned.push(SEEKER_SLOT);
            } else {
                assigned.push(next_hider.min(MAX_HIDERS - 1));
                next_hider += 1;
            }
        }
        (Sim::new(seed), assigned)
    }

    fn participants(_m: &Sim) -> usize {
        SIM_SEATS
    }

    fn step(m: &mut Sim, inputs: &[Option<Input>]) -> Vec<Event> {
        let mut all = crate::sim::Inputs::default();
        for (i, input) in inputs.iter().enumerate().take(SIM_SEATS) {
            if let Some(input) = input {
                all.0[i] = *input;
            }
        }
        m.step(&all);
        m.drain_events()
    }

    fn release(m: &mut Sim, participant: usize) {
        m.release_participant(participant);
    }

    /// The core design decision of this game: see the module documentation's redaction table.
    fn snapshot(m: &Sim, participant: Option<usize>) -> PropHuntSnapshot {
        let viewer = viewer_of(m, participant);
        let hiders = (0..MAX_HIDERS)
            .map(|i| match viewer {
                Viewer::Seeker | Viewer::Open => HiderView::real(m, i),
                Viewer::UntaggedHider(p) => {
                    if i == p || m.hiders[i].tagged {
                        HiderView::real(m, i)
                    } else {
                        HiderView::blank()
                    }
                }
            })
            .collect();
        let seeker_pos = m.seeker.position;
        PropHuntSnapshot {
            phase: m.phase,
            hiders,
            // Pre-zeroed to match `read_snapshot`'s reconstruction; see `HiderView::real`.
            seeker: SeekerView { pos: V(seeker_pos.0, 0., seeker_pos.2), yaw: m.seeker.yaw },
            outcome: m.outcome,
            inspect_progress: m.inspect_progress,
        }
    }

    fn is_over(m: &Sim) -> bool {
        m.is_over()
    }

    fn report(m: &Sim) -> serde_json::Value {
        serde_json::to_value(m.report()).unwrap_or(serde_json::Value::Null)
    }
}

/// What a client keeps: the latest (already redacted, for this client) snapshot, and — unless this
/// client's own hider is confirmed and frozen — a locally predicted `Controller` for its own entity,
/// reconciled from the snapshot's pose and replayed forward over inputs the server has not applied yet.
///
/// Look pitch is never sent on the wire at all (movement only ever depends on yaw: `Controller::update`
/// does not read pitch), so it is tracked purely client-side here and is never reconciled or corrected —
/// there is nothing for the server to disagree with the client about.
pub struct PropHuntView {
    snapshot: Option<PropHuntSnapshot>,
    participant: Option<usize>,
    mine: Option<Controller>,
    pitch: f32,
    colliders: Vec<Collider>,
    stats: PredictionStats,
}

impl PropHuntView {
    /// The latest snapshot this client has received, already redacted for whoever it is.
    pub fn snapshot(&self) -> Option<&PropHuntSnapshot> {
        self.snapshot.as_ref()
    }
    /// This client's own predicted entity (a moving hider during the hide phase, or the seeker during
    /// the seek phase). `None` once this client's hider is confirmed and frozen, or while spectating:
    /// draw straight from `snapshot()` instead.
    pub fn mine(&self) -> Option<&Controller> {
        self.mine.as_ref()
    }
    /// This client's own look pitch (never sent to or corrected by the server; see the struct docs).
    pub fn pitch(&self) -> f32 {
        self.pitch
    }
    pub fn participant(&self) -> Option<usize> {
        self.participant
    }
}

impl ClientView<PropHuntGame> for PropHuntView {
    fn new() -> Self {
        Self {
            snapshot: None,
            participant: None,
            mine: None,
            pitch: 0.,
            colliders: layout::colliders(),
            stats: PredictionStats::default(),
        }
    }

    fn on_snapshot(&mut self, s: &PropHuntSnapshot, participant: Option<usize>, pending: &[(u32, Input)], _now: f64) {
        self.participant = participant;
        self.mine = participant.and_then(|p| {
            let (pose_pos, pose_yaw, frozen) = if p == SEEKER_SLOT {
                (s.seeker.pos, s.seeker.yaw, false)
            } else if p < MAX_HIDERS {
                (s.hiders[p].pos, s.hiders[p].yaw, s.hiders[p].confirmed)
            } else {
                return None;
            };
            if frozen {
                return None;
            }
            let mut c =
                Controller::for_profile(Default::default(), pose_pos, pose_yaw).expect("default profile is valid");
            for (_, input) in pending {
                apply_movement(&mut c, input, &self.colliders);
            }
            if let Some(before) = &self.mine {
                let error = (before.position - c.position).length();
                self.stats.last_error = error;
                self.stats.max_error = self.stats.max_error.max(error);
                if error > 0.02 {
                    self.stats.corrections += 1;
                    if error > SNAP_DISTANCE {
                        self.stats.snaps += 1;
                    }
                }
            }
            Some(c)
        });
        self.snapshot = Some(s.clone());
    }

    fn on_input(&mut self, input: &Input) {
        self.pitch = (self.pitch - input.look[1]).clamp(-1.5, 1.5);
        if let Some(c) = self.mine.as_mut() {
            apply_movement(c, input, &self.colliders);
        }
    }

    fn frame(&mut self, _now: f64, _dt: f32) {
        // Everyone else (other hiders once visible, the seeker) is stationary or slow enough that a
        // direct snapshot-to-snapshot cut reads fine; only this client's own entity is predicted.
    }

    fn reset(&mut self) {
        let colliders = std::mem::take(&mut self.colliders);
        *self = Self::new();
        self.colliders = colliders;
    }

    fn prediction(&self) -> PredictionStats {
        self.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vesper3d::viewer::netplay::Seat;

    fn seat(id: u8, choice: u8) -> Seat {
        Seat { id, choice, name: format!("Player {id}") }
    }

    #[test]
    fn exactly_one_seeker_is_assigned_regardless_of_preferences() {
        for seats in [
            vec![seat(0, 0), seat(1, 0), seat(2, 0)], // nobody wants to seek
            vec![seat(0, 1), seat(1, 1), seat(2, 1)], // everybody wants to seek
            vec![seat(0, 0), seat(1, 1), seat(2, 0), seat(3, 1)],
        ] {
            let (_, assigned) = PropHuntGame::start(1, &seats, seats.len());
            assert_eq!(assigned.iter().filter(|&&p| p == SEEKER_SLOT).count(), 1, "{assigned:?}");
            let hiders: Vec<usize> = assigned.iter().copied().filter(|&p| p != SEEKER_SLOT).collect();
            let mut sorted = hiders.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), hiders.len(), "no two seats share a hider slot: {assigned:?}");
        }
    }

    #[test]
    fn the_seeded_pick_is_not_always_the_first_seat() {
        let seats: Vec<Seat> = (0..6).map(|i| seat(i, 1)).collect();
        let mut seeker_seats = std::collections::HashSet::new();
        for seed in 0..40u64 {
            let (_, assigned) = PropHuntGame::start(seed, &seats, seats.len());
            let seeker = assigned.iter().position(|&p| p == SEEKER_SLOT).unwrap();
            seeker_seats.insert(seeker);
        }
        assert!(seeker_seats.len() > 1, "every seed picked the same seat: {seeker_seats:?}");
    }
}
