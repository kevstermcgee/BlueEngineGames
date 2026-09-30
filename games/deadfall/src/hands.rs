//! What a player's hands are doing: which weapon is out, reloading, cycling, aiming, throwing, swinging.
//!
//! This is the one place the weapon state machine lives, and it is pure: the server steps it for every player,
//! and a client steps its own copy on top of each snapshot (replaying the inputs the server has not applied
//! yet), so firing, ammunition, reloads and recoil feel instant and still agree with the server. What happens in
//! the world because of a trigger pull (tracing bullets, spawning a grenade) is the caller's business: `tick`
//! only reports it as [`Out`].
use crate::input::{Input, ADS, FIRE};
use crate::weapons::{self, Class, Fire, Sight, Slot, WeaponDef, WeaponId, CROUCH_SPREAD};

pub const DT: f32 = 1. / 60.;

/// Seconds to whole ticks (at least one).
pub fn ticks(seconds: f32) -> u16 {
    (seconds * 60.).ceil().clamp(1., 65000.) as u16
}

/// One firearm the player carries, with its own ammunition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gun {
    pub id: WeaponId,
    pub mag: u16,
    pub reserve: u16,
}

impl Gun {
    /// A new weapon with a full magazine and its standard reserve.
    pub fn fresh(id: WeaponId) -> Option<Gun> {
        weapons::get(id).map(|d| Gun { id, mag: d.mag, reserve: d.reserve })
    }
}

/// Everything a player carries: two firearms, one melee weapon, up to two grenades.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inventory {
    pub primary: Option<Gun>,
    pub secondary: Option<Gun>,
    /// `0` = none (never the case for a live player: they always have the starting knife).
    pub melee: WeaponId,
    pub grenades: [WeaponId; 2],
}

impl Inventory {
    /// The loadout both teams spawn with: the starting sidearm and the combat knife.
    pub fn starting() -> Self {
        Self {
            primary: None,
            secondary: weapons::id_of(weapons::START_SIDEARM).and_then(Gun::fresh),
            melee: weapons::id_of(weapons::START_MELEE).unwrap_or(0),
            grenades: [0, 0],
        }
    }
    pub fn gun(&self, sel: Sel) -> Option<&Gun> {
        match sel {
            Sel::Primary => self.primary.as_ref(),
            Sel::Secondary => self.secondary.as_ref(),
            _ => None,
        }
    }
    pub fn gun_mut(&mut self, sel: Sel) -> Option<&mut Gun> {
        match sel {
            Sel::Primary => self.primary.as_mut(),
            Sel::Secondary => self.secondary.as_mut(),
            _ => None,
        }
    }
    /// The weapon id in a slot (`0` when empty).
    pub fn id_in(&self, sel: Sel) -> WeaponId {
        match sel {
            Sel::Primary => self.primary.map_or(0, |g| g.id),
            Sel::Secondary => self.secondary.map_or(0, |g| g.id),
            Sel::Melee => self.melee,
            Sel::Grenade => self.grenades[0],
        }
    }
    pub fn grenade_count(&self) -> usize {
        self.grenades.iter().filter(|g| **g != 0).count()
    }
}

/// Which slot is out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    Primary = 0,
    Secondary = 1,
    Melee = 2,
    Grenade = 3,
}

impl Sel {
    pub fn from_index(i: u8) -> Sel {
        match i {
            0 => Sel::Primary,
            1 => Sel::Secondary,
            2 => Sel::Melee,
            _ => Sel::Grenade,
        }
    }
    pub fn slot(self) -> Slot {
        match self {
            Sel::Primary => Slot::Primary,
            Sel::Secondary => Slot::Secondary,
            Sel::Melee => Slot::Melee,
            Sel::Grenade => Slot::Grenade,
        }
    }
}

/// What the hands are busy with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Busy {
    Idle = 0,
    Draw = 1,
    Reload = 2,
    /// Loading one shell; continues shell by shell until full.
    ShellLoad = 3,
    Cycle = 4,
    Swing = 5,
    Throw = 6,
}

