//! The observation: what a driver can perceive, and nothing else.
//!
//! [`Obs`] is a different type from the simulation. It is built by [`Obs::observe`] from a `&Sim` and a kart
//! slot, copies plain numbers out of it and keeps no reference, so a policy that receives an `Obs` has no path
//! to the rest of the race (the compiler, not a convention, keeps hidden state hidden; `validate` also
//! mutates hidden state and checks the observation does not move).
//!
//! # What IS observable (all of it is something a human on the track could see or feel)
//!
//! * **Own kart**: speed, velocity in the kart's own frame (forward and sideways), heading against the road
//!   direction, lateral offset from the centreline, how far round the lap and which lap, on or off the paved
//!   road, drifting and the drift charge, boost, slowed, spun out, phased (the Ghost's own phase), whether the
//!   perk button would work now and how recharged it is, the countdown, the position in the race (the HUD
//!   shows it), and which character it is (the public stats table follows from that).
//! * **The road ahead** (the map is public and the driver sees it): at eight distances ahead along the
//!   centreline, the bearing of that road point from the kart's heading and the bend of the road there
//!   relative to the road here.
//! * **Up to four nearest other karts within 60 m**: distance, bearing, relative velocity in the own frame,
//!   how far ahead (+) or behind (-) along the road, and how many places ahead (-) or behind (+) in the race
//!   order. Everything a driver can see from the seat.
//! * **Up to four nearest hazards within 40 m**: distance, bearing, radius and kind (bone or bandage).
//!
//! # What is NOT observable (kept out of this type on purpose)
//!
//! * Other karts' hidden state: perk cooldowns, drift charge, boost or slow timers, bot skill, their
//!   statistics counters, which character they are (the vector gives no per-kart character id), and any kart
//!   beyond the nearest four or 60 m away.
//! * Hazard owner and remaining lifetime; hazards beyond the nearest four or 40 m away.
//! * The random number generator, the exact race tick, the whole `Sim`, and the other agents' observations.
//! * Raw world positions: everything is relative to the kart or to the road, so an agent cannot memorise the
//!   map by coordinates (it can still learn it by lap fraction and the lookahead).
//!
//! # The vector
//!
//! [`Obs::to_vec`] flattens an observation to [`LEN`] floats in the fixed order of [`LAYOUT`] (names and
//! sizes; [`Obs::names`] gives one name per element). Every element is normalised and **clamped to the
//! bounds of [`Obs::bounds`]**, so the bounds are a guarantee, not an observation. Angles are in units of
//! pi radians, positive to the right of the kart's heading (the game's convention: positive yaw turns right).
//! Absent others and hazards are all zeros (their `present` element is 0).
use crate::character::{Character, ALL, MAX_RACERS};
use crate::kart::{Kart, BOOST_TIER_TWO};
use crate::sim::{HazardKind, Phase, Sim, COUNTDOWN_TICKS, LAPS};
use crate::track::{forward, right, wrap_angle, yaw_of, HALF_WIDTH};
use std::f32::consts::PI;

/// Distances ahead along the centreline at which the road is sampled, metres.
pub const LOOKAHEAD_M: [f32; 8] = [5., 10., 15., 20., 30., 40., 55., 70.];
/// How many other karts are reported (the nearest ones).
pub const MAX_OTHERS: usize = 4;
/// Other karts further than this are not seen, metres.
pub const OTHER_RANGE: f32 = 60.;
/// How many hazards are reported (the nearest ones).
pub const MAX_HAZARDS: usize = 4;
/// Hazards further than this are not seen, metres.
pub const HAZARD_RANGE: f32 = 40.;
/// Per-other-kart elements, in order.
pub const OTHER_FIELDS: [&str; 7] =
    ["present", "distance", "bearing", "rel_forward", "rel_right", "arc_ahead", "rank_delta"];
/// Per-hazard elements, in order.
pub const HAZARD_FIELDS: [&str; 5] = ["present", "distance", "bearing", "radius", "bone"];

/// Metres per second that map to 1.0 for the own speed and velocity elements.
pub const SPEED_SCALE: f32 = 30.;
/// Metres per second that map to 1.0 for relative velocity elements.
pub const REL_SPEED_SCALE: f32 = 60.;
/// The longest boost, in ticks (a tier-two drift boost), for normalising the boost timer.
const BOOST_TICKS_MAX: f32 = 90.;
const SPIN_TICKS_MAX: f32 = 50.;
const PHASE_TICKS_MAX: f32 = 120.;
const MAX_HAZARD_RADIUS: f32 = 3.;

