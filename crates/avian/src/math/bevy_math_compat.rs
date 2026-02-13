//! Standalone replacements for `bevy_math` types used by the physics engine.
//!
//! When the `bevy` feature is disabled, these types provide the same API surface
//! as their `bevy_math` counterparts, allowing physics code to compile without Bevy.

use glam::*;

/// A normalized 2D direction vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dir2(Vec2);

impl Dir2 {
    /// +X direction.
    pub const X: Self = Self(Vec2::X);
    /// +Y direction.
    pub const Y: Self = Self(Vec2::Y);
    /// -X direction.
    pub const NEG_X: Self = Self(Vec2::NEG_X);
    /// -Y direction.
    pub const NEG_Y: Self = Self(Vec2::NEG_Y);

    /// Creates a new direction from a vector, returning `None` if zero-length.
    pub fn new(value: Vec2) -> Option<Self> {
        let rcp = value.length_recip();
        if rcp.is_finite() && rcp > 0.0 {
            Some(Self(value * rcp))
        } else {
            None
        }
    }

    /// Creates a new direction without normalization checks.
    pub fn new_unchecked(value: Vec2) -> Self {
        Self(value)
    }

    /// Returns the inner vector.
    pub fn as_vec2(&self) -> Vec2 {
        self.0
    }
}

impl core::ops::Deref for Dir2 {
    type Target = Vec2;
    fn deref(&self) -> &Vec2 {
        &self.0
    }
}

impl core::ops::Neg for Dir2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl From<Dir2> for Vec2 {
    fn from(d: Dir2) -> Vec2 {
        d.0
    }
}

/// A normalized 3D direction vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dir3(Vec3);

impl Dir3 {
    /// +X direction.
    pub const X: Self = Self(Vec3::X);
    /// +Y direction.
    pub const Y: Self = Self(Vec3::Y);
    /// +Z direction.
    pub const Z: Self = Self(Vec3::Z);
    /// -X direction.
    pub const NEG_X: Self = Self(Vec3::NEG_X);
    /// -Y direction.
    pub const NEG_Y: Self = Self(Vec3::NEG_Y);
    /// -Z direction.
    pub const NEG_Z: Self = Self(Vec3::NEG_Z);

    /// Creates a new direction from a vector, returning `None` if zero-length.
    pub fn new(value: Vec3) -> Option<Self> {
        let rcp = value.length_recip();
        if rcp.is_finite() && rcp > 0.0 {
            Some(Self(value * rcp))
        } else {
            None
        }
    }

    /// Creates a new direction without normalization checks.
    pub fn new_unchecked(value: Vec3) -> Self {
        Self(value)
    }

    /// Returns the inner vector.
    pub fn as_vec3(&self) -> Vec3 {
        self.0
    }
}

impl core::ops::Deref for Dir3 {
    type Target = Vec3;
    fn deref(&self) -> &Vec3 {
        &self.0
    }
}

impl core::ops::Neg for Dir3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl From<Dir3> for Vec3 {
    fn from(d: Dir3) -> Vec3 {
        d.0
    }
}

/// A 2D ray with origin and direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray2d {
    /// The origin of the ray.
    pub origin: Vec2,
    /// The direction of the ray.
    pub direction: Dir2,
}

impl Ray2d {
    /// Creates a new ray.
    pub fn new(origin: Vec2, direction: Dir2) -> Self {
        Self { origin, direction }
    }

    /// Returns a point at parameter `t` along the ray.
    pub fn get_point(&self, t: f32) -> Vec2 {
        self.origin + *self.direction * t
    }
}

/// A 3D ray with origin and direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray3d {
    /// The origin of the ray.
    pub origin: Vec3,
    /// The direction of the ray.
    pub direction: Dir3,
}

impl Ray3d {
    /// Creates a new ray.
    pub fn new(origin: Vec3, direction: Dir3) -> Self {
        Self { origin, direction }
    }

    /// Returns a point at parameter `t` along the ray.
    pub fn get_point(&self, t: f32) -> Vec3 {
        self.origin + *self.direction * t
    }
}

