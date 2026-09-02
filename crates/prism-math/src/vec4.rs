//! 4D vector (homogeneous coordinates, matrix columns).

use crate::macros::impl_vec_ops;
use crate::Vec3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vec4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl_vec_ops!(Vec4 { x, y, z, w });

impl Vec4 {
    pub const ZERO: Vec4 = Vec4::new(0.0, 0.0, 0.0, 0.0);
    pub const ONE: Vec4 = Vec4::new(1.0, 1.0, 1.0, 1.0);
    pub const X: Vec4 = Vec4::new(1.0, 0.0, 0.0, 0.0);
    pub const Y: Vec4 = Vec4::new(0.0, 1.0, 0.0, 0.0);
    pub const Z: Vec4 = Vec4::new(0.0, 0.0, 1.0, 0.0);
    pub const W: Vec4 = Vec4::new(0.0, 0.0, 0.0, 1.0);

    #[inline]
    pub const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }

    #[inline]
    pub const fn from_vec3(v: Vec3, w: f64) -> Self {
        Self::new(v.x, v.y, v.z, w)
    }

    /// Drop `w`.
    #[inline]
    pub fn truncate(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    /// Homogeneous divide: `xyz / w`.
    #[inline]
    pub fn project(self) -> Vec3 {
        let r = 1.0 / self.w;
        Vec3::new(self.x * r, self.y * r, self.z * r)
    }

    #[inline]
    pub fn to_gpu(self) -> [f32; 4] {
        [self.x as f32, self.y as f32, self.z as f32, self.w as f32]
    }

    #[inline]
    pub fn from_gpu(v: [f32; 4]) -> Vec4 {
        Vec4::new(v[0] as f64, v[1] as f64, v[2] as f64, v[3] as f64)
    }

    #[inline]
    pub fn to_array(self) -> [f64; 4] {
        [self.x, self.y, self.z, self.w]
    }
}

impl From<[f64; 4]> for Vec4 {
    fn from(a: [f64; 4]) -> Self {
        Vec4::new(a[0], a[1], a[2], a[3])
    }
}

impl core::ops::Index<usize> for Vec4 {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        match i {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            3 => &self.w,
            _ => panic!("Vec4 index {i} out of range"),
        }
    }
}

impl core::ops::IndexMut<usize> for Vec4 {
    fn index_mut(&mut self, i: usize) -> &mut f64 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            3 => &mut self.w,
            _ => panic!("Vec4 index {i} out of range"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        let v = Vec4::new(2.0, 4.0, 6.0, 2.0);
        assert_eq!(v.project(), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(v.truncate(), Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(Vec4::from_vec3(Vec3::ONE, 0.0), Vec4::new(1.0, 1.0, 1.0, 0.0));
        assert_eq!(v.dot(Vec4::ONE), 14.0);
        assert_eq!(v[3], 2.0);
    }
}
