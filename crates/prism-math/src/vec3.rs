//! 3D vector. Right-handed, +Y up.

use crate::macros::impl_vec_ops;
use crate::{Vec2, Vec4};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl_vec_ops!(Vec3 { x, y, z });

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const ONE: Vec3 = Vec3::new(1.0, 1.0, 1.0);
    pub const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    pub const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    pub const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);
    pub const NEG_X: Vec3 = Vec3::new(-1.0, 0.0, 0.0);
    pub const NEG_Y: Vec3 = Vec3::new(0.0, -1.0, 0.0);
    pub const NEG_Z: Vec3 = Vec3::new(0.0, 0.0, -1.0);
    /// World up.
    pub const UP: Vec3 = Vec3::Y;
    /// The direction a default camera looks along in view space.
    pub const FORWARD: Vec3 = Vec3::NEG_Z;

    #[inline]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Unsigned angle between two vectors in radians, `[0, PI]`.
    pub fn angle_between(self, o: Vec3) -> f64 {
        let d = self.dot(o) / (self.length() * o.length());
        d.clamp(-1.0, 1.0).acos()
    }

    /// Some unit vector perpendicular to `self` (which must be non-zero).
    /// Picks the axis `self` is least aligned with, so it is stable.
    pub fn any_orthonormal(self) -> Vec3 {
        let a = self.abs();
        let axis = if a.x <= a.y && a.x <= a.z {
            Vec3::X
        } else if a.y <= a.z {
            Vec3::Y
        } else {
            Vec3::Z
        };
        self.cross(axis).normalize()
    }

    /// An orthonormal basis `(u, v)` spanning the plane perpendicular to
    /// unit `self`, with `u × v = self`.
    pub fn orthonormal_basis(self) -> (Vec3, Vec3) {
        let u = self.any_orthonormal();
        let v = self.cross(u);
        (u, v)
    }

    #[inline]
    pub fn truncate(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    #[inline]
    pub fn xz(self) -> Vec2 {
        Vec2::new(self.x, self.z)
    }

    #[inline]
    pub fn extend(self, w: f64) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, w)
    }

    #[inline]
    pub fn to_gpu(self) -> [f32; 3] {
        [self.x as f32, self.y as f32, self.z as f32]
    }

    #[inline]
    pub fn from_gpu(v: [f32; 3]) -> Vec3 {
        Vec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
    }

    #[inline]
    pub fn to_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
}

impl From<[f64; 3]> for Vec3 {
    fn from(a: [f64; 3]) -> Self {
        Vec3::new(a[0], a[1], a[2])
    }
}

impl From<(f64, f64, f64)> for Vec3 {
    fn from((x, y, z): (f64, f64, f64)) -> Self {
        Vec3::new(x, y, z)
    }
}

impl core::ops::Index<usize> for Vec3 {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        match i {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            _ => panic!("Vec3 index {i} out of range"),
        }
    }
}

impl core::ops::IndexMut<usize> for Vec3 {
    fn index_mut(&mut self, i: usize) -> &mut f64 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            _ => panic!("Vec3 index {i} out of range"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{EPS, FRAC_PI_2, PI};

    #[test]
    fn right_handed_cross() {
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
        assert_eq!(Vec3::Y.cross(Vec3::Z), Vec3::X);
        assert_eq!(Vec3::Z.cross(Vec3::X), Vec3::Y);
        assert_eq!(Vec3::Y.cross(Vec3::X), Vec3::NEG_Z);
    }

    #[test]
    fn lengths_and_angles() {
        assert_eq!(Vec3::new(2.0, 3.0, 6.0).length(), 7.0);
        assert!(crate::approx_eq(Vec3::X.angle_between(Vec3::Y), FRAC_PI_2, EPS));
        assert!(crate::approx_eq(Vec3::X.angle_between(Vec3::NEG_X), PI, EPS));
        assert!(crate::approx_eq(Vec3::X.angle_between(Vec3::X * 5.0), 0.0, EPS));
    }

    #[test]
    fn orthonormal() {
        for v in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(0.3, -0.8, 0.51).normalize()] {
            let (u, w) = v.orthonormal_basis();
            assert!(crate::approx_eq(u.length(), 1.0, EPS));
            assert!(crate::approx_eq(w.length(), 1.0, EPS));
            assert!(crate::approx_eq(u.dot(v), 0.0, EPS));
            assert!(crate::approx_eq(w.dot(v), 0.0, EPS));
            assert!(u.cross(w).approx_eq(v, EPS));
        }
    }

    #[test]
    fn projection_and_reflection() {
        let v = Vec3::new(1.0, 1.0, 0.0);
        assert!(v.project_onto(Vec3::X * 3.0).approx_eq(Vec3::X, EPS));
        assert!(v.reflect(Vec3::Y).approx_eq(Vec3::new(1.0, -1.0, 0.0), EPS));
    }

    #[test]
    fn swizzles() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(v.truncate(), Vec2::new(1.0, 2.0));
        assert_eq!(v.xz(), Vec2::new(1.0, 3.0));
        assert_eq!(v.extend(4.0), Vec4::new(1.0, 2.0, 3.0, 4.0));
        assert_eq!(v.to_gpu(), [1.0f32, 2.0, 3.0]);
    }
}
