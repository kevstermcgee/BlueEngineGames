//! From devices to [`Input`]: keyboard and mouse like Counter-Strike, and a controller on its standard layout.
//!
//! | | Keyboard / mouse | Controller |
//! |---|---|---|
//! | move / look | WASD / mouse | left stick / right stick |
//! | fire / aim | left / right mouse | right trigger / left trigger |
//! | jump / crouch | Space / Ctrl (hold) | A / B (hold) |
//! | reload / use | R / E | X / right bumper |
//! | weapons | 1 primary, 2 secondary, 3 knife, 4 grenade, wheel | D-pad up / right / down, left bumper (grenade), Y (other gun) |
//! | quick knife / drop | Q / G | right stick click / D-pad left |
//! | sprint | Shift (hold) | left stick click (hold) |
//! | walk (quiet) | Alt (hold) | — |
//! | scoreboard / menu | Tab / Esc | Back / Start |
//!
//! Gameplay keys use the engine native key state so packaged Windows clients share the same input path.
use crate::input::{Input, ADS, CROUCH, FIRE, JUMP, PITCH_LIMIT, SPRINT, USE_HELD, WALK};
use crate::prefs::Prefs;
use macroquad::prelude::*;
use vesper3d::viewer::game_client::GameShell;
use vesper3d::viewer::game_input::ClientInput;
use vesper3d::viewer::gamepad::Button;

/// A scripted player for runs nobody can play: `--script "fwd:60-300,turn:0.01@60-300,ads:100-200,fire:120-150,reload@220"`.
/// `name:a-b` holds from frame a to b, `name@n` presses at frame n. Names: fwd back left right ads fire crouch walk sprint
/// jump reload use melee drop slot1..slot4, `turn:RATE@a-b` (radians per frame, yaw) and `pitch:RATE@a-b`.
#[derive(Clone, Debug, Default)]
pub struct Script {
    cues: Vec<(String, f32, u32, u32)>,
    pub frame: u32,
}

impl Script {
    pub fn parse(text: &str) -> Script {
        let mut cues = Vec::new();
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (name, rest) = part.split_once([':', '@']).map_or((part, ""), |(a, b)| (a, b));
            let (rate, range) = match rest.split_once('@') {
                Some((r, range)) => (r.parse().unwrap_or(0.), range),
                None => (1., rest),
            };
            let (a, b) = match range.split_once('-') {
                Some((a, b)) => (a.parse().unwrap_or(0), b.parse().unwrap_or(0)),
                None => {
                    let n = range.parse().unwrap_or(0);
                    (n, n)
                }
            };
            cues.push((name.to_string(), rate, a, b));
        }
        Script { cues, frame: 0 }
    }
    /// Whether a one-frame press cue fires on the current frame (menus read these: accept, up, down, back).
    pub fn pressed_now(&self, name: &str) -> bool {
        self.pressed(name)
    }
    fn held(&self, name: &str) -> bool {
        self.cues.iter().any(|(n, _, a, b)| n == name && self.frame >= *a && self.frame <= *b)
    }
    fn pressed(&self, name: &str) -> bool {
        self.cues.iter().any(|(n, _, a, _)| n == name && self.frame == *a)
    }
    fn rate(&self, name: &str) -> f32 {
        self.cues.iter().filter(|(n, _, a, b)| n == name && self.frame >= *a && self.frame <= *b).map(|c| c.1).sum()
    }
}

pub struct Controls {
    pub script: Option<Script>,
    pub yaw: f32,
    pub pitch: f32,
    reload_seq: u8,
    use_seq: u8,
    use_held: bool,
    melee_seq: u8,
    drop_seq: u8,
    switch_seq: u8,
    switch_to: u8,
    jump_latch: bool,
    /// Which slot the last wheel turn / Y press landed on, for cycling.
    cycle: u8,
    pub scoreboard: bool,
    /// Held state read at frame start, used by every tick of the frame.
    fire: bool,
    ads: bool,
    crouch: bool,
    walk: bool,
    sprint: bool,
    axes: (f32, f32),
}

