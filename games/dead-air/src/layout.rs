//! Hollow Pine Relay Station: the building's static shape, shared by the simulation (collision,
//! waypoint pathing) and the window (matching visuals). One source of numbers for both.
//!
//! The wall-with-doorways builder and the waypoint graph are no longer hand-rolled here: they proved
//! general enough (zero station-specific content) to promote into the engine itself
//! (`devkit::{wall_along_x, wall_along_z, WaypointGraph}`, BlueEngine ADR 0034) after this game
//! motivated them. Only this station's own numbers — room coordinates, the 12-node graph, the patrol
//! route — stay here.
use std::sync::OnceLock;
use vesper3d::math::V;
use vesper3d::viewer::controller::Collider;
use vesper3d::viewer::devkit::{
    wall_along_x as devkit_wall_along_x, wall_along_z as devkit_wall_along_z, Waypoint, WaypointGraph,
};

pub const WALL_HEIGHT: f32 = 2.6;
const WALL_THICK: f32 = 0.2;

/// A wall along Z at fixed `x`, from `z0` to `z1`, with doorway `gaps` (each a `(z_start, z_end)` range).
pub fn wall_along_z(x: f32, z0: f32, z1: f32, gaps: &[(f32, f32)]) -> Vec<Collider> {
    devkit_wall_along_z(x, z0, z1, WALL_HEIGHT, WALL_THICK, gaps)
}

/// A wall along X at fixed `z`, from `x0` to `x1`, with doorway `gaps`.
pub fn wall_along_x(z: f32, x0: f32, x1: f32, gaps: &[(f32, f32)]) -> Vec<Collider> {
    devkit_wall_along_x(z, x0, x1, WALL_HEIGHT, WALL_THICK, gaps)
}

/// Corridor bounds: a straight hall from the yard threshold (south) to the sealed hatch (north).
pub const CORRIDOR_X: (f32, f32) = (-1.2, 1.2);
pub const CORRIDOR_Z: (f32, f32) = (-16., 2.);
/// Doorway gaps (along Z) cut into the corridor's east (Control/Dormitory) and west (Archive/Generator)
/// walls.
pub const CONTROL_DOOR: (f32, f32) = (0., 2.);
pub const ARCHIVE_DOOR: (f32, f32) = (0., 2.);
pub const DORMITORY_DOOR: (f32, f32) = (-9., -7.);
pub const GENERATOR_DOOR: (f32, f32) = (-9., -7.);

pub const CONTROL_ROOM: (f32, f32, f32, f32) = (1.2, 6., -1., 3.);
pub const ARCHIVE_ROOM: (f32, f32, f32, f32) = (-6., -1.2, -1., 3.);
pub const DORMITORY: (f32, f32, f32, f32) = (1.2, 6., -10., -6.);
pub const GENERATOR_ROOM: (f32, f32, f32, f32) = (-6., -1.2, -10., -6.);
pub const YARD: (f32, f32, f32, f32) = (-7., 7., 2., 9.);

/// Every collision wall in the station. The implicit floor at y = 0 (`Controller::set_floor(Some(0.))`,
/// the default) is the only ground; there is no separate floor collider.
pub fn colliders() -> Vec<Collider> {
    let mut walls = Vec::new();
    walls.extend(wall_along_z(CORRIDOR_X.0, CORRIDOR_Z.0, CORRIDOR_Z.1, &[ARCHIVE_DOOR, GENERATOR_DOOR]));
    walls.extend(wall_along_z(CORRIDOR_X.1, CORRIDOR_Z.0, CORRIDOR_Z.1, &[CONTROL_DOOR, DORMITORY_DOOR]));
    walls.extend(wall_along_x(CORRIDOR_Z.0, CORRIDOR_X.0, CORRIDOR_X.1, &[]));

    let (x0, x1, z0, z1) = CONTROL_ROOM;
    walls.extend(wall_along_z(x1, z0, z1, &[]));
    walls.extend(wall_along_x(z1, x0, x1, &[]));
    walls.extend(wall_along_x(z0, x0, x1, &[]));

    let (x0, x1, z0, z1) = ARCHIVE_ROOM;
    walls.extend(wall_along_z(x0, z0, z1, &[]));
    walls.extend(wall_along_x(z1, x0, x1, &[]));
    walls.extend(wall_along_x(z0, x0, x1, &[]));

    let (x0, x1, z0, z1) = DORMITORY;
    walls.extend(wall_along_z(x1, z0, z1, &[]));
    walls.extend(wall_along_x(z1, x0, x1, &[]));
    walls.extend(wall_along_x(z0, x0, x1, &[]));

    let (x0, x1, z0, z1) = GENERATOR_ROOM;
    walls.extend(wall_along_z(x0, z0, z1, &[]));
    walls.extend(wall_along_x(z1, x0, x1, &[]));
    walls.extend(wall_along_x(z0, x0, x1, &[]));

    let (x0, x1, z0, z1) = YARD;
    walls.extend(wall_along_x(z1, x0, x1, &[]));
    walls.extend(wall_along_z(x0, z0, z1, &[]));
    walls.extend(wall_along_z(x1, z0, z1, &[]));
    walls
}

