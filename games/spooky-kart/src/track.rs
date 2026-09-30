//! Haunted Hollow, the one map: a closed loop described by a centreline, a road width and grass shoulders
//! with walls beyond. Everything the simulation needs about the ground is a question asked of `Track`.
//!
//! Convention (matches the engine's yaw): yaw 0 faces -Z, positive yaw turns right, so the forward vector
//! is `(sin yaw, -cos yaw)` in (x, z) and right is `(cos yaw, sin yaw)`.
use vesper3d::math::V;

/// Half the paved width, metres.
pub const HALF_WIDTH: f32 = 9.;
/// Grass beyond the road before the wall.
pub const SHOULDER: f32 = 5.;
/// Samples per control-point span.
const SUBDIVISIONS: usize = 12;
/// How many samples either side of the hint `nearest` looks at.
const WINDOW: usize = 10;

/// Control points of the centreline (x, z). The start line is at the first point, heading east.
const CONTROL: [(f32, f32); 14] = [
    (0., 0.),
    (90., -5.),
    (170., 10.),
    (230., 50.),
    (250., 120.),
    (220., 190.),
    (150., 220.),
    (80., 200.),
    (60., 150.),
    (100., 120.),
    (110., 80.),
    (50., 70.),
    (-10., 50.),
    (-30., 20.),
];

pub fn forward(yaw: f32) -> V {
    V(yaw.sin(), 0., -yaw.cos())
}

pub fn right(yaw: f32) -> V {
    V(yaw.cos(), 0., yaw.sin())
}

/// The heading that faces along `direction` (horizontal).
pub fn yaw_of(direction: V) -> f32 {
    direction.0.atan2(-direction.2)
}

/// Wrap an angle to -PI..PI.
pub fn wrap_angle(a: f32) -> f32 {
    let two = std::f32::consts::TAU;
    let mut a = a % two;
    if a > std::f32::consts::PI {
        a -= two;
    } else if a < -std::f32::consts::PI {
        a += two;
    }
    a
}

/// Where a point sits relative to the road.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nearest {
    /// Index of the centreline segment.
    pub index: usize,
    /// Distance along the centreline from the start line, 0..length.
    pub s: f32,
    /// Signed distance from the centreline, positive to the right of the direction of travel.
    pub lateral: f32,
    pub center: V,
    pub tangent: V,
}

pub struct Track {
    samples: Vec<V>,
    /// Cumulative arc length at each sample; `cum[samples.len()]` is the loop length.
    cum: Vec<f32>,
    pub length: f32,
}

impl Default for Track {
    fn default() -> Self {
        Self::haunted_hollow()
    }
}

fn catmull(p0: V, p1: V, p2: V, p3: V, t: f32) -> V {
    let (t2, t3) = (t * t, t * t * t);
    (p1 * 2. + (p2 - p0) * t + (p0 * 2. - p1 * 5. + p2 * 4. - p3) * t2 + (p1 * 3. - p0 - p2 * 3. + p3) * t3) * 0.5
}

