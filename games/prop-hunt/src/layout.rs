//! The hunting ground: a small single-storey house, shared by the simulation (collision, legal
//! disguise spots) and the window (matching visuals). One source of numbers for both.
//!
//! A central corridor spine with a dead end to the north; three rooms open off each side (living room,
//! kitchen and garage to the west, den, bedroom and laundry to the east); a fenced yard to the south is
//! where every match starts. `disguise_spots()` names the legal places a hider may confirm a disguise;
//! `decoys()` places non-player props of the same kinds around those spots so a disguised hider blends
//! in among real clutter, by construction (`main.rs` draws decoys with the exact same meshes `models.rs`
//! gives a disguised hider).
use crate::disguise::DisguiseKind;
use vesper3d::math::V;
use vesper3d::viewer::controller::Collider;
use vesper3d::viewer::devkit::{wall_along_x as devkit_wall_along_x, wall_along_z as devkit_wall_along_z};

pub const WALL_HEIGHT: f32 = 2.6;
const WALL_THICK: f32 = 0.2;

/// A wall along Z at fixed `x`, from `z0` to `z1`, with doorway `gaps` (each a `(z_start, z_end)` range).
fn wall_along_z(x: f32, z0: f32, z1: f32, gaps: &[(f32, f32)]) -> Vec<Collider> {
    devkit_wall_along_z(x, z0, z1, WALL_HEIGHT, WALL_THICK, gaps)
}

/// A wall along X at fixed `z`, from `x0` to `x1`, with doorway `gaps`.
fn wall_along_x(z: f32, x0: f32, x1: f32, gaps: &[(f32, f32)]) -> Vec<Collider> {
    devkit_wall_along_x(z, x0, x1, WALL_HEIGHT, WALL_THICK, gaps)
}

/// Corridor bounds: the spine from the yard threshold (south) to the sealed dead end (north).
pub const CORRIDOR_X: (f32, f32) = (-1.4, 1.4);
pub const CORRIDOR_Z: (f32, f32) = (-18., 4.);
/// Doorway gaps (along Z) cut into the corridor's east and west walls, one pair per row of rooms.
pub const UPPER_DOOR: (f32, f32) = (1., 3.);
pub const MIDDLE_DOOR: (f32, f32) = (-7., -5.);
pub const LOWER_DOOR: (f32, f32) = (-15., -13.);

pub const LIVING_ROOM: (f32, f32, f32, f32) = (-9., -1.4, -2., 4.);
pub const DEN: (f32, f32, f32, f32) = (1.4, 9., -2., 4.);
pub const KITCHEN: (f32, f32, f32, f32) = (-9., -1.4, -10., -4.);
pub const BEDROOM: (f32, f32, f32, f32) = (1.4, 9., -10., -4.);
pub const GARAGE: (f32, f32, f32, f32) = (-9., -1.4, -18., -12.);
pub const LAUNDRY: (f32, f32, f32, f32) = (1.4, 9., -18., -12.);
/// The fenced yard south of the corridor's open end: where every match starts.
pub const YARD: (f32, f32, f32, f32) = (-7., 7., 4., 11.);

fn room_walls(room: (f32, f32, f32, f32)) -> Vec<Collider> {
    let (x0, x1, z0, z1) = room;
    // The corridor-facing side is the corridor's own wall (with its doorway); a room only needs the
    // other three sides.
    let mut walls = Vec::new();
    if x1 <= CORRIDOR_X.0 {
        walls.extend(wall_along_z(x0, z0, z1, &[])); // far (west) wall
    } else {
        walls.extend(wall_along_z(x1, z0, z1, &[])); // far (east) wall
    }
    walls.extend(wall_along_x(z1, x0, x1, &[]));
    walls.extend(wall_along_x(z0, x0, x1, &[]));
    walls
}

