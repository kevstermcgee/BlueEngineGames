//! How devices become a `KartInput`. Kept pure so the mapping is tested without a keyboard or a pad.
//!
//! Keyboard: W/S or Up/Down gas and brake, A/D or Left/Right steer, Shift drifts, Space uses the perk.
//! Controller: right trigger (or A) gas, left trigger brake and reverse, left stick steers, either bumper
//! drifts, X uses the perk. Start pauses; the D-pad and stick move through menus and A confirms.
use crate::kart::KartInput;

/// One frame of raw device state, already read from the keyboard and the controller.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Raw {
    /// Keyboard gas (+1) and brake (-1).
    pub key_throttle: f32,
    /// Keyboard steer right (+1) or left (-1).
    pub key_steer: f32,
    pub key_drift: bool,
    /// Left stick X, -1..1 (already dead-zoned by the engine).
    pub pad_steer: f32,
    /// Right and left trigger pressure, 0..1.
    pub pad_gas: f32,
    pub pad_brake: f32,
    /// A button held: full gas for pads without analog triggers.
    pub pad_gas_button: bool,
    /// Either bumper held.
    pub pad_drift: bool,
}

/// Stick response: gentle near the centre for fine corrections, full at the edge.
pub fn steer_curve(x: f32) -> f32 {
    let x = x.clamp(-1., 1.);
    x.signum() * x.abs().powf(1.6)
}

/// The held part of the input; the perk is an edge the caller adds.
pub fn resolve(raw: &Raw) -> KartInput {
    let pad_gas = if raw.pad_gas_button { 1.0f32.max(raw.pad_gas) } else { raw.pad_gas };
    let pad_throttle = pad_gas.clamp(0., 1.) - raw.pad_brake.clamp(0., 1.);
    let throttle = if raw.key_throttle != 0. { raw.key_throttle } else { pad_throttle }.clamp(-1., 1.);
    let pad_steer = steer_curve(raw.pad_steer);
    let key_steer = raw.key_steer.clamp(-1., 1.);
    let steer = if pad_steer.abs() > key_steer.abs() { pad_steer } else { key_steer };
    KartInput { throttle, steer, drift: raw.key_drift || raw.pad_drift, perk: false }
}

/// Whether the race should read the player's devices this frame.
///
/// Not `GameShell::playing()`: that is only true while the *mouse is captured*, which a game with no mouse look
/// never asks for, so it is permanently false and silently drops every key and button (the menus, which read
/// keys directly, would still work). A kart game accepts input whenever its pause menu is closed and the window
/// has focus; `unattended` runs (capture, script) have no window focus to lose.
pub fn accepts_input(paused: bool, focused: bool) -> bool {
    !paused && focused
}
