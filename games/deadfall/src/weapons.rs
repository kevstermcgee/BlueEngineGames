//! The armoury: every weapon's numbers in one table, compared with its real-world counterpart.
//!
//! Units: metres, seconds, degrees, health points (a player has 100). A weapon's wire id is its index in
//! [`WEAPONS`] plus one (`0` means "nothing"), so **never reorder or delete a row once a release is out**;
//! append, or bump `netgame::PROTOCOL`.
//!
//! Each weapon has its own ammunition: a magazine and a reserve that belong to that weapon alone, so picking
//! up a second rifle never refills the first one's rounds.

/// Which inventory slot a weapon occupies. A player carries two firearms (one primary, one secondary), one
/// melee weapon and up to two grenades.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Primary,
    Secondary,
    Melee,
    Grenade,
}

/// What the weapon is, for behaviour, viewmodel and sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Pistol,
    Smg,
    AssaultRifle,
    Dmr,
    Sniper,
    Shotgun,
    Lmg,
    Launcher,
    Grenade,
    Melee,
}

/// How the trigger works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fire {
    /// One shot per pull.
    Semi,
    /// Fires while held.
    Auto,
    /// `rounds` shots per pull, `gap_s` seconds apart; the next burst waits `rpm`'s cycle time.
    Burst { rounds: u8, gap_s: f32 },
    /// Semi-automatic with a cycling delay after every shot (bolt action, pump): `cycle_s` before the next.
    Cycle { cycle_s: f32 },
    /// Thrown or launched: the pull releases it (grenades), or fires one projectile (launchers).
    Throw,
    /// Swung: light attack on primary, heavy on secondary.
    Swing,
}

/// What the player looks through when aiming.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sight {
    /// Iron sights: the weapon is raised to the eye, the field of view narrows a little.
    Iron,
    /// A red-dot or holographic sight.
    Dot,
    /// A magnified scope with an overlay; `zoom` is the magnification (2.0 = 2x).
    Scope { zoom: f32 },
    /// Nothing to aim with (grenades, melee): aim is ignored.
    None,
}

/// What a fired or thrown projectile does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projectile {
    /// Muzzle or release speed, m/s.
    pub speed: f32,
    /// Extra downward acceleration as a multiple of gravity (0 = flies straight, 1 = falls like a stone).
    pub gravity: f32,
    /// Seconds until it detonates on its own (`0` = on impact).
    pub fuse_s: f32,
    /// What it does when it goes off.
    pub effect: Effect,
    /// Bounces off surfaces (grenades) instead of detonating on contact.
    pub bounces: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Explosion: full `damage` at the centre, falling to zero at `radius` metres.
    Explosion { radius: f32 },
    /// Blinds and deafens anyone looking at it for up to `blind_s` seconds.
    Flash { radius: f32, blind_s: f32 },
    /// A cloud that blocks sight for `seconds`.
    Smoke { radius: f32, seconds: f32 },
    /// A burning patch: `damage` per second to anyone standing in it, for `seconds`.
    Fire { radius: f32, seconds: f32 },
}

/// Melee numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Melee {
    /// Reach in metres.
    pub reach: f32,
    /// Damage of a light (quick) attack and of a heavy attack.
    pub light: f32,
    pub heavy: f32,
    /// Seconds for a light swing to land and recover, and for a heavy one.
    pub light_s: f32,
    pub heavy_s: f32,
    /// Damage multiplier when striking a target from behind.
    pub back_mult: f32,
}

/// One weapon. See the module docs for units.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponDef {
    /// Stable lower-case key (`"k47"`), used in logs, settings and tests.
    pub key: &'static str,
    /// Name shown in game.
    pub name: &'static str,
    /// The real weapon this is modelled on, with its calibre, for the armoury notes and the tests that compare.
    pub real: &'static str,
    pub class: Class,
    pub slot: Slot,
    pub fire: Fire,
    /// Damage per pellet or bullet to the torso at point blank (a player has 100 health).
    pub damage: f32,
    /// Headshot multiplier on `damage`.
    pub head_mult: f32,
    /// Fraction of damage that goes through body armour (1.0 = ignores it). Armour takes the rest at half.
    pub armor_pen: f32,
    /// Effective range in metres: damage falls off smoothly beyond it, to `far_fraction` at twice the range.
    pub range_m: f32,
    pub far_fraction: f32,
    /// Rounds per minute (cyclic rate). Ignored for `Cycle`, `Throw` and `Swing`.
    pub rpm: f32,
    /// Rounds in a full magazine, and spare rounds carried (this weapon's own ammunition).
    pub mag: u16,
    pub reserve: u16,
    /// Seconds to reload a magazine (for shotguns, per shell when `shell_reload`).
    pub reload_s: f32,
    /// Loads one shell at a time and can be interrupted to fire (pump and tube shotguns).
    pub shell_reload: bool,
    /// Pellets per shot (1 for a rifle bullet, 8 or 9 for a shotgun).
    pub pellets: u8,
    /// Cone half-angle in degrees for a standing, stationary, unaimed shot; add `move_spread_deg` while moving fast.
    pub spread_deg: f32,
    pub move_spread_deg: f32,
    /// Spread is multiplied by this while aiming down the sights (and by `CROUCH_SPREAD` when crouched).
    pub ads_spread_mult: f32,
    /// View kick per shot: up, and sideways (alternating, random sign), in degrees; `recoil_recover` is how
    /// fast the pattern index falls back towards zero when not firing (shots per second).
    pub recoil_up_deg: f32,
    pub recoil_side_deg: f32,
    pub recoil_recover: f32,
    /// Movement speed while carrying it, as a fraction of the unarmed run speed.
    pub move_speed: f32,
    /// Seconds to bring it up after switching to it.
    pub draw_s: f32,
    pub sight: Sight,
    /// Vertical field of view in degrees while aiming (90 = no change); scopes narrow it by `zoom`.
    pub ads_fov: f32,
    /// Seconds to settle into the aim.
    pub ads_s: f32,
    /// Thrown or launched projectile, if any.
    pub projectile: Option<Projectile>,
    /// Melee numbers, if a melee weapon.
    pub melee: Option<Melee>,
    /// Splash/direct damage of a projectile's effect at its centre (grenades, rockets). Zero otherwise.
    pub blast_damage: f32,
}

/// Spread multiplier while crouched and still.
pub const CROUCH_SPREAD: f32 = 0.7;
/// Health a player spawns with, and the armour a vest adds.
pub const MAX_HEALTH: f32 = 100.;
pub const MAX_ARMOR: f32 = 100.;

/// Wire id of a weapon: index + 1, so `0` is "nothing".
pub type WeaponId = u8;

/// The armoury. Rows are in the order of their wire ids.
pub static WEAPONS: &[WeaponDef] = &[];

/// Look a weapon up by wire id (`0` and unknown ids are `None`).
pub fn get(id: WeaponId) -> Option<&'static WeaponDef> {
    (id as usize).checked_sub(1).and_then(|i| WEAPONS.get(i))
}

/// Wire id of the weapon with this key.
pub fn id_of(key: &str) -> Option<WeaponId> {
    WEAPONS.iter().position(|w| w.key == key).map(|i| i as u8 + 1)
}
