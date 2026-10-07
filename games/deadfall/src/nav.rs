//! A walkable graph over the map for the bots: cells of 0.5 m, several floors per cell (ground, catwalk, roof),
//! joined where a player can step (up to the controller's 0.22 m step) or drop (up to 1.5 m).
use crate::level::Level;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use vesper3d::math::V;
use vesper3d::viewer::controller::Collider;
use vesper3d::viewer::devkit::Rng;

pub const CELL: f32 = 0.5;
/// What a player climbs from one cell to the next: a few stair treads fit in a cell, and a jump clears 0.85 m.
const STEP: f32 = 0.22;
const EDGE_STEP: f32 = 0.85;
const DROP: f32 = 1.5;
const CLEARANCE_RADIUS: f32 = 0.32;

#[derive(Clone, Copy, Debug)]
struct Node {
    ix: i32,
    iz: i32,
    y: f32,
}

pub struct Nav {
    min_x: f32,
    min_z: f32,
    nx: i32,
    nz: i32,
    nodes: Vec<Node>,
    /// For every column, the range of `nodes` in it.
    columns: Vec<(u32, u16)>,
    /// Short links over expansion joints, precomputed with body clearance.
    bridges: HashMap<usize, Vec<usize>>,
}

impl Nav {
    pub fn build(level: &Level) -> Nav {
        let min_x = -level.half_x;
        let min_z = -level.half_z;
        let nx = (level.half_x * 2. / CELL).ceil() as i32;
        let nz = (level.half_z * 2. / CELL).ceil() as i32;
        let colliders: Vec<Collider> = level.colliders();
        // Buckets of 4 m so each cell tests a handful of blocks, not all of them.
        let bucket = 4.;
        let bx = (level.half_x * 2. / bucket).ceil() as usize + 2;
        let bz = (level.half_z * 2. / bucket).ceil() as usize + 2;
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); bx * bz];
        for (i, c) in colliders.iter().enumerate() {
            let (x0, x1) = (
                ((c.min.0 - min_x) / bucket).floor().max(0.) as usize,
                ((c.max.0 - min_x) / bucket).floor().max(0.) as usize,
            );
            let (z0, z1) = (
                ((c.min.2 - min_z) / bucket).floor().max(0.) as usize,
                ((c.max.2 - min_z) / bucket).floor().max(0.) as usize,
            );
            for z in z0..=z1.min(bz - 1) {
                for x in x0..=x1.min(bx - 1) {
                    buckets[z * bx + x].push(i as u32);
                }
            }
        }
        let near = |x: f32, z: f32| -> &Vec<u32> {
            let ix = (((x - min_x) / bucket).floor().max(0.) as usize).min(bx - 1);
            let iz = (((z - min_z) / bucket).floor().max(0.) as usize).min(bz - 1);
            &buckets[iz * bx + ix]
        };
        let mut nodes: Vec<Node> = Vec::new();
        let mut columns = vec![(0u32, 0u16); (nx * nz) as usize];
        for iz in 0..nz {
            for ix in 0..nx {
                let (x, z) = (min_x + (ix as f32 + 0.5) * CELL, min_z + (iz as f32 + 0.5) * CELL);
                let start = nodes.len() as u32;
                let here = near(x, z);
                let mut floors: Vec<f32> = Vec::new();
                for &b in here {
                    let c = &colliders[b as usize];
                    if x >= c.min.0 && x <= c.max.0 && z >= c.min.2 && z <= c.max.2 {
                        floors.push(c.max.1);
                    }
                }
                floors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
                floors.dedup_by(|a, b| (*a - *b).abs() < 0.05);
                for y in floors {
                    // Standable: nothing occupies the body there (with some margin) and it is not a tall wall top.
                    let p = V(x, y, z);
                    let blocked = here.iter().any(|&b| {
                        colliders[b as usize].overlaps_body(p, y + STEP + 0.001, 1.8 - STEP, CLEARANCE_RADIUS)
                    });
                    // Neighbouring buckets matter at bucket edges: test the four around too.
                    let blocked = blocked
                        || [
                            (CLEARANCE_RADIUS, 0.),
                            (-CLEARANCE_RADIUS, 0.),
                            (0., CLEARANCE_RADIUS),
                            (0., -CLEARANCE_RADIUS),
                        ]
                        .iter()
                        .any(|(dx, dz)| {
                            near(x + dx, z + dz).iter().any(|&b| {
                                colliders[b as usize].overlaps_body(p, y + STEP + 0.001, 1.8 - STEP, CLEARANCE_RADIUS)
                            })
                        });
                    if !blocked && y < 12. {
                        nodes.push(Node { ix, iz, y });
                    }
                }
                columns[(iz * nx + ix) as usize] = (start, (nodes.len() as u32 - start) as u16);
            }
        }
        let mut nav = Nav { min_x, min_z, nx, nz, nodes, columns, bridges: HashMap::new() };
        for i in 0..nav.nodes.len() {
            let n = nav.nodes[i];
            let mut links = Vec::new();
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let midpoint = V(
                    min_x + (n.ix + dx) as f32 * CELL + CELL * 0.5,
                    n.y,
                    min_z + (n.iz + dz) as f32 * CELL + CELL * 0.5,
                );
                if nav.column(n.ix + dx, n.iz + dz).iter().any(|m| (m.y - n.y).abs() < 0.25) {
                    continue;
                }
                for (k, m) in nav.column(n.ix + 2 * dx, n.iz + 2 * dz).iter().enumerate() {
                    if (m.y - n.y).abs() > 0.25 {
                        continue;
                    }
                    let obstructed = near(midpoint.0, midpoint.2).iter().any(|b| {
                        colliders[*b as usize].overlaps_body(midpoint, n.y + STEP + 0.001, 1.8 - STEP, CLEARANCE_RADIUS)
                    });
                    if !obstructed {
                        links.push(nav.index(n.ix + 2 * dx, n.iz + 2 * dz, k));
                    }
                }
            }
            if !links.is_empty() {
                nav.bridges.insert(i, links);
            }
        }
        nav
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn pos(&self, n: usize) -> V {
        let node = self.nodes[n];
        V(self.min_x + (node.ix as f32 + 0.5) * CELL, node.y, self.min_z + (node.iz as f32 + 0.5) * CELL)
    }

    fn column(&self, ix: i32, iz: i32) -> &[Node] {
        if ix < 0 || iz < 0 || ix >= self.nx || iz >= self.nz {
            return &[];
        }
        let (s, n) = self.columns[(iz * self.nx + ix) as usize];
        &self.nodes[s as usize..s as usize + n as usize]
    }

    fn index(&self, ix: i32, iz: i32, k: usize) -> usize {
        self.columns[(iz * self.nx + ix) as usize].0 as usize + k
    }

    /// The node nearest to a feet position (same floor preferred), if any is close.
    pub fn nearest(&self, feet: V) -> Option<usize> {
        let ix = ((feet.0 - self.min_x) / CELL).floor() as i32;
        let iz = ((feet.2 - self.min_z) / CELL).floor() as i32;
        let mut best: Option<(f32, usize)> = None;
        for r in 0..=3i32 {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dz.abs()) != r {
                        continue;
                    }
                    let (cx, cz) = (ix + dx, iz + dz);
                    for (k, n) in self.column(cx, cz).iter().enumerate() {
                        let dy = (n.y - feet.1).abs();
                        if dy > 1.3 {
                            continue;
                        }
                        let d = (dx * dx + dz * dz) as f32 + dy * 3.;
                        if best.is_none_or(|b| d < b.0) {
                            best = Some((d, self.index(cx, cz, k)));
                        }
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.map(|b| b.1)
    }

    fn neighbours(&self, n: usize, out: &mut Vec<(usize, f32)>) {
        out.clear();
        let node = self.nodes[n];
        if let Some(links) = self.bridges.get(&n) {
            for link in links {
                out.push((*link, CELL * 2.));
            }
        }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (cx, cz) = (node.ix + dx, node.iz + dz);
            let diagonal = dx != 0 && dz != 0;
            for (k, m) in self.column(cx, cz).iter().enumerate() {
                let dy = m.y - node.y;
                if dy > EDGE_STEP || dy < -DROP {
                    continue;
                }
                if diagonal {
                    // Do not cut a corner: both cells beside the move must be walkable at about this height.
                    let side = |sx: i32, sz: i32| {
                        self.column(sx, sz)
                            .iter()
                            .any(|s| (s.y - node.y).abs() <= EDGE_STEP || (s.y < node.y && node.y - s.y <= DROP))
                    };
                    if !side(node.ix + dx, node.iz) || !side(node.ix, node.iz + dz) {
                        continue;
                    }
                }
                let len = if diagonal { 1.414 } else { 1. };
                let cost = len * CELL * (1. + if dy < -0.3 { 0.5 } else { 0. });
                out.push((self.index(cx, cz, k), cost));
            }
        }
    }

    /// A path of positions from `from` to `to` (feet), smoothed a little; `None` when unreachable.
    pub fn path(&self, from: V, to: V) -> Option<Vec<V>> {
        let start = self.nearest(from)?;
        let goal = self.nearest(to)?;
        let ids = self.astar(start, goal)?;
        let mut pts: Vec<V> = ids.iter().map(|&i| self.pos(i)).collect();
        // Drop points on straight runs.
        if pts.len() > 2 {
            let mut keep = vec![pts[0]];
            for w in 1..pts.len() - 1 {
                let a = pts[w] - pts[w - 1];
                let b = pts[w + 1] - pts[w];
                if (a.0 * b.2 - a.2 * b.0).abs() > 1e-3 || (a.1 - b.1).abs() > 0.01 {
                    keep.push(pts[w]);
                }
            }
            keep.push(*pts.last().unwrap_or(&to));
            pts = keep;
        }
        Some(pts)
    }

    fn astar(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        #[derive(PartialEq)]
        struct Item(f32, usize);
        impl Eq for Item {}
        impl PartialOrd for Item {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Item {
            fn cmp(&self, o: &Self) -> Ordering {
                o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
            }
        }
        let goal_pos = self.pos(goal);
        let h = |n: usize| {
            let p = self.pos(n);
            ((p.0 - goal_pos.0).powi(2) + (p.2 - goal_pos.2).powi(2)).sqrt() + (p.1 - goal_pos.1).abs() * 0.5
        };
        let mut g = vec![f32::INFINITY; self.nodes.len()];
        let mut came = vec![u32::MAX; self.nodes.len()];
        let mut open = BinaryHeap::new();
        g[start] = 0.;
        open.push(Item(h(start), start));
        let mut buf = Vec::new();
        let mut expanded = 0;
        while let Some(Item(priority, n)) = open.pop() {
            // A cheaper route may have queued this node again; do not expand the obsolete entry.
            if priority > g[n] + h(n) + 0.001 {
                continue;
            }
            if n == goal {
                let mut path = vec![n];
                let mut c = n;
                while came[c] != u32::MAX {
                    c = came[c] as usize;
                    path.push(c);
                }
                path.reverse();
                return Some(path);
            }
            expanded += 1;
            if expanded > 120_000 {
                return None;
            }
            self.neighbours(n, &mut buf);
            for &(m, cost) in &buf {
                let ng = g[n] + cost;
                if ng < g[m] {
                    g[m] = ng;
                    came[m] = n as u32;
                    open.push(Item(ng + h(m), m));
                }
            }
        }
        None
    }

    /// A random walkable position.
    pub fn random_pos(&self, rng: &mut Rng) -> Option<V> {
        if self.nodes.is_empty() {
            None
        } else {
            Some(self.pos(rng.below(self.nodes.len())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{placeholder, Builder, Material};

    #[test]
    fn a_path_goes_round_an_obstacle() {
        let level = placeholder();
        let nav = Nav::build(&level);
        assert!(nav.len() > 1000);
        let path = nav.path(V(0., 0., 20.), V(0., 0., -20.)).expect("a route exists round the crate");
        assert!(path.len() >= 3, "{path:?}");
        // Every leg of the route is clear of the crate.
        for w in path.windows(2) {
            let (a, b) = (V(w[0].0, 1., w[0].2), V(w[1].0, 1., w[1].2));
            assert!(level.line_of_sight(a, b), "{a:?} -> {b:?}");
        }
    }

    #[test]
    fn stairs_are_climbed_and_a_wall_is_not() {
        let mut b = Builder::new("t", 10., 10.);
        b.solid(-10., -10., 10., 10., -1., 1., Material::Concrete);
        for i in 0..5 {
            b.solid(-2. + i as f32 * 0.4, -1., -1.6 + i as f32 * 0.4, 1., 0., 0.2 * (i + 1) as f32, Material::Concrete);
        }
        b.solid(3., 4., 6., 7., 0., 1.0, Material::Concrete); // a platform too tall to step onto
        let level = b.finish();
        let nav = Nav::build(&level);
        let top = nav.nearest(V(-0.05, 1.0, 0.)).expect("the top stair is a node");
        assert!(nav.pos(top).1 > 0.9);
        assert!(nav.path(V(-4., 0., 0.), V(-0.05, 1.0, 0.)).is_some());
        let platform = nav.nearest(V(4.5, 1.0, 5.5)).expect("the platform top is a node");
        assert!(nav.path(V(-4., 0., 0.), nav.pos(platform)).is_none(), "a 1 m ledge cannot be walked up");
    }
}
