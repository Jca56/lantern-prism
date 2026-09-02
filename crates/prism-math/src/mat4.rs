//! 4×4 matrix, column-major. Transforms, view and projection matrices.
//!
//! Projections target wgpu clip space (`x, y ∈ [-1, 1]`, `z ∈ [0, 1]`) with
//! **reverse-Z**: the near plane maps to depth `1` and infinity to `0`.
//! Pair with `Depth32Float` and a `Greater` compare.

use crate::{Mat3, Quat, Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Mat4 {
    /// Columns. `cols[c][r]` is row `r` of column `c`.
    pub cols: [Vec4; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::Z, Vec4::W);
    pub const ZERO: Mat4 = Mat4::from_cols(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, Vec4::ZERO);

    #[inline]
    pub const fn from_cols(c0: Vec4, c1: Vec4, c2: Vec4, c3: Vec4) -> Self {
        Self { cols: [c0, c1, c2, c3] }
    }

    /// Build from rows (what you'd write on paper).
    pub fn from_rows(r0: Vec4, r1: Vec4, r2: Vec4, r3: Vec4) -> Self {
        Self::from_cols(
            Vec4::new(r0.x, r1.x, r2.x, r3.x),
            Vec4::new(r0.y, r1.y, r2.y, r3.y),
            Vec4::new(r0.z, r1.z, r2.z, r3.z),
            Vec4::new(r0.w, r1.w, r2.w, r3.w),
        )
    }

    pub fn from_translation(t: Vec3) -> Self {
        Self::from_cols(Vec4::X, Vec4::Y, Vec4::Z, Vec4::from_vec3(t, 1.0))
    }

    pub fn from_scale(s: Vec3) -> Self {
        Self::from_cols(Vec4::X * s.x, Vec4::Y * s.y, Vec4::Z * s.z, Vec4::W)
    }

    pub fn from_mat3(m: Mat3) -> Self {
        Self::from_cols(
            Vec4::from_vec3(m.cols[0], 0.0),
            Vec4::from_vec3(m.cols[1], 0.0),
            Vec4::from_vec3(m.cols[2], 0.0),
            Vec4::W,
        )
    }

    pub fn from_quat(q: Quat) -> Self {
        Self::from_mat3(q.to_mat3())
    }

    pub fn from_axis_angle(axis: Vec3, angle: f64) -> Self {
        Self::from_mat3(Mat3::from_axis_angle(axis, angle))
    }

    /// `T * R * S`.
    pub fn from_translation_rotation_scale(t: Vec3, r: Quat, s: Vec3) -> Self {
        let m = r.to_mat3();
        Self::from_cols(
            Vec4::from_vec3(m.cols[0] * s.x, 0.0),
            Vec4::from_vec3(m.cols[1] * s.y, 0.0),
            Vec4::from_vec3(m.cols[2] * s.z, 0.0),
            Vec4::from_vec3(t, 1.0),
        )
    }

    /// Right-handed view matrix: camera at `eye` looking at `target`, view
    /// space looks down **-Z** with `up` mapped toward +Y.
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = (target - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);
        Self::from_rows(
            Vec4::from_vec3(s, -s.dot(eye)),
            Vec4::from_vec3(u, -u.dot(eye)),
            Vec4::from_vec3(-f, f.dot(eye)),
            Vec4::W,
        )
    }

    /// Reverse-Z perspective with an infinite far plane. `fov_y` in radians.
    /// Depth is `near / dist`: `1` at the near plane, `→ 0` at infinity.
    pub fn perspective_infinite_reverse_z(fov_y: f64, aspect: f64, near: f64) -> Self {
        let f = 1.0 / (fov_y * 0.5).tan();
        Self::from_cols(
            Vec4::new(f / aspect, 0.0, 0.0, 0.0),
            Vec4::new(0.0, f, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, -1.0),
            Vec4::new(0.0, 0.0, near, 0.0),
        )
    }

    /// Reverse-Z orthographic projection. View-space `z = -near` maps to depth
    /// `1` and `z = -far` to depth `0`.
    pub fn orthographic_reverse_z(
        left: f64,
        right: f64,
        bottom: f64,
        top: f64,
        near: f64,
        far: f64,
    ) -> Self {
        let rw = 1.0 / (right - left);
        let rh = 1.0 / (top - bottom);
        let rd = 1.0 / (far - near);
        Self::from_cols(
            Vec4::new(2.0 * rw, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 * rh, 0.0, 0.0),
            Vec4::new(0.0, 0.0, rd, 0.0),
            Vec4::new(-(right + left) * rw, -(top + bottom) * rh, far * rd, 1.0),
        )
    }

    #[inline]
    pub fn at(&self, row: usize, col: usize) -> f64 {
        self.cols[col][row]
    }

    #[inline]
    pub fn col(&self, c: usize) -> Vec4 {
        self.cols[c]
    }

    #[inline]
    pub fn row(&self, r: usize) -> Vec4 {
        Vec4::new(self.cols[0][r], self.cols[1][r], self.cols[2][r], self.cols[3][r])
    }

    /// The translation column.
    #[inline]
    pub fn translation(&self) -> Vec3 {
        self.cols[3].truncate()
    }

    /// Upper-left 3×3.
    pub fn to_mat3(&self) -> Mat3 {
        Mat3::from_cols(self.cols[0].truncate(), self.cols[1].truncate(), self.cols[2].truncate())
    }

    /// Transform a point (`w = 1`), no perspective divide.
    #[inline]
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        (self.cols[0] * p.x + self.cols[1] * p.y + self.cols[2] * p.z + self.cols[3]).truncate()
    }

    /// Transform a point and perform the perspective divide.
    #[inline]
    pub fn project_point(&self, p: Vec3) -> Vec3 {
        (*self * Vec4::from_vec3(p, 1.0)).project()
    }

    /// Transform a direction (`w = 0`).
    #[inline]
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        (self.cols[0] * v.x + self.cols[1] * v.y + self.cols[2] * v.z).truncate()
    }

    pub fn transpose(&self) -> Self {
        Self::from_rows(self.cols[0], self.cols[1], self.cols[2], self.cols[3])
    }

    fn flat(&self) -> [f64; 16] {
        let mut m = [0.0; 16];
        for c in 0..4 {
            for r in 0..4 {
                m[c * 4 + r] = self.cols[c][r];
            }
        }
        m
    }

    fn from_flat(m: [f64; 16]) -> Self {
        Self::from_cols(
            Vec4::new(m[0], m[1], m[2], m[3]),
            Vec4::new(m[4], m[5], m[6], m[7]),
            Vec4::new(m[8], m[9], m[10], m[11]),
            Vec4::new(m[12], m[13], m[14], m[15]),
        )
    }

    /// Adjugate and determinant by 2×2 sub-determinants. Layout-agnostic:
    /// the same formula inverts a flat array whether read as rows or columns.
    fn adjugate_flat(m: &[f64; 16]) -> ([f64; 16], f64) {
        let mut inv = [0.0; 16];
        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
            + m[9] * m[7] * m[14] + m[13] * m[6] * m[11] - m[13] * m[7] * m[10];
        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
            - m[8] * m[7] * m[14] - m[12] * m[6] * m[11] + m[12] * m[7] * m[10];
        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
            + m[8] * m[7] * m[13] + m[12] * m[5] * m[11] - m[12] * m[7] * m[9];
        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
            - m[8] * m[6] * m[13] - m[12] * m[5] * m[10] + m[12] * m[6] * m[9];
        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
            - m[9] * m[3] * m[14] - m[13] * m[2] * m[11] + m[13] * m[3] * m[10];
        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
            + m[8] * m[3] * m[14] + m[12] * m[2] * m[11] - m[12] * m[3] * m[10];
        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
            - m[8] * m[3] * m[13] - m[12] * m[1] * m[11] + m[12] * m[3] * m[9];
        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
            + m[8] * m[2] * m[13] + m[12] * m[1] * m[10] - m[12] * m[2] * m[9];
        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
            + m[5] * m[3] * m[14] + m[13] * m[2] * m[7] - m[13] * m[3] * m[6];
        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
            - m[4] * m[3] * m[14] - m[12] * m[2] * m[7] + m[12] * m[3] * m[6];
        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
            + m[4] * m[3] * m[13] + m[12] * m[1] * m[7] - m[12] * m[3] * m[5];
        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
            - m[4] * m[2] * m[13] - m[12] * m[1] * m[6] + m[12] * m[2] * m[5];
        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
            - m[5] * m[3] * m[10] - m[9] * m[2] * m[7] + m[9] * m[3] * m[6];
        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
            + m[4] * m[3] * m[10] + m[8] * m[2] * m[7] - m[8] * m[3] * m[6];
        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
            - m[4] * m[3] * m[9] - m[8] * m[1] * m[7] + m[8] * m[3] * m[5];
        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
            + m[4] * m[2] * m[9] + m[8] * m[1] * m[6] - m[8] * m[2] * m[5];
        let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        (inv, det)
    }

    pub fn determinant(&self) -> f64 {
        Self::adjugate_flat(&self.flat()).1
    }

    /// General inverse, or `None` when singular.
    pub fn inverse(&self) -> Option<Self> {
        let (mut inv, det) = Self::adjugate_flat(&self.flat());
        if det.abs() < f64::MIN_POSITIVE {
            return None;
        }
        let r = 1.0 / det;
        for v in &mut inv {
            *v *= r;
        }
        Some(Self::from_flat(inv))
    }

    pub fn approx_eq(&self, o: &Mat4, eps: f64) -> bool {
        (0..4).all(|i| self.cols[i].approx_eq(o.cols[i], eps))
    }

    /// Column-major `[[f32; 4]; 4]`, exactly what a WGSL `mat4x4<f32>` wants.
    pub fn to_gpu(&self) -> [[f32; 4]; 4] {
        [self.cols[0].to_gpu(), self.cols[1].to_gpu(), self.cols[2].to_gpu(), self.cols[3].to_gpu()]
    }
}

