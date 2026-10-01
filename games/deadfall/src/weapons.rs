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
    /// Rounds per minute: the game's cyclic rate (`Semi`: the fastest the trigger can be worked; `Burst`: the
    /// pause between bursts; `Throw`: the re-throw rate). Zero and ignored for `Cycle` and `Swing`.
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

/// Key of the sidearm every player spawns with.
pub const START_SIDEARM: &str = "k9";
/// Key of the melee weapon every player spawns with.
pub const START_MELEE: &str = "knife";

/// Simulation rate the tick helpers assume.
pub const TICK_HZ: f32 = 60.;

// ---------------------------------------------------------------------------------------------------------
// Tuning scale (100-health, armour-optional, 12 players, 120 m map)
//
// Damage per bullet to the torso at point blank:
//   pistols 24-40 (Hand Cannon 55), SMGs 18-24, assault rifles 24-33, battle/marksman rifles 40-50,
//   scout 85, AW-M 110, Anvil 130 (one body shot), shotgun pellets 10-12 x 8-9, LMGs 28-32,
//   rocket 130, 40 mm 90, frag 100 at the centre.
// Headshots multiply by `head_mult` (2.0 typical, 2.2-2.5 for rifles that should one-shot the head).
//
// Rates of fire: the real cyclic rate is capped where it would make a kill faster than about 0.35 s at
// 10 m (a 60 Hz server and human reaction time make sub-third-of-a-second kills feel like coin flips).
// Where a row's `rpm` is below the real weapon's, a comment says so. Magazine sizes, reload character,
// calibre-driven power, sights and weight (move speed) follow the real weapons.
// ---------------------------------------------------------------------------------------------------------

/// Defaults shared by most rows; each row overrides what makes the weapon itself.
const BASE: WeaponDef = WeaponDef {
    key: "",
    name: "",
    real: "",
    class: Class::Pistol,
    slot: Slot::Primary,
    fire: Fire::Semi,
    damage: 0.,
    head_mult: 2.0,
    armor_pen: 0.5,
    range_m: 30.,
    far_fraction: 0.5,
    rpm: 0.,
    mag: 0,
    reserve: 0,
    reload_s: 2.0,
    shell_reload: false,
    pellets: 1,
    spread_deg: 1.0,
    move_spread_deg: 2.5,
    ads_spread_mult: 0.45,
    recoil_up_deg: 1.0,
    recoil_side_deg: 0.3,
    recoil_recover: 8.,
    move_speed: 1.0,
    draw_s: 0.5,
    sight: Sight::Iron,
    ads_fov: 70.,
    ads_s: 0.2,
    projectile: None,
    melee: None,
    blast_damage: 0.,
};

/// Base for a thrown grenade: no aim, no ammunition beyond the one thrown.
const GRENADE: WeaponDef = WeaponDef {
    class: Class::Grenade,
    slot: Slot::Grenade,
    fire: Fire::Throw,
    head_mult: 1.0,
    armor_pen: 1.0,
    range_m: 30.,
    far_fraction: 1.0,
    rpm: 40.,
    mag: 1,
    reserve: 0,
    reload_s: 0.,
    spread_deg: 0.,
    move_spread_deg: 0.,
    ads_spread_mult: 1.0,
    recoil_up_deg: 0.,
    recoil_side_deg: 0.,
    recoil_recover: 1.,
    move_speed: 1.0,
    draw_s: 0.5,
    sight: Sight::None,
    ads_fov: 90.,
    ads_s: 0.,
    ..BASE
};

/// Base for a melee weapon.
const MELEE: WeaponDef = WeaponDef {
    class: Class::Melee,
    slot: Slot::Melee,
    fire: Fire::Swing,
    head_mult: 1.0,
    armor_pen: 0.8,
    far_fraction: 1.0,
    rpm: 0.,
    mag: 0,
    reserve: 0,
    reload_s: 0.,
    spread_deg: 0.,
    move_spread_deg: 0.,
    ads_spread_mult: 1.0,
    recoil_up_deg: 0.,
    recoil_side_deg: 0.,
    recoil_recover: 1.,
    sight: Sight::None,
    ads_fov: 90.,
    ads_s: 0.,
    ..BASE
};

