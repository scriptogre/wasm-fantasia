//! Standalone physics simulation API for use without Bevy.
//!
//! Provides a [`PhysicsWorld`] that drives avian's collision detection and
//! constraint solving pipeline without requiring Bevy's ECS or scheduling.
//!
//! # Example
//!
//! ```rust,no_run
//! use avian3d::prelude::*;
//! use avian3d::standalone::*;
//!
//! let mut world = PhysicsWorld::new(PhysicsConfig {
//!     gravity: Vector::new(0.0, -9.81, 0.0),
//!     substeps: 4,
//!     ..Default::default()
//! });
//!
//! // Static floor
//! let floor = world.add_body(RigidBodyBundle::static_body(Vector::ZERO));
//! world.add_collider(floor, ColliderBundle::half_space(Vector::Y));
//!
//! // Dynamic body
//! let body = world.add_body(RigidBodyBundle::dynamic(Vector::new(0.0, 5.0, 0.0), 1.0));
//! world.add_collider(body, ColliderBundle::sphere(0.5));
//!
//! let _result = world.step(1.0 / 60.0);
//! ```

use crate::collision::collider::Collider;
use crate::collision::collider::contact_query;
use crate::math::*;
use crate::physics_transform::{Position, Rotation};

/// Opaque handle to a rigid body in the [`PhysicsWorld`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyHandle(usize);

/// Configuration for the physics simulation.
#[derive(Clone, Debug)]
pub struct PhysicsConfig {
    /// Gravity vector applied to all dynamic bodies each step.
    pub gravity: Vector,
    /// Number of solver substeps per call to [`PhysicsWorld::step`].
    pub substeps: u32,
    /// Speculative collision margin for contact detection.
    pub speculative_margin: Scalar,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: Vector::new(0.0, -9.81, 0.0),
            substeps: 4,
            speculative_margin: 0.1,
        }
    }
}

/// The type of a rigid body.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RigidBodyType {
    /// Affected by forces and collisions.
    #[default]
    Dynamic,
    /// Immovable.
    Static,
    /// Movable programmatically, not affected by forces.
    Kinematic,
}

/// Bundle of properties for creating a rigid body.
#[derive(Clone, Debug)]
pub struct RigidBodyBundle {
    /// Whether the body is dynamic, static, or kinematic.
    pub body_type: RigidBodyType,
    /// World-space position.
    pub position: Vector,
    /// Rotation quaternion.
    pub rotation: Quaternion,
    /// Linear velocity in meters per second.
    pub linear_velocity: Vector,
    /// Angular velocity in radians per second.
    pub angular_velocity: Vector,
    /// Mass in kilograms.
    pub mass: Scalar,
    /// Gravity scale multiplier.
    pub gravity_scale: Scalar,
    /// Linear damping coefficient.
    pub linear_damping: Scalar,
    /// Angular damping coefficient.
    pub angular_damping: Scalar,
    /// Coefficient of friction.
    pub friction: Scalar,
    /// Coefficient of restitution (bounciness).
    pub restitution: Scalar,
}

impl Default for RigidBodyBundle {
    fn default() -> Self {
        Self {
            body_type: RigidBodyType::Dynamic,
            position: Vector::ZERO,
            rotation: Quaternion::IDENTITY,
            linear_velocity: Vector::ZERO,
            angular_velocity: Vector::ZERO,
            mass: 1.0,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            friction: 0.3,
            restitution: 0.0,
        }
    }
}

impl RigidBodyBundle {
    /// Create a static (immovable) body at the given position.
    pub fn static_body(position: Vector) -> Self {
        Self {
            body_type: RigidBodyType::Static,
            position,
            mass: 0.0,
            ..Default::default()
        }
    }

    /// Create a dynamic body at the given position with the given mass.
    pub fn dynamic(position: Vector, mass: Scalar) -> Self {
        Self {
            body_type: RigidBodyType::Dynamic,
            position,
            mass,
            ..Default::default()
        }
    }
}

/// Bundle of properties for creating a collider.
#[derive(Clone, Debug)]
pub struct ColliderBundle {
    shape: Collider,
}

impl ColliderBundle {
    /// Create a half-space (infinite plane) collider with the given outward normal.
    pub fn half_space(outward_normal: Vector) -> Self {
        Self {
            shape: Collider::half_space(outward_normal),
        }
    }

    /// Create a sphere collider with the given radius.
    pub fn sphere(radius: Scalar) -> Self {
        Self {
            shape: Collider::sphere(radius),
        }
    }