impl Busy {
    pub fn from_index(i: u8) -> Busy {
        match i {
            1 => Busy::Draw,
            2 => Busy::Reload,
            3 => Busy::ShellLoad,
            4 => Busy::Cycle,
            5 => Busy::Swing,
            6 => Busy::Throw,
            _ => Busy::Idle,
        }
    }
}

/// The last value of each press counter the hands have acted on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    pub reload: u8,
    pub use_: u8,
    pub melee: u8,
    pub drop: u8,
    pub switch: u8,
}

/// The weapon state machine of one player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hands {
    pub sel: Sel,
    pub busy: Busy,
    /// Ticks left of the busy action and its full length (for animation progress).
    pub left: u16,
    pub total: u16,
    /// Ticks until the next shot may leave (fractional so 800 rpm averages exactly).
    pub cooldown: f32,
    pub burst_left: u8,
    pub burst_gap: u16,
    /// Shots fired in the current spray; decays while not firing.
    pub recoil: f32,
    /// Aim-down-sights amount, 0..1.
    pub ads: f32,
    /// A grenade's pin is out: release the trigger to throw.
    pub pin: bool,
    /// Weapon to return to after a quick melee or a thrown last grenade.
    pub return_to: Option<Sel>,
    pub prev_fire: bool,
    pub prev_alt: bool,
    /// Tick-countdown value at which a swing lands (0 = none pending).
    pub impact_at: u16,
    pub heavy: bool,
    pub seen: Seen,
}

impl Default for Hands {
    fn default() -> Self {
        Self {
            sel: Sel::Secondary,
            busy: Busy::Idle,
            left: 0,
            total: 0,
            cooldown: 0.,
            burst_left: 0,
            burst_gap: 0,
            recoil: 0.,
            ads: 0.,
            pin: false,
            return_to: None,
            prev_fire: false,
            prev_alt: false,
            impact_at: 0,
            heavy: false,
            seen: Seen::default(),
        }
    }
}

/// What the body is doing, which decides how accurate a shot is.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ctx {
    /// Horizontal speed as a fraction of the unarmed run speed (0 still, 1 flat out).
    pub speed_frac: f32,
    pub crouched: bool,
    pub airborne: bool,
}

/// What a tick of the hands asks the world to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    /// `pellets` bullets leave the muzzle. `punch` is the (pitch up, yaw) view kick in radians the shot is
    /// aimed with; `spread` is the random cone half-angle in degrees.
    Shot { weapon: WeaponId, pellets: u8, punch: (f32, f32), spread: f32 },
    /// A launcher fires its projectile.
    Launch { weapon: WeaponId, punch: (f32, f32) },
    /// A grenade leaves the hand (`lob`: a short underarm toss).
    Throw { weapon: WeaponId, lob: bool },
    /// A melee strike lands now.
    Strike { weapon: WeaponId, heavy: bool },
    /// The trigger was pulled on an empty weapon.
    Dry { weapon: WeaponId },
    ReloadStarted { weapon: WeaponId },
    Switched { weapon: WeaponId },
}

/// The view kick after `recoil` shots of a spray: (pitch up, yaw), radians. Deterministic, so a client and the
/// server agree. Kick per shot falls off with the spray, the sideways kick wanders left and right.
pub fn punch(def: &WeaponDef, recoil: f32) -> (f32, f32) {
    let r = recoil.max(0.);
    let up = def.recoil_up_deg * (1. - (-0.35 * r).exp()) / 0.35;
    let side = def.recoil_side_deg * (0.9 * r).sin() * (r / 6.).min(1.);
    (up.to_radians(), side.to_radians())
}

