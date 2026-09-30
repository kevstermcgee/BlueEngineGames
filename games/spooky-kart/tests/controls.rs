//! Keyboard and controller mapping, tested without any device.
use spooky_kart::controls::{accepts_input, resolve, steer_curve, Raw};

#[test]
fn the_right_trigger_is_analog_gas_and_the_left_trigger_brakes_and_reverses() {
    assert_eq!(resolve(&Raw { pad_gas: 0.5, ..Default::default() }).throttle, 0.5);
    assert_eq!(resolve(&Raw { pad_brake: 1., ..Default::default() }).throttle, -1.);
    assert_eq!(resolve(&Raw { pad_gas: 1., pad_brake: 0.25, ..Default::default() }).throttle, 0.75);
}

#[test]
fn the_a_button_is_full_gas_for_pads_without_analog_triggers() {
    assert_eq!(resolve(&Raw { pad_gas_button: true, ..Default::default() }).throttle, 1.);
    assert_eq!(resolve(&Raw { pad_gas_button: true, pad_brake: 1., ..Default::default() }).throttle, 0.);
}

#[test]
fn the_keyboard_wins_gas_when_a_key_is_held_and_the_pad_fills_in_otherwise() {
    assert_eq!(resolve(&Raw { key_throttle: -1., pad_gas: 1., ..Default::default() }).throttle, -1.);
    assert_eq!(resolve(&Raw { key_throttle: 0., pad_gas: 1., ..Default::default() }).throttle, 1.);
}

#[test]
fn the_stick_curve_is_gentle_near_the_centre_and_full_at_the_edge() {
    assert!(steer_curve(0.5) < 0.35 && steer_curve(0.5) > 0.2, "half a stick is a gentle turn");
    assert_eq!(steer_curve(1.), 1.);
    assert_eq!(steer_curve(-1.), -1.);
    assert_eq!(steer_curve(0.), 0.);
    assert_eq!(steer_curve(-0.5), -steer_curve(0.5), "symmetric left and right");
    assert_eq!(steer_curve(3.), 1., "out-of-range samples clamp");
}

#[test]
fn steering_takes_whichever_device_is_pushed_harder() {
    let both = resolve(&Raw { key_steer: 1., pad_steer: -0.4, ..Default::default() });
    assert_eq!(both.steer, 1.);
    let stick = resolve(&Raw { key_steer: 0., pad_steer: -0.9, ..Default::default() });
    assert!(stick.steer < -0.8);
    assert_eq!(resolve(&Raw::default()).steer, 0.);
}

#[test]
fn either_bumper_or_shift_drifts_and_the_perk_is_left_to_the_caller() {
    assert!(resolve(&Raw { pad_drift: true, ..Default::default() }).drift);
    assert!(resolve(&Raw { key_drift: true, ..Default::default() }).drift);
    assert!(!resolve(&Raw::default()).drift);
    assert!(!resolve(&Raw { pad_drift: true, key_drift: true, ..Default::default() }).perk);
}

#[test]
fn nothing_pressed_means_no_input() {
    assert_eq!(resolve(&Raw::default()), Default::default());
}

#[test]
fn input_is_read_whenever_the_game_is_unpaused_and_focused_and_never_otherwise() {
    // Regression: the race once gated input on `GameShell::playing()`, which needs a captured mouse. This game
    // has no mouse look, so it was always false and no key or button worked in the race (the menus did).
    assert!(accepts_input(false, true), "an unpaused, focused game takes input");
    assert!(!accepts_input(true, true), "the pause menu takes it instead");
    assert!(!accepts_input(false, false), "an unfocused window takes none");
    assert!(!accepts_input(true, false));
}