    /// Create a capsule collider with the given radius and total height.
    ///
    /// The `height` is the total height including the hemispherical caps.
    pub fn capsule(radius: Scalar, height: Scalar) -> Self {
        Self {
            shape: Collider::capsule(radius, height),
        }
    }

    /// Create a cuboid (box) collider with the given half-extents.
    pub fn cuboid(half_x: Scalar, half_y: Scalar, half_z: Scalar) -> Self {
        Self {
            shape: Collider::cuboid(half_x, half_y, half_z),
        }
    }
}

/// Read-only view of a rigid body's state.
pub struct RigidBodyRef<'a> {
    body: &'a Body,
}

impl RigidBodyRef<'_> {
    /// World-space position.
    pub fn position(&self) -> Vector {
        self.body.position.0
    }

    /// Linear velocity in meters per second.
    pub fn linear_velocity(&self) -> Vector {
        self.body.linear_velocity
    }

    /// Angular velocity in radians per second.
    pub fn angular_velocity(&self) -> Vector {
        self.body.angular_velocity
    }

    /// Rotation quaternion.
    pub fn rotation(&self) -> Quaternion {
        self.body.rotation.0
    }
}

/// Result of a physics step.
#[derive(Clone, Debug, Default)]
pub struct StepResult {
    /// Number of contact pairs detected.
    pub contact_count: usize,
}

// --- Internal types ---

#[derive(Clone, Debug)]
struct Body {
    body_type: RigidBodyType,
    position: Position,
    rotation: Rotation,
    linear_velocity: Vector,
    angular_velocity: Vector,
    inv_mass: Scalar,
    gravity_scale: Scalar,
    linear_damping: Scalar,
    angular_damping: Scalar,
    friction: Scalar,
    restitution: Scalar,
    /// Pending impulse to apply before the next step.
    pending_impulse: Vector,
    /// Whether this body is still active (false = tombstoned).
    alive: bool,
}

#[derive(Clone, Debug)]
struct ColliderEntry {
    body: BodyHandle,
    shape: Collider,
}

/// Standalone physics world that drives the simulation without Bevy.
pub struct PhysicsWorld {
    config: PhysicsConfig,
    bodies: Vec<Body>,
    colliders: Vec<ColliderEntry>,
}

impl PhysicsWorld {
    /// Create a new physics world with the given configuration.
    pub fn new(config: PhysicsConfig) -> Self {
        Self {
            config,
            bodies: Vec::new(),
            colliders: Vec::new(),
        }
    }

    /// Add a rigid body and return a handle to it.
    pub fn add_body(&mut self, bundle: RigidBodyBundle) -> BodyHandle {
        let handle = BodyHandle(self.bodies.len());
        let inv_mass = if bundle.body_type == RigidBodyType::Dynamic && bundle.mass > 0.0 {
            1.0 / bundle.mass
        } else {
            0.0
        };
        self.bodies.push(Body {
            body_type: bundle.body_type,
            position: Position(bundle.position),
            rotation: Rotation(bundle.rotation),
            linear_velocity: bundle.linear_velocity,
            angular_velocity: bundle.angular_velocity,
            inv_mass,
            gravity_scale: bundle.gravity_scale,
            linear_damping: bundle.linear_damping,
            angular_damping: bundle.angular_damping,
            friction: bundle.friction,
            restitution: bundle.restitution,
            pending_impulse: Vector::ZERO,
            alive: true,
        });
        handle
    }

    /// Attach a collider to the body referenced by `handle`.
    pub fn add_collider(&mut self, handle: BodyHandle, collider: ColliderBundle) {
        self.colliders.push(ColliderEntry {
            body: handle,
            shape: collider.shape,
        });
    }

    /// Mark a body as dead (tombstoned). Dead bodies are skipped by the solver
    /// but their slot is preserved so existing handles remain valid.
    pub fn remove_body(&mut self, handle: BodyHandle) {
        self.bodies[handle.0].alive = false;
    }

    /// Check whether a body is still alive (not tombstoned).
    pub fn is_alive(&self, handle: BodyHandle) -> bool {
        self.bodies[handle.0].alive
    }

