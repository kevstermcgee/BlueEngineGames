//! Prop Hunt: the rules. A pure, deterministic, fixed-step simulation of seven hiders and one seeker in
//! Hollow Pine's old house (`layout.rs`). No window, no sound device, no wall clock, no networking: this
//! is what both the offline window and `netgame.rs`'s server-authoritative match step on.
//!
//! One round: `Hiding` (everyone free to move; hiders pick and confirm a disguise at a legal spot) then
//! `Seeking` (the seeker hunts; hiders are stationary once confirmed) then `RoundOver`. Hiders win if
//! anyone is still untagged when the clock runs out; the seeker wins by tagging everyone first.
use crate::disguise::DisguiseKind;
use crate::layout;
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller, ControllerState, Movement};
use vesper3d::viewer::devkit::{Rng, SavePolicy, Simulation, Snapshot, StateHasher, TICK};

/// Hiders in a round; the seeker takes the next participant slot, so a round has `MAX_SEATS` seats.
pub const MAX_HIDERS: usize = 7;
pub const MAX_SEATS: usize = MAX_HIDERS + 1;
/// The participant slot the seeker always drives.
pub const SEEKER_SLOT: usize = MAX_HIDERS;
/// 30 s at 60 Hz. Tune later.
pub const HIDE_PHASE_TICKS: u32 = 1800;
/// 3 minutes at 60 Hz.
pub const SEEK_PHASE_TICKS: u32 = 10800;
/// How close the seeker must be (metres, feet to feet) to inspect a hider.
pub const INSPECT_RADIUS: f32 = 2.2;
/// How long a continuous inspection takes to tag a hider: 0.6 s at 60 Hz.
pub const INSPECT_HOLD_TICKS: u32 = 36;
/// How close to a legal spot a hider must stand to confirm a disguise there.
pub const CONFIRM_RADIUS: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Ticks left before the hide phase ends and unconfirmed hiders are auto-placed.
    Hiding(u32),
    /// Ticks left before time runs out (a hiders' win if anyone is still untagged).
    Seeking(u32),
    RoundOver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    HidersWin,
    SeekerWins,
}

/// One tick of a participant's intent. The same shape drives a hider or the seeker; fields that do not
/// apply to whichever role is reading it are simply ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub sprint: bool,
    /// Radian look deltas since the previous tick: `[yaw, pitch]`.
    pub look: [f32; 2],
    /// Hide phase only: cycle the disguise preview to `DisguiseKind::from_index(choose_disguise)`.
    pub choose_disguise: Option<u8>,
    /// Hide phase only: lock position, yaw and the current disguise preview at the nearest legal spot.
    pub confirm_placement: bool,
    /// Seek phase only: held to inspect the nearest untagged hider within [`INSPECT_RADIUS`].
    pub inspect: bool,
}

/// One tick of intent for every seat (`0..MAX_HIDERS` are hiders, [`SEEKER_SLOT`] is the seeker).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Inputs(pub [Input; MAX_SEATS]);

/// What happened this tick; the window reacts with sound and effects. None of these carry a live
/// hider's position or disguise before they are tagged: the server broadcasts events identically to
/// everyone (`netplay::server`), so anything here is, by construction, safe for every viewer to see.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    HidePhaseEnded,
    SeekPhaseBegan,
    /// The seeker started inspecting someone (no hint who, or where).
    InspectStarted,
    Tagged {
        hider: usize,
    },
    RoundOver(Outcome),
}

/// One hider: a free-roaming `Controller` until `confirmed`, then frozen in place for the rest of the
/// round (the seek phase's MVP simplification: hiders do not move once hidden).
#[derive(Clone, Debug)]
pub struct Hider {
    pub controller: Controller,
    /// The disguise preview while cycling, before it is locked in by `confirm_placement`.
    pub pending_disguise: Option<DisguiseKind>,
    /// The locked-in disguise, `None` until confirmed (or auto-placed at the end of the hide phase).
    pub disguise: Option<DisguiseKind>,
    pub confirmed: bool,
    pub tagged: bool,
    pub tagged_tick: Option<u32>,
    /// Which `layout::DISGUISE_SPOTS` index this hider occupies, if any (auto-placement with no free
    /// spot left, which should not happen with `MAX_HIDERS <= DISGUISE_SPOTS.len()`, leaves this `None`).
    pub spot: Option<usize>,
}