/// A 2D rotation represented as sine and cosine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rot2 {
    /// Cosine of the rotation angle.
    pub cos: f32,
    /// Sine of the rotation angle.
    pub sin: f32,
}

impl Rot2 {
    /// The identity rotation.
    pub const IDENTITY: Self = Self { cos: 1.0, sin: 0.0 };

    /// Creates a rotation from radians.
    pub fn radians(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self { cos, sin }
    }

    /// Creates a rotation from sine and cosine values.
    pub fn from_sin_cos(sin: f32, cos: f32) -> Self {
        Self { cos, sin }
    }

    /// Returns the rotation angle in radians.
    pub fn as_radians(&self) -> f32 {
        f32::atan2(self.sin, self.cos)
    }

    /// Rotates a vector.
    pub fn mul_vec2(&self, v: Vec2) -> Vec2 {
        Vec2::new(
            self.cos * v.x - self.sin * v.y,
            self.sin * v.x + self.cos * v.y,
        )
    }
}

impl Default for Rot2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl core::ops::Mul<Vec2> for Rot2 {
    type Output = Vec2;
    fn mul(self, rhs: Vec2) -> Vec2 {
        self.mul_vec2(rhs)
    }
}

/// A 2D isometry (translation + rotation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Isometry2d {
    /// The rotation component.
    pub rotation: Rot2,
    /// The translation component.
    pub translation: Vec2,
}

impl Isometry2d {
    /// The identity isometry.
    pub const IDENTITY: Self = Self {
        rotation: Rot2::IDENTITY,
        translation: Vec2::ZERO,
    };

    /// Creates a new isometry.
    pub fn new(translation: Vec2, rotation: Rot2) -> Self {
        Self {
            rotation,
            translation,
        }
    }

    /// Returns the inverse of this isometry.
    pub fn inverse(&self) -> Self {
        let inv_rot = Rot2::from_sin_cos(-self.rotation.sin, self.rotation.cos);
        Self {
            rotation: inv_rot,
            translation: inv_rot.mul_vec2(-self.translation),
        }
    }

    /// Transforms a point.
    pub fn transform_point(&self, point: Vec2) -> Vec2 {
        self.rotation.mul_vec2(point) + self.translation
    }
}

impl Default for Isometry2d {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A 3D isometry (translation + rotation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Isometry3d {
    /// The rotation component.
    pub rotation: Quat,
    /// The translation component.
    pub translation: Vec3,
}

impl Isometry3d {
    /// The identity isometry.
    pub const IDENTITY: Self = Self {
        rotation: Quat::IDENTITY,
        translation: Vec3::ZERO,
    };

    /// Creates a new isometry from translation and rotation.
    pub fn new(translation: Vec3, rotation: Quat) -> Self {
        Self {
            rotation,
            translation,
        }
    }

    /// Creates an isometry from a rotation only.
    pub fn from_rotation(rotation: Quat) -> Self {
        Self {
            rotation,
            translation: Vec3::ZERO,
        }
    }

    /// Creates an isometry from a translation only.
    pub fn from_translation(translation: Vec3) -> Self {
        Self {
            rotation: Quat::IDENTITY,
            translation,
        }
    }

    /// Returns the inverse of this isometry.
    pub fn inverse(&self) -> Self {
        let inv_rot = self.rotation.inverse();
        Self {
            rotation: inv_rot,
            translation: inv_rot.mul_vec3(-self.translation),
        }
    }

    /// Transforms a point.
    pub fn transform_point(&self, point: Vec3) -> Vec3 {
        self.rotation.mul_vec3(point) + self.translation
    }
}

impl Default for Isometry3d {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl core::ops::Mul for Isometry3d {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            rotation: self.rotation * rhs.rotation,
            translation: self.rotation.mul_vec3(rhs.translation) + self.translation,
        }
    }
}

impl core::ops::Mul<Vec3> for Isometry3d {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Vec3 {
        self.transform_point(rhs)
    }
}

impl core::ops::Mul<Dir3> for Isometry3d {
    type Output = Dir3;
    fn mul(self, rhs: Dir3) -> Dir3 {
        Dir3::new_unchecked(self.rotation.mul_vec3(rhs.0))
    }
}