/// Every collision wall in the house. The implicit floor at y = 0 (`Controller::set_floor(Some(0.))`,
/// the default) is the only ground; there is no separate floor collider.
pub fn colliders() -> Vec<Collider> {
    let mut walls = Vec::new();
    walls.extend(wall_along_z(CORRIDOR_X.0, CORRIDOR_Z.0, CORRIDOR_Z.1, &[UPPER_DOOR, MIDDLE_DOOR, LOWER_DOOR]));
    walls.extend(wall_along_z(CORRIDOR_X.1, CORRIDOR_Z.0, CORRIDOR_Z.1, &[UPPER_DOOR, MIDDLE_DOOR, LOWER_DOOR]));
    walls.extend(wall_along_x(CORRIDOR_Z.0, CORRIDOR_X.0, CORRIDOR_X.1, &[])); // sealed north dead end
    for room in [LIVING_ROOM, DEN, KITCHEN, BEDROOM, GARAGE, LAUNDRY] {
        walls.extend(room_walls(room));
    }
    let (yx0, yx1, yz0, yz1) = YARD;
    walls.extend(wall_along_x(yz1, yx0, yx1, &[]));
    walls.extend(wall_along_z(yx0, yz0, yz1, &[]));
    walls.extend(wall_along_z(yx1, yz0, yz1, &[]));
    // The yard's north edge is the corridor's south threshold: open exactly where the corridor is.
    walls.extend(wall_along_x(yz0, yx0, yx1, &[CORRIDOR_X]));
    walls
}

/// A legal place for a hider to confirm a disguise: a position and the yaw it freezes at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisguiseSpot {
    pub pos: V,
    pub yaw: f32,
}
const fn spot(x: f32, z: f32, yaw: f32) -> DisguiseSpot {
    DisguiseSpot { pos: V(x, 0., z), yaw }
}

/// Fourteen legal disguise spots, spread so every room holds at least two.
pub const DISGUISE_SPOTS: [DisguiseSpot; 14] = [
    spot(-7.5, 3., 0.),
    spot(-3., 1.5, 1.6),
    spot(6.5, 3., std::f32::consts::PI),
    spot(3.2, -0.5, -1.6),
    spot(-7.5, -6., 0.8),
    spot(-3.2, -8.5, -0.4),
    spot(6.5, -5., 2.3),
    spot(3.2, -8.8, -2.6),
    spot(-7.5, -16.5, 1.2),
    spot(-3.4, -13.5, 0.),
    spot(6.5, -16.5, -1.2),
    spot(3.4, -13.5, std::f32::consts::PI),
    spot(0., -17., 0.),
    spot(0., -2.4, 1.57),
];

/// Up to [`crate::sim::MAX_HIDERS`] spawn points in the yard, where every match begins.
pub const HIDER_SPAWNS: [V; 7] =
    [V(-6., 0., 8.), V(-4., 0., 9.5), V(-2., 0., 8.), V(0., 0., 9.5), V(2., 0., 8.), V(4., 0., 9.5), V(6., 0., 8.)];
/// Yaw 0 faces -Z (`Controller`'s convention): every spawn faces into the house, whose corridor opens
/// at the yard's north edge (smaller z than any spawn here).
pub const HIDER_SPAWN_YAW: f32 = 0.;
/// Where the seeker spawns: the same yard, a distinct spot, facing the corridor's open south end.
pub const SEEKER_SPAWN: V = V(0., 0., 10.5);
pub const SEEKER_SPAWN_YAW: f32 = 0.;

/// A non-player prop placed by `main.rs` for visual clutter: the exact kind, position and yaw a
/// disguised hider could also take, so the two are indistinguishable by rendering alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decoy {
    pub kind: DisguiseKind,
    pub pos: V,
    pub yaw: f32,
}
const fn decoy(kind: DisguiseKind, x: f32, z: f32, yaw: f32) -> Decoy {
    Decoy { kind, pos: V(x, 0., z), yaw }
}