/// The observation vector's layout: `(name, number of floats)` in order. `others` is slot-major, each slot
/// holding [`OTHER_FIELDS`]; `hazards` likewise with [`HAZARD_FIELDS`].
pub const LAYOUT: &[(&str, usize)] = &[
    ("speed", 1),
    ("vel_forward", 1),
    ("vel_right", 1),
    ("heading_error", 1),
    ("lateral", 1),
    ("lap_fraction", 1),
    ("lap_index", 1),
    ("offroad", 1),
    ("drifting", 1),
    ("drift_dir", 1),
    ("drift_charge", 1),
    ("boost", 1),
    ("slowed", 1),
    ("spin", 1),
    ("phased", 1),
    ("perk_ready", 1),
    ("perk_cooldown", 1),
    ("countdown", 1),
    ("rank", 1),
    ("character", 8),
    ("lookahead_bearing", 8),
    ("lookahead_bend", 8),
    ("others", MAX_OTHERS * 7),
    ("hazards", MAX_HAZARDS * 5),
];

const fn layout_len(layout: &[(&str, usize)]) -> usize {
    let (mut i, mut n) = (0, 0);
    while i < layout.len() {
        n += layout[i].1;
        i += 1;
    }
    n
}

/// Number of floats in [`Obs::to_vec`].
pub const LEN: usize = layout_len(LAYOUT);

/// What the kart itself knows. Natural units unless said otherwise; see [`LAYOUT`] for the vector scaling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnView {
    pub character: Character,
    /// Ground speed, m/s.
    pub speed: f32,
    /// Velocity along the heading, m/s (negative when reversing).
    pub vel_forward: f32,
    /// Velocity across the heading, m/s (positive to the right): the slide.
    pub vel_right: f32,
    /// Heading minus the road direction, radians (positive: pointing right of the road).
    pub heading_error: f32,
    /// Signed distance from the centreline, metres (positive: right of the road's direction).
    pub lateral: f32,
    /// Laps completed.
    pub lap: u32,
    /// Distance round the current lap as a fraction (slightly negative on the grid).
    pub lap_fraction: f32,
    /// On the grass (the paved road is `HALF_WIDTH` either side of the centreline).
    pub offroad: bool,
    pub drifting: bool,
    /// -1, 0 or +1: which way the drift leans.
    pub drift_dir: f32,
    /// Drift charge in boost-tier units: 1.0 pays a tier-one boost, 2.0 a tier-two.
    pub drift_charge: f32,
    /// Boost time left as a fraction of the longest boost.
    pub boost: f32,
    /// Slowed by a hazard or crows.
    pub slowed: bool,
    /// Spin-out time left as a fraction of a full spin-out.
    pub spin: f32,
    /// Phase time left as a fraction of a full phase.
    pub phased: f32,
    /// The perk button would do something right now (an active perk, recharged, not spinning).
    pub perk_ready: bool,
    /// Perk recharge left as a fraction of its cooldown (0 when ready or passive).
    pub perk_cooldown: f32,
    /// Countdown left as a fraction (0 once racing).
    pub countdown: f32,
    /// Race position, 1 = first (the HUD shows it).
    pub rank: u32,
    /// Karts in the race.
    pub racers: u32,
}

/// The road at one distance ahead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lookahead {
    /// Bearing of the centreline point from the kart's heading, radians (positive: to the right).
    pub bearing: f32,
    /// The road direction there minus the road direction here, radians (positive: the road turns right).
    pub bend: f32,
}

/// Another kart as seen from the seat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OtherView {
    pub distance: f32,
    /// From the kart's heading, radians (positive: to the right).
    pub bearing: f32,
    /// Their velocity minus ours, along our heading, m/s.
    pub rel_forward: f32,
    /// Their velocity minus ours, across our heading (positive: to our right), m/s.
    pub rel_right: f32,
    /// Distance along the road: positive when they are ahead of us, metres.
    pub arc_ahead: f32,
    /// Their race position minus ours (negative: they are ahead in the order).
    pub rank_delta: i32,
}

/// A hazard on the road.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazardView {
    pub distance: f32,
    pub bearing: f32,
    pub radius: f32,
    /// A bone (spins you out) rather than a bandage (slows you).
    pub bone: bool,
}

/// One driver's view of the race. See the module documentation for exactly what it holds and omits.
#[derive(Clone, Debug, PartialEq)]
pub struct Obs {
    pub own: OwnView,
    pub lookahead: [Lookahead; 8],
    /// Nearest first; `None` where fewer than four karts are within range.
    pub others: [Option<OtherView>; MAX_OTHERS],
    /// Nearest first.
    pub hazards: [Option<HazardView>; MAX_HAZARDS],
}