/// Cone half-angle (degrees) for a shot now.
pub fn spread_deg(def: &WeaponDef, ads: f32, ctx: &Ctx, recoil: f32) -> f32 {
    let mut s = def.spread_deg + def.move_spread_deg * ctx.speed_frac.clamp(0., 1.);
    if ctx.airborne {
        s = s * 2.5 + 1.;
    } else if ctx.crouched && ctx.speed_frac < 0.15 {
        s *= CROUCH_SPREAD;
    }
    let aim = 1. + (def.ads_spread_mult - 1.) * ads.clamp(0., 1.);
    (s * aim + recoil * def.spread_deg * 0.04).max(0.)
}

fn period_ticks(def: &WeaponDef) -> f32 {
    3600. / def.rpm.max(30.)
}

impl Hands {
    pub fn new(inv: &Inventory) -> Self {
        let sel = if inv.primary.is_some() { Sel::Primary } else { Sel::Secondary };
        Hands { sel, ..Default::default() }
    }

    /// The weapon currently out (`0` when its slot is empty).
    pub fn weapon(&self, inv: &Inventory) -> WeaponId {
        inv.id_in(self.sel)
    }

    /// Progress of the busy action, 0 at its start to 1 at its end.
    pub fn progress(&self) -> f32 {
        if self.total == 0 {
            0.
        } else {
            1. - self.left as f32 / self.total as f32
        }
    }

    fn start(&mut self, busy: Busy, t: u16) {
        self.busy = busy;
        self.total = t;
        self.left = t;
    }

    fn select(&mut self, inv: &Inventory, sel: Sel, out: &mut Vec<Out>) {
        let id = inv.id_in(sel);
        if id == 0 {
            return;
        }
        if sel == self.sel && sel != Sel::Grenade {
            return;
        }
        self.sel = sel;
        self.pin = false;
        self.burst_left = 0;
        self.impact_at = 0;
        self.ads = 0.;
        let draw = weapons::get(id).map_or(0.4, |d| d.draw_s);
        self.start(Busy::Draw, ticks(draw));
        out.push(Out::Switched { weapon: id });
    }

    /// Advance one tick. `inv` is changed by reloads, shots and throws.
    pub fn tick(&mut self, inv: &mut Inventory, input: &Input, ctx: &Ctx) -> Vec<Out> {
        let mut out = Vec::new();
        let fire = input.held(FIRE);
        let alt = input.held(ADS);
        let fire_edge = fire && !self.prev_fire;
        let alt_edge = alt && !self.prev_alt;
        self.prev_fire = fire;
        self.prev_alt = alt;
        self.cooldown = (self.cooldown - 1.).max(-1.);
        self.burst_gap = self.burst_gap.saturating_sub(1);

        // Requests carried by counters.
        if input.switch_seq != self.seen.switch {
            self.seen.switch = input.switch_seq;
            let want = Sel::from_index(input.switch_to);
            if want == Sel::Grenade && self.sel == Sel::Grenade && inv.grenades[1] != 0 {
                inv.grenades.swap(0, 1);
                self.sel = Sel::Melee; // re-selecting below draws the other grenade
            }
            self.select(inv, want, &mut out);
        }
        let quick_melee = input.melee_seq != self.seen.melee;
        self.seen.melee = input.melee_seq;
        if quick_melee && self.sel != Sel::Melee && inv.melee != 0 && self.busy != Busy::Throw {
            self.return_to = Some(self.sel);
            self.sel = Sel::Melee;
            self.pin = false;
            self.burst_left = 0;
            self.ads = 0.;
            self.busy = Busy::Idle;
            self.left = 0;
            out.push(Out::Switched { weapon: inv.melee });
            self.swing(inv, false);
        }
        let reload_req = input.reload_seq != self.seen.reload;
        self.seen.reload = input.reload_seq;

        let id = inv.id_in(self.sel);
        let Some(def) = weapons::get(id) else {
            self.ads = 0.;
            return out;
        };

        // Busy countdown.
        if self.left > 0 {
            self.left -= 1;
            if self.impact_at != 0 && self.left == self.impact_at {
                out.push(Out::Strike { weapon: id, heavy: self.heavy });
                self.impact_at = 0;
            }
            if self.left == 0 {
                self.finish_busy(inv, def, &mut out);
            }
        }

        // Aiming down the sights.
        let can_aim = matches!(self.busy, Busy::Idle | Busy::Cycle)
            && !matches!(def.sight, Sight::None)
            && !matches!(def.class, Class::Grenade | Class::Melee);
        let target = if alt && can_aim { 1. } else { 0. };
        let rate = DT / def.ads_s.max(0.03);
        self.ads = (self.ads + (target - self.ads).clamp(-rate, rate)).clamp(0., 1.);
        self.recoil = (self.recoil - def.recoil_recover * DT).max(0.);

        match def.class {
            Class::Melee => self.melee_tick(inv, def, fire_edge, alt_edge, &mut out),
            Class::Grenade => self.grenade_tick(inv, def, fire, alt, &mut out),
            _ => self.gun_tick(inv, def, ctx, fire, fire_edge, reload_req, &mut out),
        }
        out
    }

