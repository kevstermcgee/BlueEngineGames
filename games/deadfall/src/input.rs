//! One tick of a player's intent, as the network carries it and as the movement code reads it.
//!
//! Held keys are plain bits; anything that must not be lost (reload, use, melee, weapon switch, drop) is a
//! **counter** that changes each press, so a dropped datagram can never swallow a press: the receiver compares
//! with the last counter it saw. A jump is a press bit computed by the client (an edge needs the previous tick,
//! which a replaying predictor does not have).
use std::f32::consts::TAU;
use vesper3d::viewer::controller::Movement;
use vesper3d::viewer::net::codec::{Reader, WireResult, Writer};

pub const FIRE: u8 = 1;
pub const ADS: u8 = 2;
pub const CROUCH: u8 = 4;
pub const WALK: u8 = 8;
pub const JUMP: u8 = 16;

/// The most an aim may point up or down (radians).
pub const PITCH_LIMIT: f32 = 1.5;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    /// -127..127: right, forward.
    pub right: i8,
    pub forward: i8,
    /// Absolute view angles (radians). Yaw 0 faces -Z, positive turns towards +X.
    pub yaw: f32,
    pub pitch: f32,
    pub buttons: u8,
    pub reload_seq: u8,
    pub use_seq: u8,
    pub melee_seq: u8,
    pub drop_seq: u8,
    pub switch_seq: u8,
    /// Which slot the last switch asked for: 0 primary, 1 secondary, 2 melee, 3 grenade.
    pub switch_to: u8,
    /// Low 16 bits of the server tick of the newest world state this player had on screen when they acted:
    /// lag compensation rewinds other players to it.
    pub seen_tick: u16,
}

impl Input {
    pub fn held(&self, bit: u8) -> bool {
        self.buttons & bit != 0
    }
    /// Stick/keys as -1..1.
    pub fn axes(&self) -> (f32, f32) {
        (self.right as f32 / 127., self.forward as f32 / 127.)
    }
    pub fn set_axes(&mut self, right: f32, forward: f32) {
        let q = |v: f32| (v.clamp(-1., 1.) * 127.).round() as i8;
        self.right = q(right);
        self.forward = q(forward);
    }
    /// The engine movement for this input at `speed_scale` (weapon weight, walking): the controller scales its
    /// run speed by the length of the axes, so the scale multiplies them.
    pub fn movement(&self, speed_scale: f32) -> Movement {
        let (r, f) = self.axes();
        let s = if self.held(WALK) { 0.5 } else { 1. } * speed_scale;
        Movement {
            forward: f * s,
            right: r * s,
            sprint: false,
            jump: self.held(JUMP),
            crouch: self.held(CROUCH),
        }
    }

    pub fn write(&self, w: &mut Writer) {
        w.u8(self.right as u8);
        w.u8(self.forward as u8);
        w.u16((self.yaw.rem_euclid(TAU) / TAU * 65536.).min(65535.) as u16);
        w.u16(((self.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT) / PITCH_LIMIT * 32000.).round() as i16) as u16);
        w.u8(self.buttons);
        w.u8(self.reload_seq);
        w.u8(self.use_seq);
        w.u8(self.melee_seq);
        w.u8(self.drop_seq);
        w.u8(self.switch_seq);
        w.u8(self.switch_to);
        w.u16(self.seen_tick);
    }

    pub fn read(r: &mut Reader) -> WireResult<Self> {
        let right = r.u8()? as i8;
        let forward = r.u8()? as i8;
        let yaw = r.u16()? as f32 / 65536. * TAU;
        let pitch = (r.u16()? as i16) as f32 / 32000. * PITCH_LIMIT;
        Ok(Self {
            right: right.max(-127),
            forward: forward.max(-127),
            yaw,
            pitch: pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT),
            buttons: r.u8()? & (FIRE | ADS | CROUCH | WALK | JUMP),
            reload_seq: r.u8()?,
            use_seq: r.u8()?,
            melee_seq: r.u8()?,
            drop_seq: r.u8()?,
            switch_seq: r.u8()?,
            switch_to: r.u8()?.min(3),
            seen_tick: r.u16()?,
        })
    }
}

/// Angles survive the wire at this precision: a client predicting with the quantised value agrees with the server.
pub fn quantise_angles(yaw: f32, pitch: f32) -> (f32, f32) {
    let mut w = Writer::new();
    Input { yaw, pitch, ..Default::default() }.write(&mut w);
    let bytes = w.finish();
    let i = Input::read(&mut Reader::new(&bytes)).unwrap_or_default();
    (i.yaw, i.pitch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_input_round_trips_within_quantisation() {
        let input = Input {
            right: -127,
            forward: 64,
            yaw: 5.5,
            pitch: -1.2,
            buttons: FIRE | JUMP,
            reload_seq: 9,
            use_seq: 255,
            melee_seq: 1,
            drop_seq: 2,
            switch_seq: 3,
            switch_to: 2,
            seen_tick: 65000,
        };
        let mut w = Writer::new();
        input.write(&mut w);
        let bytes = w.finish();
        let back = Input::read(&mut Reader::new(&bytes)).unwrap();
        assert_eq!((back.right, back.forward, back.buttons, back.use_seq, back.seen_tick), (-127, 64, FIRE | JUMP, 255, 65000));
        assert!((back.yaw - 5.5).abs() < 0.001 && (back.pitch + 1.2).abs() < 0.001);
    }

    #[test]
    fn hostile_bytes_are_an_error_or_clamped_never_a_panic() {
        for len in 0..16 {
            let _ = Input::read(&mut Reader::new(&vec![0xFFu8; len]));
        }
        let back = Input::read(&mut Reader::new(&[0x80, 0x80, 0xFF, 0xFF, 0xFF, 0x7F, 0xFF, 0, 0, 0, 0, 0, 9, 0xFF, 0xFF])).unwrap();
        assert!(back.pitch.abs() <= PITCH_LIMIT && back.right >= -127 && back.switch_to <= 3);
    }

    #[test]
    fn walking_halves_the_speed_and_quantising_is_idempotent() {
        let mut i = Input::default();
        i.set_axes(0., 1.);
        let run = i.movement(1.).forward;
        i.buttons |= WALK;
        assert!((i.movement(1.).forward - run * 0.5).abs() < 1e-4);
        let (y, p) = quantise_angles(1.234, 0.5);
        assert_eq!(quantise_angles(y, p), (y, p));
    }
}
