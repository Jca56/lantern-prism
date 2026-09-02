//! 2D vector.

use crate::macros::impl_vec_ops;
use crate::Vec3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl_vec_ops!(Vec2 { x, y });

impl Vec2 {
    pub const ZERO: Vec2 = Vec2::new(0.0, 0.0);
    pub const ONE: Vec2 = Vec2::new(1.0, 1.0);
    pub const X: Vec2 = Vec2::new(1.0, 0.0);
    pub const Y: Vec2 = Vec2::new(0.0, 1.0);

    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// 2D cross product (the z of the 3D cross): positive when `o` is
    /// counter-clockwise from `self`.
    #[inline]
    pub fn perp_dot(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }

    /// Rotate 90° counter-clockwise.
    #[inline]
    pub fn perp(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }

    /// Angle from +X in radians, in `(-PI, PI]`.
    #[inline]
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    /// Unit vector at `angle` radians from +X.
    #[inline]
    pub fn from_angle(angle: f64) -> Vec2 {
        Vec2::new(angle.cos(), angle.sin())
    }

    /// Rotate counter-clockwise by `angle` radians.
    #[inline]
    pub fn rotate(self, angle: f64) -> Vec2 {
        let (s, c) = angle.sin_cos();
        Vec2::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    #[inline]
    pub fn extend(self, z: f64) -> Vec3 {
        Vec3::new(self.x, self.y, z)
    }

    #[inline]
    pub fn to_gpu(self) -> [f32; 2] {
        [self.x as f32, self.y as f32]
    }

    #[inline]
    pub fn from_gpu(v: [f32; 2]) -> Vec2 {
        Vec2::new(v[0] as f64, v[1] as f64)
    }

    #[inline]
    pub fn to_array(self) -> [f64; 2] {
        [self.x, self.y]
    }
}

impl From<[f64; 2]> for Vec2 {
    fn from(a: [f64; 2]) -> Self {
        Vec2::new(a[0], a[1])
    }
}

impl From<(f64, f64)> for Vec2 {
    fn from((x, y): (f64, f64)) -> Self {
        Vec2::new(x, y)
    }
}

impl core::ops::Index<usize> for Vec2 {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        match i {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("Vec2 index {i} out of range"),
        }
    }
}

impl core::ops::IndexMut<usize> for Vec2 {
    fn index_mut(&mut self, i: usize) -> &mut f64 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("Vec2 index {i} out of range"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{EPS, FRAC_PI_2};

    #[test]
    fn arithmetic() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, -4.0);
        assert_eq!(a + b, Vec2::new(4.0, -2.0));
        assert_eq!(a - b, Vec2::new(-2.0, 6.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(2.0 * a, Vec2::new(2.0, 4.0));
        assert_eq!(b / 2.0, Vec2::new(1.5, -2.0));
        assert_eq!(-a, Vec2::new(-1.0, -2.0));
        assert_eq!(a.dot(b), -5.0);
        assert_eq!(b.length(), 5.0);
        assert_eq!(a.perp_dot(b), -10.0);
    }

    #[test]
    fn rotation() {
        let r = Vec2::X.rotate(FRAC_PI_2);
        assert!(r.approx_eq(Vec2::Y, EPS));
        assert!(Vec2::X.perp().approx_eq(Vec2::Y, EPS));
        assert!(crate::approx_eq(Vec2::Y.angle(), FRAC_PI_2, EPS));
        assert!(Vec2::from_angle(FRAC_PI_2).approx_eq(Vec2::Y, EPS));
    }

    #[test]
    fn normalize_cases() {
        assert!(Vec2::new(3.0, 4.0).normalize().approx_eq(Vec2::new(0.6, 0.8), EPS));
        assert_eq!(Vec2::ZERO.try_normalize(), None);
        assert_eq!(Vec2::ZERO.normalize_or(Vec2::X), Vec2::X);
    }

    #[test]
    fn misc() {
        let a = Vec2::new(-1.5, 2.5);
        assert_eq!(a.abs(), Vec2::new(1.5, 2.5));
        assert_eq!(a.min_element(), -1.5);
        assert_eq!(a.max_element(), 2.5);
        assert_eq!(a.lerp(Vec2::ZERO, 0.5), Vec2::new(-0.75, 1.25));
        assert_eq!(a[0], -1.5);
        assert_eq!(a.to_gpu(), [-1.5f32, 2.5f32]);
        let sum: Vec2 = [Vec2::X, Vec2::Y, Vec2::ONE].into_iter().sum();
        assert_eq!(sum, Vec2::new(2.0, 2.0));
    }
}
