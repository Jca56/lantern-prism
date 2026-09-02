//! Plane in Hessian normal form: `normal · p + d = 0`.

use crate::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// Unit normal.
    pub normal: Vec3,
    /// Signed offset: `-normal · (any point on the plane)`.
    pub d: f64,
}

impl Plane {
    /// From a unit `normal` and a point on the plane.
    pub fn from_point_normal(point: Vec3, normal: Vec3) -> Self {
        Self { normal, d: -normal.dot(point) }
    }

    /// From three counter-clockwise points (normal by the right-hand rule).
    pub fn from_points(a: Vec3, b: Vec3, c: Vec3) -> Self {
        Self::from_point_normal(a, (b - a).cross(c - a).normalize())
    }

    /// From raw coefficients `(a, b, c, d)`; normalizes so the normal is unit.
    /// Returns `None` for a degenerate (zero-normal) plane.
    pub fn from_coefficients(a: f64, b: f64, c: f64, d: f64) -> Option<Self> {
        let n = Vec3::new(a, b, c);
        let len = n.length();
        if len < crate::EPS {
            return None;
        }
        Some(Self { normal: n / len, d: d / len })
    }

    /// Positive on the side the normal points to.
    #[inline]
    pub fn signed_distance(&self, p: Vec3) -> f64 {
        self.normal.dot(p) + self.d
    }

    /// Closest point on the plane.
    #[inline]
    pub fn project(&self, p: Vec3) -> Vec3 {
        p - self.normal * self.signed_distance(p)
    }

    #[inline]
    pub fn flipped(&self) -> Plane {
        Plane { normal: -self.normal, d: -self.d }
    }

    /// Any point on the plane (the one closest to the origin).
    #[inline]
    pub fn origin(&self) -> Vec3 {
        self.normal * -self.d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::EPS;

    #[test]
    fn distances() {
        let ground = Plane::from_point_normal(Vec3::new(0.0, 2.0, 0.0), Vec3::Y);
        assert_eq!(ground.d, -2.0);
        assert_eq!(ground.signed_distance(Vec3::new(5.0, 5.0, 5.0)), 3.0);
        assert_eq!(ground.signed_distance(Vec3::ZERO), -2.0);
        assert_eq!(ground.project(Vec3::new(1.0, 7.0, 1.0)), Vec3::new(1.0, 2.0, 1.0));
        assert_eq!(ground.flipped().signed_distance(Vec3::ZERO), 2.0);
        assert!(ground.origin().approx_eq(Vec3::new(0.0, 2.0, 0.0), EPS));
    }

    #[test]
    fn from_points_is_right_handed() {
        let p = Plane::from_points(Vec3::ZERO, Vec3::X, Vec3::Z);
        // X then Z counter-clockwise viewed from -Y: normal points down.
        assert!(p.normal.approx_eq(Vec3::NEG_Y, EPS));
        let q = Plane::from_points(Vec3::ZERO, Vec3::Z, Vec3::X);
        assert!(q.normal.approx_eq(Vec3::Y, EPS));
        assert!(Plane::from_coefficients(0.0, 0.0, 0.0, 1.0).is_none());
        let c = Plane::from_coefficients(0.0, 2.0, 0.0, -4.0).unwrap();
        assert_eq!(c.normal, Vec3::Y);
        assert_eq!(c.d, -2.0);
    }
}
