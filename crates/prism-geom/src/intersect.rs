//! Ray and closest-point queries against triangles and segments.

use prism_math::{Ray, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub t: f64,
    /// Barycentric coordinates of `b` and `c` (`a` gets `1 - u - v`).
    pub u: f64,
    pub v: f64,
}

/// Möller–Trumbore, two-sided. Hits behind the origin are rejected.
pub fn ray_triangle(ray: &Ray, a: Vec3, b: Vec3, c: Vec3) -> Option<RayHit> {
    let e1 = b - a;
    let e2 = c - a;
    let p = ray.dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = ray.origin - a;
    let u = s.dot(p) * inv;
    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = ray.dir.dot(q) * inv;
    if v < -1e-9 || u + v > 1.0 + 1e-9 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t >= 0.0).then_some(RayHit { t, u, v })
}

/// Closest point on segment `ab` to `p`, and its parameter in `[0, 1]`.
pub fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> (Vec3, f64) {
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 <= 0.0 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (a + ab * t, t)
}

/// Closest point on triangle `abc` to `p` (Ericson, Real-Time Collision Detection).
pub fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_math::EPS;

    #[test]
    fn ray_hits_and_misses() {
        let (a, b, c) = (Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, -1.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let hit = ray_triangle(&Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z), a, b, c).unwrap();
        assert!((hit.t - 5.0).abs() < EPS);
        // Two-sided: from below works too.
        assert!(ray_triangle(&Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::Z), a, b, c).is_some());
        // Miss to the side, parallel, behind.
        assert!(ray_triangle(&Ray::new(Vec3::new(5.0, 0.0, 5.0), Vec3::NEG_Z), a, b, c).is_none());
        assert!(ray_triangle(&Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::X), a, b, c).is_none());
        assert!(ray_triangle(&Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::Z), a, b, c).is_none());
        // Barycentrics at vertex b.
        let hb = ray_triangle(&Ray::new(b + Vec3::Z, Vec3::NEG_Z), a, b, c).unwrap();
        assert!((hb.u - 1.0).abs() < 1e-9 && hb.v.abs() < 1e-9);
    }

    #[test]
    fn closest_points() {
        let (p, t) = closest_on_segment(Vec3::new(5.0, 1.0, 0.0), Vec3::ZERO, Vec3::X * 2.0);
        assert_eq!((p, t), (Vec3::X * 2.0, 1.0));
        let (p, t) = closest_on_segment(Vec3::new(1.0, 1.0, 0.0), Vec3::ZERO, Vec3::X * 2.0);
        assert_eq!((p, t), (Vec3::X, 0.5));
        let (a, b, c) = (Vec3::ZERO, Vec3::X * 2.0, Vec3::Y * 2.0);
        assert!(closest_on_triangle(Vec3::new(0.5, 0.5, 3.0), a, b, c).approx_eq(Vec3::new(0.5, 0.5, 0.0), EPS), "interior");
        assert!(closest_on_triangle(Vec3::new(-1.0, -1.0, 0.0), a, b, c).approx_eq(a, EPS), "vertex");
        assert!(closest_on_triangle(Vec3::new(1.0, -1.0, 0.0), a, b, c).approx_eq(Vec3::X, EPS), "edge");
        assert!(closest_on_triangle(Vec3::new(2.0, 2.0, 0.0), a, b, c).approx_eq(Vec3::new(1.0, 1.0, 0.0), EPS), "hypotenuse");
    }
}
