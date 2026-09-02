//! 3×3 matrix, column-major. Rotations, scales, normal matrices.

use crate::{Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Mat3 {
    /// Columns. `cols[c][r]` is row `r` of column `c`.
    pub cols: [Vec3; 3],
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3::from_cols(Vec3::X, Vec3::Y, Vec3::Z);
    pub const ZERO: Mat3 = Mat3::from_cols(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);

    #[inline]
    pub const fn from_cols(c0: Vec3, c1: Vec3, c2: Vec3) -> Self {
        Self { cols: [c0, c1, c2] }
    }

    /// Build from rows (what you'd write on paper).
    #[inline]
    pub fn from_rows(r0: Vec3, r1: Vec3, r2: Vec3) -> Self {
        Self::from_cols(
            Vec3::new(r0.x, r1.x, r2.x),
            Vec3::new(r0.y, r1.y, r2.y),
            Vec3::new(r0.z, r1.z, r2.z),
        )
    }

    #[inline]
    pub fn from_diagonal(d: Vec3) -> Self {
        Self::from_cols(Vec3::X * d.x, Vec3::Y * d.y, Vec3::Z * d.z)
    }

    #[inline]
    pub fn from_scale(s: Vec3) -> Self {
        Self::from_diagonal(s)
    }

    /// Rotation about a unit `axis` by `angle` radians (Rodrigues).
    pub fn from_axis_angle(axis: Vec3, angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        let Vec3 { x, y, z } = axis;
        Self::from_cols(
            Vec3::new(t * x * x + c, t * x * y + s * z, t * x * z - s * y),
            Vec3::new(t * x * y - s * z, t * y * y + c, t * y * z + s * x),
            Vec3::new(t * x * z + s * y, t * y * z - s * x, t * z * z + c),
        )
    }

    pub fn from_rotation_x(a: f64) -> Self {
        let (s, c) = a.sin_cos();
        Self::from_cols(Vec3::X, Vec3::new(0.0, c, s), Vec3::new(0.0, -s, c))
    }

    pub fn from_rotation_y(a: f64) -> Self {
        let (s, c) = a.sin_cos();
        Self::from_cols(Vec3::new(c, 0.0, -s), Vec3::Y, Vec3::new(s, 0.0, c))
    }

    pub fn from_rotation_z(a: f64) -> Self {
        let (s, c) = a.sin_cos();
        Self::from_cols(Vec3::new(c, s, 0.0), Vec3::new(-s, c, 0.0), Vec3::Z)
    }

    #[inline]
    pub fn from_quat(q: Quat) -> Self {
        q.to_mat3()
    }

    /// Element at `row`, `col`.
    #[inline]
    pub fn at(&self, row: usize, col: usize) -> f64 {
        self.cols[col][row]
    }

    #[inline]
    pub fn col(&self, c: usize) -> Vec3 {
        self.cols[c]
    }

    #[inline]
    pub fn row(&self, r: usize) -> Vec3 {
        Vec3::new(self.cols[0][r], self.cols[1][r], self.cols[2][r])
    }

    pub fn transpose(&self) -> Self {
        Self::from_rows(self.cols[0], self.cols[1], self.cols[2])
    }

    pub fn determinant(&self) -> f64 {
        let [a, b, c] = self.cols;
        a.dot(b.cross(c))
    }

    /// Inverse, or `None` when singular.
    pub fn inverse(&self) -> Option<Self> {
        let [a, b, c] = self.cols;
        let r0 = b.cross(c);
        let r1 = c.cross(a);
        let r2 = a.cross(b);
        let det = a.dot(r0);
        if det.abs() < f64::MIN_POSITIVE {
            return None;
        }
        let inv_det = 1.0 / det;
        // Rows of the inverse are the scaled cross products.
        Some(Self::from_rows(r0 * inv_det, r1 * inv_det, r2 * inv_det))
    }

    /// `true` when every element is within `eps` of `o`'s.
    pub fn approx_eq(&self, o: &Mat3, eps: f64) -> bool {
        (0..3).all(|i| self.cols[i].approx_eq(o.cols[i], eps))
    }

    /// Column-major `[[f32; 3]; 3]` for upload. Note WGSL `mat3x3<f32>` has
    /// 16-byte column stride in uniform buffers; pad on the GPU-struct side.
    pub fn to_gpu(&self) -> [[f32; 3]; 3] {
        [self.cols[0].to_gpu(), self.cols[1].to_gpu(), self.cols[2].to_gpu()]
    }
}

impl core::ops::Mul<Vec3> for Mat3 {
    type Output = Vec3;
    #[inline]
    fn mul(self, v: Vec3) -> Vec3 {
        self.cols[0] * v.x + self.cols[1] * v.y + self.cols[2] * v.z
    }
}

impl core::ops::Mul for Mat3 {
    type Output = Mat3;
    #[inline]
    fn mul(self, o: Mat3) -> Mat3 {
        Mat3::from_cols(self * o.cols[0], self * o.cols[1], self * o.cols[2])
    }
}

impl core::ops::Mul<f64> for Mat3 {
    type Output = Mat3;
    fn mul(self, s: f64) -> Mat3 {
        Mat3::from_cols(self.cols[0] * s, self.cols[1] * s, self.cols[2] * s)
    }
}

impl core::ops::Add for Mat3 {
    type Output = Mat3;
    fn add(self, o: Mat3) -> Mat3 {
        Mat3::from_cols(self.cols[0] + o.cols[0], self.cols[1] + o.cols[1], self.cols[2] + o.cols[2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{EPS, FRAC_PI_2};

    #[test]
    fn rotations_are_right_handed() {
        // Rotating +X by 90° about +Z gives +Y (counter-clockwise looking down +Z).
        assert!((Mat3::from_rotation_z(FRAC_PI_2) * Vec3::X).approx_eq(Vec3::Y, EPS));
        // Rotating +Y by 90° about +X gives +Z.
        assert!((Mat3::from_rotation_x(FRAC_PI_2) * Vec3::Y).approx_eq(Vec3::Z, EPS));
        // Rotating +Z by 90° about +Y gives +X.
        assert!((Mat3::from_rotation_y(FRAC_PI_2) * Vec3::Z).approx_eq(Vec3::X, EPS));
        // Axis-angle agrees with the fixed-axis constructors.
        let a = Mat3::from_axis_angle(Vec3::Y, 0.7);
        assert!(a.approx_eq(&Mat3::from_rotation_y(0.7), EPS));
    }

    #[test]
    fn inverse_and_transpose() {
        let m = Mat3::from_rows(
            Vec3::new(2.0, 0.0, 1.0),
            Vec3::new(1.0, 3.0, 0.0),
            Vec3::new(0.0, 1.0, 4.0),
        );
        assert_eq!(m.at(0, 2), 1.0);
        assert_eq!(m.transpose().at(2, 0), 1.0);
        assert!(crate::approx_eq(m.determinant(), 25.0, EPS));
        let inv = m.inverse().unwrap();
        assert!((m * inv).approx_eq(&Mat3::IDENTITY, EPS));
        assert!((inv * m).approx_eq(&Mat3::IDENTITY, EPS));
        assert!(Mat3::ZERO.inverse().is_none());
        // A rotation's inverse is its transpose.
        let r = Mat3::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 1.1);
        assert!(r.inverse().unwrap().approx_eq(&r.transpose(), EPS));
    }

    #[test]
    fn multiplication_order() {
        // (A * B) * v == A * (B * v)
        let a = Mat3::from_rotation_x(0.3);
        let b = Mat3::from_scale(Vec3::new(1.0, 2.0, 3.0));
        let v = Vec3::new(1.0, 1.0, 1.0);
        assert!(((a * b) * v).approx_eq(a * (b * v), EPS));
    }
}
