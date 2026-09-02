//! Translation + rotation + scale, composed to a `Mat4` on demand.

use crate::{Mat4, Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub const fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self { translation, rotation, scale }
    }

    pub const fn from_translation(t: Vec3) -> Self {
        Self { translation: t, ..Self::IDENTITY }
    }

    pub const fn from_rotation(r: Quat) -> Self {
        Self { rotation: r, ..Self::IDENTITY }
    }

    pub const fn from_scale(s: Vec3) -> Self {
        Self { scale: s, ..Self::IDENTITY }
    }

    /// `T * R * S`.
    pub fn to_mat4(&self) -> Mat4 {
        Mat4::from_translation_rotation_scale(self.translation, self.rotation, self.scale)
    }

    #[inline]
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        self.rotation * (p * self.scale) + self.translation
    }

    #[inline]
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        self.rotation * (v * self.scale)
    }

    /// Exact inverse as a matrix. (A TRS with non-uniform scale under
    /// rotation has no TRS inverse, so this is the honest form.)
    pub fn inverse_mat4(&self) -> Mat4 {
        self.to_mat4().inverse().unwrap_or(Mat4::IDENTITY)
    }

    /// `self * child`: `child` expressed in `self`'s space. Exact when scales
    /// are uniform or there is no rotation; otherwise the shear is dropped,
    /// which is the convention every scene graph uses.
    pub fn mul(&self, child: &Transform) -> Transform {
        Transform {
            translation: self.transform_point(child.translation),
            rotation: self.rotation * child.rotation,
            scale: self.scale * child.scale,
        }
    }

    pub fn approx_eq(&self, o: &Transform, eps: f64) -> bool {
        self.translation.approx_eq(o.translation, eps)
            && self.rotation.approx_eq(o.rotation, eps)
            && self.scale.approx_eq(o.scale, eps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{EPS, FRAC_PI_2};

    #[test]
    fn matches_matrix() {
        let t = Transform::new(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_rotation_y(FRAC_PI_2),
            Vec3::new(2.0, 2.0, 2.0),
        );
        let p = Vec3::new(1.0, 0.0, 0.0);
        // Scale to (2,0,0), rotate 90° about Y → (0,0,-2), translate → (1,2,1).
        assert!(t.transform_point(p).approx_eq(Vec3::new(1.0, 2.0, 1.0), EPS));
        assert!(t.to_mat4().transform_point(p).approx_eq(t.transform_point(p), EPS));
        assert!((t.inverse_mat4() * t.to_mat4()).approx_eq(&Mat4::IDENTITY, EPS));
    }

    #[test]
    fn composition_with_uniform_scale_is_exact() {
        let parent = Transform::new(Vec3::new(0.0, 5.0, 0.0), Quat::from_rotation_z(0.6), Vec3::splat(2.0));
        let child = Transform::new(Vec3::new(1.0, 0.0, 0.0), Quat::from_rotation_x(-0.3), Vec3::splat(0.5));
        let composed = parent.mul(&child);
        let m = parent.to_mat4() * child.to_mat4();
        assert!(composed.to_mat4().approx_eq(&m, EPS));
    }
}
