//! View frustum as a set of inward-facing planes, extracted from a
//! view-projection matrix (Gribb & Hartmann) for wgpu's `[0, 1]` reverse-Z
//! clip space. An infinite far plane is degenerate and is simply omitted.

use crate::{Aabb, Mat4, Plane, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    planes: [Plane; 6],
    len: usize,
}

impl Frustum {
    /// Planes from `view_proj`. Inside is where every plane's signed
    /// distance is `>= 0`.
    pub fn from_view_proj(m: &Mat4) -> Self {
        let r0 = m.row(0);
        let r1 = m.row(1);
        let r2 = m.row(2);
        let r3 = m.row(3);
        let candidates = [
            r3 + r0, // left:   x/w >= -1
            r3 - r0, // right:  x/w <=  1
            r3 + r1, // bottom: y/w >= -1
            r3 - r1, // top:    y/w <=  1
            r3 - r2, // near:   z/w <=  1  (reverse-Z: depth 1 at the near plane)
            r2,      // far:    z/w >=  0  (degenerate for an infinite projection)
        ];
        let dummy = Plane { normal: Vec3::Y, d: 0.0 };
        let mut planes = [dummy; 6];
        let mut len = 0;
        for c in candidates {
            if let Some(p) = Plane::from_coefficients(c.x, c.y, c.z, c.w) {
                planes[len] = p;
                len += 1;
            }
        }
        Self { planes, len }
    }

    pub fn planes(&self) -> &[Plane] {
        &self.planes[..self.len]
    }

    pub fn contains_point(&self, p: Vec3) -> bool {
        self.planes().iter().all(|pl| pl.signed_distance(p) >= 0.0)
    }

    /// Conservative box test: `false` only when the box is fully outside
    /// some plane. Boxes near a corner may pass despite being outside.
    pub fn intersects_aabb(&self, b: &Aabb) -> bool {
        self.planes().iter().all(|pl| {
            // The corner furthest along the normal.
            let p = Vec3::new(
                if pl.normal.x >= 0.0 { b.max.x } else { b.min.x },
                if pl.normal.y >= 0.0 { b.max.y } else { b.min.y },
                if pl.normal.z >= 0.0 { b.max.z } else { b.min.z },
            );
            pl.signed_distance(p) >= 0.0
        })
    }

    /// `true` when the box is entirely inside every plane.
    pub fn contains_aabb(&self, b: &Aabb) -> bool {
        self.planes().iter().all(|pl| {
            let p = Vec3::new(
                if pl.normal.x >= 0.0 { b.min.x } else { b.max.x },
                if pl.normal.y >= 0.0 { b.min.y } else { b.max.y },
                if pl.normal.z >= 0.0 { b.min.z } else { b.max.z },
            );
            pl.signed_distance(p) >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::FRAC_PI_2;

    fn camera() -> Frustum {
        let view = Mat4::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_infinite_reverse_z(FRAC_PI_2, 1.0, 0.1);
        Frustum::from_view_proj(&(proj * view))
    }

    #[test]
    fn infinite_projection_has_five_planes() {
        assert_eq!(camera().planes().len(), 5);
        let view = Mat4::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let ortho = Mat4::orthographic_reverse_z(-1.0, 1.0, -1.0, 1.0, 0.1, 100.0);
        assert_eq!(Frustum::from_view_proj(&(ortho * view)).planes().len(), 6);
    }

    #[test]
    fn points() {
        let f = camera();
        assert!(f.contains_point(Vec3::ZERO));
        assert!(f.contains_point(Vec3::new(0.0, 0.0, -1000.0)), "infinite far");
        assert!(!f.contains_point(Vec3::new(0.0, 0.0, 11.0)), "behind the camera");
        assert!(!f.contains_point(Vec3::new(0.0, 0.0, 9.95)), "closer than near");
        // 90° fov: at distance 10 the half-width is 10.
        assert!(f.contains_point(Vec3::new(9.9, 0.0, 0.0)));
        assert!(!f.contains_point(Vec3::new(10.1, 0.0, 0.0)));
        assert!(!f.contains_point(Vec3::new(0.0, -10.1, 0.0)));
    }

    #[test]
    fn boxes() {
        let f = camera();
        let unit = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        assert!(f.intersects_aabb(&unit));
        assert!(f.contains_aabb(&unit));
        let straddling = Aabb::new(Vec3::new(9.0, -1.0, -1.0), Vec3::new(20.0, 1.0, 1.0));
        assert!(f.intersects_aabb(&straddling));
        assert!(!f.contains_aabb(&straddling));
        let behind = Aabb::new(Vec3::new(-1.0, -1.0, 20.0), Vec3::new(1.0, 1.0, 30.0));
        assert!(!f.intersects_aabb(&behind));
        let way_off = Aabb::new(Vec3::new(50.0, 50.0, -1.0), Vec3::new(51.0, 51.0, 1.0));
        assert!(!f.intersects_aabb(&way_off));
    }
}
