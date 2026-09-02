//! 3D axis-aligned bounding box.

use crate::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Aabb {
    /// Inverted box: including any point makes it valid.
    pub const EMPTY: Aabb = Aabb {
        min: Vec3::splat(f64::INFINITY),
        max: Vec3::splat(f64::NEG_INFINITY),
    };

    #[inline]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn from_center_half_extents(center: Vec3, half: Vec3) -> Self {
        Self::new(center - half, center + half)
    }

    pub fn from_points<I: IntoIterator<Item = Vec3>>(points: I) -> Self {
        points.into_iter().fold(Self::EMPTY, |b, p| b.including(p))
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }

    #[inline]
    pub fn including(&self, p: Vec3) -> Aabb {
        Aabb::new(self.min.min(p), self.max.max(p))
    }

    #[inline]
    pub fn include(&mut self, p: Vec3) {
        *self = self.including(p);
    }

    #[inline]
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb::new(self.min.min(o.min), self.max.max(o.max))
    }

    #[inline]
    pub fn intersection(&self, o: &Aabb) -> Aabb {
        Aabb::new(self.min.max(o.min), self.max.min(o.max))
    }

    #[inline]
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    #[inline]
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    #[inline]
    pub fn half_extents(&self) -> Vec3 {
        self.size() * 0.5
    }

    /// Index (0 = x, 1 = y, 2 = z) of the longest side.
    pub fn longest_axis(&self) -> usize {
        let s = self.size();
        if s.x >= s.y && s.x >= s.z {
            0
        } else if s.y >= s.z {
            1
        } else {
            2
        }
    }

    pub fn surface_area(&self) -> f64 {
        let s = self.size();
        2.0 * (s.x * s.y + s.y * s.z + s.z * s.x)
    }

    pub fn volume(&self) -> f64 {
        let s = self.size();
        s.x * s.y * s.z
    }

    /// Closed on all faces.
    #[inline]
    pub fn contains(&self, p: Vec3) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }

    #[inline]
    pub fn intersects(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x
            && o.min.x <= self.max.x
            && self.min.y <= o.max.y
            && o.min.y <= self.max.y
            && self.min.z <= o.max.z
            && o.min.z <= self.max.z
    }

    #[inline]
    pub fn expand(&self, d: f64) -> Aabb {
        Aabb::new(self.min - Vec3::splat(d), self.max + Vec3::splat(d))
    }

    /// The eight corners; bit 0 → x, bit 1 → y, bit 2 → z picks min/max.
    pub fn corners(&self) -> [Vec3; 8] {
        let mut c = [Vec3::ZERO; 8];
        for (i, p) in c.iter_mut().enumerate() {
            *p = Vec3::new(
                if i & 1 == 0 { self.min.x } else { self.max.x },
                if i & 2 == 0 { self.min.y } else { self.max.y },
                if i & 4 == 0 { self.min.z } else { self.max.z },
            );
        }
        c
    }

    /// Bounds of this box after transformation (bounds of the 8 corners).
    pub fn transformed(&self, m: &Mat4) -> Aabb {
        Aabb::from_points(self.corners().iter().map(|&c| m.transform_point(c)))
    }

    /// Squared distance from `p` to the box (0 inside).
    pub fn distance_squared(&self, p: Vec3) -> f64 {
        let q = p.clamp(self.min, self.max);
        (p - q).length_squared()
    }

    pub fn approx_eq(&self, o: &Aabb, eps: f64) -> bool {
        self.min.approx_eq(o.min, eps) && self.max.approx_eq(o.max, eps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{EPS, FRAC_PI_2};

    #[test]
    fn build_and_measure() {
        let b = Aabb::from_points([Vec3::new(1.0, -1.0, 2.0), Vec3::new(-3.0, 4.0, 0.0), Vec3::ZERO]);
        assert_eq!(b.min, Vec3::new(-3.0, -1.0, 0.0));
        assert_eq!(b.max, Vec3::new(1.0, 4.0, 2.0));
        assert_eq!(b.size(), Vec3::new(4.0, 5.0, 2.0));
        assert_eq!(b.longest_axis(), 1);
        assert_eq!(b.volume(), 40.0);
        assert_eq!(b.surface_area(), 2.0 * (20.0 + 10.0 + 8.0));
        assert!(Aabb::EMPTY.is_empty());
        assert!(!b.is_empty());
        assert!(b.contains(b.center()));
        assert!(b.contains(b.max));
        assert!(!b.contains(b.max + Vec3::X));
    }

    #[test]
    fn set_ops_and_distance() {
        let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb::new(Vec3::splat(0.5), Vec3::splat(2.0));
        assert!(a.intersects(&b));
        assert_eq!(a.intersection(&b), Aabb::new(Vec3::splat(0.5), Vec3::ONE));
        assert_eq!(a.union(&b), Aabb::new(Vec3::ZERO, Vec3::splat(2.0)));
        let far = Aabb::new(Vec3::splat(5.0), Vec3::splat(6.0));
        assert!(!a.intersects(&far));
        assert!(a.intersection(&far).is_empty());
        assert_eq!(a.distance_squared(Vec3::new(3.0, 0.5, 0.5)), 4.0);
        assert_eq!(a.distance_squared(Vec3::splat(0.5)), 0.0);
    }

    #[test]
    fn transform_rotates_bounds() {
        let a = Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
        let m = Mat4::from_axis_angle(Vec3::Y, FRAC_PI_2);
        let t = a.transformed(&m);
        // Rotating 90° about Y maps +X to -Z: the long side now runs along -Z.
        assert!(t.approx_eq(&Aabb::new(Vec3::new(0.0, 0.0, -2.0), Vec3::new(1.0, 1.0, 0.0)), EPS));
        assert_eq!(a.corners()[7], a.max);
        assert_eq!(a.corners()[0], a.min);
    }
}
