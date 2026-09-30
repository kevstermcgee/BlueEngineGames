//! A small rigid-body layer over the engine's `rapier` (re-exported as `vesper3d::rapier`): fixed-step, deterministic,
//! bodies addressed by a stable slot number, builders for the handful of shapes a mini game needs, and pose
//! capture for save states and state hashes. It is graphics-free and has no wall clock, so a game's rules stay a pure
//! function of (seed, inputs).
use vesper3d::math::V;
use vesper3d::rapier::na::{Quaternion, UnitQuaternion};
use vesper3d::rapier::prelude::*;
use vesper3d::viewer::devkit::{StateHasher, TICK};

/// How a body feels. `restitution` is bounciness (0 dead, 1 perfect), `density` is kg per cubic metre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub friction: f32,
    pub restitution: f32,
    pub density: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// Continuous collision detection: needed for anything fast and small, or it tunnels through walls.
    pub ccd: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self { friction: 0.6, restitution: 0.1, density: 1., linear_damping: 0., angular_damping: 0., ccd: false }
    }
}

/// A body's stable slot. Slots are never reused, so a `Body` stays valid (or is dead) for the world's life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Body(pub usize);

/// Everything a save needs to put a body back: pose and velocities.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BodyState {
    pub alive: bool,
    pub pos: V,
    /// Orientation as x, y, z, w.
    pub rot: [f32; 4],
    pub linvel: V,
    pub angvel: V,
    pub sleeping: bool,
}

pub struct Physics {
    gravity: Vector<Real>,
    params: IntegrationParameters,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad: BroadPhaseMultiSap,
    narrow: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    joints: ImpulseJointSet,
    multi: MultibodyJointSet,
    ccd: CCDSolver,
    slots: Vec<Option<RigidBodyHandle>>,
}

fn vector(v: V) -> Vector<Real> {
    Vector::new(v.0, v.1, v.2)
}
fn value(v: &Vector<Real>) -> V {
    V(v.x, v.y, v.z)
}

impl Physics {
    /// An empty world with gravity `gravity` (m/s^2), stepping at the engine's fixed tick.
    pub fn new(gravity: V) -> Self {
        let mut params = IntegrationParameters { dt: TICK, ..Default::default() };
        // Stacking and small fast balls want more solver work than the default.
        params.num_solver_iterations = std::num::NonZeroUsize::new(8).expect("8 is not zero");
        Self {
            gravity: vector(gravity),
            params,
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: BroadPhaseMultiSap::new(),
            narrow: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            joints: ImpulseJointSet::new(),
            multi: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            slots: Vec::new(),
        }
    }

    pub fn gravity(&self) -> V {
        value(&self.gravity)
    }

    pub fn set_gravity(&mut self, gravity: V) {
        self.gravity = vector(gravity);
    }

    /// Advance exactly one fixed tick.
    pub fn step(&mut self) {
        self.pipeline.step(
            &self.gravity,
            &self.params,
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.joints,
            &mut self.multi,
            &mut self.ccd,
            None,
            &(),
            &(),
        );
    }

    fn insert(&mut self, builder: RigidBodyBuilder, shape: ColliderBuilder, m: Material) -> Body {
        let handle = self.bodies.insert(
            builder.linear_damping(m.linear_damping).angular_damping(m.angular_damping).ccd_enabled(m.ccd).build(),
        );
        self.colliders.insert_with_parent(
            shape.friction(m.friction).restitution(m.restitution).density(m.density).build(),
            handle,
            &mut self.bodies,
        );
        self.slots.push(Some(handle));
        Body(self.slots.len() - 1)
    }

    /// An immovable box (walls, floors, posts). `yaw` turns it about +Y.
    pub fn fixed_box(&mut self, center: V, half: V, yaw: f32, m: Material) -> Body {
        let b = RigidBodyBuilder::fixed().translation(vector(center)).rotation(Vector::y() * yaw);
        self.insert(b, ColliderBuilder::cuboid(half.0, half.1, half.2), m)
    }

    /// An immovable upright cylinder (bumpers, pegs).
    pub fn fixed_cylinder(&mut self, center: V, radius: f32, half_height: f32, m: Material) -> Body {
        let b = RigidBodyBuilder::fixed().translation(vector(center));
        self.insert(b, ColliderBuilder::cylinder(half_height, radius), m)
    }

    pub fn dynamic_box(&mut self, center: V, half: V, yaw: f32, m: Material) -> Body {
        let b = RigidBodyBuilder::dynamic().translation(vector(center)).rotation(Vector::y() * yaw);
        self.insert(b, ColliderBuilder::cuboid(half.0, half.1, half.2), m)
    }