impl core::ops::Mul<Vec4> for Mat4 {
    type Output = Vec4;
    #[inline]
    fn mul(self, v: Vec4) -> Vec4 {
        self.cols[0] * v.x + self.cols[1] * v.y + self.cols[2] * v.z + self.cols[3] * v.w
    }
}

impl core::ops::Mul for Mat4 {
    type Output = Mat4;
    #[inline]
    fn mul(self, o: Mat4) -> Mat4 {
        Mat4::from_cols(self * o.cols[0], self * o.cols[1], self * o.cols[2], self * o.cols[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{EPS, FRAC_PI_2};

    #[test]
    fn translation_and_scale() {
        let t = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(t.transform_point(Vec3::ZERO), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(t.transform_vector(Vec3::X), Vec3::X);
        assert_eq!(t.translation(), Vec3::new(1.0, 2.0, 3.0));
        let s = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(s.transform_point(Vec3::ONE), Vec3::new(2.0, 3.0, 4.0));
        // T * S scales first, then translates.
        assert_eq!((t * s).transform_point(Vec3::ONE), Vec3::new(3.0, 5.0, 7.0));
    }

    #[test]
    fn trs_matches_composition() {
        let t = Vec3::new(1.0, -2.0, 0.5);
        let r = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 1.0).normalize(), 0.8);
        let s = Vec3::new(2.0, 0.5, 3.0);
        let a = Mat4::from_translation_rotation_scale(t, r, s);
        let b = Mat4::from_translation(t) * Mat4::from_quat(r) * Mat4::from_scale(s);
        assert!(a.approx_eq(&b, EPS));
    }

    #[test]
    fn inverse_roundtrip() {
        let m = Mat4::from_translation_rotation_scale(
            Vec3::new(3.0, 1.0, -7.0),
            Quat::from_axis_angle(Vec3::X, 0.4),
            Vec3::new(1.0, 2.0, 0.5),
        );
        let inv = m.inverse().unwrap();
        assert!((m * inv).approx_eq(&Mat4::IDENTITY, EPS));
        assert!((inv * m).approx_eq(&Mat4::IDENTITY, EPS));
        assert!(crate::approx_eq(m.determinant(), 1.0, EPS)); // det(R)=1, det(S)=1*2*0.5
        assert!(Mat4::ZERO.inverse().is_none());
    }

    #[test]
    fn look_at_looks_down_negative_z() {
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let v = Mat4::look_at(eye, Vec3::ZERO, Vec3::Y);
        // The target is straight ahead: on -Z at distance 5.
        assert!(v.transform_point(Vec3::ZERO).approx_eq(Vec3::new(0.0, 0.0, -5.0), EPS));
        // World +Y stays up, world +X stays right (camera on +Z looking back).
        assert!(v.transform_vector(Vec3::Y).approx_eq(Vec3::Y, EPS));
        assert!(v.transform_vector(Vec3::X).approx_eq(Vec3::X, EPS));
        // The eye itself is the origin of view space.
        assert!(v.transform_point(eye).approx_eq(Vec3::ZERO, EPS));
    }

    #[test]
    fn reverse_z_perspective() {
        let near = 0.1;
        let p = Mat4::perspective_infinite_reverse_z(FRAC_PI_2, 1.0, near);
        // A point on the near plane has depth 1.
        let d_near = p.project_point(Vec3::new(0.0, 0.0, -near)).z;
        assert!(crate::approx_eq(d_near, 1.0, EPS));
        // Far away approaches 0, and is monotonically decreasing with distance.
        let d_10 = p.project_point(Vec3::new(0.0, 0.0, -10.0)).z;
        let d_1000 = p.project_point(Vec3::new(0.0, 0.0, -1000.0)).z;
        assert!(d_10 > d_1000 && d_1000 > 0.0);
        assert!(crate::approx_eq(d_10, near / 10.0, EPS));
        // With a 90° fov and aspect 1, x = -z lands on the right clip edge.
        let edge = p.project_point(Vec3::new(4.0, 0.0, -4.0));
        assert!(crate::approx_eq(edge.x, 1.0, EPS));
    }

    #[test]
    fn reverse_z_orthographic() {
        let o = Mat4::orthographic_reverse_z(-2.0, 2.0, -1.0, 1.0, 0.5, 10.5);
        assert!(o.project_point(Vec3::new(2.0, 1.0, -0.5)).approx_eq(Vec3::new(1.0, 1.0, 1.0), EPS));
        assert!(o.project_point(Vec3::new(-2.0, -1.0, -10.5)).approx_eq(Vec3::new(-1.0, -1.0, 0.0), EPS));
        assert!(crate::approx_eq(o.project_point(Vec3::new(0.0, 0.0, -5.5)).z, 0.5, EPS));
    }

    #[test]
    fn gpu_layout_is_column_major() {
        let m = Mat4::from_translation(Vec3::new(7.0, 8.0, 9.0));
        let g = m.to_gpu();
        assert_eq!(g[3], [7.0f32, 8.0, 9.0, 1.0]);
        assert_eq!(m.row(0), Vec4::new(1.0, 0.0, 0.0, 7.0));
    }
}