/// The armoury. Rows are in the order of their wire ids.
pub static WEAPONS: &[WeaponDef] = &[
    // ---- Pistols (Secondary). 24-40 per bullet, Hand Cannon 55. `rpm` is the trigger-finger limit. ----
    WeaponDef {
        // Glock 17: 17 rounds, ~1.7 s mag change, light and snappy. Real pistol can be pulled faster.
        key: "k9",
        name: "K-9 Sidearm",
        real: "Glock 17, 9x19mm",
        class: Class::Pistol,
        slot: Slot::Secondary,
        fire: Fire::Semi,
        damage: 24.,
        head_mult: 2.0,
        armor_pen: 0.4,
        range_m: 25.,
        far_fraction: 0.55,
        rpm: 330.,
        mag: 17,
        reserve: 51,
        reload_s: 1.7,
        spread_deg: 0.9,
        move_spread_deg: 2.0,
        recoil_up_deg: 0.9,
        recoil_side_deg: 0.25,
        recoil_recover: 6.,
        move_speed: 0.98,
        draw_s: 0.35,
        ads_s: 0.15,
        ..BASE
    },
    WeaponDef {
        // M1911: seven heavy .45 rounds, slower fire, firmer kick.
        key: "m45",
        name: "Bulldog .45",
        real: "Colt M1911A1, .45 ACP",
        class: Class::Pistol,
        slot: Slot::Secondary,
        fire: Fire::Semi,
        damage: 31.,
        head_mult: 2.0,
        armor_pen: 0.45,
        range_m: 25.,
        far_fraction: 0.5,
        rpm: 270.,
        mag: 7,
        reserve: 28,
        reload_s: 1.9,
        spread_deg: 0.8,
        move_spread_deg: 2.0,
        recoil_up_deg: 1.3,
        recoil_side_deg: 0.35,
        recoil_recover: 5.,
        move_speed: 0.97,
        draw_s: 0.4,
        ads_s: 0.16,
        ..BASE
    },
    WeaponDef {
        // Desert Eagle: seven .50 AE rounds, two body shots, violent muzzle rise, slow to re-aim.
        key: "hc50",
        name: "Hand Cannon",
        real: "Desert Eagle, .50 AE",
        class: Class::Pistol,
        slot: Slot::Secondary,
        fire: Fire::Semi,
        damage: 55.,
        head_mult: 2.0,
        armor_pen: 0.7,
        range_m: 35.,
        far_fraction: 0.6,
        rpm: 95.,
        mag: 7,
        reserve: 28,
        reload_s: 2.2,
        spread_deg: 1.0,
        move_spread_deg: 2.5,
        recoil_up_deg: 3.4,
        recoil_side_deg: 0.9,
        recoil_recover: 3.,
        move_speed: 0.96,
        draw_s: 0.5,
        ads_s: 0.2,
        ..BASE
    },
    WeaponDef {
        // Colt Python: six .357 Magnum rounds, accurate, three body shots, slow reload (speedloader).
        key: "rv357",
        name: "Marshal .357",
        real: "Colt Python revolver, .357 Magnum",
        class: Class::Pistol,
        slot: Slot::Secondary,
        fire: Fire::Semi,
        damage: 40.,
        head_mult: 2.0,
        armor_pen: 0.6,
        range_m: 40.,
        far_fraction: 0.55,
        rpm: 130.,
        mag: 6,
        reserve: 24,
        reload_s: 3.3,
        spread_deg: 0.6,
        move_spread_deg: 2.0,
        recoil_up_deg: 2.4,
        recoil_side_deg: 0.6,
        recoil_recover: 4.,
        move_speed: 0.96,
        draw_s: 0.5,
        ads_s: 0.18,
        ..BASE
    },
    // ---- Submachine guns (Primary). 18-24 per bullet, fast fire, low kick, wide spread, fast on foot. ----
    WeaponDef {
        // MP5: 30 rounds, real cyclic 800.
        key: "mp9",
        name: "MP-9",
        real: "Heckler & Koch MP5, 9x19mm",
        class: Class::Smg,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 19.,
        head_mult: 2.0,
        armor_pen: 0.35,
        range_m: 30.,
        far_fraction: 0.5,
        rpm: 800.,
        mag: 30,
        reserve: 120,
        reload_s: 2.1,
        spread_deg: 1.2,
        move_spread_deg: 2.0,
        recoil_up_deg: 0.55,
        recoil_side_deg: 0.25,
        recoil_recover: 14.,
        move_speed: 0.95,
        draw_s: 0.45,
        ads_s: 0.17,
        ..BASE
    },
    WeaponDef {
        // UMP45: 25 .45 ACP rounds, slower and harder-hitting than the nine; red dot.
        key: "ump",
        name: "UMP-45",
        real: "H&K UMP45, .45 ACP",
        class: Class::Smg,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 24.,
        head_mult: 2.0,
        armor_pen: 0.4,
        range_m: 32.,
        far_fraction: 0.5,
        rpm: 600.,
        mag: 25,
        reserve: 100,
        reload_s: 2.3,
        spread_deg: 1.25,
        move_spread_deg: 2.2,
        recoil_up_deg: 0.75,
        recoil_side_deg: 0.3,
        recoil_recover: 12.,
        move_speed: 0.94,
        draw_s: 0.5,
        sight: Sight::Dot,
        ads_fov: 66.,
        ads_s: 0.18,
        ..BASE
    },
    WeaponDef {
        // P90: 50 rounds, small-calibre armour-piercing 5.7x28, almost no kick, compact; real cyclic ~900.
        key: "pdw",
        name: "PDW-57",
        real: "FN P90, 5.7x28mm",
        class: Class::Smg,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 18.,
        head_mult: 2.0,
        armor_pen: 0.75,
        range_m: 40.,
        far_fraction: 0.6,
        rpm: 800.,
        mag: 50,
        reserve: 150,
        reload_s: 2.7,
        spread_deg: 1.1,
        move_spread_deg: 2.0,
        recoil_up_deg: 0.4,
        recoil_side_deg: 0.2,
        recoil_recover: 16.,
        move_speed: 0.95,
        draw_s: 0.5,
        sight: Sight::Dot,
        ads_fov: 66.,
        ads_s: 0.18,
        ..BASE
    },
    WeaponDef {
        // KRISS Vector: 25 .45 ACP rounds, real cyclic ~1200 (a round every three ticks), the recoil-
        // absorbing Super V keeps the muzzle down but the gun is hard to place; capped to 840.
        key: "vkr",
        name: "Vektor-K",
        real: "KRISS Vector, .45 ACP",
        class: Class::Smg,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 18.,
        head_mult: 2.0,
        armor_pen: 0.4,
        range_m: 28.,
        far_fraction: 0.5,
        rpm: 840.,
        mag: 25,
        reserve: 100,
        reload_s: 2.0,
        spread_deg: 1.6,
        move_spread_deg: 2.2,
        recoil_up_deg: 0.45,
        recoil_side_deg: 0.3,
        recoil_recover: 16.,
        move_speed: 0.94,
        draw_s: 0.45,
        sight: Sight::Dot,
        ads_fov: 66.,
        ads_s: 0.17,
        ..BASE
    },
    // ---- Assault rifles (Primary). 24-33 per bullet, four-shot kills at about 0.36-0.4 s. ----
    WeaponDef {
        // AK-47: 30 rounds, hardest-hitting and hardest-kicking of the carbines; real cyclic 600, 450 here.
        key: "k47",
        name: "K-47",
        real: "AK-47, 7.62x39mm",
        class: Class::AssaultRifle,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 33.,
        head_mult: 2.0,
        armor_pen: 0.65,
        range_m: 55.,
        far_fraction: 0.6,
        rpm: 450.,
        mag: 30,
        reserve: 120,
        reload_s: 2.5,
        spread_deg: 1.1,
        move_spread_deg: 3.0,
        ads_spread_mult: 0.4,
        recoil_up_deg: 1.1,
        recoil_side_deg: 0.4,
        recoil_recover: 9.,
        move_speed: 0.9,
        draw_s: 0.6,
        ads_s: 0.22,
        ..BASE
    },
    WeaponDef {
        // M4A1: 30 rounds, mild recoil, red dot, real cyclic ~800, 500 here.
        key: "m4c",
        name: "M4-C Carbine",
        real: "Colt M4A1, 5.56x45mm",
        class: Class::AssaultRifle,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 26.,
        head_mult: 2.0,
        armor_pen: 0.6,
        range_m: 60.,
        far_fraction: 0.6,
        rpm: 500.,
        mag: 30,
        reserve: 120,
        reload_s: 2.0,
        spread_deg: 0.9,
        move_spread_deg: 3.0,
        ads_spread_mult: 0.4,
        recoil_up_deg: 0.7,
        recoil_side_deg: 0.25,
        recoil_recover: 10.,
        move_speed: 0.91,
        draw_s: 0.55,
        sight: Sight::Dot,
        ads_fov: 66.,
        ads_s: 0.2,
        ..BASE
    },
    WeaponDef {
        // FAMAS F1: 25 rounds, three-round bursts at the real ~1000 rpm (0.06 s apart), then a recovery
        // pause of 0.24 s (rpm 250). One burst is 87 damage: the fourth round of the second burst kills.
        key: "fm2",
        name: "F-2 Burst",
        real: "FAMAS F1, 5.56x45mm (3-round burst)",
        class: Class::AssaultRifle,
        slot: Slot::Primary,
        fire: Fire::Burst { rounds: 3, gap_s: 0.06 },
        damage: 29.,
        head_mult: 2.0,
        armor_pen: 0.6,
        range_m: 60.,
        far_fraction: 0.6,
        rpm: 250.,
        mag: 25,
        reserve: 100,
        reload_s: 2.4,
        spread_deg: 0.8,
        move_spread_deg: 3.0,
        ads_spread_mult: 0.4,
        recoil_up_deg: 0.9,
        recoil_side_deg: 0.25,
        recoil_recover: 8.,
        move_speed: 0.9,
        draw_s: 0.55,
        ads_s: 0.2,
        ..BASE
    },
    WeaponDef {
        // Steyr AUG: 30 rounds, integral 1.5x optic, best accuracy of the rifles, lower damage, five-shot kill.
        key: "bpa",
        name: "Bullpup A1",
        real: "Steyr AUG, 5.56x45mm (1.5x optic)",
        class: Class::AssaultRifle,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 24.,
        head_mult: 2.0,
        armor_pen: 0.6,
        range_m: 65.,
        far_fraction: 0.6,
        rpm: 650.,
        mag: 30,
        reserve: 120,
        reload_s: 2.3,
        spread_deg: 0.8,
        move_spread_deg: 2.8,
        ads_spread_mult: 0.35,
        recoil_up_deg: 0.65,
        recoil_side_deg: 0.2,
        recoil_recover: 10.,
        move_speed: 0.9,
        draw_s: 0.6,
        sight: Sight::Scope { zoom: 1.5 },
        ads_fov: 67.4,
        ads_s: 0.24,
        ..BASE
    },
    // ---- Battle and marksman rifles (Primary). 40-50 per bullet, three- or two-shot kills, semi/slow. ----
    WeaponDef {
        // G3: 20 rounds of 7.62x51, heavy kick; real cyclic 500-600, 330 here.
        key: "gl4",
        name: "G-4 Battle Rifle",
        real: "H&K G3, 7.62x51mm",
        class: Class::Dmr,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 40.,
        head_mult: 2.0,
        armor_pen: 0.75,
        range_m: 70.,
        far_fraction: 0.65,
        rpm: 330.,
        mag: 20,
        reserve: 80,
        reload_s: 2.7,
        spread_deg: 1.0,
        move_spread_deg: 3.5,
        ads_spread_mult: 0.35,
        recoil_up_deg: 1.6,
        recoil_side_deg: 0.5,
        recoil_recover: 7.,
        move_speed: 0.86,
        draw_s: 0.7,
        ads_s: 0.28,
        ..BASE
    },
    WeaponDef {
        // SCAR-20 / SR-25: 20 rounds, semi-automatic, 3x scope, three-shot body kill, one-shot head.
        key: "dmr20",
        name: "DMR-20",
        real: "SCAR-20 / SR-25, 7.62x51mm (3x scope)",
        class: Class::Dmr,
        slot: Slot::Primary,
        fire: Fire::Semi,
        damage: 44.,
        head_mult: 2.4,
        armor_pen: 0.8,
        range_m: 90.,
        far_fraction: 0.75,
        rpm: 240.,
        mag: 20,
        reserve: 60,
        reload_s: 2.5,
        spread_deg: 0.5,
        move_spread_deg: 4.0,
        ads_spread_mult: 0.25,
        recoil_up_deg: 1.3,
        recoil_side_deg: 0.3,
        recoil_recover: 6.,
        move_speed: 0.87,
        draw_s: 0.75,
        sight: Sight::Scope { zoom: 3.0 },
        ads_fov: 36.9,
        ads_s: 0.3,
        ..BASE
    },
    WeaponDef {
        // Dragunov: 10 rounds of 7.62x54R, 4x PSO scope, two-shot body kill, slow to fire (real ~30 effective).
        key: "svd",
        name: "SVD Marksman",
        real: "Dragunov SVD, 7.62x54R (4x scope)",
        class: Class::Dmr,
        slot: Slot::Primary,
        fire: Fire::Semi,
        damage: 50.,
        head_mult: 2.2,
        armor_pen: 0.8,
        range_m: 95.,
        far_fraction: 0.75,
        rpm: 150.,
        mag: 10,
        reserve: 30,
        reload_s: 2.9,
        spread_deg: 0.55,
        move_spread_deg: 4.5,
        ads_spread_mult: 0.25,
        recoil_up_deg: 1.5,
        recoil_side_deg: 0.35,
        recoil_recover: 5.,
        move_speed: 0.86,
        draw_s: 0.8,
        sight: Sight::Scope { zoom: 4.0 },
        ads_fov: 28.1,
        ads_s: 0.32,
        ..BASE
    },
    // ---- Snipers (Primary). Bolt and heavy semi; scout 85 (two shots), AW-M 110 and Anvil 130 (one). ----
    WeaponDef {
        // Steyr Scout: ten .308 rounds, fast bolt (1.2 s), light enough to carry quickly.
        key: "scout",
        name: "Scout",
        real: "Steyr Scout / SSG 08 bolt, .308 (4x scope)",
        class: Class::Sniper,
        slot: Slot::Primary,
        fire: Fire::Cycle { cycle_s: 1.2 },
        damage: 85.,
        head_mult: 2.0,
        armor_pen: 0.85,
        range_m: 110.,
        far_fraction: 0.8,
        rpm: 0.,
        mag: 10,
        reserve: 30,
        reload_s: 3.3,
        spread_deg: 0.3,
        move_spread_deg: 7.0,
        ads_spread_mult: 0.1,
        recoil_up_deg: 2.6,
        recoil_side_deg: 0.4,
        recoil_recover: 3.,
        move_speed: 0.9,
        draw_s: 0.7,
        sight: Sight::Scope { zoom: 4.0 },
        ads_fov: 28.1,
        ads_s: 0.3,
        ..BASE
    },
    WeaponDef {
        // AWM: five .338 Lapua rounds, 1.7 s bolt, kills with any torso shot, heavy to carry and to aim.
        key: "awm",
        name: "AW-M Magnum",
        real: "Accuracy International AWM, .338 Lapua (6x scope)",
        class: Class::Sniper,
        slot: Slot::Primary,
        fire: Fire::Cycle { cycle_s: 1.7 },
        damage: 110.,
        head_mult: 2.0,
        armor_pen: 1.0,
        range_m: 140.,
        far_fraction: 0.9,
        rpm: 0.,
        mag: 5,
        reserve: 20,
        reload_s: 3.4,
        spread_deg: 0.15,
        move_spread_deg: 10.0,
        ads_spread_mult: 0.05,
        recoil_up_deg: 3.5,
        recoil_side_deg: 0.5,
        recoil_recover: 2.,
        move_speed: 0.8,
        draw_s: 0.9,
        sight: Sight::Scope { zoom: 6.0 },
        ads_fov: 18.9,
        ads_s: 0.4,
        ..BASE
    },
    WeaponDef {
        // Barrett M82: ten .50 BMG rounds, semi-automatic (~0.67 s), 8x scope, the hardest hitter, very heavy.
        key: "m82",
        name: "Anvil .50",
        real: "Barrett M82, .50 BMG semi-auto (8x scope)",
        class: Class::Sniper,
        slot: Slot::Primary,
        fire: Fire::Semi,
        damage: 130.,
        head_mult: 2.0,
        armor_pen: 1.0,
        range_m: 150.,
        far_fraction: 0.9,
        rpm: 90.,
        mag: 10,
        reserve: 20,
        reload_s: 3.8,
        spread_deg: 0.3,
        move_spread_deg: 12.0,
        ads_spread_mult: 0.08,
        recoil_up_deg: 4.0,
        recoil_side_deg: 0.7,
        recoil_recover: 2.,
        move_speed: 0.72,
        draw_s: 1.2,
        sight: Sight::Scope { zoom: 8.0 },
        ads_fov: 14.3,
        ads_s: 0.5,
        ..BASE
    },
    // ---- Shotguns (Primary). Pellets all hit only at point blank; the cone does the balancing. ----
    WeaponDef {
        // Remington 870: eight 12-gauge rounds loaded one at a time, pump 0.95 s, tight choke.
        key: "pump12",
        name: "Pump-12",
        real: "Remington 870, 12 gauge, pump",
        class: Class::Shotgun,
        slot: Slot::Primary,
        fire: Fire::Cycle { cycle_s: 0.95 },
        damage: 12.,
        head_mult: 1.5,
        armor_pen: 0.25,
        range_m: 16.,
        far_fraction: 0.15,
        rpm: 0.,
        mag: 8,
        reserve: 32,
        reload_s: 0.55,
        shell_reload: true,
        pellets: 9,
        spread_deg: 3.0,
        move_spread_deg: 1.5,
        ads_spread_mult: 0.8,
        recoil_up_deg: 3.2,
        recoil_side_deg: 0.6,
        recoil_recover: 4.,
        move_speed: 0.88,
        draw_s: 0.65,
        ads_fov: 75.,
        ads_s: 0.22,
        ..BASE
    },
    WeaponDef {
        // Benelli M4: seven rounds, semi-automatic (~0.25 s), slightly wider cone and 8x10 pellets.
        key: "auto12",
        name: "Auto-12",
        real: "Benelli M4 / XM1014, 12 gauge semi-auto",
        class: Class::Shotgun,
        slot: Slot::Primary,
        fire: Fire::Semi,
        damage: 10.,
        head_mult: 1.5,
        armor_pen: 0.25,
        range_m: 15.,
        far_fraction: 0.15,
        rpm: 240.,
        mag: 7,
        reserve: 28,
        reload_s: 0.5,
        shell_reload: true,
        pellets: 8,
        spread_deg: 3.6,
        move_spread_deg: 1.5,
        ads_spread_mult: 0.8,
        recoil_up_deg: 2.6,
        recoil_side_deg: 0.5,
        recoil_recover: 6.,
        move_speed: 0.86,
        draw_s: 0.65,
        ads_fov: 75.,
        ads_s: 0.22,
        ..BASE
    },
    WeaponDef {
        // Double barrel, sawn off: two shells, both barrels one after the other, very wide cone, dies past 9 m.
        key: "sawn",
        name: "Sawn-Off",
        real: "double-barrel 12 gauge, 2 shells",
        class: Class::Shotgun,
        slot: Slot::Primary,
        fire: Fire::Semi,
        damage: 12.,
        head_mult: 1.5,
        armor_pen: 0.25,
        range_m: 9.,
        far_fraction: 0.1,
        rpm: 400.,
        mag: 2,
        reserve: 24,
        reload_s: 2.3,
        pellets: 9,
        spread_deg: 8.0,
        move_spread_deg: 1.0,
        ads_spread_mult: 0.85,
        recoil_up_deg: 4.5,
        recoil_side_deg: 0.8,
        recoil_recover: 3.,
        move_speed: 0.92,
        draw_s: 0.5,
        ads_fov: 80.,
        ads_s: 0.18,
        ..BASE
    },
    // ---- Light machine guns (Primary). 28-32 per bullet, 100-round belts, slow reloads, heavy. ----
    WeaponDef {
        // M249: 100-round belt, 5.4 s reload; real cyclic ~850, 440 here so a belt lasts 13.6 s.
        key: "para",
        name: "Para-SAW",
        real: "FN Minimi / M249, 5.56x45mm belt",
        class: Class::Lmg,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 28.,
        head_mult: 2.0,
        armor_pen: 0.6,
        range_m: 55.,
        far_fraction: 0.6,
        rpm: 440.,
        mag: 100,
        reserve: 100,
        reload_s: 5.4,
        spread_deg: 1.6,
        move_spread_deg: 4.0,
        ads_spread_mult: 0.55,
        recoil_up_deg: 0.85,
        recoil_side_deg: 0.35,
        recoil_recover: 8.,
        move_speed: 0.78,
        draw_s: 1.1,
        ads_fov: 72.,
        ads_s: 0.35,
        ..BASE
    },
    WeaponDef {
        // PKM: 100-round belt of 7.62x54R, hardest-hitting LMG, slowest reload; real cyclic ~650, 400 here.
        key: "pk",
        name: "PK-74",
        real: "PKM, 7.62x54R belt",
        class: Class::Lmg,
        slot: Slot::Primary,
        fire: Fire::Auto,
        damage: 32.,
        head_mult: 2.0,
        armor_pen: 0.7,
        range_m: 65.,
        far_fraction: 0.65,
        rpm: 400.,
        mag: 100,
        reserve: 100,
        reload_s: 6.2,
        spread_deg: 1.8,
        move_spread_deg: 4.5,
        ads_spread_mult: 0.55,
        recoil_up_deg: 1.15,
        recoil_side_deg: 0.45,
        recoil_recover: 7.,
        move_speed: 0.72,
        draw_s: 1.2,
        ads_fov: 72.,
        ads_s: 0.38,
        ..BASE
    },
    // ---- Launchers (Primary). Explode on impact. ----
    WeaponDef {
        // RPG-7: one rocket, 130 at the centre (a one-shot kill), radius 5 m; slow reload (4.3 s), very heavy to
        // carry. Real muzzle speed ~115 m/s after the booster; 55 m/s with a little drop keeps it dodgeable.
        key: "rpg",
        name: "Kobra RL",
        real: "RPG-7 rocket launcher",
        class: Class::Launcher,
        slot: Slot::Primary,
        fire: Fire::Throw,
        damage: 130.,
        head_mult: 1.0,
        armor_pen: 1.0,
        range_m: 120.,
        far_fraction: 1.0,
        rpm: 12.,
        mag: 1,
        reserve: 2,
        reload_s: 4.3,
        spread_deg: 0.4,
        move_spread_deg: 3.0,
        ads_spread_mult: 0.5,
        recoil_up_deg: 3.0,
        recoil_side_deg: 0.4,
        recoil_recover: 2.,
        move_speed: 0.75,
        draw_s: 1.1,
        ads_fov: 60.,
        ads_s: 0.35,
        projectile: Some(Projectile {
            speed: 55.,
            gravity: 0.08,
            fuse_s: 0.,
            effect: Effect::Explosion { radius: 5. },
            bounces: false,
        }),
        blast_damage: 130.,
        ..BASE
    },
    WeaponDef {
        // M79: one 40 mm round, 90 at the centre over 4 m, lobbed (real muzzle speed 76 m/s), break-action reload.
        key: "thumper",
        name: "Thumper GL",
        real: "M79 40mm grenade launcher",
        class: Class::Launcher,
        slot: Slot::Primary,
        fire: Fire::Throw,
        damage: 90.,
        head_mult: 1.0,
        armor_pen: 1.0,
        range_m: 100.,
        far_fraction: 1.0,
        rpm: 20.,
        mag: 1,
        reserve: 6,
        reload_s: 2.4,
        spread_deg: 0.3,
        move_spread_deg: 2.5,
        ads_spread_mult: 0.6,
        recoil_up_deg: 2.6,
        recoil_side_deg: 0.3,
        recoil_recover: 3.,
        move_speed: 0.85,
        draw_s: 0.7,
        ads_fov: 66.,
        ads_s: 0.25,
        projectile: Some(Projectile {
            speed: 40.,
            gravity: 0.5,
            fuse_s: 0.,
            effect: Effect::Explosion { radius: 4. },
            bounces: false,
        }),
        blast_damage: 90.,
        ..BASE
    },
    // ---- Grenades (Grenade slot). Thrown at 18 m/s, fall like stones, bounce, go off on a fuse. ----
    WeaponDef {
        // M67: 100 at the centre falling to zero at 7 m (the real casualty radius is 15 m; the map is small).
        key: "frag",
        name: "Frag Grenade",
        real: "M67 fragmentation grenade",
        damage: 100.,
        projectile: Some(Projectile {
            speed: 18.,
            gravity: 1.0,
            fuse_s: 1.6,
            effect: Effect::Explosion { radius: 7. },
            bounces: true,
        }),
        blast_damage: 100.,
        ..GRENADE
    },
    WeaponDef {
        // M84: no damage, blinds and deafens for up to 4.5 s within 12 m if looked at.
        key: "flash",
        name: "Flashbang",
        real: "M84 stun grenade",
        projectile: Some(Projectile {
            speed: 18.,
            gravity: 1.0,
            fuse_s: 1.6,
            effect: Effect::Flash { radius: 12., blind_s: 4.5 },
            bounces: true,
        }),
        ..GRENADE
    },
    WeaponDef {
        // M18: no damage, a 5 m cloud that blocks sight for 18 s; real burn time is 50-90 s.
        key: "smoke",
        name: "Smoke Grenade",
        real: "M18 smoke grenade",
        projectile: Some(Projectile {
            speed: 18.,
            gravity: 1.0,
            fuse_s: 1.2,
            effect: Effect::Smoke { radius: 5., seconds: 18. },
            bounces: true,
        }),
        ..GRENADE
    },
    WeaponDef {
        // M14 thermite: a 3.5 m burning patch for 7 s; `blast_damage` is 20 health per second (five seconds to kill).
        key: "incen",
        name: "Incendiary",
        real: "M14 incendiary grenade",
        projectile: Some(Projectile {
            speed: 18.,
            gravity: 1.0,
            fuse_s: 1.8,
            effect: Effect::Fire { radius: 3.5, seconds: 7. },
            bounces: true,
        }),
        blast_damage: 20.,
        ..GRENADE
    },
    // ---- Melee (Melee slot). `damage` mirrors the light attack. Back-stab multiplies either attack. ----
    WeaponDef {
        // Ka-Bar: fastest, 1.6 m reach; a heavy attack from behind does 130 (one-hit kill).
        key: "knife",
        name: "Combat Knife",
        real: "USMC Ka-Bar",
        damage: 35.,
        range_m: 1.6,
        move_speed: 1.0,
        draw_s: 0.3,
        melee: Some(Melee { reach: 1.6, light: 35., heavy: 65., light_s: 0.4, heavy_s: 0.75, back_mult: 2.0 }),
        ..MELEE
    },
    WeaponDef {
        // Jungle machete: longer and heavier than the knife; heavy from behind 135.
        key: "machete",
        name: "Machete",
        real: "jungle machete",
        damage: 42.,
        range_m: 1.9,
        move_speed: 0.98,
        draw_s: 0.4,
        melee: Some(Melee { reach: 1.9, light: 42., heavy: 78., light_s: 0.55, heavy_s: 1.0, back_mult: 1.8 }),
        ..MELEE
    },
    WeaponDef {
        // Fire axe: slow and heavy; the heavy swing kills from the front (100), light 55.
        key: "axe",
        name: "Breaching Axe",
        real: "fire axe",
        damage: 55.,
        range_m: 2.0,
        move_speed: 0.9,
        draw_s: 0.6,
        melee: Some(Melee { reach: 2.0, light: 55., heavy: 100., light_s: 0.8, heavy_s: 1.4, back_mult: 1.5 }),
        ..MELEE
    },
    WeaponDef {
        // Steel crowbar: blunt, weakest of the four, long reach; heavy from behind 114.
        key: "crowbar",
        name: "Crowbar",
        real: "steel crowbar",
        damage: 30.,
        range_m: 1.8,
        move_speed: 0.96,
        draw_s: 0.45,
        melee: Some(Melee { reach: 1.8, light: 30., heavy: 60., light_s: 0.5, heavy_s: 0.9, back_mult: 1.9 }),
        ..MELEE
    },
];