    fn finish_busy(&mut self, inv: &mut Inventory, def: &WeaponDef, out: &mut Vec<Out>) {
        let was = self.busy;
        self.busy = Busy::Idle;
        match was {
            Busy::Reload => {
                let sel = self.sel;
                if let Some(g) = inv.gun_mut(sel) {
                    let take = def.mag.saturating_sub(g.mag).min(g.reserve);
                    g.mag += take;
                    g.reserve -= take;
                }
            }
            Busy::ShellLoad => {
                let sel = self.sel;
                let mut more = false;
                if let Some(g) = inv.gun_mut(sel) {
                    if g.mag < def.mag && g.reserve > 0 {
                        g.mag += 1;
                        g.reserve -= 1;
                    }
                    more = g.mag < def.mag && g.reserve > 0;
                }
                if more {
                    self.start(Busy::ShellLoad, ticks(def.reload_s));
                }
            }
            Busy::Swing => {
                if let Some(back) = self.return_to.take() {
                    self.select(inv, back, out);
                }
            }
            Busy::Throw => {
                // The grenade is gone: the next one, or back to a gun.
                let next = if inv.grenades[0] != 0 {
                    Some(Sel::Grenade)
                } else {
                    self.return_to.take().or(Some(if inv.primary.is_some() { Sel::Primary } else { Sel::Secondary }))
                };
                if let Some(n) = next {
                    self.sel = Sel::Melee; // force a draw
                    self.select(inv, n, out);
                }
            }
            _ => {}
        }
    }

    fn swing(&mut self, inv: &Inventory, heavy: bool) {
        let Some(def) = weapons::get(inv.melee) else { return };
        let Some(m) = def.melee else { return };
        let t = ticks(if heavy { m.heavy_s } else { m.light_s });
        self.heavy = heavy;
        self.start(Busy::Swing, t);
        // The blow lands 40% of the way through.
        self.impact_at = t - (t as f32 * 0.4).ceil() as u16;
        if self.impact_at == 0 {
            self.impact_at = 1;
        }
    }

    fn melee_tick(&mut self, inv: &Inventory, _def: &WeaponDef, fire_edge: bool, alt_edge: bool, _out: &mut [Out]) {
        if self.busy == Busy::Idle && (fire_edge || alt_edge) {
            self.swing(inv, alt_edge && !fire_edge);
        }
    }

    fn grenade_tick(&mut self, inv: &mut Inventory, def: &WeaponDef, fire: bool, alt: bool, out: &mut Vec<Out>) {
        if self.busy == Busy::Idle {
            if fire && !self.pin {
                self.pin = true;
            } else if self.pin && !fire {
                self.pin = false;
                out.push(Out::Throw { weapon: def_id(inv, self.sel), lob: alt });
                inv.grenades[0] = inv.grenades[1];
                inv.grenades[1] = 0;
                self.start(Busy::Throw, ticks(0.45));
            }
        }
    }

