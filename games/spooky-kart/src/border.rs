//! Where the drawn road edge, kerbs, verges and walls stand. Presentation geometry only: the simulation
//! never reads it (the wall karts hit is `Track::wall`, a distance from the centreline), and nothing here
//! is part of the save state. It lives in the library so a headless test can prove the drawn border is a
//! clean strip without opening a window.
//!
//! Offsetting each sample along its own segment normal leaves a gap or a fold at every corner, and on the
//! inside of a bend tighter than the offset (the road turns with a radius under 14 m in places, the wall
//! stands 14 m out) the offset curve doubles back on itself in a small loop, so the drawn strip folds over
//! and flickers. [`Border::line`] builds the exact offset instead: each sample is mitred (the two
//! neighbouring segment normals averaged and stretched, so the line keeps its distance from both
//! segments), and any loop on the inside of a tight bend is cut off at the point where the line crosses
//! itself, which is where a distance-clamped wall stands. The line never crosses itself and, wherever the
//! road bends tighter than the offset, it collapses onto that one corner point.
use crate::track::Track;

/// A mitre never stretches more than this much (a hairpin would otherwise spike).
const MAX_MITER: f32 = 2.;
/// How many segments ahead a loop may reach before it is not treated as a loop.
const LOOP_REACH: usize = 24;

type P = [f32; 2];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(v: P) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

fn unit(v: P) -> P {
    let l = len(v).max(1e-6);
    [v[0] / l, v[1] / l]
}

fn dot(a: P, b: P) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

/// Right of a direction of travel, in (x, z), matching `Track` (right of yaw 0 is +x).
fn right_of(t: P) -> P {
    [-t[1], t[0]]
}

/// Twice the signed area of the triangle `a b c`.
fn orient(a: P, b: P, c: P) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Where segments `ab` and `cd` properly cross, if they do.
fn cross_point(a: P, b: P, c: P, d: P) -> Option<P> {
    let (o1, o2) = (orient(a, b, c), orient(a, b, d));
    let (o3, o4) = (orient(c, d, a), orient(c, d, b));
    if o1 * o2 < 0. && o3 * o4 < 0. {
        let t = o3 / (o3 - o4);
        Some([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
    } else {
        None
    }
}

/// Mitred offsets of the centreline samples.
pub struct Border {
    centre: Vec<P>,
    /// Mitre direction at each sample, pointing to the right of travel, already stretched.
    miter: Vec<P>,
}

impl Border {
    pub fn new(track: &Track) -> Self {
        let centre: Vec<P> = track.samples().iter().map(|v| [v.0, v.2]).collect();
        let n = centre.len();
        let mut miter = Vec::with_capacity(n);
        for i in 0..n {
            let (prev, p, next) = (centre[(i + n - 1) % n], centre[i], centre[(i + 1) % n]);
            let (t_in, t_out) = (unit(sub(p, prev)), unit(sub(next, p)));
            let (a, c) = (right_of(t_in), right_of(t_out));
            let m = unit([a[0] + c[0], a[1] + c[1]]);
            let stretch = 1. / dot(m, c).max(1. / MAX_MITER);
            miter.push([m[0] * stretch, m[1] * stretch]);
        }
        Border { centre, miter }
    }

    pub fn len(&self) -> usize {
        self.centre.len()
    }

    pub fn is_empty(&self) -> bool {
        self.centre.is_empty()
    }

    /// The centreline sample `i` (wraps), as (x, z).
    pub fn centre(&self, i: usize) -> [f32; 2] {
        self.centre[i % self.len()]
    }

    /// Direction of travel along the segment from sample `i` to `i + 1`, as (x, z).
    pub fn tangent(&self, i: usize) -> [f32; 2] {
        unit(sub(self.centre((i + 1) % self.len()), self.centre(i)))
    }

    /// The raw mitred point `lateral` metres to the right of sample `i` (negative is left). On the inside
    /// of a bend tighter than `lateral` the raw offset loops back over itself; use [`Border::line`] for a
    /// line that is drawn, and this only to place things that are checked some other way.
    pub fn at(&self, i: usize, lateral: f32) -> [f32; 2] {
        let i = i % self.len();
        let (c, m) = (self.centre[i], self.miter[i]);
        [c[0] + m[0] * lateral, c[1] + m[1] * lateral]
    }

    /// The whole line `lateral` metres to the right of the centreline (negative is left), one point per
    /// sample: the exact offset with every loop on the inside of a tight bend cut off at the point where the
    /// line crosses itself. Points inside a cut loop all sit on that crossing, so the line never reverses.
    pub fn line(&self, lateral: f32) -> Vec<[f32; 2]> {
        let n = self.len();
        let mut q: Vec<P> = (0..n).map(|i| self.at(i, lateral)).collect();
        for i in 0..n {
            let (a, b) = (q[i], q[(i + 1) % n]);
            // The furthest segment ahead that this one crosses closes a loop: collapse everything between.
            let hit = (i + 2..=i + LOOP_REACH).rev().find_map(|j| {
                let (c, d) = (q[j % n], q[(j + 1) % n]);
                cross_point(a, b, c, d).map(|p| (j, p))
            });
            if let Some((j, p)) = hit {
                for k in i + 1..=j {
                    q[k % n] = p;
                }
            }
        }
        q
    }
}