/// Look a weapon up by wire id (`0` and unknown ids are `None`).
pub fn get(id: WeaponId) -> Option<&'static WeaponDef> {
    (id as usize).checked_sub(1).and_then(|i| WEAPONS.get(i))
}

/// Wire id of the weapon with this key.
pub fn id_of(key: &str) -> Option<WeaponId> {
    WEAPONS.iter().position(|w| w.key == key).map(|i| i as u8 + 1)
}

/// Seconds rounded up to whole 60 Hz ticks (never below one tick for a positive duration).
pub fn seconds_to_ticks(seconds: f32) -> u32 {
    if !(seconds > 0.) {
        return 0;
    }
    ((seconds * TICK_HZ - 1e-3).ceil().max(1.)) as u32
}

impl WeaponDef {
    /// Seconds from one trigger pull (or one burst start) to the earliest next one.
    ///
    /// * `Semi`, `Auto`, `Throw`: `60 / rpm` (`Throw` with `rpm == 0` falls back to one second).
    /// * `Burst { rounds, gap_s }`: the whole burst, `(rounds - 1) * gap_s`, plus the recovery `60 / rpm`
    ///   after its last round. The rounds themselves leave the muzzle `gap_s` apart.
    /// * `Cycle { cycle_s }`: `cycle_s`.
    /// * `Swing`: the light swing's `light_s` (a heavy swing takes `melee.heavy_s`).
    pub fn cycle_seconds(&self) -> f32 {
        match self.fire {
            Fire::Semi | Fire::Auto => 60. / self.rpm.max(1.),
            Fire::Throw => {
                if self.rpm > 0. {
                    60. / self.rpm
                } else {
                    1.
                }
            }
            Fire::Burst { rounds, gap_s } => (rounds.max(1) - 1) as f32 * gap_s + 60. / self.rpm.max(1.),
            Fire::Cycle { cycle_s } => cycle_s,
            Fire::Swing => self.melee.map_or(0.5, |m| m.light_s),
        }
    }