impl Controls {
    pub fn new() -> Self {
        Controls {
            script: None,
            yaw: 0.,
            pitch: 0.,
            reload_seq: 0,
            use_seq: 0,
            use_held: false,
            melee_seq: 0,
            drop_seq: 0,
            switch_seq: 0,
            switch_to: 0,
            jump_latch: false,
            cycle: 1,
            scoreboard: false,
            fire: false,
            ads: false,
            crouch: false,
            walk: false,
            sprint: false,
            axes: (0., 0.),
        }
    }

    /// Start a new life or match facing `yaw`, and forget held state.
    pub fn face(&mut self, yaw: f32, pitch: f32) {
        self.yaw = yaw;
        self.pitch = pitch;
        self.jump_latch = false;
    }

    /// Copy the server's press counters so a new life does not replay old presses.
    pub fn sync_counters(&mut self, reload: u8, use_: u8, melee: u8, drop: u8, switch: u8) {
        self.reload_seq = reload;
        self.use_seq = use_;
        self.melee_seq = melee;
        self.drop_seq = drop;
        self.switch_seq = switch;
    }

    fn switch(&mut self, slot: u8) {
        self.switch_to = slot;
        self.switch_seq = self.switch_seq.wrapping_add(1);
        self.cycle = slot;
    }

