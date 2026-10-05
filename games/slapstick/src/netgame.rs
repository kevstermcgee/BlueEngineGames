//! Slapstick as a [`NetGame`]: everything BlueEngine's netplay kit needs to host and join a match. The
//! kit owns the lobby, sessions, input streaming, snapshots and statistics; this file is what is
//! particular to air hockey: the layouts of an input, a snapshot and an event, how seats become
//! paddles, and how a client predicts its own paddle and smooths the puck and the opponent.
//!
//! **No hidden information** (the mirror image of Prop Hunt's redaction table): both players always see
//! identical puck/paddle/score state, so [`AirHockeyGame::snapshot`] ignores `participant` entirely,
//! exactly like Spooky Kart's `KartGame::snapshot`. `tests/net.rs`'s
//! `both_clients_snapshot_identical_state_every_tick` is this game's key networking invariant: the
//! opposite of Prop Hunt's "never leaks", here the property is "never differs".
use crate::sim::{
    apply_paddle_input, Event, Input, Inputs, Paddle, Phase, Sim, PUCK_MAX_SPEED, TABLE_HALF_LENGTH, TABLE_HALF_WIDTH,
};
use vesper3d::math::V;
use vesper3d::viewer::net::codec::{Reader, WireError, WireResult, Writer};
use vesper3d::viewer::netplay::{ClientView, NetGame, PredictionStats, Seat};

/// A prediction error larger than this snaps instead of easing (metres).
const SNAP_DISTANCE: f32 = 0.5;
/// The time between snapshots (the server sends 30 a second).
const SNAPSHOT_INTERVAL: f64 = 1. / 30.;

pub struct AirHockeyGame;

/// The match as one server tick shows it: identical for every viewer (see the module doc).
#[derive(Clone, Debug, PartialEq)]
pub struct AirHockeySnapshot {
    pub phase: Phase,
    /// Paddle positions only (slot 0, slot 1): a paddle is a kinematic target-tracker, so a client needs
    /// no velocity to draw or interpolate one smoothly.
    pub paddles: [V; 2],
    /// Puck position and velocity, so a client can extrapolate a little on a late packet.
    pub puck: (V, V),
    pub score: [u32; 2],
}

fn write_phase(w: &mut Writer, phase: Phase) {
    match phase {
        Phase::Serve { ticks_left, server } => {
            w.u8(0);
            w.u32(ticks_left);
            w.u8(server as u8);
        }
        Phase::Playing => {
            w.u8(1);
            w.u32(0);
            w.u8(0);
        }
        Phase::GameOver { winner } => {
            w.u8(2);
            w.u32(0);
            w.u8(winner as u8);
        }
    }
}

fn read_phase(r: &mut Reader) -> WireResult<Phase> {
    let tag = r.u8()?;
    let ticks_left = r.u32()?;
    let slot = r.u8()?;
    match tag {
        0 => {
            if slot >= 2 {
                return Err(WireError("bad server slot"));
            }
            Ok(Phase::Serve { ticks_left, server: slot as usize })
        }
        1 => Ok(Phase::Playing),
        2 => {
            if slot >= 2 {
                return Err(WireError("bad winner slot"));
            }
            Ok(Phase::GameOver { winner: slot as usize })
        }
        _ => Err(WireError("bad phase")),
    }
}

impl NetGame for AirHockeyGame {
    type Input = Input;
    type Match = Sim;
    type View = AirHockeyView;
    type Snapshot = AirHockeySnapshot;
    type Event = Event;

    const NAME: &'static str = "slapstick";
    const MAX_SEATS: usize = 2;
    /// A lobby cosmetic (paddle colour). The two players are never the same colour: `UNIQUE_CHOICES`
    /// stays the trait default (`true`).
    const CHOICES: u8 = 4;

    fn fingerprint() -> u32 {
        crate::content_fingerprint()
    }

    fn write_input(i: &Input, w: &mut Writer) {
        w.f32(i.target_x.clamp(-1., 1.));
        w.f32(i.target_z.clamp(-1., 1.));
    }

    fn read_input(r: &mut Reader) -> WireResult<Input> {
        Ok(Input { target_x: r.f32_within(1.0001)?, target_z: r.f32_within(1.0001)? })
    }

    fn write_snapshot(s: &AirHockeySnapshot, w: &mut Writer) {
        write_phase(w, s.phase);
        for p in s.paddles {
            w.f32(p.0);
            w.f32(p.2);
        }
        w.f32(s.puck.0 .0);
        w.f32(s.puck.0 .2);
        w.f32(s.puck.1 .0);
        w.f32(s.puck.1 .2);
        w.u8(s.score[0].min(255) as u8);
        w.u8(s.score[1].min(255) as u8);
    }