    /// [`cycle_seconds`](Self::cycle_seconds) in whole 60 Hz ticks.
    pub fn cycle_ticks(&self) -> u32 {
        seconds_to_ticks(self.cycle_seconds())
    }

    /// Ticks between the rounds of one burst (`0` if the weapon does not burst).
    pub fn burst_gap_ticks(&self) -> u32 {
        match self.fire {
            Fire::Burst { gap_s, .. } => seconds_to_ticks(gap_s),
            _ => 0,
        }
    }

    /// Ticks to reload: a whole magazine, or one shell when `shell_reload`.
    pub fn reload_ticks(&self) -> u32 {
        seconds_to_ticks(self.reload_s)
    }

    /// Seconds to refill `missing` rounds: `missing` shells for a `shell_reload` weapon, else one magazine
    /// change whatever is missing (`0` if nothing is missing).
    pub fn reload_seconds_for(&self, missing: u16) -> f32 {
        if missing == 0 {
            0.
        } else if self.shell_reload {
            missing as f32 * self.reload_s
        } else {
            self.reload_s
        }
    }

    /// Ticks to bring the weapon up after switching to it.
    pub fn draw_ticks(&self) -> u32 {
        seconds_to_ticks(self.draw_s)
    }

    /// True for weapons whose `damage` is a bullet or pellet that can be range-attenuated (not projectiles or
    /// melee).
    pub fn is_ballistic(&self) -> bool {
        self.projectile.is_none() && self.melee.is_none()
    }