    /// A dynamic box confined to the X-Y plane: it moves in x and y and turns only about z (`angle`), so a stack of
    /// them behaves like a 2D stack while the game is still drawn in 3D.
    pub fn dynamic_box_planar(&mut self, center: V, half: V, angle: f32, m: Material) -> Body {
        let b = RigidBodyBuilder::dynamic()
            .translation(vector(center))
            .rotation(Vector::z() * angle)
            .enabled_translations(true, true, false)
            .enabled_rotations(false, false, true);
        self.insert(b, ColliderBuilder::cuboid(half.0, half.1, half.2), m)
    }

    pub fn dynamic_ball(&mut self, center: V, radius: f32, m: Material) -> Body {
        let b = RigidBodyBuilder::dynamic().translation(vector(center));
        self.insert(b, ColliderBuilder::ball(radius), m)
    }

    /// A box the game moves by hand each tick with [`Physics::move_kinematic`]; it pushes dynamic bodies with the
    /// velocity implied by its motion (flippers, sliding bars, the crane).
    pub fn kinematic_box(&mut self, center: V, half: V, yaw: f32, m: Material) -> Body {
        let b = RigidBodyBuilder::kinematic_position_based().translation(vector(center)).rotation(Vector::y() * yaw);
        self.insert(b, ColliderBuilder::cuboid(half.0, half.1, half.2), m)
    }

    /// Remove a body for good (its slot stays dead).
    pub fn remove(&mut self, body: Body) {
        if let Some(handle) = self.slots.get_mut(body.0).and_then(Option::take) {
            self.bodies.remove(handle, &mut self.islands, &mut self.colliders, &mut self.joints, &mut self.multi, true);
        }
    }

    pub fn alive(&self, body: Body) -> bool {
        self.slots.get(body.0).is_some_and(Option::is_some)
    }

    fn get(&self, body: Body) -> Option<&RigidBody> {
        self.bodies.get((*self.slots.get(body.0)?)?)
    }

    fn get_mut(&mut self, body: Body) -> Option<&mut RigidBody> {
        let handle = (*self.slots.get(body.0)?)?;
        self.bodies.get_mut(handle)
    }

    /// Bodies ever created (alive or not): the length of a [`Physics::states`] list.
    pub fn slots(&self) -> usize {
        self.slots.len()
    }

    pub fn position(&self, body: Body) -> V {
        self.get(body).map_or(V(0., 0., 0.), |b| value(b.translation()))
    }

    /// Orientation as x, y, z, w.
    pub fn rotation(&self, body: Body) -> [f32; 4] {
        self.get(body).map_or([0., 0., 0., 1.], |b| {
            let q = b.rotation().quaternion();
            [q.i, q.j, q.k, q.w]
        })
    }

    /// The turn about +Z, -PI..PI (for bodies built with [`Physics::dynamic_box_planar`]).
    pub fn roll(&self, body: Body) -> f32 {
        let [x, y, z, w] = self.rotation(body);
        (2. * (w * z + x * y)).atan2(1. - 2. * (z * z + y * y))
    }

    /// The turn about +Y, -PI..PI (enough for things that only spin flat).
    pub fn yaw(&self, body: Body) -> f32 {
        let [x, y, z, w] = self.rotation(body);
        (2. * (w * y + x * z)).atan2(1. - 2. * (y * y + x * x))
    }

    pub fn linvel(&self, body: Body) -> V {
        self.get(body).map_or(V(0., 0., 0.), |b| value(b.linvel()))
    }

    pub fn angvel(&self, body: Body) -> V {
        self.get(body).map_or(V(0., 0., 0.), |b| value(b.angvel()))
    }

    pub fn speed(&self, body: Body) -> f32 {
        self.linvel(body).length()
    }

    pub fn mass(&self, body: Body) -> f32 {
        self.get(body).map_or(0., |b| b.mass())
    }

    pub fn is_sleeping(&self, body: Body) -> bool {
        self.get(body).is_some_and(|b| b.is_sleeping())
    }

    pub fn set_linvel(&mut self, body: Body, v: V) {
        if let Some(b) = self.get_mut(body) {
            b.set_linvel(vector(v), true);
        }
    }

    pub fn set_angvel(&mut self, body: Body, v: V) {
        if let Some(b) = self.get_mut(body) {
            b.set_angvel(vector(v), true);
        }
    }

    /// Teleport (keeps orientation and velocity).
    pub fn set_position(&mut self, body: Body, pos: V) {
        if let Some(b) = self.get_mut(body) {
            b.set_translation(vector(pos), true);
        }
    }

