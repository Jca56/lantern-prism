//! Orientation predicates. Plain `f64` with a relative tolerance for now; the
//! signatures are what an exact (Shewchuk-style adaptive) implementation
//! would have, so callers never change when the internals do.

use prism_math::{Vec2, Vec3};

/// Twice the signed area of triangle `abc`: positive when counter-clockwise.
#[inline]
pub fn orient2d(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Six times the signed volume of tetrahedron `abcd`: positive when `d` is on
/// the side of plane `abc` that the right-hand rule normal points to.
#[inline]
pub fn orient3d(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> f64 {
    (b - a).cross(c - a).dot(d - a)
}

/// `true` when `abc` is counter-clockwise by more than a relative tolerance.
pub fn is_ccw(a: Vec2, b: Vec2, c: Vec2) -> bool {
    orient2d(a, b, c) > tolerance2(a, b, c)
}

/// `true` when `abc` has (near) zero area.
pub fn is_collinear(a: Vec2, b: Vec2, c: Vec2) -> bool {
    orient2d(a, b, c).abs() <= tolerance2(a, b, c)
}

fn tolerance2(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    let scale = (b - a).length_squared().max((c - a).length_squared());
    scale * 1e-12
}

/// Is `p` inside (or on the boundary of) counter-clockwise triangle `abc`?
pub fn point_in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let tol = -tolerance2(a, b, c);
    orient2d(a, b, p) >= tol && orient2d(b, c, p) >= tol && orient2d(c, a, p) >= tol
}

/// Is `p` strictly inside `abc` (not on an edge)?
pub fn point_strictly_in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let tol = tolerance2(a, b, c);
    orient2d(a, b, p) > tol && orient2d(b, c, p) > tol && orient2d(c, a, p) > tol
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation() {
        let (a, b, c) = (Vec2::ZERO, Vec2::X, Vec2::Y);
        assert!(orient2d(a, b, c) > 0.0);
        assert!(orient2d(a, c, b) < 0.0);
        assert!(is_ccw(a, b, c));
        assert!(!is_ccw(a, c, b));
        assert!(is_collinear(a, b, Vec2::X * 3.0));
        assert!(orient3d(Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z) > 0.0);
        assert!(orient3d(Vec3::ZERO, Vec3::Y, Vec3::X, Vec3::Z) < 0.0);
    }

    #[test]
    fn containment() {
        let (a, b, c) = (Vec2::ZERO, Vec2::X * 2.0, Vec2::Y * 2.0);
        assert!(point_in_triangle(Vec2::new(0.5, 0.5), a, b, c));
        assert!(point_in_triangle(Vec2::new(1.0, 0.0), a, b, c), "on an edge");
        assert!(!point_strictly_in_triangle(Vec2::new(1.0, 0.0), a, b, c));
        assert!(!point_in_triangle(Vec2::new(1.5, 1.5), a, b, c));
    }
}
