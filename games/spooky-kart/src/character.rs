//! The eight drivers: one table of numbers, so balancing is a data edit and telemetry can say whether it worked.
use serde::{Deserialize, Serialize};

/// At most this many karts race (the engine server's session cap is also eight).
pub const MAX_RACERS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Character {
    Vampire,
    Frankenstein,
    Mummy,
    Ghost,
    Scarecrow,
    Zombie,
    Clown,
    Skeleton,
}

/// Every character, in grid order.
pub const ALL: [Character; MAX_RACERS] = [
    Character::Vampire,
    Character::Frankenstein,
    Character::Mummy,
    Character::Ghost,
    Character::Scarecrow,
    Character::Zombie,
    Character::Clown,
    Character::Skeleton,
];

/// A signature perk. `Active` perks fire on the perk button and then cool down; `Passive` ones are always on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Perk {
    /// Vampire: drifting charges faster and pays out a stronger boost.
    BatBoost,
    /// Frankenstein's Monster: at speed, a bump launches and slows lighter karts.
    Ram,
    /// Mummy: drops a strip of bandage behind that slows karts driving over it.
    BandageTrail,
    /// Ghost: for a moment, pass through karts, hazards and the off-road slowdown.
    Phase,
    /// Scarecrow: crows slow the nearest kart ahead.
    CrowSwarm,
    /// Zombie: hardly slowed off-road, and stuns and slowdowns wear off in half the time.
    Undead,
    /// Clown: a honk that shoves nearby karts away.
    Honk,
    /// Skeleton: drops bones that spin karts out; the Skeleton is immune to hazards.
    Rattle,
}

/// Every perk, in wire order.
pub const PERKS: [Perk; 8] = [
    Perk::BatBoost,
    Perk::Ram,
    Perk::BandageTrail,
    Perk::Phase,
    Perk::CrowSwarm,
    Perk::Undead,
    Perk::Honk,
    Perk::Rattle,
];

impl Perk {
    pub fn index(self) -> u8 {
        PERKS.iter().position(|p| *p == self).unwrap_or(0) as u8
    }

    pub fn from_index(index: u8) -> Option<Perk> {
        PERKS.get(index as usize).copied()
    }

    pub fn is_active(self) -> bool {
        !matches!(self, Perk::BatBoost | Perk::Ram | Perk::Undead)
    }
}

/// How a kart drives. Speeds are m/s, rates are per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub top_speed: f32,
    pub accel: f32,
    /// Steering rate at speed, radians per second.
    pub handling: f32,
    /// How fast sideways slide dies out (higher grips harder).
    pub grip: f32,
    pub mass: f32,
    /// Fraction of top speed left on grass.
    pub offroad: f32,
    /// Drift charge per second while drifting through a corner.
    pub drift_rate: f32,
    /// Extra top speed a drift boost gives, as a fraction.
    pub boost_power: f32,
}

impl Character {
    pub fn index(self) -> usize {
        ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Character {
        ALL[index % MAX_RACERS]
    }

    /// The character for a wire byte, or `None` if it is out of range.
    pub fn from_wire(index: u8) -> Option<Character> {
        ALL.get(index as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Character::Vampire => "Vampire",
            Character::Frankenstein => "Frankenstein's Monster",
            Character::Mummy => "Mummy",
            Character::Ghost => "Ghost",
            Character::Scarecrow => "Scarecrow",
            Character::Zombie => "Zombie",
            Character::Clown => "Clown",
            Character::Skeleton => "Skeleton",
        }
    }

    pub fn kart_name(self) -> &'static str {
        match self {
            Character::Vampire => "Coffin Cruiser",
            Character::Frankenstein => "Bolt Bucket",
            Character::Mummy => "Sarcophagus Sprinter",
            Character::Ghost => "Wisp Racer",
            Character::Scarecrow => "Hay Wagon",
            Character::Zombie => "Grave Digger",
            Character::Clown => "Jack-in-the-Box",
            Character::Skeleton => "Bone Buggy",
        }
    }