    /// Get a read-only reference to a body's state.
    pub fn body(&self, handle: BodyHandle) -> RigidBodyRef<'_> {
        RigidBodyRef {
            body: &self.bodies[handle.0],
        }
    }

    /// Set the linear velocity of a body.
    pub fn set_linear_velocity(&mut self, handle: BodyHandle, velocity: Vector) {
        self.bodies[handle.0].linear_velocity = velocity;
    }

    /// Set the angular velocity of a body.
    pub fn set_angular_velocity(&mut self, handle: BodyHandle, velocity: Vector) {
        self.bodies[handle.0].angular_velocity = velocity;
    }

    /// Apply an instantaneous impulse to a body.
    ///
    /// The impulse is accumulated and applied at the beginning of the next
    /// [`step`](Self::step) call.
    pub fn apply_impulse(&mut self, handle: BodyHandle, impulse: Vector) {
        self.bodies[handle.0].pending_impulse += impulse;
    }

    /// Step the physics simulation forward by `delta_time` seconds.
    ///
    /// Performs velocity integration, collision detection, contact resolution,
    /// and position integration using the configured number of substeps.
    pub fn step(&mut self, delta_time: Scalar) -> StepResult {
        let substeps = self.config.substeps.max(1);
        let sub_dt = delta_time / substeps as Scalar;
        let mut total_contacts = 0;

        // Apply pending impulses before the substep loop.
        for body in &mut self.bodies {
            if !body.alive {
                continue;
            }
            if body.body_type == RigidBodyType::Dynamic && body.inv_mass > 0.0 {
                body.linear_velocity += body.pending_impulse * body.inv_mass;
                body.pending_impulse = Vector::ZERO;
            }
        }

        for _ in 0..substeps {
            // 1. Integrate velocities (apply gravity and damping).
            self.integrate_velocities(sub_dt);

            // 2. Detect and resolve collisions.
            total_contacts += self.solve_contacts(sub_dt);

            // 3. Integrate positions.
            self.integrate_positions(sub_dt);
        }

        StepResult {
            contact_count: total_contacts,
        }
    }

    /// Apply gravity and damping to dynamic body velocities.
    fn integrate_velocities(&mut self, dt: Scalar) {
        let gravity = self.config.gravity;
        for body in &mut self.bodies {
            if !body.alive {
                continue;
            }
            if body.body_type != RigidBodyType::Dynamic {
                continue;
            }

            // Gravity
            body.linear_velocity += gravity * body.gravity_scale * dt;

            // Linear damping: v *= 1 / (1 + dt * c)
            if body.linear_damping > 0.0 {
                body.linear_velocity /= 1.0 + dt * body.linear_damping;
            }

            // Angular damping
            if body.angular_damping > 0.0 {
                body.angular_velocity /= 1.0 + dt * body.angular_damping;
            }
        }
    }

    /// Detect collisions and apply contact impulses. Returns the number of contacts.
    fn solve_contacts(&mut self, _dt: Scalar) -> usize {
        let mut contact_count = 0;
        let num_colliders = self.colliders.len();

        // Iterate over all collider pairs (broad phase: brute-force for simplicity).
        for i in 0..num_colliders {
            for j in (i + 1)..num_colliders {
                let body_a_idx = self.colliders[i].body.0;
                let body_b_idx = self.colliders[j].body.0;

                // Skip if either body is tombstoned.
                if !self.bodies[body_a_idx].alive || !self.bodies[body_b_idx].alive {
                    continue;
                }

                // Only resolve static-vs-dynamic pairs (e.g. floor-vs-enemy).
                // Dynamic-dynamic contacts are skipped because the server uses
                // soft separation (not rigid contacts) for enemy-enemy repulsion,
                // and the impulse chaos from dense groups causes oscillation.
                let a_dynamic = self.bodies[body_a_idx].body_type == RigidBodyType::Dynamic;
                let b_dynamic = self.bodies[body_b_idx].body_type == RigidBodyType::Dynamic;
                if a_dynamic == b_dynamic {
                    continue;
                }

                let pos_a = self.bodies[body_a_idx].position;
                let rot_a = self.bodies[body_a_idx].rotation;
                let pos_b = self.bodies[body_b_idx].position;
                let rot_b = self.bodies[body_b_idx].rotation;

                // Use avian's contact query (wraps Parry).
                let contact = contact_query::contact(
                    &self.colliders[i].shape,
                    pos_a,
                    rot_a,
                    &self.colliders[j].shape,
                    pos_b,
                    rot_b,
                    self.config.speculative_margin,
                );

                let Some(Some(c)) = contact.ok() else {
                    continue;
                };

                // Only resolve penetrating or touching contacts.
                if c.penetration < 0.0 {
                    continue;
                }

                contact_count += 1;

                let normal = c.global_normal1(&rot_a);
                let inv_mass_a = if a_dynamic {
                    self.bodies[body_a_idx].inv_mass
                } else {
                    0.0
                };
                let inv_mass_b = if b_dynamic {
                    self.bodies[body_b_idx].inv_mass
                } else {
                    0.0
                };
                let inv_mass_sum = inv_mass_a + inv_mass_b;

                if inv_mass_sum <= 0.0 {
                    continue;
                }

                // Relative velocity at contact point.
                let vel_a = self.bodies[body_a_idx].linear_velocity;
                let vel_b = self.bodies[body_b_idx].linear_velocity;
                let relative_vel = vel_a - vel_b;
                let normal_speed = relative_vel.dot(normal);

                // Skip impulse if bodies are separating (normal points from A to B,
                // so normal_speed > 0 means A is approaching B).
                if normal_speed < 0.0 {
                    // Bodies are separating; only apply positional correction.
                    self.apply_positional_correction(
                        body_a_idx,
                        body_b_idx,
                        normal,
                        c.penetration,
                        inv_mass_a,
                        inv_mass_b,
                        inv_mass_sum,
                    );
                    continue;
                }

                // Restitution from the pair.
                let restitution = (self.bodies[body_a_idx].restitution
                    + self.bodies[body_b_idx].restitution)
                    * 0.5;

                // Normal impulse magnitude.
                let j_n = -(1.0 + restitution) * normal_speed / inv_mass_sum;
                let impulse = normal * j_n;

                if a_dynamic {
                    self.bodies[body_a_idx].linear_velocity += impulse * inv_mass_a;
                }
                if b_dynamic {
                    self.bodies[body_b_idx].linear_velocity -= impulse * inv_mass_b;
                }

                // Friction impulse.
                let friction =
                    (self.bodies[body_a_idx].friction + self.bodies[body_b_idx].friction) * 0.5;
                if friction > 0.0 {
                    // Recompute relative velocity after normal impulse.
                    let vel_a = self.bodies[body_a_idx].linear_velocity;
                    let vel_b = self.bodies[body_b_idx].linear_velocity;
                    let relative_vel = vel_a - vel_b;
                    let tangent_vel = relative_vel - normal * relative_vel.dot(normal);
                    let tangent_speed = tangent_vel.length();

                    if tangent_speed > 1e-6 {
                        let tangent = tangent_vel / tangent_speed;
                        // Coulomb friction: clamp tangent impulse magnitude.
                        let j_t = (-tangent_speed / inv_mass_sum).max(-j_n * friction);
                        let friction_impulse = tangent * j_t;

                        if a_dynamic {
                            self.bodies[body_a_idx].linear_velocity +=
                                friction_impulse * inv_mass_a;
                        }
                        if b_dynamic {
                            self.bodies[body_b_idx].linear_velocity -=
                                friction_impulse * inv_mass_b;
                        }
                    }
                }

                // Positional correction (Baumgarte stabilization).
                self.apply_positional_correction(
                    body_a_idx,
                    body_b_idx,
                    normal,
                    c.penetration,
                    inv_mass_a,
                    inv_mass_b,
                    inv_mass_sum,
                );
            }
        }

        contact_count
    }

    /// Push bodies apart to resolve penetration.
    #[allow(clippy::too_many_arguments)]
    fn apply_positional_correction(
        &mut self,
        body_a: usize,
        body_b: usize,
        normal: Vector,
        penetration: Scalar,
        inv_mass_a: Scalar,
        inv_mass_b: Scalar,
        inv_mass_sum: Scalar,
    ) {
        // Baumgarte stabilization factor.
        const BAUMGARTE: Scalar = 0.2;
        const SLOP: Scalar = 0.005;

        let correction = (penetration - SLOP).max(0.0) * BAUMGARTE / inv_mass_sum;
        let correction_vec = normal * correction;

        if inv_mass_a > 0.0 {
            self.bodies[body_a].position.0 -= correction_vec * inv_mass_a;
        }
        if inv_mass_b > 0.0 {
            self.bodies[body_b].position.0 += correction_vec * inv_mass_b;
        }
    }

    /// Move bodies according to their velocities.
    fn integrate_positions(&mut self, dt: Scalar) {
        for body in &mut self.bodies {
            if !body.alive {
                continue;
            }
            if body.body_type != RigidBodyType::Dynamic {
                continue;
            }

            body.position.0 += body.linear_velocity * dt;

            // Integrate rotation from angular velocity.
            let ang_vel_mag = body.angular_velocity.length();
            if ang_vel_mag > 1e-8 {
                let half_angle = ang_vel_mag * dt * 0.5;
                let axis = body.angular_velocity / ang_vel_mag;
                let delta = Quaternion::from_axis_angle(axis, half_angle * 2.0);
                body.rotation.0 = (delta * body.rotation.0).normalize();
            }
        }
    }
}