/// 1-based race position of every kart (the order the HUD shows).
pub fn ranks(sim: &Sim) -> Vec<u32> {
    let mut ranks = vec![0; sim.karts.len()];
    for (position, kart) in sim.standings().into_iter().enumerate() {
        ranks[kart] = position as u32 + 1;
    }
    ranks
}

fn bearing_of(kart: &Kart, to: vesper3d::math::V) -> f32 {
    wrap_angle(yaw_of(to - kart.pos) - kart.yaw)
}

impl Obs {
    /// What the driver of kart `slot` perceives right now.
    pub fn observe(sim: &Sim, slot: usize) -> Obs {
        Self::observe_with(sim, slot, &ranks(sim))
    }

    /// [`Obs::observe`] with the race order already computed (see [`ranks`]), for observing several karts.
    pub fn observe_with(sim: &Sim, slot: usize, ranks: &[u32]) -> Obs {
        let track = sim.track();
        let kart = &sim.karts[slot];
        let (fwd, rgt) = (forward(kart.yaw), right(kart.yaw));
        let here = yaw_of(track.tangent_at(kart.s));
        let character = kart.character;
        let cooldown_ticks = character.cooldown_ticks();
        let own = OwnView {
            character,
            speed: kart.vel.length(),
            vel_forward: kart.vel.dot(fwd),
            vel_right: kart.vel.dot(rgt),
            heading_error: wrap_angle(kart.yaw - here),
            lateral: kart.lateral,
            lap: kart.lap,
            lap_fraction: (kart.progress - kart.lap as f32 * track.length) / track.length,
            offroad: kart.offroad,
            drifting: kart.drifting,
            drift_dir: if kart.drifting { kart.drift_dir } else { 0. },
            drift_charge: if kart.drifting { kart.drift_charge } else { 0. },
            boost: kart.boost_ticks as f32 / BOOST_TICKS_MAX,
            slowed: kart.slow_ticks > 0,
            spin: kart.spin_ticks as f32 / SPIN_TICKS_MAX,
            phased: kart.phase_ticks as f32 / PHASE_TICKS_MAX,
            perk_ready: character.perk().is_active()
                && kart.cooldown == 0
                && kart.spin_ticks == 0
                && kart.finished_tick.is_none(),
            perk_cooldown: if cooldown_ticks > 0 { kart.cooldown as f32 / cooldown_ticks as f32 } else { 0. },
            countdown: match sim.phase {
                Phase::Countdown(n) => n as f32 / COUNTDOWN_TICKS as f32,
                _ => 0.,
            },
            rank: ranks[slot],
            racers: sim.karts.len() as u32,
        };

        let mut lookahead = [Lookahead { bearing: 0., bend: 0. }; 8];
        for (look, d) in lookahead.iter_mut().zip(LOOKAHEAD_M) {
            look.bearing = bearing_of(kart, track.point_at(kart.s + d));
            look.bend = wrap_angle(yaw_of(track.tangent_at(kart.s + d)) - here);
        }

        let mut near: Vec<(f32, usize)> = sim
            .karts
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != slot)
            .map(|(j, o)| ((o.pos - kart.pos).length(), j))
            .filter(|&(d, _)| d <= OTHER_RANGE)
            .collect();
        near.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
        let mut others = [None; MAX_OTHERS];
        for (out, &(distance, j)) in others.iter_mut().zip(&near) {
            let o = &sim.karts[j];
            let rel = o.vel - kart.vel;
            *out = Some(OtherView {
                distance,
                bearing: bearing_of(kart, o.pos),
                rel_forward: rel.dot(fwd),
                rel_right: rel.dot(rgt),
                arc_ahead: track.arc_delta(kart.s, o.s),
                rank_delta: ranks[j] as i32 - ranks[slot] as i32,
            });
        }