impl Hider {
    fn new(pos: V, yaw: f32) -> Self {
        Self {
            controller: Controller::for_profile(Default::default(), pos, yaw).expect("the default profile is valid"),
            pending_disguise: None,
            disguise: None,
            confirmed: false,
            tagged: false,
            tagged_tick: None,
            spot: None,
        }
    }
}

/// One hider's line in the round report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HiderReport {
    pub slot: usize,
    pub disguise: Option<DisguiseKind>,
    pub tagged: bool,
    pub tagged_tick: Option<u32>,
}

/// The evidence one round leaves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoundReport {
    pub game: String,
    pub outcome: Option<Outcome>,
    pub round_ticks: u32,
    pub hiders: Vec<HiderReport>,
}

pub struct Sim {
    pub tick: u64,
    pub phase: Phase,
    pub hiders: Vec<Hider>,
    pub seeker: Controller,
    /// The hider the seeker is currently holding `inspect` on, within range, if any.
    pub seeker_inspecting: Option<usize>,
    pub inspect_progress: u32,
    pub outcome: Option<Outcome>,
    colliders: Vec<Collider>,
    /// Which `layout::DISGUISE_SPOTS` are already occupied by a confirmed hider.
    spot_taken: Vec<bool>,
    seed: u64,
    rng: Rng,
    events: Vec<Event>,
}

/// Apply one tick of free WASD-plus-mouse-look movement to `controller`. Shared by the authoritative
/// `Sim::step` (for a hider during the hide phase, or the seeker during the seek phase) and a client's
/// own prediction in `netgame.rs`, so the two can never compute movement differently.
pub fn apply_movement(controller: &mut Controller, input: &Input, colliders: &[Collider]) {
    controller.look(input.look[0], input.look[1], 1., false);
    let movement = Movement { forward: input.forward, right: input.right, sprint: input.sprint, ..Default::default() };
    controller.update(movement, TICK, colliders);
}

impl Sim {
    /// A fresh round; the same seed always plays out the same way (which free spot and disguise an
    /// unconfirmed hider is auto-placed with, at the end of the hide phase).
    pub fn new(seed: u64) -> Self {
        let hiders = layout::HIDER_SPAWNS.iter().map(|&pos| Hider::new(pos, layout::HIDER_SPAWN_YAW)).collect();
        let seeker = Controller::for_profile(Default::default(), layout::SEEKER_SPAWN, layout::SEEKER_SPAWN_YAW)
            .expect("the default profile is valid");
        Self {
            tick: 0,
            phase: Phase::Hiding(HIDE_PHASE_TICKS),
            hiders,
            seeker,
            seeker_inspecting: None,
            inspect_progress: 0,
            outcome: None,
            colliders: layout::colliders(),
            spot_taken: vec![false; layout::DISGUISE_SPOTS.len()],
            seed,
            rng: Rng::new(seed),
            events: Vec::new(),
        }
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::RoundOver
    }

    fn feet_xz(pos: V) -> V {
        V(pos.0, 0., pos.2)
    }