    /// Read the devices once per frame: look, presses (counters), held state.
    /// `ads_ratio` is how much narrower the aiming field of view is (1 = not aiming); mouse speed follows it.
    pub fn frame(
        &mut self,
        input: &ClientInput,
        shell: &GameShell,
        dt: f32,
        prefs: &Prefs,
        ads_ratio: f32,
        has: [bool; 4],
    ) {
        if let Some(sc) = self.script.as_ref() {
            let sc = sc.clone();
            self.yaw = (self.yaw + sc.rate("turn")).rem_euclid(std::f32::consts::TAU);
            self.pitch = (self.pitch + sc.rate("pitch")).clamp(-PITCH_LIMIT, PITCH_LIMIT);
            self.axes = (
                f32::from(sc.held("right")) - f32::from(sc.held("left")),
                f32::from(sc.held("fwd")) - f32::from(sc.held("back")),
            );
            self.fire = sc.held("fire");
            self.ads = sc.held("ads");
            self.crouch = sc.held("crouch");
            self.walk = sc.held("walk");
            self.sprint = sc.held("sprint");
            self.use_held = sc.held("use");
            self.scoreboard = sc.held("scores");
            if sc.pressed("jump") {
                self.jump_latch = true;
            }
            for (name, f) in [("reload", 0), ("use", 1), ("melee", 2), ("drop", 3)] {
                if sc.pressed(name) {
                    match f {
                        0 => self.reload_seq = self.reload_seq.wrapping_add(1),
                        1 => self.use_seq = self.use_seq.wrapping_add(1),
                        2 => self.melee_seq = self.melee_seq.wrapping_add(1),
                        _ => self.drop_seq = self.drop_seq.wrapping_add(1),
                    }
                }
            }
            for (i, name) in ["slot1", "slot2", "slot3", "slot4"].iter().enumerate() {
                if sc.pressed(name) {
                    self.switch(i as u8);
                }
            }
            return;
        }
        let live = shell.accepting_input();
        if !live {
            self.fire = false;
            self.ads = false;
            self.scoreboard = false;
            self.use_held = false;
            self.crouch = false;
            self.walk = false;
            self.sprint = false;
            self.jump_latch = false;
            self.axes = (0., 0.);
            return;
        }
        // Look: mouse needs the captured cursor, the stick does not.
        let [dx, dy] = input.look_delta_with(shell, dt);
        let k = ads_ratio.clamp(0.1, 1.);
        let stick = input.stick_look(shell, dt);
        let mouse = [dx - stick[0], dy - stick[1]];
        let (lx, ly) = (
            mouse[0] * prefs.sensitivity * k + stick[0] * prefs.stick_sensitivity * k,
            mouse[1] * prefs.sensitivity * k + stick[1] * prefs.stick_sensitivity * k,
        );
        self.yaw = (self.yaw + lx).rem_euclid(std::f32::consts::TAU);
        let sign = if prefs.invert_y { -1. } else { 1. };
        self.pitch = (self.pitch - ly * sign).clamp(-PITCH_LIMIT, PITCH_LIMIT);

        let pad = input.gamepad();
        let key = |k| input.down(k);
        let pressed = |k| input.pressed(k);
        // Movement.
        let mut right = f32::from(key(KeyCode::D)) - f32::from(key(KeyCode::A));
        let mut forward = f32::from(key(KeyCode::W)) - f32::from(key(KeyCode::S));
        right += pad.left_stick[0];
        forward += pad.left_stick[1];
        let len = right.hypot(forward).max(1.);
        self.axes = (right / len, forward / len);
        // Held buttons.
        self.fire = is_mouse_button_down(MouseButton::Left) || pad.triggers[1] > 0.4 || pad.down(Button::RightTrigger2);
        self.ads = is_mouse_button_down(MouseButton::Right) || pad.triggers[0] > 0.4 || pad.down(Button::LeftTrigger2);
        self.crouch = key(KeyCode::LeftControl) || key(KeyCode::C) || pad.down(Button::East);
        self.walk = key(KeyCode::LeftAlt) || key(KeyCode::RightAlt);
        self.sprint = key(KeyCode::LeftShift) || key(KeyCode::RightShift) || pad.down(Button::LeftThumb);
        self.scoreboard = key(KeyCode::Tab) || pad.down(Button::Select);
        // Presses.
        if pressed(KeyCode::Space) || pad.pressed(Button::South) {
            self.jump_latch = true;
        }
        if pressed(KeyCode::R) || pad.pressed(Button::West) {
            self.reload_seq = self.reload_seq.wrapping_add(1);
        }
        self.use_held = key(KeyCode::E) || pad.down(Button::RightTrigger);
        if pressed(KeyCode::E) || pad.pressed(Button::RightTrigger) {
            self.use_seq = self.use_seq.wrapping_add(1);
        }
        if pressed(KeyCode::Q) || pad.pressed(Button::RightThumb) {
            self.melee_seq = self.melee_seq.wrapping_add(1);
        }
        if pressed(KeyCode::G) || pad.pressed(Button::DPadLeft) {
            self.drop_seq = self.drop_seq.wrapping_add(1);
        }
        for (k, slot) in [(KeyCode::Key1, 0u8), (KeyCode::Key2, 1), (KeyCode::Key3, 2), (KeyCode::Key4, 3)] {
            if pressed(k) {
                self.switch(slot);
            }
        }
        if pad.pressed(Button::DPadUp) {
            self.switch(0);
        }
        if pad.pressed(Button::DPadRight) {
            self.switch(1);
        }
        if pad.pressed(Button::DPadDown) {
            self.switch(2);
        }
        if pad.pressed(Button::LeftTrigger) {
            self.switch(3);
        }
        // Wheel and Y step through what is carried.
        let wheel = mouse_wheel().1;
        let step = if wheel > 0. {
            -1
        } else if wheel < 0. || pad.pressed(Button::North) {
            1
        } else {
            0
        };
        if step != 0 {
            let mut slot = self.cycle as i32;
            for _ in 0..4 {
                slot = (slot + step).rem_euclid(4);
                if has[slot as usize] {
                    self.switch(slot as u8);
                    break;
                }
            }
        }
    }

    /// One fixed tick of input (the caller has the newest server tick the screen shows, for lag compensation).
    pub fn tick(&mut self, seen_tick: u16) -> Input {
        let mut i = Input {
            yaw: self.yaw,
            pitch: self.pitch,
            reload_seq: self.reload_seq,
            use_seq: self.use_seq,
            melee_seq: self.melee_seq,
            drop_seq: self.drop_seq,
            switch_seq: self.switch_seq,
            switch_to: self.switch_to,
            seen_tick,
            ..Default::default()
        };
        i.set_axes(self.axes.0, self.axes.1);
        if self.fire {
            i.buttons |= FIRE;
        }
        if self.ads {
            i.buttons |= ADS;
        }
        if self.crouch {
            i.buttons |= CROUCH;
        }
        if self.walk {
            i.buttons |= WALK;
        }
        if self.sprint {
            i.buttons |= SPRINT;
        }
        if self.use_held {
            i.buttons |= USE_HELD;
        }
        if self.jump_latch {
            i.buttons |= JUMP;
            self.jump_latch = false;
        }
        i
    }
}

impl Default for Controls {
    fn default() -> Self {
        Self::new()
    }
}