        let mut seen: Vec<(f32, usize)> = sim
            .hazards
            .iter()
            .enumerate()
            .map(|(i, h)| ((h.pos - kart.pos).length(), i))
            .filter(|&(d, _)| d <= HAZARD_RANGE)
            .collect();
        seen.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
        let mut hazards = [None; MAX_HAZARDS];
        for (out, &(distance, i)) in hazards.iter_mut().zip(&seen) {
            let h = &sim.hazards[i];
            *out = Some(HazardView {
                distance,
                bearing: bearing_of(kart, h.pos),
                radius: h.radius,
                bone: h.kind == HazardKind::Bone,
            });
        }
        Obs { own, lookahead, others, hazards }
    }

    /// The flat, normalised, clamped vector in the order of [`LAYOUT`], [`LEN`] floats.
    pub fn to_vec(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(LEN);
        let bounds = Self::bounds();
        let o = &self.own;
        let racers = o.racers.max(2) as f32 - 1.;
        v.extend([
            o.speed / SPEED_SCALE,
            o.vel_forward / SPEED_SCALE,
            o.vel_right / SPEED_SCALE,
            o.heading_error / PI,
            o.lateral / HALF_WIDTH,
            o.lap_fraction,
            o.lap as f32 / LAPS as f32,
            flag(o.offroad),
            flag(o.drifting),
            o.drift_dir,
            o.drift_charge / BOOST_TIER_TWO,
            o.boost,
            flag(o.slowed),
            o.spin,
            o.phased,
            flag(o.perk_ready),
            o.perk_cooldown,
            o.countdown,
            if o.racers > 1 { (o.rank as f32 - 1.) / racers } else { 0. },
        ]);
        v.extend(ALL.iter().map(|c| flag(*c == o.character)));
        v.extend(self.lookahead.iter().map(|l| l.bearing / PI));
        v.extend(self.lookahead.iter().map(|l| l.bend / PI));
        for other in &self.others {
            match other {
                Some(k) => v.extend([
                    1.,
                    k.distance / OTHER_RANGE,
                    k.bearing / PI,
                    k.rel_forward / REL_SPEED_SCALE,
                    k.rel_right / REL_SPEED_SCALE,
                    k.arc_ahead / OTHER_RANGE,
                    k.rank_delta as f32 / racers,
                ]),
                None => v.extend([0.; 7]),
            }
        }
        for hazard in &self.hazards {
            match hazard {
                Some(h) => v.extend([
                    1.,
                    h.distance / HAZARD_RANGE,
                    h.bearing / PI,
                    h.radius / MAX_HAZARD_RADIUS,
                    flag(h.bone),
                ]),
                None => v.extend([0.; 5]),
            }
        }
        debug_assert_eq!(v.len(), LEN);
        for (x, (lo, hi)) in v.iter_mut().zip(bounds) {
            *x = if x.is_finite() { x.clamp(lo, hi) } else { 0. };
        }
        v
    }

    /// `(low, high)` for every element of [`Obs::to_vec`], in order.
    pub fn bounds() -> Vec<(f32, f32)> {
        let mut b = vec![
            (0., 2.),      // speed
            (-1., 2.),     // vel_forward
            (-1., 1.),     // vel_right
            (-1., 1.),     // heading_error
            (-1.6, 1.6),   // lateral (the wall stands at 1.56)
            (-0.25, 1.25), // lap_fraction
            (0., 1.),      // lap_index
            (0., 1.),      // offroad
            (0., 1.),      // drifting
            (-1., 1.),     // drift_dir
            (0., 1.5),     // drift_charge
            (0., 1.),      // boost
            (0., 1.),      // slowed
            (0., 1.),      // spin
            (0., 1.),      // phased
            (0., 1.),      // perk_ready
            (0., 1.),      // perk_cooldown
            (0., 1.),      // countdown
            (0., 1.),      // rank
        ];
        b.extend([(0., 1.); MAX_RACERS]);
        b.extend([(-1., 1.); 8]);
        b.extend([(-1., 1.); 8]);
        for _ in 0..MAX_OTHERS {
            b.extend([(0., 1.), (0., 1.), (-1., 1.), (-1., 1.), (-1., 1.), (-1., 1.), (-1., 1.)]);
        }
        for _ in 0..MAX_HAZARDS {
            b.extend([(0., 1.), (0., 1.), (-1., 1.), (0., 1.), (0., 1.)]);
        }
        debug_assert_eq!(b.len(), LEN);
        b
    }

    /// The layout as `(name, size)`, same as [`LAYOUT`].
    pub fn layout() -> &'static [(&'static str, usize)] {
        LAYOUT
    }

    /// One name per vector element, e.g. `"speed"`, `"lookahead_bearing.3"`, `"others.1.distance"`.
    pub fn names() -> Vec<String> {
        let mut names = Vec::with_capacity(LEN);
        for &(group, size) in LAYOUT {
            match group {
                "others" => {
                    for i in 0..MAX_OTHERS {
                        names.extend(OTHER_FIELDS.iter().map(|f| format!("others.{i}.{f}")));
                    }
                }
                "hazards" => {
                    for i in 0..MAX_HAZARDS {
                        names.extend(HAZARD_FIELDS.iter().map(|f| format!("hazards.{i}.{f}")));
                    }
                }
                "character" => names.extend(ALL.iter().map(|c| format!("character.{}", c.name()))),
                _ if size == 1 => names.push(group.to_string()),
                _ => names.extend((0..size).map(|i| format!("{group}.{i}"))),
            }
        }
        names
    }
}

fn flag(b: bool) -> f32 {
    if b {
        1.
    } else {
        0.
    }
}