impl Track {
    pub fn name(&self) -> &'static str {
        "Haunted Hollow"
    }

    pub fn haunted_hollow() -> Self {
        let pts: Vec<V> = CONTROL.iter().map(|&(x, z)| V(x, 0., z)).collect();
        let n = pts.len();
        let mut samples = Vec::with_capacity(n * SUBDIVISIONS);
        for i in 0..n {
            let (p0, p1, p2, p3) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
            for k in 0..SUBDIVISIONS {
                samples.push(catmull(p0, p1, p2, p3, k as f32 / SUBDIVISIONS as f32));
            }
        }
        let mut cum = Vec::with_capacity(samples.len() + 1);
        let mut total = 0.;
        for i in 0..samples.len() {
            cum.push(total);
            total += (samples[(i + 1) % samples.len()] - samples[i]).length();
        }
        cum.push(total);
        Self { samples, cum, length: total }
    }

    pub fn samples(&self) -> &[V] {
        &self.samples
    }

    pub fn half_width(&self) -> f32 {
        HALF_WIDTH
    }

    /// Lateral distance at which the wall stands.
    pub fn wall(&self) -> f32 {
        HALF_WIDTH + SHOULDER
    }

    fn segment(&self, index: usize) -> (V, V) {
        (self.samples[index], self.samples[(index + 1) % self.samples.len()])
    }

    fn project(&self, pos: V, j: usize) -> (f32, Nearest) {
        let (a, b) = self.segment(j);
        let ab = b - a;
        let len2 = ab.dot(ab).max(1e-6);
        let t = ((pos - a).dot(ab) / len2).clamp(0., 1.);
        let center = a + ab * t;
        let tangent = ab.norm();
        let off = pos - center;
        let off = V(off.0, 0., off.2);
        let lateral = off.dot(V(-tangent.2, 0., tangent.0));
        let s = self.cum[j] + t * (self.cum[j + 1] - self.cum[j]);
        (off.length(), Nearest { index: j, s, lateral, center, tangent })
    }

    /// The closest point on the road near `hint` (the previous answer), a cheap local search.
    pub fn nearest(&self, pos: V, hint: usize) -> Nearest {
        let n = self.samples.len();
        let mut best: Option<(f32, Nearest)> = None;
        for step in 0..=(2 * WINDOW) {
            let j = (hint + n + step - WINDOW) % n;
            let candidate = self.project(pos, j);
            if best.as_ref().map_or(true, |b| candidate.0 < b.0) {
                best = Some(candidate);
            }
        }
        best.map(|b| b.1).expect("the window is never empty")
    }

    /// The closest point on the whole loop (for placing a kart, not for every tick).
    pub fn nearest_global(&self, pos: V) -> Nearest {
        let mut best: Option<(f32, Nearest)> = None;
        for j in 0..self.samples.len() {
            let candidate = self.project(pos, j);
            if best.as_ref().map_or(true, |b| candidate.0 < b.0) {
                best = Some(candidate);
            }
        }
        best.map(|b| b.1).expect("the track has samples")
    }

    fn locate(&self, s: f32) -> (usize, f32) {
        let s = s.rem_euclid(self.length);
        let j = match self.cum.binary_search_by(|c| c.partial_cmp(&s).unwrap_or(std::cmp::Ordering::Equal)) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
        .min(self.samples.len() - 1);
        let span = (self.cum[j + 1] - self.cum[j]).max(1e-6);
        (j, ((s - self.cum[j]) / span).clamp(0., 1.))
    }

    /// The centreline point `s` metres along the loop (wraps).
    pub fn point_at(&self, s: f32) -> V {
        let (j, t) = self.locate(s);
        let (a, b) = self.segment(j);
        a + (b - a) * t
    }

    /// The direction of travel at `s`.
    pub fn tangent_at(&self, s: f32) -> V {
        let (j, _) = self.locate(s);
        let (a, b) = self.segment(j);
        (b - a).norm()
    }

    /// Starting position and heading for grid row `row` (two karts a row, staggered), behind the line.
    pub fn grid_slot(&self, slot: usize) -> (V, f32) {
        let row = slot / 2;
        let side = if slot % 2 == 0 { -1. } else { 1. };
        let s = -(6. + 6. * row as f32) - if side > 0. { 3. } else { 0. };
        let tangent = self.tangent_at(s);
        let right = V(-tangent.2, 0., tangent.0);
        (self.point_at(s) + right * (side * 3.), yaw_of(tangent))
    }

    /// Signed arc distance from `from` to `to`, wrapped to (-length/2, length/2].
    pub fn arc_delta(&self, from: f32, to: f32) -> f32 {
        let mut d = (to - from) % self.length;
        if d > self.length * 0.5 {
            d -= self.length;
        } else if d < -self.length * 0.5 {
            d += self.length;
        }
        d
    }
}
