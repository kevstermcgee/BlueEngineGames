//! Computer players. A bot is an [`Input`] generator: it sees the match the way a player could (walls and smoke
//! block sight, there is a reaction delay), walks the nav graph, picks up weapons, fights at a range that suits
//! its weapon, and throws the odd grenade. It feeds the same input path as a human, so nothing else in the game
//! treats it specially (and a human who leaves is replaced by one mid-match).
use crate::hands::{Busy, Sel};
use crate::input::{Input, ADS, CROUCH, FIRE, JUMP};
use crate::sim::{Match, Player};
use crate::weapons::{self, Class, Fire, Slot, WeaponDef};
use vesper3d::math::V;
use vesper3d::viewer::devkit::Rng;

const NAMES: [&str; 12] =
    ["Rook", "Vesper", "Flint", "Mako", "Tango", "Ember", "Kestrel", "Bishop", "Nomad", "Sable", "Ranger", "Echo"];

pub fn name(n: usize) -> String {
    NAMES[n % NAMES.len()].to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grenade {
    None,
    Switching(u16),
    Pin(u16),
}

#[derive(Clone, Debug)]
pub struct BotState {
    skill: u8,
    react: u32,
    err_deg: f32,
    turn: f32,
    target: Option<usize>,
    spotted_at: u32,
    seen_at: u32,
    last_known: Option<V>,
    path: Vec<V>,
    path_i: usize,
    goal: Option<V>,
    repath_at: u32,
    strafe: f32,
    strafe_until: u32,
    last_pos: V,
    stuck: u32,
    jump_until: u32,
    aim_yaw: f32,
    aim_pitch: f32,
    offset: (f32, f32),
    offset_until: u32,
    trigger_until: u32,
    rest_until: u32,
    grenade: Grenade,
    next_grenade_at: u32,
    crouch_until: u32,
    pub reload_seq: u8,
    pub switch_seq: u8,
    pub switch_to: u8,
    pub use_seq: u8,
    pub melee_seq: u8,
    pub drop_seq: u8,
    wander: u32,
    last_hurt_tick: u32,
}

impl BotState {
    pub fn new(skill: u8, rng: &mut Rng) -> Self {
        let skill = skill.min(2);
        let (react, err_deg, turn) = match skill {
            0 => (42, 4.5, 0.05),
            1 => (26, 2.4, 0.08),
            _ => (13, 1.1, 0.12),
        };
        BotState {
            skill,
            react,
            err_deg,
            turn,
            target: None,
            spotted_at: 0,
            seen_at: 0,
            last_known: None,
            path: Vec::new(),
            path_i: 0,
            goal: None,
            repath_at: 0,
            strafe: 1.,
            strafe_until: 0,
            last_pos: V::ZERO,
            stuck: 0,
            jump_until: 0,
            aim_yaw: rng.range(0., std::f32::consts::TAU),
            aim_pitch: 0.,
            offset: (0., 0.),
            offset_until: 0,
            trigger_until: 0,
            rest_until: 0,
            grenade: Grenade::None,
            next_grenade_at: 60 * 20 + rng.below(60 * 20) as u32,
            crouch_until: 0,
            reload_seq: 0,
            switch_seq: 0,
            switch_to: 0,
            use_seq: 0,
            melee_seq: 0,
            drop_seq: 0,
            wander: 0,
            last_hurt_tick: 0,
        }
    }
}

fn fwd(yaw: f32) -> V {
    V(yaw.sin(), 0., -yaw.cos())
}

fn wrap(a: f32) -> f32 {
    let mut a = a.rem_euclid(std::f32::consts::TAU);
    if a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    a
}

/// How far (metres) a bot likes to fight with this weapon.
fn preferred_range(def: &WeaponDef) -> f32 {
    match def.class {
        Class::Shotgun => 6.,
        Class::Smg => 14.,
        Class::Pistol => 14.,
        Class::AssaultRifle | Class::Lmg => 24.,
        Class::Dmr => 40.,
        Class::Sniper => 55.,
        Class::Launcher => 22.,
        Class::Melee | Class::Grenade => 2.,
    }
}

pub fn think(m: &mut Match, slot: usize, b: &mut BotState) -> Input {
    let tick = m.tick;
    let me: Player = m.players[slot].clone();
    let mut input = Input {
        reload_seq: b.reload_seq,
        switch_seq: b.switch_seq,
        switch_to: b.switch_to,
        use_seq: b.use_seq,
        melee_seq: b.melee_seq,
        drop_seq: b.drop_seq,
        seen_tick: tick as u16,
        ..Default::default()
    };
    if !me.alive {
        return input;
    }
    let eye = me.eye();
    let feet = me.feet();
    let world = m.world.clone();

    // ---- perception (every third tick, staggered) ----
    if (tick + slot as u32).is_multiple_of(3) {
        let mut best: Option<(f32, usize)> = None;
        for e in &m.players {
            if !e.alive || e.team == me.team {
                continue;
            }
            let d = (e.eye() - eye).length();
            if d > 90. {
                continue;
            }
            let dir = (e.eye() - eye).norm();
            let facing = fwd(b.aim_yaw);
            let in_view = dir.0 * facing.0 + dir.2 * facing.2 > 0.25;
            let heard = d < 7. || tick.saturating_sub(e.last_shot_tick) < 20 && d < 35.;
            if !(in_view || heard) {
                continue;
            }
            let chest = e.eye() - V(0., 0.4, 0.);
            if m.can_see(eye, chest) && best.is_none_or(|x| d < x.0) {
                best = Some((d, e.slot));
            }
        }
        match best {
            Some((_, s)) => {
                if b.target != Some(s) || tick.saturating_sub(b.seen_at) > 60 {
                    b.spotted_at = tick;
                }
                b.target = Some(s);
                b.seen_at = tick;
                b.last_known = Some(m.players[s].feet());
            }
            None => {
                if tick.saturating_sub(b.seen_at) > 20 {
                    b.target = None;
                }
            }
        }
    }
    if tick.saturating_sub(b.seen_at) > 60 * 6 {
        b.last_known = None;
    }
    let target = b.target.filter(|t| m.players[*t].alive);

    // ---- the weapon in hand ----
    let weapon = me.weapon();
    let def = weapons::get(weapon);
    let gun = me.inv.gun(me.hands.sel).copied();
    // Manage ammunition and choice while nothing is happening.
    if me.hands.busy == Busy::Idle && b.grenade == Grenade::None {
        let empty = gun.is_some_and(|g| g.mag == 0 && g.reserve == 0);
        if empty || (matches!(me.hands.sel, Sel::Melee) && target.is_none()) {
            let next = if me.inv.primary.is_some_and(|g| g.mag + g.reserve > 0) {
                Some(0)
            } else if me.inv.secondary.is_some_and(|g| g.mag + g.reserve > 0) {
                Some(1)
            } else {
                None
            };
            if let Some(n) = next {
                if me.hands.sel as u8 != n {
                    b.switch_to = n;
                    b.switch_seq = b.switch_seq.wrapping_add(1);
                }
            }
        } else if target.is_none()
            && gun.is_some_and(|g| g.mag * 2 < def.map_or(1, |d| d.mag) && g.reserve > 0)
            && tick.is_multiple_of(40)
        {
            b.reload_seq = b.reload_seq.wrapping_add(1);
        } else if me.hands.sel == Sel::Secondary
            && me.inv.primary.is_some_and(|g| g.mag + g.reserve > 0)
            && target.is_none()
        {
            b.switch_to = 0;
            b.switch_seq = b.switch_seq.wrapping_add(1);
        }
    }

    // ---- where to go ----
    let mut want_dir = V::ZERO;
    let mut goal: Option<V> = None;
    let mut dist_to_target = f32::MAX;
    if let Some(t) = target {
        let tp = m.players[t].feet();
        dist_to_target = (tp - feet).length();
        let pref = def.map_or(15., preferred_range);
        if dist_to_target > pref * 1.3 || !m.can_see(eye, m.players[t].eye() - V(0., 0.4, 0.)) {
            goal = Some(tp);
        } else if dist_to_target < pref * 0.45 && def.is_some_and(|d| d.class != Class::Shotgun) {
            let away = (feet - tp).norm();
            want_dir = V(away.0, 0., away.2) * 0.7;
        }
        // Strafe across the target.
        if tick >= b.strafe_until {
            b.strafe = if m.rng.chance(0.5) { 1. } else { -1. };
            b.strafe_until = tick + 20 + m.rng.below(50) as u32;
        }
        let to = (tp - feet).norm();
        let side = V(-to.2, 0., to.0) * b.strafe * if b.skill == 0 { 0.4 } else { 0.85 };
        want_dir = want_dir + side;
    } else if let Some(k) = b.last_known {
        goal = Some(k);
        if (k - feet).length() < 2. {
            b.last_known = None;
        }
    } else {
        // Fetch a weapon if the hands are empty, otherwise roam the map.
        let want_primary = me.inv.primary.is_none();
        if b.goal.is_none_or(|g| (g - feet).length() < 1.6) || tick >= b.repath_at + 60 * 12 {
            let mut pick: Option<(f32, V)> = None;
            for (i, l) in m.loot.iter().enumerate() {
                if !l.available {
                    continue;
                }
                let Some(d) = weapons::get(l.weapon) else { continue };
                let pos = world.level.loot[i].pos;
                let score = (pos - feet).length()
                    - if want_primary && d.slot == Slot::Primary {
                        50.
                    } else if d.slot == Slot::Grenade && me.inv.grenade_count() < 2 {
                        10.
                    } else {
                        0.
                    }
                    + m.rng.range(0., 15.);
                if d.slot != Slot::Melee && pick.is_none_or(|p| score < p.0) {
                    pick = Some((score, pos));
                }
            }
            b.goal = match pick {
                Some((_, p)) if want_primary || m.rng.chance(0.35) => Some(p),
                _ => world.nav.random_pos(&mut m.rng),
            };
            b.path.clear();
        }
        goal = b.goal;
    }
    if let Some(g) = goal {
        let stale = tick >= b.repath_at || b.path.is_empty() && (g - feet).length() > 1.5;
        if stale {
            b.repath_at = tick + 45;
            b.path = world.nav.path(feet, g).unwrap_or_default();
            b.path_i = 0;
            if b.path.is_empty() && b.target.is_none() {
                b.goal = None;
            }
        }
        while b.path_i < b.path.len() && {
            let p = b.path[b.path_i];
            let d = V(p.0 - feet.0, 0., p.2 - feet.2);
            d.length() < 0.6 && (p.1 - feet.1).abs() < 1.2
        } {
            b.path_i += 1;
        }
        if let Some(p) = b.path.get(b.path_i) {
            let d = V(p.0 - feet.0, 0., p.2 - feet.2);
            if d.length() > 0.05 {
                want_dir = want_dir + d.norm();
            }
        } else if (g - feet).length() > 0.8 {
            let d = V(g.0 - feet.0, 0., g.2 - feet.2);
            want_dir = want_dir + d.norm();
        }
    }

    // ---- stuck recovery ----
    if tick.is_multiple_of(15) {
        let moved = (feet - b.last_pos).length();
        b.last_pos = feet;
        if want_dir.length() > 0.3 && moved < 0.15 {
            b.stuck += 1;
            if b.stuck >= 2 {
                b.jump_until = tick + 10;
                b.strafe = -b.strafe;
                b.strafe_until = tick + 40;
                b.repath_at = tick;
                b.path.clear();
            }
            if b.stuck >= 6 {
                b.goal = None;
                b.stuck = 0;
            }
        } else {
            b.stuck = 0;
        }
    }

    // ---- aiming ----
    let (mut want_yaw, mut want_pitch) = (b.aim_yaw, 0.);
    if let Some(t) = target {
        let e = &m.players[t];
        let vel = e.ctrl.velocity();
        let lead = V(vel.0, 0., vel.2) * (dist_to_target / 120.).min(0.3);
        let headshot = b.skill >= 2 && tick % 90 < 30;
        let aim_at = if headshot { e.eye() } else { e.eye() - V(0., 0.42, 0.) } + lead;
        let d = aim_at - eye;
        want_yaw = d.0.atan2(-d.2);
        want_pitch = d.1.atan2((d.0 * d.0 + d.2 * d.2).sqrt());
        if tick >= b.offset_until {
            let s = (b.err_deg * (1. + dist_to_target / 60.)).to_radians();
            b.offset = (m.rng.range(-s, s), m.rng.range(-s * 0.7, s * 0.7));
            b.offset_until = tick + 14;
        }
        want_yaw += b.offset.0;
        want_pitch += b.offset.1;
    } else if want_dir.length() > 0.1 {
        want_yaw = want_dir.0.atan2(-want_dir.2);
    }
    let turn = b.turn * if target.is_some() { 1. } else { 0.5 };
    let dyaw = wrap(want_yaw - b.aim_yaw);
    b.aim_yaw = (b.aim_yaw + dyaw.clamp(-turn, turn)).rem_euclid(std::f32::consts::TAU);
    b.aim_pitch += (want_pitch - b.aim_pitch).clamp(-turn, turn);
    input.yaw = b.aim_yaw;
    input.pitch = b.aim_pitch.clamp(-1.4, 1.4);

    // ---- movement axes ----
    if want_dir.length() > 0.05 {
        let d = want_dir.norm();
        let f = fwd(b.aim_yaw);
        let r = V(-f.2, 0., f.0); // right = (cos yaw, sin yaw) = (-f.2, f.0)
        let forward = d.0 * f.0 + d.2 * f.2;
        let right = d.0 * r.0 + d.2 * r.2;
        let speed = if target.is_some() && dist_to_target < 10. && b.skill < 2 { 0.7 } else { 1. };
        input.set_axes(right * speed, forward * speed);
    }
    if tick < b.jump_until && tick % 10 == 0 {
        input.buttons |= JUMP;
    }

    // ---- shooting ----
    let mut fire = false;
    let mut ads = false;
    if let (Some(t), Some(d)) = (target, def) {
        let e = &m.players[t];
        let aligned = {
            let diff = wrap(want_yaw - b.aim_yaw).abs().max((want_pitch - b.aim_pitch).abs());
            diff < (0.03 + 0.25 / (1. + dist_to_target * 0.4)).min(0.2)
        };
        let ready = tick.saturating_sub(b.spotted_at) >= b.react;
        let in_range = dist_to_target < d.range_m * 1.2;
        let visible = tick.saturating_sub(b.seen_at) < 10;
        if ready && aligned && in_range && visible && me.hands.busy != Busy::Draw && b.grenade == Grenade::None {
            if matches!(d.sight, weapons::Sight::Scope { .. }) && dist_to_target > 20. {
                ads = true;
            }
            match d.fire {
                Fire::Auto => {
                    // Bursts whose length suits the distance, with rests between.
                    if tick >= b.rest_until {
                        if tick >= b.trigger_until {
                            b.trigger_until = tick + (4. + 18. / (1. + dist_to_target / 10.)) as u32;
                            b.rest_until = b.trigger_until + 6 + m.rng.below(14) as u32;
                        }
                        fire = tick < b.trigger_until;
                    }
                }
                Fire::Burst { .. } | Fire::Semi | Fire::Cycle { .. } | Fire::Throw | Fire::Swing => {
                    // Tap: press for a few ticks, release for a few.
                    let gap = if d.class == Class::Sniper { 50 } else { 7 };
                    fire = tick % gap < 3;
                }
            }
            if ads && me.hands.ads < 0.8 {
                fire = false;
            }
        }
        // Crouch to steady a long shot when nobody is shooting back.
        if ready && dist_to_target > 30. && e.last_shot_tick + 40 < tick && tick >= b.crouch_until {
            b.crouch_until = tick + 45;
        }
        // Close enough to stab and the gun is empty or awkward: quick melee.
        if dist_to_target < 1.7 && d.class != Class::Shotgun && d.class != Class::Smg && tick.is_multiple_of(60) {
            b.melee_seq = b.melee_seq.wrapping_add(1);
        }
    }
    // A grenade now and then at a visible enemy in mid range.
    match b.grenade {
        Grenade::None => {
            if let Some(t) = target {
                if tick >= b.next_grenade_at
                    && me.inv.grenades[0] != 0
                    && (8. ..32.).contains(&dist_to_target)
                    && me.hands.busy == Busy::Idle
                    && m.rng.chance(0.02)
                {
                    b.switch_to = 3;
                    b.switch_seq = b.switch_seq.wrapping_add(1);
                    b.grenade = Grenade::Switching(40);
                    b.next_grenade_at = tick + 60 * 25;
                    let _ = t;
                }
            }
        }
        Grenade::Switching(n) => {
            b.grenade = if n == 0 { Grenade::Pin(12) } else { Grenade::Switching(n - 1) };
        }
        Grenade::Pin(n) => {
            if n > 0 {
                fire = true;
                b.grenade = Grenade::Pin(n - 1);
                // Lob it a little high so it lands near the target.
                input.pitch = (input.pitch + 0.25).clamp(-1.4, 1.4);
            } else {
                fire = false;
                b.grenade = Grenade::None;
                b.switch_to = if me.inv.primary.is_some() { 0 } else { 1 };
                b.switch_seq = b.switch_seq.wrapping_add(1);
            }
        }
    }
    if fire {
        input.buttons |= FIRE;
    }
    if ads {
        input.buttons |= ADS;
    }
    if tick < b.crouch_until {
        input.buttons |= CROUCH;
    }
    input.reload_seq = b.reload_seq;
    input.switch_seq = b.switch_seq;
    input.switch_to = b.switch_to;
    input.melee_seq = b.melee_seq;
    input.use_seq = b.use_seq;
    input.drop_seq = b.drop_seq;
    let _ = b.last_hurt_tick;
    let _ = b.wander;
    input
}
