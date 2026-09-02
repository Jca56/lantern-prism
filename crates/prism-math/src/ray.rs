//! Ray with an origin and a (not necessarily unit) direction.

use crate::{Aabb, Mat4, Plane, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl Ray {
    #[inline]
    pub const fn new(origin: Vec3, dir: Vec3) -> Self {
        Self { origin, dir }
    }

    #[inline]
    pub fn at(&self, t: f64) -> Vec3 {
        self.origin + self.dir * t
    }

    /// Parameter `t` where the ray hits the plane, if it does so ahead of the
    /// origin (`t >= 0`) and is not parallel.
    pub fn intersect_plane(&self, plane: &Plane) -> Option<f64> {
        let denom = plane.normal.dot(self.dir);
        if denom.abs() < crate::EPS {
            return None;
        }
        let t = -plane.signed_distance(self.origin) / denom;
        (t >= 0.0).then_some(t)
    }

    /// Slab test. Returns `(t_enter, t_exit)` clipped to `t >= 0`, or `None`
    /// when the ray misses (or the box is entirely behind the origin).
    pub fn intersect_aabb(&self, b: &Aabb) -> Option<(f64, f64)> {
        let mut t0 = 0.0_f64;
        let mut t1 = f64::INFINITY;
        for i in 0..3 {
            let inv = 1.0 / self.dir[i];
            let mut near = (b.min[i] - self.origin[i]) * inv;
            let mut far = (b.max[i] - self.origin[i]) * inv;
            if inv < 0.0 {
                core::mem::swap(&mut near, &mut far);
            }
            // NaN from 0*inf when the ray lies exactly on a slab face: treat
            // as no constraint by using max/min that ignore NaN.
            t0 = if near > t0 { near } else { t0 };
            t1 = if far < t1 { far } else { t1 };
            if t0 > t1 {
                return None;
            }
        }
        Some((t0, t1))
    }

    /// Point on the ray closest to `p`, as a parameter (clamped to `t >= 0`).
    pub fn closest_t(&self, p: Vec3) -> f64 {
        ((p - self.origin).dot(self.dir) / self.dir.length_squared()).max(0.0)
    }

    pub fn transformed(&self, m: &Mat4) -> Ray {
        Ray::new(m.transform_point(self.origin), m.transform_vector(self.dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::EPS;

    #[test]
    fn plane_hits() {
        let ground = Plane::from_point_normal(Vec3::ZERO, Vec3::Y);
        let r = Ray::new(Vec3::new(0.0, 10.0, 0.0), Vec3::new(1.0, -1.0, 0.0));
        let t = r.intersect_plane(&ground).unwrap();
        assert!(r.at(t).approx_eq(Vec3::new(10.0, 0.0, 0.0), EPS));
        // Parallel and behind both miss.
        assert!(Ray::new(Vec3::Y, Vec3::X).intersect_plane(&ground).is_none());
        assert!(Ray::new(Vec3::Y, Vec3::Y).intersect_plane(&ground).is_none());
    }

    #[test]
    fn aabb_hits() {
        let b = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        let r = Ray::new(Vec3::new(-5.0, 0.0, 0.0), Vec3::X);
        assert_eq!(r.intersect_aabb(&b), Some((4.0, 6.0)));
        // Starting inside: enter is clamped to 0.
        let inside = Ray::new(Vec3::ZERO, Vec3::Z);
        assert_eq!(inside.intersect_aabb(&b), Some((0.0, 1.0)));
        // Miss and behind.
        assert!(Ray::new(Vec3::new(-5.0, 3.0, 0.0), Vec3::X).intersect_aabb(&b).is_none());
        assert!(Ray::new(Vec3::new(5.0, 0.0, 0.0), Vec3::X).intersect_aabb(&b).is_none());
        // Axis-parallel ray with a zero direction component.
        let r = Ray::new(Vec3::new(0.5, -9.0, 0.5), Vec3::Y);
        assert_eq!(r.intersect_aabb(&b), Some((8.0, 10.0)));
    }

    #[test]
    fn closest_and_transform() {
        let r = Ray::new(Vec3::ZERO, Vec3::X * 2.0);
        assert_eq!(r.closest_t(Vec3::new(4.0, 3.0, 0.0)), 2.0);
        assert_eq!(r.closest_t(Vec3::new(-4.0, 3.0, 0.0)), 0.0);
        let m = Mat4::from_translation(Vec3::Y);
        let t = r.transformed(&m);
        assert_eq!(t.origin, Vec3::Y);
        assert_eq!(t.dir, r.dir);
    }
}