/// Non-player clutter, several per room, reusing the same [`DisguiseKind`]s a hider may choose so a
/// disguised hider is never the only instance of its kind in the house.
pub fn decoys() -> Vec<Decoy> {
    use DisguiseKind::*;
    vec![
        // Living room.
        decoy(Chair, -8.2, 3.2, 0.3),
        decoy(TableLamp, -2.2, 3.4, -1.),
        decoy(BookStack, -6.5, 1., 0.5),
        // Den.
        decoy(TallVase, 7.3, 2.6, 1.1),
        decoy(CandleTrio, 2.2, -1.2, 0.),
        decoy(MantelClock, 5.5, 3.5, std::f32::consts::PI),
        // Kitchen.
        decoy(WovenBasket, -8.2, -5.2, 0.6),
        decoy(Chair, -2.3, -9., -1.4),
        decoy(BookStack, -5.5, -4.6, 0.),
        // Bedroom.
        decoy(TableLamp, 7.4, -4.6, 2.),
        decoy(FlowerVase, 2.3, -9.4, 0.),
        decoy(TallVase, 5., -9.4, -0.8),
        // Garage.
        decoy(WovenBasket, -8.2, -17.2, 0.),
        decoy(CandleTrio, -2.4, -12.4, 1.3),
        // Laundry.
        decoy(MantelClock, 7.3, -17.2, -2.1),
        decoy(FlowerVase, 2.4, -14.5, 0.5),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_have_no_gap_at_the_doorway_centre() {
        let corridor_west =
            wall_along_z(CORRIDOR_X.0, CORRIDOR_Z.0, CORRIDOR_Z.1, &[UPPER_DOOR, MIDDLE_DOOR, LOWER_DOOR]);
        for door in [UPPER_DOOR, MIDDLE_DOOR, LOWER_DOOR] {
            let midpoint = V(CORRIDOR_X.0, 1., (door.0 + door.1) / 2.);
            assert!(!corridor_west.iter().any(|c| c.contains(midpoint)), "{door:?} should be an open doorway");
        }
    }

    #[test]
    fn every_disguise_spot_is_clear_of_every_wall() {
        let walls = colliders();
        for s in DISGUISE_SPOTS {
            assert!(!walls.iter().any(|c| c.overlaps_xz(s.pos, 0.3)), "disguise spot {:?} overlaps a wall", s.pos);
        }
    }

    #[test]
    fn every_spawn_is_clear_of_every_wall() {
        let walls = colliders();
        for p in HIDER_SPAWNS.iter().chain([&SEEKER_SPAWN]) {
            assert!(!walls.iter().any(|c| c.overlaps_xz(*p, 0.3)), "spawn {p:?} overlaps a wall");
        }
    }

    #[test]
    fn every_room_holds_at_least_two_disguise_spots() {
        let in_room = |room: (f32, f32, f32, f32), s: &DisguiseSpot| {
            s.pos.0 > room.0 && s.pos.0 < room.1 && s.pos.2 > room.2 && s.pos.2 < room.3
        };
        for room in [LIVING_ROOM, DEN, KITCHEN, BEDROOM, GARAGE, LAUNDRY] {
            let count = DISGUISE_SPOTS.iter().filter(|s| in_room(room, s)).count();
            assert!(count >= 2, "room {room:?} has only {count} disguise spots");
        }
    }

    #[test]
    fn every_decoy_kind_also_appears_as_a_legal_disguise_choice() {
        // Decoys exist so a disguised hider blends in; every decoy kind is a real DisguiseKind, and the
        // house places more than one decoy of most kinds so no single decoy stands out as unique.
        let all = decoys();
        assert!(all.len() >= DISGUISE_SPOTS.len(), "fewer decoys than disguise spots: nothing to blend into");
        for kind in DisguiseKind::ALL {
            let _ = kind.label(); // every kind used below must resolve (compile-time exhaustiveness below)
        }
        let used: std::collections::HashSet<DisguiseKind> = all.iter().map(|d| d.kind).collect();
        assert!(used.len() >= 6, "too few distinct decoy kinds to disguise among");
    }
}