    fn read_snapshot(r: &mut Reader) -> WireResult<AirHockeySnapshot> {
        let phase = read_phase(r)?;
        let bound_x = TABLE_HALF_WIDTH + 1.;
        let bound_z = TABLE_HALF_LENGTH + 1.;
        let mut paddles = [V::ZERO; 2];
        for p in &mut paddles {
            let x = r.f32_within(bound_x)?;
            let z = r.f32_within(bound_z)?;
            *p = V(x, 0., z);
        }
        let px = r.f32_within(bound_x)?;
        let pz = r.f32_within(bound_z)?;
        let vx = r.f32_within(PUCK_MAX_SPEED + 5.)?;
        let vz = r.f32_within(PUCK_MAX_SPEED + 5.)?;
        let score = [u32::from(r.u8()?), u32::from(r.u8()?)];
        Ok(AirHockeySnapshot { phase, paddles, puck: (V(px, 0., pz), V(vx, 0., vz)), score })
    }

    fn write_event(e: &Event, w: &mut Writer) {
        match e {
            Event::Serve { server } => {
                w.u8(0);
                w.u8(*server as u8);
            }
            Event::Goal { scorer, score } => {
                w.u8(1);
                w.u8(*scorer as u8);
                w.u8(score[0].min(255) as u8);
                w.u8(score[1].min(255) as u8);
            }
            Event::PaddleHit { paddle, speed } => {
                w.u8(2);
                w.u8(*paddle as u8);
                w.f32(*speed);
            }
            Event::WallHit { speed } => {
                w.u8(3);
                w.f32(*speed);
            }
            Event::GameOver { winner } => {
                w.u8(4);
                w.u8(*winner as u8);
            }
        }
    }

    fn read_event(r: &mut Reader) -> WireResult<Event> {
        Ok(match r.u8()? {
            0 => {
                let server = r.u8()? as usize;
                if server >= 2 {
                    return Err(WireError("bad server slot"));
                }
                Event::Serve { server }
            }
            1 => {
                let scorer = r.u8()? as usize;
                if scorer >= 2 {
                    return Err(WireError("bad scorer slot"));
                }
                let score = [u32::from(r.u8()?), u32::from(r.u8()?)];
                Event::Goal { scorer, score }
            }
            2 => {
                let paddle = r.u8()? as usize;
                if paddle >= 2 {
                    return Err(WireError("bad paddle slot"));
                }
                Event::PaddleHit { paddle, speed: r.f32()? }
            }
            3 => Event::WallHit { speed: r.f32()? },
            4 => {
                let winner = r.u8()? as usize;
                if winner >= 2 {
                    return Err(WireError("bad winner slot"));
                }
                Event::GameOver { winner }
            }
            _ => return Err(WireError("unknown event")),
        })
    }

    /// Seat `i` drives paddle `i`: with only two seats there is nothing to shuffle.
    fn start(seed: u64, seats: &[Seat], _participants: usize) -> (Sim, Vec<usize>) {
        (Sim::new(seed), (0..seats.len().min(2)).collect())
    }

    fn participants(_m: &Sim) -> usize {
        2
    }

    fn step(m: &mut Sim, inputs: &[Option<Input>]) -> Vec<Event> {
        let mut all = Inputs::default();
        for (i, input) in inputs.iter().enumerate().take(2) {
            if let Some(input) = input {
                all.0[i] = *input;
            }
        }
        m.step(&all);
        m.drain_events()
    }

    /// A departed player's paddle is handed to the bot AI rather than left dead: with only two seats,
    /// one player leaving would otherwise end the match for the other.
    fn release(m: &mut Sim, participant: usize) {
        if let Some(paddle) = m.paddles.get_mut(participant) {
            paddle.ai = true;
        }
    }

    /// Identical for every viewer: this game has no hidden information (see the module doc).
    fn snapshot(m: &Sim, _participant: Option<usize>) -> AirHockeySnapshot {
        AirHockeySnapshot {
            phase: m.phase,
            paddles: [m.paddles[0].pos, m.paddles[1].pos],
            puck: (m.puck.pos, m.puck.vel),
            score: m.score,
        }
    }

    fn is_over(m: &Sim) -> bool {
        m.is_over()
    }

    fn report(m: &Sim) -> serde_json::Value {
        serde_json::to_value(m.report()).unwrap_or(serde_json::Value::Null)
    }
}

/// What a client keeps: the last two snapshots (to interpolate the puck and the opponent's paddle
/// between), and its own predicted paddle, reconciled from the snapshot's position and replayed forward
/// over inputs the server has not applied yet.
pub struct AirHockeyView {
    participant: Option<usize>,
    predicted: Option<Paddle>,
    /// The visible gap between where prediction put the paddle and where the server says it is; eases out.
    offset: V,
    prev: Option<(AirHockeySnapshot, f64)>,
    cur: Option<(AirHockeySnapshot, f64)>,
    now: f64,
    stats: PredictionStats,
    phase: Phase,
    score: [u32; 2],
    paddles: [V; 2],
    puck: V,
}

