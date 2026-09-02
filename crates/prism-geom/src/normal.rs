//! Polygon normals and the weights used to blend them at vertices.

use prism_math::Vec3;

/// Newell's method: robust for non-planar and concave polygons. The result
/// is **not** normalized; its length is twice the projected area.
pub fn newell(points: &[Vec3]) -> Vec3 {
    let n = points.len();
    if n < 3 {
        return Vec3::ZERO;
    }
    let mut acc = Vec3::ZERO;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        acc.x += (a.y - b.y) * (a.z + b.z);
        acc.y += (a.z - b.z) * (a.x + b.x);
        acc.z += (a.x - b.x) * (a.y + b.y);
    }
    acc
}

/// Unit normal, or `ZERO` for a degenerate polygon.
pub fn polygon_normal(points: &[Vec3]) -> Vec3 {
    newell(points).normalize_or_zero()
}

/// Area of a (possibly non-planar) polygon.
pub fn polygon_area(points: &[Vec3]) -> f64 {
    newell(points).length() * 0.5
}

pub fn centroid(points: &[Vec3]) -> Vec3 {
    if points.is_empty() {
        return Vec3::ZERO;
    }
    points.iter().copied().sum::<Vec3>() / points.len() as f64
}

/// Interior angle at `cur` between the edges to `prev` and `next`. Used to
/// weight face normals when averaging at a vertex, so a face that wraps a
/// wide angle around the vertex counts more.
pub fn corner_angle(prev: Vec3, cur: Vec3, next: Vec3) -> f64 {
    let a = prev - cur;
    let b = next - cur;
    let la = a.length();
    let lb = b.length();
    if la <= 0.0 || lb <= 0.0 {
        return 0.0;
    }
    (a.dot(b) / (la * lb)).clamp(-1.0, 1.0).acos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_math::{EPS, FRAC_PI_2, approx_eq};

    #[test]
    fn ccw_square_points_up() {
        let sq = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, -1.0), Vec3::new(0.0, 0.0, -1.0)];
        // Counter-clockwise when viewed from +Y (right-handed, Y up).
        assert!(polygon_normal(&sq).approx_eq(Vec3::Y, EPS));
        assert!(approx_eq(polygon_area(&sq), 1.0, EPS));
        assert!(centroid(&sq).approx_eq(Vec3::new(0.5, 0.0, -0.5), EPS));
        let mut rev = sq;
        rev.reverse();
        assert!(polygon_normal(&rev).approx_eq(Vec3::NEG_Y, EPS));
    }

    #[test]
    fn non_planar_and_degenerate() {
        let bent = [Vec3::ZERO, Vec3::X, Vec3::new(1.0, 0.3, 1.0), Vec3::Z];
        let n = polygon_normal(&bent);
        assert!(n.y < 0.0 && approx_eq(n.length(), 1.0, EPS));
        assert_eq!(polygon_normal(&[Vec3::ZERO, Vec3::X]), Vec3::ZERO);
        assert_eq!(polygon_normal(&[Vec3::ZERO, Vec3::X, Vec3::X * 2.0]), Vec3::ZERO, "collinear");
    }

    #[test]
    fn angles() {
        assert!(approx_eq(corner_angle(Vec3::X, Vec3::ZERO, Vec3::Y), FRAC_PI_2, EPS));
        assert_eq!(corner_angle(Vec3::ZERO, Vec3::ZERO, Vec3::Y), 0.0);
    }
}