    fn nearest_free_spot(&self, pos: V) -> Option<usize> {
        let feet = Self::feet_xz(pos);
        layout::DISGUISE_SPOTS
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.spot_taken[*i])
            .map(|(i, s)| (i, (Self::feet_xz(s.pos) - feet).length()))
            .filter(|(_, d)| *d <= CONFIRM_RADIUS)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    }

    /// Lock hider `i` in at `spot_index`: snap its controller to the spot's position and yaw, with no
    /// velocity, and lock in `disguise`. The spot is marked occupied so nobody else can take it.
    fn confirm_hider_at(&mut self, i: usize, spot_index: usize, disguise: DisguiseKind) {
        let spot = layout::DISGUISE_SPOTS[spot_index];
        self.hiders[i].controller =
            Controller::for_profile(Default::default(), spot.pos, spot.yaw).expect("the default profile is valid");
        self.hiders[i].disguise = Some(disguise);
        self.hiders[i].confirmed = true;
        self.hiders[i].spot = Some(spot_index);
        self.spot_taken[spot_index] = true;
    }

    /// Lock hider `i` in wherever it is standing (no legal spot was free: should not happen with
    /// `MAX_HIDERS <= DISGUISE_SPOTS.len()`, but a hider must never be left without a disguise).
    fn confirm_hider_in_place(&mut self, i: usize, disguise: DisguiseKind) {
        self.hiders[i].disguise = Some(disguise);
        self.hiders[i].confirmed = true;
    }

    fn move_hider(&mut self, i: usize, input: &Input) {
        if self.hiders[i].confirmed {
            return;
        }
        apply_movement(&mut self.hiders[i].controller, input, &self.colliders);
        if let Some(choice) = input.choose_disguise.and_then(DisguiseKind::from_index) {
            self.hiders[i].pending_disguise = Some(choice);
        }
        if input.confirm_placement {
            if let Some(disguise) = self.hiders[i].pending_disguise {
                if let Some(spot_index) = self.nearest_free_spot(self.hiders[i].controller.position) {
                    self.confirm_hider_at(i, spot_index, disguise);
                }
            }
        }
    }

    /// Any hider who never confirmed gets a random free spot and a random disguise: nobody is left
    /// undisguised or AFK-stuck when the hide phase ends.
    fn auto_place_unconfirmed(&mut self) {
        for i in 0..self.hiders.len() {
            if self.hiders[i].confirmed {
                continue;
            }
            let disguise = *self.rng.pick(&DisguiseKind::ALL).expect("DisguiseKind::ALL is not empty");
            let free: Vec<usize> = (0..layout::DISGUISE_SPOTS.len()).filter(|&j| !self.spot_taken[j]).collect();
            match self.rng.pick(&free) {
                Some(&spot_index) => self.confirm_hider_at(i, spot_index, disguise),
                None => self.confirm_hider_in_place(i, disguise),
            }
        }
    }

    fn move_seeker(&mut self, input: &Input) {
        apply_movement(&mut self.seeker, input, &self.colliders);
    }

    /// Who the seeker could inspect right now: the nearest untagged hider within [`INSPECT_RADIUS`].
    fn nearest_inspectable(&self) -> Option<usize> {
        let feet = Self::feet_xz(self.seeker.position);
        self.hiders
            .iter()
            .enumerate()
            .filter(|(_, h)| !h.tagged)
            .map(|(i, h)| (i, (Self::feet_xz(h.controller.position) - feet).length()))
            .filter(|(_, d)| *d <= INSPECT_RADIUS)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    }

    fn update_inspection(&mut self, input: &Input) {
        let target = if input.inspect { self.nearest_inspectable() } else { None };
        match target {
            Some(i) if self.seeker_inspecting == Some(i) => {
                self.inspect_progress += 1;
                if self.inspect_progress >= INSPECT_HOLD_TICKS {
                    self.hiders[i].tagged = true;
                    self.hiders[i].tagged_tick = Some(self.tick as u32);
                    self.events.push(Event::Tagged { hider: i });
                    self.seeker_inspecting = None;
                    self.inspect_progress = 0;
                }
            }
            Some(i) => {
                self.seeker_inspecting = Some(i);
                self.inspect_progress = 1;
                self.events.push(Event::InspectStarted);
            }
            None => {
                self.seeker_inspecting = None;
                self.inspect_progress = 0;
            }
        }
    }

    fn all_hiders_tagged(&self) -> bool {
        self.hiders.iter().all(|h| h.tagged)
    }

    fn end_round(&mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
        self.events.push(Event::RoundOver(outcome));
        self.phase = Phase::RoundOver;
    }

    fn step_hiding(&mut self, inputs: &Inputs, left: u32) {
        if left == 0 {
            self.auto_place_unconfirmed();
            self.events.push(Event::HidePhaseEnded);
            self.phase = Phase::Seeking(SEEK_PHASE_TICKS);
            self.events.push(Event::SeekPhaseBegan);
            return;
        }
        for i in 0..MAX_HIDERS {
            self.move_hider(i, &inputs.0[i]);
        }
        self.phase = Phase::Hiding(left - 1);
    }

    fn step_seeking(&mut self, inputs: &Inputs, left: u32) {
        if left == 0 {
            self.end_round(Outcome::HidersWin);
            return;
        }
        self.move_seeker(&inputs.0[SEEKER_SLOT]);
        self.update_inspection(&inputs.0[SEEKER_SLOT]);
        if self.all_hiders_tagged() {
            self.end_round(Outcome::SeekerWins);
            return;
        }
        self.phase = Phase::Seeking(left - 1);
    }

    /// Advance exactly one 60 Hz tick. A finished round ignores further input.
    pub fn step(&mut self, inputs: &Inputs) {
        if self.phase == Phase::RoundOver {
            return;
        }
        self.tick += 1;
        match self.phase {
            Phase::Hiding(left) => self.step_hiding(inputs, left),
            Phase::Seeking(left) => self.step_seeking(inputs, left),
            Phase::RoundOver => unreachable!("checked above"),
        }
    }

    /// A human left. A hider who never confirmed is auto-placed first, so the game still has something
    /// sane to show and save; a departed hider is then simply marked tagged (no AI needed: a frozen
    /// hider who counts as tagged is exactly what an eliminated player looks like). A departed seeker
    /// ends the round early rather than handing seeking to an AI.
    pub fn release_participant(&mut self, participant: usize) {
        if participant >= MAX_HIDERS {
            if self.outcome.is_none() {
                self.end_round(Outcome::HidersWin);
            }
            return;
        }
        if !self.hiders[participant].confirmed {
            let disguise = *self.rng.pick(&DisguiseKind::ALL).expect("DisguiseKind::ALL is not empty");
            let free: Vec<usize> = (0..layout::DISGUISE_SPOTS.len()).filter(|&j| !self.spot_taken[j]).collect();
            match self.rng.pick(&free) {
                Some(&spot_index) => self.confirm_hider_at(participant, spot_index, disguise),
                None => self.confirm_hider_in_place(participant, disguise),
            }
        }
        self.hiders[participant].tagged = true;
        self.hiders[participant].tagged_tick.get_or_insert(self.tick as u32);
    }

    /// Events since the last call, oldest first.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The evidence this round leaves, one line per hider.
    pub fn report(&self) -> RoundReport {
        RoundReport {
            game: "prop-hunt".into(),
            outcome: self.outcome,
            round_ticks: self.tick as u32,
            hiders: self
                .hiders
                .iter()
                .enumerate()
                .map(|(slot, h)| HiderReport {
                    slot,
                    disguise: h.disguise,
                    tagged: h.tagged,
                    tagged_tick: h.tagged_tick,
                })
                .collect(),
        }
    }
}