impl AirHockeyView {
    /// This client's own slot, once a match has started.
    pub fn participant(&self) -> Option<usize> {
        self.participant
    }
    /// The match phase as this client currently sees it.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn score(&self) -> [u32; 2] {
        self.score
    }
    /// Both paddles as this client should draw them: the local player's own predicted paddle, the
    /// opponent's interpolated between the last two snapshots.
    pub fn paddles(&self) -> [V; 2] {
        self.paddles
    }
    /// The puck, interpolated (or lightly extrapolated on a late packet) between the last two snapshots.
    pub fn puck(&self) -> V {
        self.puck
    }
    /// The exact last snapshot this client received from the server, unsmoothed: since this game has no
    /// hidden information, two clients' own `snapshot()` must always be equal (see
    /// `tests/net.rs`'s `both_clients_snapshot_identical_state_every_tick`).
    pub fn snapshot(&self) -> Option<&AirHockeySnapshot> {
        self.cur.as_ref().map(|(s, _)| s)
    }

    fn refresh(&mut self, now: f64) {
        let Some((cur, arrived)) = &self.cur else { return };
        let t = (((now - arrived) / SNAPSHOT_INTERVAL) as f32).clamp(0., 1.6);
        let mut paddles = cur.paddles;
        let mut puck = cur.puck.0;
        if let Some((prev, _)) = &self.prev {
            paddles[0] = prev.paddles[0].lerp(cur.paddles[0], t);
            paddles[1] = prev.paddles[1].lerp(cur.paddles[1], t);
            puck = prev.puck.0.lerp(cur.puck.0, t);
        }
        if t > 1. {
            // A late packet: keep moving the puck along its last known velocity rather than freezing it.
            puck = puck + cur.puck.1 * ((t - 1.) * SNAPSHOT_INTERVAL as f32);
        }
        if let (Some(slot), Some(mine)) = (self.participant, &self.predicted) {
            if slot < 2 {
                paddles[slot] = mine.pos + self.offset;
            }
        }
        self.phase = cur.phase;
        self.score = cur.score;
        self.paddles = paddles;
        self.puck = puck;
    }
}

impl ClientView<AirHockeyGame> for AirHockeyView {
    fn new() -> Self {
        Self {
            participant: None,
            predicted: None,
            offset: V::ZERO,
            prev: None,
            cur: None,
            now: 0.,
            stats: PredictionStats::default(),
            phase: Phase::Serve { ticks_left: 0, server: 0 },
            score: [0, 0],
            paddles: [V::ZERO; 2],
            puck: V::ZERO,
        }
    }

    /// Reset the predicted paddle to the server's and replay the inputs the server has not applied yet.
    fn on_snapshot(&mut self, s: &AirHockeySnapshot, participant: Option<usize>, pending: &[(u32, Input)], now: f64) {
        self.now = now;
        self.participant = participant.filter(|p| *p < 2);
        if let Some(slot) = self.participant {
            let mut mine = Paddle { pos: s.paddles[slot], vel: V::ZERO, target: s.paddles[slot], ai: false };
            for (_, input) in pending {
                apply_paddle_input(&mut mine, input, slot);
            }
            if let Some(before) = &self.predicted {
                let error = before.pos - mine.pos;
                let distance = error.length();
                self.stats.last_error = distance;
                self.stats.max_error = self.stats.max_error.max(distance);
                if distance > 0.005 {
                    self.stats.corrections += 1;
                    if distance > SNAP_DISTANCE {
                        self.stats.snaps += 1;
                        self.offset = V::ZERO;
                    } else {
                        // Keep showing the old position and ease toward the new one.
                        self.offset = self.offset + error;
                    }
                }
            }
            self.predicted = Some(mine);
        }
        self.prev = self.cur.take().or_else(|| Some((s.clone(), now)));
        self.cur = Some((s.clone(), now));
        self.refresh(now);
    }

    fn on_input(&mut self, input: &Input) {
        if let (Some(slot), Some(paddle)) = (self.participant, self.predicted.as_mut()) {
            apply_paddle_input(paddle, input, slot);
        }
    }

    fn frame(&mut self, now: f64, dt: f32) {
        self.now = now;
        self.offset = self.offset * (-10. * dt.min(0.1)).exp();
        self.refresh(now);
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn prediction(&self) -> PredictionStats {
        self.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(id: u8, choice: u8) -> Seat {
        Seat { id, choice, name: format!("Player {id}") }
    }

    #[test]
    fn seat_order_drives_paddle_order_with_no_shuffling() {
        let seats = [seat(0, 1), seat(1, 2)];
        let (_, assigned) = AirHockeyGame::start(7, &seats, 2);
        assert_eq!(assigned, vec![0, 1]);
    }

    #[test]
    fn the_snapshot_is_identical_whichever_participant_asks() {
        let mut sim = Sim::new(5);
        sim.step(&Inputs::default());
        let a = AirHockeyGame::snapshot(&sim, Some(0));
        let b = AirHockeyGame::snapshot(&sim, Some(1));
        let c = AirHockeyGame::snapshot(&sim, None);
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn release_sends_the_paddle_to_the_bot_ai() {
        let mut sim = Sim::new(6);
        assert!(!sim.paddles[0].ai);
        AirHockeyGame::release(&mut sim, 0);
        assert!(sim.paddles[0].ai);
    }
}