    pub fn perk(self) -> Perk {
        match self {
            Character::Vampire => Perk::BatBoost,
            Character::Frankenstein => Perk::Ram,
            Character::Mummy => Perk::BandageTrail,
            Character::Ghost => Perk::Phase,
            Character::Scarecrow => Perk::CrowSwarm,
            Character::Zombie => Perk::Undead,
            Character::Clown => Perk::Honk,
            Character::Skeleton => Perk::Rattle,
        }
    }

    pub fn perk_description(self) -> &'static str {
        match self.perk() {
            Perk::BatBoost => "Bat Boost: drifts charge faster and boost harder",
            Perk::Ram => "Ram: bumps at speed launch lighter karts",
            Perk::BandageTrail => "Bandage Trail: drop a strip that slows karts behind",
            Perk::Phase => "Phase: slip through karts, hazards and grass",
            Perk::CrowSwarm => "Crow Swarm: slow the nearest kart ahead",
            Perk::Undead => "Undead: shrug off grass, stuns wear off fast",
            Perk::Honk => "Honk: shove nearby karts away",
            Perk::Rattle => "Rattle: drop bones that spin karts out",
        }
    }

    /// Ticks an active perk needs to recharge (zero for passives).
    pub fn cooldown_ticks(self) -> u32 {
        match self.perk() {
            Perk::BandageTrail => 480,
            Perk::Phase => 600,
            Perk::CrowSwarm => 600,
            Perk::Honk => 360,
            Perk::Rattle => 540,
            _ => 0,
        }
    }

    /// Ticks the Ghost stays phased.
    pub const PHASE_TICKS: u32 = 120;

    pub fn stats(self) -> Stats {
        match self {
            Character::Vampire => Stats {
                top_speed: 27.,
                accel: 15.,
                handling: 1.9,
                grip: 5.5,
                mass: 1.0,
                offroad: 0.55,
                drift_rate: 1.5,
                boost_power: 0.30,
            },
            Character::Frankenstein => Stats {
                top_speed: 28.5,
                accel: 10.,
                handling: 1.4,
                grip: 5.0,
                mass: 1.7,
                offroad: 0.50,
                drift_rate: 0.9,
                boost_power: 0.20,
            },
            Character::Mummy => Stats {
                top_speed: 26.5,
                accel: 12.,
                handling: 2.3,
                grip: 6.5,
                mass: 1.0,
                offroad: 0.55,
                drift_rate: 1.0,
                boost_power: 0.20,
            },
            Character::Ghost => Stats {
                top_speed: 27.,
                accel: 13.,
                handling: 2.0,
                grip: 4.0,
                mass: 0.7,
                offroad: 0.55,
                drift_rate: 1.0,
                boost_power: 0.20,
            },
            Character::Scarecrow => Stats {
                top_speed: 27.,
                accel: 12.,
                handling: 1.9,
                grip: 5.5,
                mass: 1.0,
                offroad: 0.6,
                drift_rate: 1.0,
                boost_power: 0.20,
            },
            Character::Zombie => Stats {
                top_speed: 26.,
                accel: 12.,
                handling: 1.7,
                grip: 6.0,
                mass: 1.4,
                offroad: 0.9,
                drift_rate: 0.9,
                boost_power: 0.20,
            },
            Character::Clown => Stats {
                top_speed: 26.,
                accel: 13.5,
                handling: 2.5,
                grip: 6.0,
                mass: 0.9,
                offroad: 0.55,
                drift_rate: 1.1,
                boost_power: 0.20,
            },
            Character::Skeleton => Stats {
                top_speed: 26.5,
                accel: 15.5,
                handling: 2.0,
                grip: 5.0,
                mass: 0.75,
                offroad: 0.55,
                drift_rate: 1.0,
                boost_power: 0.20,
            },
        }
    }
}