/// A named point of interest: pick up, interact, or read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub pos: V,
    pub radius: f32,
}
impl Spot {
    const fn at(x: f32, z: f32, radius: f32) -> Self {
        Self { pos: V(x, 0.9, z), radius }
    }
    pub fn contains(&self, feet: V) -> bool {
        let d = V(self.pos.0 - feet.0, 0., self.pos.2 - feet.2);
        d.length() < self.radius
    }
}

pub const PLAYER_SPAWN: V = V(0., 0., 6.);
/// Yaw 0 faces -Z (`Controller`'s convention): the player spawns in the yard facing into the station.
pub const PLAYER_SPAWN_YAW: f32 = 0.;

pub const RADIO_DESK: Spot = Spot::at(4.5, 2.2, 1.3);
pub const GENERATOR: Spot = Spot::at(-4.5, -8.5, 1.3);

pub const FUEL_CANS: [Spot; 2] = [Spot::at(-5.2, 0.5, 0.9), Spot::at(-5.3, -6.8, 0.9)];
pub const BATTERIES: [Spot; 2] = [Spot::at(4.7, -9.3, 0.9), Spot::at(-3.2, -9.4, 0.9)];
pub const AMMO: Spot = Spot::at(2.2, -9.3, 0.9);

/// Where each lore note lives, in reading order (not pickup order: the station does not stop a player
/// reading them out of sequence).
pub const NOTES: [Spot; 5] = [
    Spot::at(-5.3, 2.2, 0.9),
    Spot::at(-2.3, 1.2, 0.9),
    Spot::at(-5.3, 0.3, 0.9),
    Spot::at(5.2, -8.6, 0.9),
    Spot::at(5.5, 1.2, 0.9),
];

/// The station's walkable graph: doorway thresholds and room centres, tree-shaped (no loops), matching
/// `colliders()`. Index order matches the edge lists below. Built once; `graph()` is the only accessor.
fn build_graph() -> WaypointGraph {
    WaypointGraph::new(vec![
        Waypoint { pos: V(0., 0., 5.5), edges: vec![1] }, // 0: yard centre
        Waypoint { pos: V(0., 0., 1.), edges: vec![0, 2, 4, 6] }, // 1: corridor south junction
        Waypoint { pos: V(1.2, 0., 1.), edges: vec![1, 3] }, // 2: control doorway
        Waypoint { pos: V(3.5, 0., 1.), edges: vec![2] }, // 3: control centre
        Waypoint { pos: V(-1.2, 0., 1.), edges: vec![1, 5] }, // 4: archive doorway
        Waypoint { pos: V(-3.5, 0., 1.), edges: vec![4] }, // 5: archive centre
        Waypoint { pos: V(0., 0., -4.), edges: vec![1, 7, 9, 11] }, // 6: corridor mid junction
        Waypoint { pos: V(1.2, 0., -8.), edges: vec![6, 8] }, // 7: dormitory doorway
        Waypoint { pos: V(3.5, 0., -8.), edges: vec![7] }, // 8: dormitory centre
        Waypoint { pos: V(-1.2, 0., -8.), edges: vec![6, 10] }, // 9: generator doorway
        Waypoint { pos: V(-3.5, 0., -8.), edges: vec![9] }, // 10: generator centre
        Waypoint { pos: V(0., 0., -14.), edges: vec![6] }, // 11: sealed hatch (dead end)
    ])
}

static GRAPH: OnceLock<WaypointGraph> = OnceLock::new();
/// The station's waypoint graph (built once on first use).
pub fn graph() -> &'static WaypointGraph {
    GRAPH.get_or_init(build_graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Doorway-gap cutting itself is tested in devkit::level (BlueEngine). These tests stay narrow:
    // does this station's own data produce the walls/graph it claims to.

    #[test]
    fn walls_have_no_gap_at_the_doorway_centre() {
        let corridor_east = wall_along_z(CORRIDOR_X.1, CORRIDOR_Z.0, CORRIDOR_Z.1, &[CONTROL_DOOR, DORMITORY_DOOR]);
        let midpoint = V(CORRIDOR_X.1, 1., (CONTROL_DOOR.0 + CONTROL_DOOR.1) / 2.);
        assert!(!corridor_east.iter().any(|c| c.contains(midpoint)), "the control-room doorway must be open");
    }

    #[test]
    fn every_waypoint_reaches_every_other_waypoint() {
        let g = graph();
        for from in 0..g.len() {
            for to in 0..g.len() {
                let route = g.path(from, to);
                assert!(!route.is_empty(), "{from} -> {to}");
                assert_eq!(route[0], from);
                assert_eq!(*route.last().unwrap(), to);
            }
        }
    }

    #[test]
    fn nearest_waypoint_finds_the_closest_node() {
        assert_eq!(graph().nearest(V(3.4, 0., 1.1)), 3, "near the control-room centre");
        assert_eq!(graph().nearest(V(0., 0., -13.8)), 11, "near the sealed hatch");
    }

    #[test]
    fn key_spots_sit_inside_their_own_room() {
        let (x0, x1, z0, z1) = GENERATOR_ROOM;
        assert!(GENERATOR.pos.0 > x0 && GENERATOR.pos.0 < x1 && GENERATOR.pos.2 > z0 && GENERATOR.pos.2 < z1);
        let (x0, x1, z0, z1) = CONTROL_ROOM;
        assert!(RADIO_DESK.pos.0 > x0 && RADIO_DESK.pos.0 < x1 && RADIO_DESK.pos.2 > z0 && RADIO_DESK.pos.2 < z1);
    }
}