    /// Damage of one bullet or pellet (one melee light hit) against the torso at `distance_m`.
    ///
    /// Ballistic weapons deal `damage` out to `range_m`, then fall smoothly (smoothstep) to
    /// `damage * far_fraction` at `2 * range_m` and stay there: monotonic non-increasing. Explosive projectiles
    /// return their centre `blast_damage` (use [`blast_at`](Self::blast_at) for the radial fall-off) and melee
    /// ignores distance.
    pub fn damage_at(&self, distance_m: f32) -> f32 {
        if self.projectile.is_some() {
            return self.blast_damage;
        }
        if self.melee.is_some() || self.range_m <= 0. {
            return self.damage;
        }
        let d = distance_m.max(0.);
        if d <= self.range_m {
            return self.damage;
        }
        let t = ((d - self.range_m) / self.range_m).min(1.);
        let s = t * t * (3. - 2. * t);
        self.damage * (1. - (1. - self.far_fraction) * s)
    }

    /// Explosion damage at `distance_m` from the centre: `blast_damage` falling linearly to zero at the effect's
    /// radius. Zero for weapons that do not explode.
    pub fn blast_at(&self, distance_m: f32) -> f32 {
        match self.projectile {
            Some(Projectile { effect: Effect::Explosion { radius }, .. }) if radius > 0. => {
                self.blast_damage * (1. - distance_m.max(0.) / radius).clamp(0., 1.)
            }
            _ => 0.,
        }
    }

    /// Damage of one trigger pull if every pellet hits: `damage_at(distance) * pellets`, times `head_mult` for a
    /// headshot. `0` for the effect grenades (flash, smoke, incendiary), whose effect is not a hit.
    pub fn damage_per_shot(&self, headshot: bool, distance_m: f32) -> f32 {
        if let Some(p) = self.projectile {
            if !matches!(p.effect, Effect::Explosion { .. }) {
                return 0.;
            }
        }
        let base = self.damage_at(distance_m) * self.pellets.max(1) as f32;
        if headshot {
            base * self.head_mult
        } else {
            base
        }
    }

    /// Trigger pulls (shots, all pellets landing) to take `health` at point blank; `u32::MAX` if the weapon
    /// cannot damage by a direct hit. Melee counts light hits.
    pub fn shots_to_kill(&self, health: f32, headshot: bool) -> u32 {
        self.shots_to_kill_at(health, headshot, 0.)
    }

    /// As [`shots_to_kill`](Self::shots_to_kill) at a distance.
    pub fn shots_to_kill_at(&self, health: f32, headshot: bool, distance_m: f32) -> u32 {
        let per = self.damage_per_shot(headshot, distance_m);
        if !(per > 0.) {
            return u32::MAX;
        }
        if health <= 0. {
            return 0;
        }
        ((health / per - 1e-4).ceil().max(1.)) as u32
    }

    /// Seconds of shooting from the first round to the killing one (all pellets landing, nothing missed):
    /// `(shots - 1)` cycles, with bursts counted as `rounds` shots per `cycle_seconds` and `gap_s` inside a burst.
    /// `f32::INFINITY` when the weapon cannot kill by a direct hit.
    pub fn time_to_kill(&self, health: f32, headshot: bool, distance_m: f32) -> f32 {
        let n = self.shots_to_kill_at(health, headshot, distance_m);
        if n == u32::MAX {
            return f32::INFINITY;
        }
        if n <= 1 {
            return 0.;
        }
        match self.fire {
            Fire::Burst { rounds, gap_s } => {
                let rounds = rounds.max(1) as u32;
                let bursts = (n - 1) / rounds;
                let within = (n - 1) % rounds;
                bursts as f32 * self.cycle_seconds() + within as f32 * gap_s
            }
            _ => (n - 1) as f32 * self.cycle_seconds(),
        }
    }
}