impl Simulation for Sim {
    type Input = Inputs;
    fn step(&mut self, input: &Inputs) {
        Sim::step(self, input);
    }
    fn state_hash(&self) -> u64 {
        self.hash_parts()
            .iter()
            .fold(StateHasher::new(), |mut h, (_, part)| {
                h.u64(*part);
                h
            })
            .finish()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        let one = |f: &dyn Fn(&mut StateHasher)| {
            let mut h = StateHasher::new();
            f(&mut h);
            h.finish()
        };
        vec![
            (
                "round",
                one(&|h| {
                    h.u64(self.tick);
                    match self.phase {
                        Phase::Hiding(n) => h.u32(n),
                        Phase::Seeking(n) => h.u32(u32::MAX / 2 + n),
                        Phase::RoundOver => h.u32(u32::MAX),
                    };
                    h.u32(match self.outcome {
                        None => 0,
                        Some(Outcome::HidersWin) => 1,
                        Some(Outcome::SeekerWins) => 2,
                    });
                }),
            ),
            (
                "hiders",
                one(&|h| {
                    for hider in &self.hiders {
                        let p = hider.controller.position;
                        h.f32(p.0).f32(p.1).f32(p.2).f32(hider.controller.yaw);
                        h.bool(hider.confirmed).bool(hider.tagged);
                        h.u32(hider.disguise.map_or(255, |d| d.index() as u32));
                        h.u32(hider.pending_disguise.map_or(255, |d| d.index() as u32));
                        h.u32(hider.tagged_tick.unwrap_or(u32::MAX));
                    }
                }),
            ),
            (
                "seeker",
                one(&|h| {
                    let p = self.seeker.position;
                    h.f32(p.0).f32(p.1).f32(p.2).f32(self.seeker.yaw).f32(self.seeker.pitch);
                    h.u32(self.seeker_inspecting.map_or(255, |i| i as u32));
                    h.u32(self.inspect_progress);
                }),
            ),
            (
                "rng",
                one(&|h| {
                    h.u64(self.rng.state());
                }),
            ),
        ]
    }
}