    fn gun_tick(&mut self, inv: &mut Inventory, def: &WeaponDef, ctx: &Ctx, fire: bool, fire_edge: bool, reload_req: bool, out: &mut Vec<Out>) {
        let sel = self.sel;
        let Some(gun) = inv.gun(sel).copied() else { return };
        // Bursts in progress continue by themselves.
        if self.burst_left > 0 && self.burst_gap == 0 && self.busy == Busy::Idle {
            if gun.mag > 0 {
                self.shoot(inv, def, ctx, out);
                self.burst_left -= 1;
                self.burst_gap = if let Fire::Burst { gap_s, .. } = def.fire { ticks(gap_s) } else { 1 };
                if self.burst_left == 0 {
                    self.cooldown += period_ticks(def);
                }
            } else {
                self.burst_left = 0;
            }
        }
        let gun = inv.gun(sel).copied().unwrap_or(gun);
        let wants_fire = match def.fire {
            Fire::Auto => fire,
            _ => fire_edge,
        };
        // A shell reload is interrupted by a shot.
        if wants_fire && self.busy == Busy::ShellLoad && gun.mag > 0 {
            self.busy = Busy::Idle;
            self.left = 0;
        }
        if reload_req && self.busy == Busy::Idle && gun.mag < def.mag && gun.reserve > 0 && self.burst_left == 0 {
            self.begin_reload(def, gun.id, out);
            return;
        }
        if !wants_fire || self.busy != Busy::Idle || self.cooldown > 0. || self.burst_left > 0 {
            return;
        }
        if gun.mag == 0 {
            if gun.reserve > 0 {
                self.begin_reload(def, gun.id, out);
            } else if fire_edge {
                out.push(Out::Dry { weapon: gun.id });
            }
            return;
        }
        match def.fire {
            Fire::Burst { rounds, gap_s } => {
                self.shoot(inv, def, ctx, out);
                self.burst_left = rounds.saturating_sub(1);
                self.burst_gap = ticks(gap_s);
                if self.burst_left == 0 {
                    self.cooldown += period_ticks(def);
                }
            }
            Fire::Cycle { cycle_s } => {
                self.shoot(inv, def, ctx, out);
                self.start(Busy::Cycle, ticks(cycle_s));
            }
            _ => {
                self.shoot(inv, def, ctx, out);
                self.cooldown += period_ticks(def);
            }
        }
    }

    fn begin_reload(&mut self, def: &WeaponDef, weapon: WeaponId, out: &mut Vec<Out>) {
        self.ads = self.ads.min(0.0);
        self.burst_left = 0;
        if def.shell_reload {
            self.start(Busy::ShellLoad, ticks(def.reload_s));
        } else {
            self.start(Busy::Reload, ticks(def.reload_s));
        }
        out.push(Out::ReloadStarted { weapon });
    }

    fn shoot(&mut self, inv: &mut Inventory, def: &WeaponDef, ctx: &Ctx, out: &mut Vec<Out>) {
        let sel = self.sel;
        let Some(g) = inv.gun_mut(sel) else { return };
        if g.mag == 0 {
            return;
        }
        g.mag -= 1;
        let weapon = g.id;
        let p = punch(def, self.recoil);
        let spread = spread_deg(def, self.ads, ctx, self.recoil);
        self.recoil += 1.;
        if def.class == Class::Launcher {
            out.push(Out::Launch { weapon, punch: p });
            let (mag, reserve) = inv.gun(sel).map_or((0, 0), |g| (g.mag, g.reserve));
            if mag == 0 && reserve > 0 {
                self.start(Busy::Reload, ticks(def.reload_s));
            }
        } else {
            out.push(Out::Shot { weapon, pellets: def.pellets.max(1), punch: p, spread });
        }
    }
}

fn def_id(inv: &Inventory, sel: Sel) -> WeaponId {
    inv.id_in(sel)
}