/// The armoury as a Markdown table (what `docs/ARMOURY.md` is generated from; see the ignored test
/// `write_armoury_doc`).
pub fn armoury_markdown() -> String {
    let mut s = String::new();
    s.push_str("# Deadfall armoury\n\n");
    s.push_str(
        "Generated from `src/weapons.rs` (`cargo test --lib write_armoury_doc -- --ignored`). Damage is per bullet or\n\
         pellet to the torso at point blank (players have 100 health); `TTK` is seconds of shooting for a 100-health\n\
         body kill at 10 m with every pellet landing; `shots` is trigger pulls for that kill. Speed is the fraction\n\
         of unarmed run speed. Rates of fire are the game's cyclic rate (capped below the real one where noted in\n\
         the source so no gun kills in under about a third of a second).\n\n",
    );
    s.push_str(
        "| id | key | name | real counterpart | class | fire | dmg | head | shots | TTK | rpm | mag/res | reload | range | speed | sight |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for (i, w) in WEAPONS.iter().enumerate() {
        let fire = match w.fire {
            Fire::Semi => "semi".to_string(),
            Fire::Auto => "auto".to_string(),
            Fire::Burst { rounds, .. } => format!("burst x{rounds}"),
            Fire::Cycle { cycle_s } => format!("bolt/pump {cycle_s:.2}s"),
            Fire::Throw => "throw".to_string(),
            Fire::Swing => "swing".to_string(),
        };
        let dmg = match (w.melee, w.pellets) {
            (Some(m), _) => format!("{}/{}", m.light, m.heavy),
            (None, p) if p > 1 => format!("{} x{}", w.damage, p),
            _ => format!("{}", w.damage.max(w.blast_damage)),
        };
        let shots = w.shots_to_kill(MAX_HEALTH, false);
        let (shots_s, ttk_s) = if shots == u32::MAX {
            ("-".to_string(), "-".to_string())
        } else {
            (shots.to_string(), format!("{:.2}", w.time_to_kill(MAX_HEALTH, false, 10.)))
        };
        let rpm = if w.rpm > 0. { format!("{}", w.rpm) } else { "-".to_string() };
        let ammo = if w.mag > 0 { format!("{}/{}", w.mag, w.reserve) } else { "-".to_string() };
        let reload = if w.reload_s > 0. {
            format!("{:.1}s{}", w.reload_s, if w.shell_reload { "/shell" } else { "" })
        } else {
            "-".to_string()
        };
        let sight = match w.sight {
            Sight::Iron => "iron".to_string(),
            Sight::Dot => "dot".to_string(),
            Sight::Scope { zoom } => format!("{zoom}x"),
            Sight::None => "-".to_string(),
        };
        s.push_str(&format!(
            "| {} | {} | {} | {} | {:?} | {} | {} | x{} | {} | {} | {} | {} | {} | {} m | {:.2} | {} |\n",
            i + 1,
            w.key,
            w.name,
            w.real,
            w.class,
            fire,
            dmg,
            w.head_mult,
            shots_s,
            ttk_s,
            rpm,
            ammo,
            reload,
            w.range_m,
            w.move_speed,
            sight
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: [&str; 33] = [
        "k9", "m45", "hc50", "rv357", "mp9", "ump", "pdw", "vkr", "k47", "m4c", "fm2", "bpa", "gl4", "dmr20", "svd",
        "scout", "awm", "m82", "pump12", "auto12", "sawn", "para", "pk", "rpg", "thumper", "frag", "flash", "smoke",
        "incen", "knife", "machete", "axe", "crowbar",
    ];

    fn w(key: &str) -> &'static WeaponDef {
        get(id_of(key).unwrap_or_else(|| panic!("no weapon {key}"))).unwrap()
    }

    fn of_class(c: Class) -> impl Iterator<Item = &'static WeaponDef> {
        WEAPONS.iter().filter(move |x| x.class == c)
    }

    #[test]
    fn table_has_the_roster_in_order() {
        assert_eq!(WEAPONS.len(), 33);
        let keys: Vec<&str> = WEAPONS.iter().map(|x| x.key).collect();
        assert_eq!(keys, KEYS);
    }

    #[test]
    fn keys_unique_lowercase_and_names_filled() {
        let mut seen = std::collections::HashSet::new();
        for x in WEAPONS {
            assert!(seen.insert(x.key), "duplicate key {}", x.key);
            assert!(!x.key.is_empty());
            assert_eq!(x.key, x.key.to_lowercase());
            assert!(x.key.chars().all(|c| c.is_ascii_alphanumeric()));
            assert!(!x.name.is_empty() && !x.real.is_empty(), "{}", x.key);
        }
    }

    #[test]
    fn names_match_the_design_brief() {
        let names = [
            "K-9 Sidearm",
            "Bulldog .45",
            "Hand Cannon",
            "Marshal .357",
            "MP-9",
            "UMP-45",
            "PDW-57",
            "Vektor-K",
            "K-47",
            "M4-C Carbine",
            "F-2 Burst",
            "Bullpup A1",
            "G-4 Battle Rifle",
            "DMR-20",
            "SVD Marksman",
            "Scout",
            "AW-M Magnum",
            "Anvil .50",
            "Pump-12",
            "Auto-12",
            "Sawn-Off",
            "Para-SAW",
            "PK-74",
            "Kobra RL",
            "Thumper GL",
            "Frag Grenade",
            "Flashbang",
            "Smoke Grenade",
            "Incendiary",
            "Combat Knife",
            "Machete",
            "Breaching Axe",
            "Crowbar",
        ];
        for (x, n) in WEAPONS.iter().zip(names) {
            assert_eq!(x.name, n, "{}", x.key);
        }
    }

    #[test]
    fn id_and_key_round_trip() {
        assert!(get(0).is_none());
        assert!(get(34).is_none());
        assert!(get(255).is_none());
        assert!(id_of("nope").is_none());
        for (i, x) in WEAPONS.iter().enumerate() {
            let id = id_of(x.key).unwrap();
            assert_eq!(id as usize, i + 1);
            assert_eq!(get(id).unwrap().key, x.key);
        }
        assert_eq!(id_of("k9"), Some(1));
        assert_eq!(id_of("crowbar"), Some(33));
    }

    #[test]
    fn starting_weapons_exist_in_the_right_slots() {
        assert_eq!(START_SIDEARM, "k9");
        assert_eq!(START_MELEE, "knife");
        assert_eq!(w(START_SIDEARM).slot, Slot::Secondary);
        assert_eq!(w(START_MELEE).slot, Slot::Melee);
    }

    #[test]
    fn every_numeric_field_is_finite_and_sane() {
        for x in WEAPONS {
            let k = x.key;
            let fields = [
                ("damage", x.damage, 0., 200.),
                ("head_mult", x.head_mult, 1., 4.),
                ("armor_pen", x.armor_pen, 0., 1.),
                ("range_m", x.range_m, 1., 200.),
                ("far_fraction", x.far_fraction, 0.05, 1.),
                ("rpm", x.rpm, 0., 1300.),
                ("reload_s", x.reload_s, 0., 8.),
                ("spread_deg", x.spread_deg, 0., 10.),
                ("move_spread_deg", x.move_spread_deg, 0., 15.),
                ("ads_spread_mult", x.ads_spread_mult, 0., 1.),
                ("recoil_up_deg", x.recoil_up_deg, 0., 6.),
                ("recoil_side_deg", x.recoil_side_deg, 0., 2.),
                ("recoil_recover", x.recoil_recover, 0.5, 30.),
                ("move_speed", x.move_speed, 0.6, 1.1),
                ("draw_s", x.draw_s, 0.1, 2.),
                ("ads_fov", x.ads_fov, 10., 90.),
                ("ads_s", x.ads_s, 0., 1.),
                ("blast_damage", x.blast_damage, 0., 200.),
            ];
            for (name, v, lo, hi) in fields {
                assert!(v.is_finite(), "{k}.{name} not finite");
                assert!((lo..=hi).contains(&v), "{k}.{name} = {v} outside {lo}..={hi}");
            }
            assert!(x.pellets >= 1 && x.pellets <= 12, "{k} pellets");
            assert!(x.mag <= 200 && x.reserve <= 400, "{k} ammo");
            assert!(x.cycle_seconds() > 0.02 && x.cycle_seconds() < 6., "{k} cycle");
            if let Fire::Burst { rounds, gap_s } = x.fire {
                assert!((2..=5).contains(&rounds) && gap_s > 0.02 && gap_s < 0.3, "{k}");
            }
            if let Fire::Cycle { cycle_s } = x.fire {
                assert!(cycle_s.is_finite() && (0.3..3.).contains(&cycle_s), "{k}");
                assert_eq!(x.rpm, 0., "{k}: rpm is ignored for Cycle, keep it 0");
            }
            if matches!(x.fire, Fire::Semi | Fire::Auto | Fire::Burst { .. }) {
                assert!(x.rpm > 0., "{k} needs an rpm");
            }
            if let Some(p) = x.projectile {
                assert!(p.speed > 5. && p.speed < 200., "{k}");
                assert!((0. ..=1.5).contains(&p.gravity), "{k}");
                assert!((0. ..=4.).contains(&p.fuse_s), "{k}");
            }
            if let Some(m) = x.melee {
                assert!((1.0..=2.5).contains(&m.reach), "{k}");
                assert!(m.light > 0. && m.heavy > m.light, "{k}");
                assert!(m.light_s > 0.1 && m.heavy_s > m.light_s && m.heavy_s < 2.5, "{k}");
                assert!((1.0..=3.0).contains(&m.back_mult), "{k}");
            }
        }
    }

    #[test]
    fn every_class_is_represented() {
        for c in [
            Class::Pistol,
            Class::Smg,
            Class::AssaultRifle,
            Class::Dmr,
            Class::Sniper,
            Class::Shotgun,
            Class::Lmg,
            Class::Launcher,
            Class::Grenade,
            Class::Melee,
        ] {
            assert!(of_class(c).count() > 0, "no {c:?}");
        }
        assert_eq!(of_class(Class::Pistol).count(), 4);
        assert_eq!(of_class(Class::Smg).count(), 4);
        assert_eq!(of_class(Class::Grenade).count(), 4);
        assert_eq!(of_class(Class::Melee).count(), 4);
        assert_eq!(of_class(Class::Shotgun).count(), 3);
        assert_eq!(of_class(Class::Launcher).count(), 2);
        assert_eq!(of_class(Class::Lmg).count(), 2);
    }

    #[test]
    fn slots_follow_classes() {
        let mut n = [0usize; 4];
        for x in WEAPONS {
            let want = match x.class {
                Class::Pistol => Slot::Secondary,
                Class::Melee => Slot::Melee,
                Class::Grenade => Slot::Grenade,
                _ => Slot::Primary,
            };
            assert_eq!(x.slot, want, "{}", x.key);
            n[x.slot as usize] += 1;
        }
        // Primary, Secondary, Melee, Grenade.
        assert_eq!(n, [21, 4, 4, 4]);
    }

    #[test]
    fn fire_modes_carry_their_payloads() {
        for x in WEAPONS {
            match x.class {
                Class::Grenade | Class::Launcher => {
                    assert_eq!(x.fire, Fire::Throw, "{}", x.key);
                    assert!(x.projectile.is_some(), "{}", x.key);
                }
                Class::Melee => {
                    assert!(matches!(x.fire, Fire::Swing), "{}", x.key);
                    assert!(x.melee.is_some(), "{}", x.key);
                    assert_eq!(x.sight, Sight::None, "{}", x.key);
                    assert_eq!(x.mag, 0);
                }
                _ => {
                    assert!(x.projectile.is_none() && x.melee.is_none(), "{}", x.key);
                    assert!(x.damage > 0. && x.mag > 0, "{}", x.key);
                }
            }
            if x.class != Class::Melee {
                assert!(x.melee.is_none(), "{}", x.key);
            }
            // Only the burst gun bursts; only bolt/pump guns cycle.
            assert_eq!(matches!(x.fire, Fire::Burst { .. }), x.key == "fm2");
        }
        assert!(matches!(w("scout").fire, Fire::Cycle { .. }));
        assert!(matches!(w("awm").fire, Fire::Cycle { .. }));
        assert!(matches!(w("pump12").fire, Fire::Cycle { .. }));
    }

    #[test]
    fn grenades_bounce_and_launchers_detonate_on_impact() {
        for x in of_class(Class::Grenade) {
            let p = x.projectile.unwrap();
            assert!(p.bounces && p.fuse_s > 0., "{}", x.key);
            assert_eq!((x.mag, x.reserve), (1, 0), "{}", x.key);
            assert_eq!(x.sight, Sight::None, "{}", x.key);
        }
        for x in of_class(Class::Launcher) {
            let p = x.projectile.unwrap();
            assert!(!p.bounces && p.fuse_s == 0., "{}", x.key);
            assert!(matches!(p.effect, Effect::Explosion { .. }), "{}", x.key);
            assert_eq!(x.mag, 1, "{}", x.key);
            assert_eq!(x.damage, x.blast_damage, "{}", x.key);
        }
        assert_eq!(w("frag").fire, Fire::Throw);
    }

    #[test]
    fn effect_grenades_match_their_roles() {
        let frag = w("frag");
        assert_eq!(frag.blast_damage, 100.);
        assert!(matches!(frag.projectile.unwrap().effect, Effect::Explosion { .. }));
        for k in ["flash", "smoke"] {
            assert_eq!(w(k).damage, 0.);
            assert_eq!(w(k).blast_damage, 0.);
        }
        let Effect::Flash { blind_s, radius } = w("flash").projectile.unwrap().effect else {
            panic!("flash effect");
        };
        assert!(blind_s >= 2. && radius >= 6.);
        let Effect::Smoke { seconds, radius } = w("smoke").projectile.unwrap().effect else {
            panic!("smoke effect");
        };
        assert!(seconds >= 10. && radius >= 3.);
        let inc = w("incen");
        let Effect::Fire { seconds, radius } = inc.projectile.unwrap().effect else {
            panic!("fire effect");
        };
        assert!(inc.blast_damage > 0. && seconds > 3. && radius >= 2.);
        // Fire can kill, but only slowly (more than 3 s in the patch), and is no direct hit.
        assert!(MAX_HEALTH / inc.blast_damage > 3.);
        assert_eq!(inc.shots_to_kill(100., false), u32::MAX);
        assert_eq!(w("flash").shots_to_kill(100., true), u32::MAX);
        assert!(w("flash").time_to_kill(100., false, 5.).is_infinite());
    }

    #[test]
    fn real_magazine_sizes() {
        let real = [
            ("k9", 17),
            ("m45", 7),
            ("hc50", 7),
            ("rv357", 6),
            ("mp9", 30),
            ("ump", 25),
            ("pdw", 50),
            ("vkr", 25),
            ("k47", 30),
            ("m4c", 30),
            ("fm2", 25),
            ("bpa", 30),
            ("gl4", 20),
            ("dmr20", 20),
            ("svd", 10),
            ("scout", 10),
            ("awm", 5),
            ("m82", 10),
            ("pump12", 8),
            ("auto12", 7),
            ("sawn", 2),
            ("para", 100),
            ("pk", 100),
            ("rpg", 1),
            ("thumper", 1),
        ];
        for (k, m) in real {
            assert_eq!(w(k).mag, m, "{k}");
        }
    }

    #[test]
    fn reserves_are_sensible() {
        for x in WEAPONS {
            match x.class {
                Class::Pistol | Class::Smg | Class::AssaultRifle | Class::Dmr | Class::Lmg => {
                    assert!(x.reserve >= x.mag, "{} reserve below one magazine", x.key);
                    if x.class != Class::Lmg && x.class != Class::Dmr {
                        let mags = x.reserve as f32 / x.mag as f32;
                        assert!((3.0..=5.0).contains(&mags), "{} carries {mags} mags", x.key);
                    }
                }
                Class::Sniper => {
                    assert!((20..=30).contains(&x.reserve), "{}", x.key);
                    assert!(x.reserve >= x.mag, "{}", x.key);
                }
                Class::Shotgun => {
                    assert!((24..=32).contains(&x.reserve), "{}", x.key);
                    assert_eq!(x.shell_reload, x.key != "sawn", "{}", x.key);
                }
                Class::Launcher => {
                    assert_eq!(x.reserve, if x.key == "rpg" { 2 } else { 6 });
                }
                Class::Grenade | Class::Melee => assert_eq!(x.reserve, 0),
            }
        }
        assert_eq!(w("para").reserve, 100);
        assert_eq!(w("pk").reserve, 100);
        for x in WEAPONS.iter().filter(|x| !x.shell_reload) {
            if x.class != Class::Melee {
                assert!(x.reload_s >= 0.0);
            }
        }
    }

    #[test]
    fn scopes_match_their_zoom() {
        let zooms = [("bpa", 1.5), ("dmr20", 3.0), ("svd", 4.0), ("scout", 4.0), ("awm", 6.0), ("m82", 8.0)];
        for (k, z) in zooms {
            assert_eq!(w(k).sight, Sight::Scope { zoom: z }, "{k}");
        }
        for x in WEAPONS {
            match x.sight {
                Sight::Scope { zoom } => {
                    let fov = 2. * ((45f32).to_radians().tan() / zoom).atan().to_degrees();
                    assert!((x.ads_fov - fov).abs() < 0.3, "{}: {} vs {}", x.key, x.ads_fov, fov);
                }
                Sight::Iron | Sight::Dot => assert!(x.ads_fov <= 80., "{}", x.key),
                Sight::None => assert_eq!(x.ads_fov, 90., "{}", x.key),
            }
        }
    }

    #[test]
    fn damage_scale_per_class() {
        for x in of_class(Class::Pistol) {
            assert!((22. ..=55.).contains(&x.damage), "{}", x.key);
        }
        for x in of_class(Class::Smg) {
            assert!((18. ..=26.).contains(&x.damage), "{}", x.key);
        }
        for x in of_class(Class::AssaultRifle) {
            assert!((24. ..=36.).contains(&x.damage), "{}", x.key);
        }
        for x in of_class(Class::Dmr) {
            assert!((40. ..=50.).contains(&x.damage), "{}", x.key);
        }
        for x in of_class(Class::Lmg) {
            assert!((28. ..=32.).contains(&x.damage), "{}", x.key);
        }
        for x in of_class(Class::Shotgun) {
            assert!((8. ..=12.).contains(&x.damage) && (8..=9).contains(&x.pellets), "{}", x.key);
        }
        assert!((85. ..=130.).contains(&w("scout").damage));
        assert_eq!(w("rpg").damage, 130.);
        assert_eq!(w("thumper").damage, 90.);
    }

    #[test]
    fn damage_at_is_flat_then_falls_then_floors() {
        for x in WEAPONS.iter().filter(|x| x.is_ballistic()) {
            assert_eq!(x.damage_at(0.), x.damage, "{}", x.key);
            assert_eq!(x.damage_at(-5.), x.damage, "{}", x.key);
            assert_eq!(x.damage_at(x.range_m), x.damage, "{}", x.key);
            let far = x.damage * x.far_fraction;
            assert!((x.damage_at(2. * x.range_m) - far).abs() < 1e-3, "{}", x.key);
            assert!((x.damage_at(10. * x.range_m) - far).abs() < 1e-3, "{}", x.key);
            // Halfway through the fall-off is halfway between (smoothstep).
            let mid = x.damage_at(1.5 * x.range_m);
            assert!((mid - (x.damage + far) / 2.).abs() < 1e-3, "{}", x.key);
            assert!(x.damage_at(f32::NAN) <= x.damage);
        }
    }

    #[test]
    fn damage_at_is_monotonic() {
        for x in WEAPONS {
            let mut prev = f32::INFINITY;
            let mut d = 0.;
            while d < 400. {
                let v = x.damage_at(d);
                assert!(v.is_finite() && v >= 0., "{} at {d}", x.key);
                assert!(v <= prev + 1e-4, "{} rose at {d}: {v} > {prev}", x.key);
                prev = v;
                d += 0.25;
            }
        }
    }

    #[test]
    fn explosions_and_melee_ignore_range_falloff() {
        for k in ["rpg", "thumper", "frag", "incen", "knife", "axe"] {
            let x = w(k);
            assert_eq!(x.damage_at(0.), x.damage_at(150.), "{k}");
        }
        let r = w("rpg");
        assert_eq!(r.blast_at(0.), 130.);
        assert!((r.blast_at(2.5) - 65.).abs() < 1e-3);
        assert_eq!(r.blast_at(5.), 0.);
        assert_eq!(r.blast_at(50.), 0.);
        assert_eq!(w("k47").blast_at(0.), 0.);
        assert_eq!(w("flash").blast_at(0.), 0.);
        // A frag at your feet kills, at 3.5 m does not, and nothing reaches 7 m.
        assert!(w("frag").blast_at(0.) >= MAX_HEALTH);
        assert!(w("frag").blast_at(3.5) < MAX_HEALTH);
        assert_eq!(w("frag").blast_at(7.), 0.);
    }

    #[test]
    fn shots_to_kill_hand_checks() {
        assert_eq!(w("awm").shots_to_kill(100., false), 1);
        assert_eq!(w("awm").shots_to_kill(100., true), 1);
        assert_eq!(w("m82").shots_to_kill(100., false), 1);
        assert_eq!(w("m82").shots_to_kill(100., true), 1);
        assert_eq!(w("scout").shots_to_kill(100., false), 2);
        assert_eq!(w("scout").shots_to_kill(100., true), 1);
        assert_eq!(w("hc50").shots_to_kill(100., false), 2);
        assert_eq!(w("hc50").shots_to_kill(100., true), 1);
        assert_eq!(w("svd").shots_to_kill(100., false), 2);
        assert_eq!(w("dmr20").shots_to_kill(100., true), 1);
        assert_eq!(w("k47").shots_to_kill(100., false), 4);
        assert_eq!(w("k47").shots_to_kill(100., true), 2);
        assert_eq!(w("rpg").shots_to_kill(100., false), 1);
        assert_eq!(w("thumper").shots_to_kill(100., false), 2);
        assert_eq!(w("pump12").shots_to_kill(100., false), 1);
        assert_eq!(w("knife").shots_to_kill(100., false), 3);
        assert_eq!(w("k9").shots_to_kill(0., false), 0);
        assert_eq!(w("k9").shots_to_kill(1., false), 1);
        // Exact multiples do not round up: 50 damage kills 50 health in one shot.
        assert_eq!(w("svd").shots_to_kill(50., false), 1);
        // Far away the damage has fallen, so more shots are needed.
        let m = w("mp9");
        assert!(m.shots_to_kill_at(100., false, 100.) > m.shots_to_kill(100., false));
    }

    #[test]
    fn cycle_and_tick_helpers() {
        assert!((w("mp9").cycle_seconds() - 60. / 800.).abs() < 1e-6);
        assert!((w("awm").cycle_seconds() - 1.7).abs() < 1e-6);
        assert!((w("knife").cycle_seconds() - 0.4).abs() < 1e-6);
        // Burst: two gaps of 0.06 then a 0.24 s recovery.
        assert!((w("fm2").cycle_seconds() - 0.36).abs() < 1e-5);
        assert_eq!(w("fm2").burst_gap_ticks(), 4);
        assert_eq!(w("k47").burst_gap_ticks(), 0);
        assert_eq!(w("awm").cycle_ticks(), 102);
        assert_eq!(w("pump12").cycle_ticks(), 57);
        assert_eq!(w("k47").reload_ticks(), 150);
        assert_eq!(seconds_to_ticks(0.), 0);
        assert_eq!(seconds_to_ticks(-1.), 0);
        assert_eq!(seconds_to_ticks(f32::NAN), 0);
        assert_eq!(seconds_to_ticks(0.001), 1);
        assert_eq!(seconds_to_ticks(1.0), 60);
        for x in WEAPONS {
            assert!(x.cycle_ticks() >= 1, "{}", x.key);
            assert!(x.draw_ticks() >= 1, "{}", x.key);
        }
        // Shell reload: per shell; magazine weapons: one change.
        let p = w("pump12");
        assert_eq!(p.reload_seconds_for(0), 0.);
        assert!((p.reload_seconds_for(8) - 4.4).abs() < 1e-5);
        assert!((w("k47").reload_seconds_for(3) - 2.5).abs() < 1e-6);
        assert!((w("k47").reload_seconds_for(30) - 2.5).abs() < 1e-6);
        // No weapon fires faster than a round every 3 ticks except bursts inside a burst.
        for x in WEAPONS.iter().filter(|x| matches!(x.fire, Fire::Auto | Fire::Semi)) {
            assert!(x.rpm <= 1200., "{}", x.key);
        }
    }

    #[test]
    fn time_to_kill_bands_at_ten_metres() {
        // Seconds of shooting (first round to killing round) for a 100-health target, body shots, 10 m.
        let band = |c: Class, lo: f32, hi: f32| {
            for x in of_class(c) {
                let t = x.time_to_kill(MAX_HEALTH, false, 10.);
                assert!(t >= lo && t <= hi, "{} ({c:?}) TTK {t:.3} s outside {lo}..{hi}", x.key);
            }
        };
        band(Class::Pistol, 0.6, 1.4);
        band(Class::Smg, 0.35, 0.8);
        band(Class::AssaultRifle, 0.35, 0.75);
        band(Class::Dmr, 0.35, 0.75);
        band(Class::Lmg, 0.4, 0.8);
        // Single-shot (or two-shot) precision weapons kill without waiting.
        assert_eq!(w("awm").time_to_kill(100., false, 10.), 0.);
        assert_eq!(w("m82").time_to_kill(100., false, 10.), 0.);
        assert!((w("scout").time_to_kill(100., false, 10.) - 1.2).abs() < 1e-5);
        // The burst rifle: two shots of the second burst are not needed, one is (4th round).
        assert!((w("fm2").time_to_kill(100., false, 10.) - 0.36).abs() < 1e-5);
    }

    #[test]
    fn hitscan_precision_ordering() {
        let hc = w("hc50");
        for x in of_class(Class::Pistol).filter(|x| x.key != "hc50") {
            assert!(hc.damage > x.damage, "{}", x.key);
        }
        let anvil = w("m82");
        for x in WEAPONS.iter().filter(|x| x.is_ballistic() && x.key != "m82") {
            assert!(anvil.damage > x.damage, "{}", x.key);
            assert!(anvil.damage > x.damage * x.pellets as f32, "{} all pellets", x.key);
        }
        // Snipers: AW-M below Anvil, both kill in one body shot, scout needs two.
        assert!(w("awm").damage >= MAX_HEALTH && w("awm").damage < anvil.damage);
        assert!(w("scout").damage < MAX_HEALTH && w("scout").damage * 2. >= MAX_HEALTH);
        // Everything in the sniper class outranges every rifle in effective range.
        let best_rifle = WEAPONS
            .iter()
            .filter(|x| matches!(x.class, Class::AssaultRifle | Class::Smg | Class::Lmg))
            .map(|x| x.range_m)
            .fold(0., f32::max);
        for x in of_class(Class::Sniper) {
            assert!(x.range_m > best_rifle, "{}", x.key);
        }
    }

    #[test]
    fn shotguns_are_short_ranged_wide_and_lethal_up_close() {
        for x in of_class(Class::Shotgun) {
            assert!(x.pellets >= 8);
            assert!(x.range_m <= 16.);
            assert!(x.spread_deg >= 3.0);
            assert!(x.damage_per_shot(false, 0.) >= 80.);
            assert!(x.damage_at(40.) < x.damage * 0.5, "{}", x.key);
        }
        // Sawn-off: widest cone, shortest reach, both barrels on two trigger pulls.
        let sawn = w("sawn");
        for x in of_class(Class::Shotgun).filter(|x| x.key != "sawn") {
            assert!(sawn.spread_deg > x.spread_deg && sawn.range_m < x.range_m);
        }
        assert!(w("pump12").spread_deg < w("auto12").spread_deg);
    }

    #[test]
    fn heavier_guns_are_slower_on_foot() {
        assert!(w("m82").move_speed < w("awm").move_speed);
        assert!(w("pk").move_speed < w("para").move_speed);
        assert!(w("para").move_speed < w("m4c").move_speed);
        assert!(w("m4c").move_speed < w("mp9").move_speed);
        assert!(w("mp9").move_speed < w("k9").move_speed);
        assert!(w("axe").move_speed < w("knife").move_speed);
        for x in WEAPONS {
            assert!(x.move_speed <= 1.0, "{}", x.key);
        }
    }

    #[test]
    fn recoil_grows_with_power() {
        assert!(w("hc50").recoil_up_deg > w("k9").recoil_up_deg * 2.);
        assert!(w("m82").recoil_up_deg > w("awm").recoil_up_deg);
        assert!(w("k47").recoil_up_deg > w("m4c").recoil_up_deg);
        assert!(w("gl4").recoil_up_deg > w("k47").recoil_up_deg);
        assert!(w("pdw").recoil_up_deg < w("mp9").recoil_up_deg);
        // Scoped snipers are nearly perfect when aimed, terrible when moving.
        for k in ["scout", "awm", "m82"] {
            assert!(w(k).ads_spread_mult <= 0.1 && w(k).move_spread_deg >= 7., "{k}");
        }
    }

    #[test]
    fn melee_numbers() {
        let knife = w("knife").melee.unwrap();
        assert_eq!((knife.light, knife.heavy, knife.back_mult), (35., 65., 2.0));
        assert!(knife.heavy * knife.back_mult >= MAX_HEALTH, "back-stab kills");
        assert!(knife.light * knife.back_mult < MAX_HEALTH);
        let axe = w("axe").melee.unwrap();
        assert_eq!(axe.heavy, 100.);
        assert!(axe.heavy_s > knife.heavy_s && axe.light_s > knife.light_s);
        for x in of_class(Class::Melee) {
            let m = x.melee.unwrap();
            assert_eq!(x.damage, m.light, "{}", x.key);
            assert_eq!(x.range_m, m.reach, "{}", x.key);
            assert!(m.heavy * m.back_mult >= MAX_HEALTH, "{} heavy back-stab", x.key);
            assert!(m.heavy < MAX_HEALTH || x.key == "axe", "{}", x.key);
        }
        // Knife has the best cycle of the four.
        for x in of_class(Class::Melee).filter(|x| x.key != "knife") {
            assert!(x.cycle_seconds() > w("knife").cycle_seconds());
        }
    }

    #[test]
    fn markdown_has_a_row_per_weapon() {
        let md = armoury_markdown();
        assert_eq!(md.lines().filter(|l| l.starts_with("| ")).count(), 34);
        for x in WEAPONS {
            assert!(md.contains(&format!("| {} |", x.key)), "{}", x.key);
            assert!(md.contains(x.real), "{}", x.key);
        }
    }

    /// Regenerates `docs/ARMOURY.md`. Ignored so a normal test run writes no files:
    /// `cargo test --lib write_armoury_doc -- --ignored`.
    #[test]
    #[ignore]
    fn write_armoury_doc() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/ARMOURY.md");
        std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/docs")).unwrap();
        std::fs::write(path, armoury_markdown()).unwrap();
    }
}