/// A hider's complete state for a save file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HiderState {
    pub controller: ControllerState,
    pub pending_disguise: Option<DisguiseKind>,
    pub disguise: Option<DisguiseKind>,
    pub confirmed: bool,
    pub tagged: bool,
    pub tagged_tick: Option<u32>,
    pub spot: Option<usize>,
}

/// Everything that decides where the round goes next, as plain data for a save. The house is a constant
/// of the map and presentation is rebuilt from the state, so neither is in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub phase: Phase,
    pub hiders: Vec<HiderState>,
    pub seeker: ControllerState,
    pub seeker_inspecting: Option<usize>,
    pub inspect_progress: u32,
    pub outcome: Option<Outcome>,
    pub spot_taken: Vec<bool>,
    pub seed: u64,
    pub rng: Rng,
}

impl Snapshot for Sim {
    const KIND: &'static str = "prop-hunt";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            phase: self.phase,
            hiders: self
                .hiders
                .iter()
                .map(|h| HiderState {
                    controller: h.controller.network_state(),
                    pending_disguise: h.pending_disguise,
                    disguise: h.disguise,
                    confirmed: h.confirmed,
                    tagged: h.tagged,
                    tagged_tick: h.tagged_tick,
                    spot: h.spot,
                })
                .collect(),
            seeker: self.seeker.network_state(),
            seeker_inspecting: self.seeker_inspecting,
            inspect_progress: self.inspect_progress,
            outcome: self.outcome,
            spot_taken: self.spot_taken.clone(),
            seed: self.seed,
            rng: self.rng.clone(),
        }
    }
    /// Refuse a state this game could not have produced; the caller then keeps the running round.
    fn restore(&mut self, state: SimState) -> Result<(), String> {
        if state.hiders.len() != self.hiders.len() {
            return Err(format!("the save has {} hiders, this round has {}", state.hiders.len(), self.hiders.len()));
        }
        if state.spot_taken.len() != self.spot_taken.len() {
            return Err("the save's disguise spots do not match this house".into());
        }
        let sane = |v: V| v.finite() && v.0.abs() < 1000. && v.2.abs() < 1000.;
        if !sane(state.seeker.position) {
            return Err("the save puts the seeker outside the house".into());
        }
        for h in &state.hiders {
            if !sane(h.controller.position) {
                return Err("the save puts a hider outside the house".into());
            }
            if let Some(spot) = h.spot {
                if spot >= layout::DISGUISE_SPOTS.len() {
                    return Err("the save references a disguise spot this house does not have".into());
                }
            }
            if h.confirmed && h.disguise.is_none() {
                return Err("the save has a confirmed hider with no disguise".into());
            }
        }
        self.tick = state.tick;
        self.phase = state.phase;
        for (hider, saved) in self.hiders.iter_mut().zip(state.hiders) {
            hider.controller.restore_network_state(&saved.controller);
            hider.pending_disguise = saved.pending_disguise;
            hider.disguise = saved.disguise;
            hider.confirmed = saved.confirmed;
            hider.tagged = saved.tagged;
            hider.tagged_tick = saved.tagged_tick;
            hider.spot = saved.spot;
        }
        self.seeker.restore_network_state(&state.seeker);
        self.seeker_inspecting = state.seeker_inspecting;
        self.inspect_progress = state.inspect_progress;
        self.outcome = state.outcome;
        self.spot_taken = state.spot_taken;
        self.seed = state.seed;
        self.rng = state.rng;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn still() -> Inputs {
        Inputs::default()
    }

    fn input_for(slot: usize, input: Input) -> Inputs {
        let mut inputs = Inputs::default();
        inputs.0[slot] = input;
        inputs
    }

    #[test]
    fn a_fresh_round_starts_hiding_with_nobody_confirmed() {
        let sim = Sim::new(1);
        assert_eq!(sim.phase, Phase::Hiding(HIDE_PHASE_TICKS));
        assert!(sim.hiders.iter().all(|h| !h.confirmed && h.disguise.is_none() && !h.tagged));
        assert!(sim.outcome.is_none());
    }

    #[test]
    fn the_hide_phase_ends_at_the_exact_tick_and_the_seek_phase_begins() {
        let mut sim = Sim::new(1);
        for _ in 0..HIDE_PHASE_TICKS {
            sim.step(&still());
            assert!(matches!(sim.phase, Phase::Hiding(_)), "still hiding at tick {}", sim.tick);
        }
        assert_eq!(sim.phase, Phase::Hiding(0));
        sim.step(&still());
        assert_eq!(sim.phase, Phase::Seeking(SEEK_PHASE_TICKS));
    }

    #[test]
    fn hide_phase_auto_placement_never_leaves_a_hider_undisguised() {
        let mut sim = Sim::new(3);
        for _ in 0..=HIDE_PHASE_TICKS {
            sim.step(&still());
        }
        assert!(matches!(sim.phase, Phase::Seeking(_)));
        assert!(sim.hiders.iter().all(|h| h.confirmed && h.disguise.is_some()), "every hider must be disguised");
    }

    #[test]
    fn a_hider_can_walk_to_a_spot_choose_a_disguise_and_confirm_it() {
        let mut sim = Sim::new(5);
        let target = layout::DISGUISE_SPOTS[0];
        sim.hiders[0].controller =
            Controller::for_profile(Default::default(), target.pos, target.yaw).expect("valid profile");
        sim.step(&input_for(0, Input { choose_disguise: Some(2), confirm_placement: true, ..Default::default() }));
        assert!(sim.hiders[0].confirmed);
        assert_eq!(sim.hiders[0].disguise, DisguiseKind::from_index(2));
        assert_eq!(sim.hiders[0].spot, Some(0));
        let settled = sim.hiders[0].controller.position;
        let moved_xz = (V(settled.0, 0., settled.2) - target.pos).length();
        assert!(moved_xz < 0.01, "confirming snaps to the spot, moved {moved_xz}");
        assert!((sim.hiders[0].controller.feet_height() - target.pos.1).abs() < 0.01);
        assert!((sim.hiders[0].controller.yaw - target.yaw).abs() < 0.01);
    }

    #[test]
    fn confirming_far_from_any_legal_spot_does_nothing() {
        let mut sim = Sim::new(5);
        let before = sim.hiders[0].controller.position;
        sim.step(&input_for(0, Input { choose_disguise: Some(0), confirm_placement: true, ..Default::default() }));
        assert!(!sim.hiders[0].confirmed);
        assert_eq!(sim.hiders[0].controller.position, before);
    }

    #[test]
    fn confirming_without_a_chosen_disguise_does_nothing() {
        let mut sim = Sim::new(5);
        let target = layout::DISGUISE_SPOTS[1];
        sim.hiders[0].controller =
            Controller::for_profile(Default::default(), target.pos, target.yaw).expect("valid profile");
        sim.step(&input_for(0, Input { confirm_placement: true, ..Default::default() }));
        assert!(!sim.hiders[0].confirmed, "no disguise chosen yet");
    }

    #[test]
    fn two_hiders_cannot_confirm_the_same_spot() {
        let mut sim = Sim::new(5);
        let target = layout::DISGUISE_SPOTS[3];
        for slot in [0usize, 1] {
            sim.hiders[slot].controller =
                Controller::for_profile(Default::default(), target.pos, target.yaw).expect("valid profile");
        }
        let confirm = Input { choose_disguise: Some(0), confirm_placement: true, ..Default::default() };
        sim.step(&input_for(0, confirm));
        assert!(sim.hiders[0].confirmed);
        sim.step(&input_for(1, confirm));
        assert!(!sim.hiders[1].confirmed, "the spot is already taken");
    }

    #[test]
    fn confirmed_placements_never_overlap_a_wall_collider() {
        let colliders = layout::colliders();
        for spot in layout::DISGUISE_SPOTS {
            assert!(
                !colliders.iter().any(|c| c.overlaps_xz(spot.pos, 0.05)),
                "disguise spot {:?} overlaps a wall",
                spot.pos
            );
        }
    }

    #[test]
    fn a_confirmed_hider_does_not_move_even_with_input() {
        let mut sim = Sim::new(5);
        let target = layout::DISGUISE_SPOTS[0];
        sim.hiders[0].controller =
            Controller::for_profile(Default::default(), target.pos, target.yaw).expect("valid profile");
        sim.step(&input_for(0, Input { choose_disguise: Some(0), confirm_placement: true, ..Default::default() }));
        let locked = sim.hiders[0].controller.position;
        sim.step(&input_for(0, Input { forward: 1., ..Default::default() }));
        assert_eq!(sim.hiders[0].controller.position, locked, "a confirmed hider is frozen");
    }

    fn seeking_sim(seed: u64) -> Sim {
        let mut sim = Sim::new(seed);
        for _ in 0..=HIDE_PHASE_TICKS {
            sim.step(&still());
        }
        assert!(matches!(sim.phase, Phase::Seeking(_)));
        sim
    }

    #[test]
    fn inspect_hold_only_tags_after_continuous_proximity_and_resets_on_walk_away() {
        let mut sim = seeking_sim(7);
        let hider_pos = sim.hiders[0].controller.position;
        sim.seeker = Controller::for_profile(Default::default(), V(hider_pos.0, 0., hider_pos.2), 0.).unwrap();
        for n in 0..(INSPECT_HOLD_TICKS - 1) {
            sim.step(&input_for(SEEKER_SLOT, Input { inspect: true, ..Default::default() }));
            assert!(!sim.hiders[0].tagged, "tagged too early at tick {n}");
        }
        assert_eq!(sim.inspect_progress, INSPECT_HOLD_TICKS - 1);
        // Walking away (no inspect input) resets progress instead of carrying it over.
        sim.step(&input_for(SEEKER_SLOT, Input::default()));
        assert_eq!(sim.inspect_progress, 0);
        assert_eq!(sim.seeker_inspecting, None);
        for _ in 0..INSPECT_HOLD_TICKS {
            sim.step(&input_for(SEEKER_SLOT, Input { inspect: true, ..Default::default() }));
        }
        assert!(sim.hiders[0].tagged, "continuous inspection eventually tags");
    }

    #[test]
    fn inspecting_a_different_hider_restarts_progress() {
        let mut sim = seeking_sim(7);
        let a = sim.hiders[0].controller.position;
        sim.seeker = Controller::for_profile(Default::default(), V(a.0, 0., a.2), 0.).unwrap();
        sim.step(&input_for(SEEKER_SLOT, Input { inspect: true, ..Default::default() }));
        assert_eq!(sim.seeker_inspecting, Some(0));
        let b = sim.hiders[1].controller.position;
        sim.seeker = Controller::for_profile(Default::default(), V(b.0, 0., b.2), 0.).unwrap();
        sim.step(&input_for(SEEKER_SLOT, Input { inspect: true, ..Default::default() }));
        assert_eq!(sim.seeker_inspecting, Some(1));
        assert_eq!(sim.inspect_progress, 1, "progress restarts on a new target");
    }

    #[test]
    fn the_seeker_wins_the_instant_every_hider_is_tagged() {
        let mut sim = seeking_sim(11);
        for h in &mut sim.hiders {
            h.tagged = true;
        }
        sim.hiders[0].tagged = false;
        let pos = sim.hiders[0].controller.position;
        sim.seeker = Controller::for_profile(Default::default(), V(pos.0, 0., pos.2), 0.).unwrap();
        for _ in 0..INSPECT_HOLD_TICKS {
            sim.step(&input_for(SEEKER_SLOT, Input { inspect: true, ..Default::default() }));
        }
        assert_eq!(sim.outcome, Some(Outcome::SeekerWins));
        assert_eq!(sim.phase, Phase::RoundOver);
    }

    #[test]
    fn hiders_win_at_the_exact_tick_the_clock_runs_out_if_anyone_is_untagged() {
        let mut sim = seeking_sim(13);
        for _ in 0..SEEK_PHASE_TICKS {
            sim.step(&still());
            assert!(sim.outcome.is_none(), "the round is not over early at tick {}", sim.tick);
        }
        assert_eq!(sim.phase, Phase::Seeking(0));
        sim.step(&still());
        assert_eq!(sim.outcome, Some(Outcome::HidersWin));
        assert_eq!(sim.phase, Phase::RoundOver);
    }

    #[test]
    fn a_finished_round_ignores_further_input() {
        let mut sim = seeking_sim(17);
        for h in &mut sim.hiders {
            h.tagged = true;
        }
        sim.step(&still());
        assert_eq!(sim.phase, Phase::RoundOver);
        let tick = sim.tick;
        sim.step(&input_for(SEEKER_SLOT, Input { forward: 1., ..Default::default() }));
        assert_eq!(sim.tick, tick, "a finished round does not advance");
    }

    #[test]
    fn releasing_an_unconfirmed_hider_auto_places_and_tags_them() {
        let mut sim = Sim::new(9);
        sim.release_participant(2);
        assert!(sim.hiders[2].confirmed);
        assert!(sim.hiders[2].disguise.is_some());
        assert!(sim.hiders[2].tagged);
    }

    #[test]
    fn releasing_a_confirmed_hider_just_tags_them_in_place() {
        let mut sim = seeking_sim(9);
        let pos = sim.hiders[0].controller.position;
        sim.release_participant(0);
        assert!(sim.hiders[0].tagged);
        assert_eq!(sim.hiders[0].controller.position, pos, "a confirmed hider is not moved by leaving");
    }

    #[test]
    fn a_departed_seeker_ends_the_round_as_a_hiders_win() {
        let mut sim = seeking_sim(9);
        sim.release_participant(SEEKER_SLOT);
        assert_eq!(sim.outcome, Some(Outcome::HidersWin));
        assert_eq!(sim.phase, Phase::RoundOver);
    }

    #[test]
    fn the_same_seed_and_inputs_replay_identically() {
        vesper3d::viewer::devkit::assert_deterministic(|| Sim::new(21), &[still(); HIDE_PHASE_TICKS as usize + 400]);
    }

    #[test]
    fn save_and_restore_round_trips_exactly() {
        vesper3d::viewer::devkit::snapshot::assert_resumes_as_promised(
            || Sim::new(23),
            &[still(); HIDE_PHASE_TICKS as usize + 200],
            50,
        );
    }
}