    /// An instant change of momentum, in newton-seconds.
    pub fn impulse(&mut self, body: Body, impulse: V) {
        if let Some(b) = self.get_mut(body) {
            b.apply_impulse(vector(impulse), true);
        }
    }

    /// A steady force for this tick, in newtons (call it every tick it should act; wind, magnets, thrusters).
    pub fn push(&mut self, body: Body, force: V) {
        self.impulse(body, force * TICK);
    }

    /// Where a kinematic body should be at the end of the next step (position and a turn about +Y).
    pub fn move_kinematic(&mut self, body: Body, pos: V, yaw: f32) {
        if let Some(b) = self.get_mut(body) {
            b.set_next_kinematic_position(Isometry::new(vector(pos), Vector::y() * yaw));
        }
    }

    /// The bodies `body` is touching right now (contact manifolds with points), nearest slot first.
    pub fn touching(&self, body: Body) -> Vec<Body> {
        let Some(handle) = self.slots.get(body.0).copied().flatten() else { return Vec::new() };
        let Some(rb) = self.bodies.get(handle) else { return Vec::new() };
        let mut out = Vec::new();
        for &collider in rb.colliders() {
            for pair in self.narrow.contact_pairs_with(collider) {
                if !pair.has_any_active_contact {
                    continue;
                }
                let other = if pair.collider1 == collider { pair.collider2 } else { pair.collider1 };
                let Some(parent) = self.colliders.get(other).and_then(|c| c.parent()) else { continue };
                if let Some(slot) = self.slots.iter().position(|s| *s == Some(parent)) {
                    if !out.contains(&Body(slot)) {
                        out.push(Body(slot));
                    }
                }
            }
        }
        out.sort_by_key(|b| b.0);
        out
    }

    /// Every slot's pose and velocities, in slot order.
    pub fn states(&self) -> Vec<BodyState> {
        (0..self.slots.len())
            .map(|i| match self.get(Body(i)) {
                Some(b) => {
                    let q = b.rotation().quaternion();
                    BodyState {
                        alive: true,
                        pos: value(b.translation()),
                        rot: [q.i, q.j, q.k, q.w],
                        // A kinematic body's velocity is derived from how the game moves it, so it is not state.
                        linvel: if b.is_kinematic() { V(0., 0., 0.) } else { value(b.linvel()) },
                        angvel: if b.is_kinematic() { V(0., 0., 0.) } else { value(b.angvel()) },
                        sleeping: b.is_sleeping(),
                    }
                }
                None => BodyState {
                    alive: false,
                    pos: V(0., 0., 0.),
                    rot: [0., 0., 0., 1.],
                    linvel: V(0., 0., 0.),
                    angvel: V(0., 0., 0.),
                    sleeping: false,
                },
            })
            .collect()
    }

    /// Put the bodies back as `states` says. The world must already contain the same bodies in the same slots
    /// (rebuild them the way the game built them, then restore); returns false, changing nothing, if not.
    pub fn restore(&mut self, states: &[BodyState]) -> bool {
        if states.len() != self.slots.len() || states.iter().enumerate().any(|(i, s)| s.alive != self.alive(Body(i))) {
            return false;
        }
        let finite = |v: V| v.0.is_finite() && v.1.is_finite() && v.2.is_finite();
        if states.iter().any(|s| {
            s.alive
                && (!finite(s.pos) || !finite(s.linvel) || !finite(s.angvel) || s.rot.iter().any(|x| !x.is_finite()))
        }) {
            return false;
        }
        for (i, s) in states.iter().enumerate() {
            let Some(b) = self.get_mut(Body(i)) else { continue };
            // Not re-normalised: a saved unit quaternion must come back bit for bit.
            let q = UnitQuaternion::new_unchecked(Quaternion::new(s.rot[3], s.rot[0], s.rot[1], s.rot[2]));
            b.set_position(Isometry::from_parts(Translation::from(vector(s.pos)), q), true);
            b.set_linvel(vector(s.linvel), true);
            b.set_angvel(vector(s.angvel), true);
            if s.sleeping {
                b.sleep();
            } else {
                b.wake_up(true);
            }
        }
        true
    }

    /// Feed every body's pose and velocity to a state hash.
    pub fn hash_into(&self, h: &mut StateHasher) {
        for s in self.states() {
            h.bool(s.alive);
            if s.alive {
                h.f32(s.pos.0).f32(s.pos.1).f32(s.pos.2);
                h.f32(s.rot[0]).f32(s.rot[1]).f32(s.rot[2]).f32(s.rot[3]);
                h.f32(s.linvel.0).f32(s.linvel.1).f32(s.linvel.2);
                h.f32(s.angvel.0).f32(s.angvel.1).f32(s.angvel.2);
            }
        }
    }
}
