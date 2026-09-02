//! Unit quaternion rotation. `(x, y, z)` is the vector part, `w` the scalar.

use crate::{Mat3, Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Quat {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat::new(0.0, 0.0, 0.0, 1.0);

    #[inline]
    pub const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }

    /// Rotation about unit `axis` by `angle` radians.
    pub fn from_axis_angle(axis: Vec3, angle: f64) -> Self {
        let (s, c) = (angle * 0.5).sin_cos();
        Self::new(axis.x * s, axis.y * s, axis.z * s, c)
    }

    pub fn from_rotation_x(a: f64) -> Self {
        let (s, c) = (a * 0.5).sin_cos();
        Self::new(s, 0.0, 0.0, c)
    }

    pub fn from_rotation_y(a: f64) -> Self {
        let (s, c) = (a * 0.5).sin_cos();
        Self::new(0.0, s, 0.0, c)
    }

    pub fn from_rotation_z(a: f64) -> Self {
        let (s, c) = (a * 0.5).sin_cos();
        Self::new(0.0, 0.0, s, c)
    }

    /// Euler angles applied X first, then Y, then Z about the fixed world
    /// axes (`Rz * Ry * Rx`). This is the "XYZ Euler" a properties panel shows.
    pub fn from_euler_xyz(x: f64, y: f64, z: f64) -> Self {
        Self::from_rotation_z(z) * Self::from_rotation_y(y) * Self::from_rotation_x(x)
    }

    /// Inverse of [`Self::from_euler_xyz`]. Returns `(x, y, z)`; at gimbal
    /// lock (`|y| = 90°`) `x` is reported as `0`.
    pub fn to_euler_xyz(self) -> (f64, f64, f64) {
        let m = self.to_mat3();
        let sy = -m.at(2, 0);
        if sy.abs() < 1.0 - 1e-12 {
            let y = sy.asin();
            let x = m.at(2, 1).atan2(m.at(2, 2));
            let z = m.at(1, 0).atan2(m.at(0, 0));
            (x, y, z)
        } else {
            let y = if sy > 0.0 { crate::FRAC_PI_2 } else { -crate::FRAC_PI_2 };
            let z = (-m.at(0, 1)).atan2(m.at(1, 1));
            (0.0, y, z)
        }
    }

    /// The shortest rotation taking unit vector `from` onto unit vector `to`.
    pub fn from_rotation_arc(from: Vec3, to: Vec3) -> Self {
        let d = from.dot(to);
        if d < -1.0 + 1e-12 {
            // Opposite vectors: 180° about anything perpendicular.
            let axis = from.any_orthonormal();
            return Self::new(axis.x, axis.y, axis.z, 0.0);
        }
        let c = from.cross(to);
        Self::new(c.x, c.y, c.z, 1.0 + d).normalize()
    }

    /// From a pure rotation matrix (Shepperd's method).
    pub fn from_mat3(m: &Mat3) -> Self {
        let (m00, m11, m22) = (m.at(0, 0), m.at(1, 1), m.at(2, 2));
        let trace = m00 + m11 + m22;
        if trace > 0.0 {
            let s = (trace + 1.0).sqrt() * 2.0;
            Self::new(
                (m.at(2, 1) - m.at(1, 2)) / s,
                (m.at(0, 2) - m.at(2, 0)) / s,
                (m.at(1, 0) - m.at(0, 1)) / s,
                0.25 * s,
            )
        } else if m00 > m11 && m00 > m22 {
            let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
            Self::new(
                0.25 * s,
                (m.at(0, 1) + m.at(1, 0)) / s,
                (m.at(0, 2) + m.at(2, 0)) / s,
                (m.at(2, 1) - m.at(1, 2)) / s,
            )
        } else if m11 > m22 {
            let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
            Self::new(
                (m.at(0, 1) + m.at(1, 0)) / s,
                0.25 * s,
                (m.at(1, 2) + m.at(2, 1)) / s,
                (m.at(0, 2) - m.at(2, 0)) / s,
            )
        } else {
            let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
            Self::new(
                (m.at(0, 2) + m.at(2, 0)) / s,
                (m.at(1, 2) + m.at(2, 1)) / s,
                0.25 * s,
                (m.at(1, 0) - m.at(0, 1)) / s,
            )
        }
    }

    pub fn to_mat3(self) -> Mat3 {
        let Quat { x, y, z, w } = self;
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        Mat3::from_cols(
            Vec3::new(1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy)),
            Vec3::new(2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx)),
            Vec3::new(2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy)),
        )
    }

    /// `(axis, angle)` with angle in `[0, PI]`. Identity yields `(+Y, 0)`.
    pub fn to_axis_angle(self) -> (Vec3, f64) {
        let q = if self.w < 0.0 { -self } else { self };
        let angle = 2.0 * q.w.clamp(-1.0, 1.0).acos();
        let s = (1.0 - q.w * q.w).sqrt();
        if s < 1e-12 {
            (Vec3::Y, 0.0)
        } else {
            (Vec3::new(q.x, q.y, q.z) / s, angle)
        }
    }

    #[inline]
    pub fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    #[inline]
    pub fn dot(self, o: Quat) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w
    }

    #[inline]
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    #[inline]
    pub fn normalize(self) -> Quat {
        let r = 1.0 / self.length();
        Quat::new(self.x * r, self.y * r, self.z * r, self.w * r)
    }

    /// For a unit quaternion this is the inverse.
    #[inline]
    pub fn conjugate(self) -> Quat {
        Quat::new(-self.x, -self.y, -self.z, self.w)
    }

    #[inline]
    pub fn inverse(self) -> Quat {
        let r = 1.0 / self.dot(self);
        let c = self.conjugate();
        Quat::new(c.x * r, c.y * r, c.z * r, c.w * r)
    }

    /// Rotation angle in `[0, PI]`.
    pub fn angle(self) -> f64 {
        self.to_axis_angle().1
    }

    /// Normalized linear interpolation, shortest path. Cheap, non-constant speed.
    pub fn nlerp(self, mut o: Quat, t: f64) -> Quat {
        if self.dot(o) < 0.0 {
            o = -o;
        }
        Quat::new(
            self.x + (o.x - self.x) * t,
            self.y + (o.y - self.y) * t,
            self.z + (o.z - self.z) * t,
            self.w + (o.w - self.w) * t,
        )
        .normalize()
    }

    /// Spherical interpolation, shortest path, constant angular speed.
    pub fn slerp(self, mut o: Quat, t: f64) -> Quat {
        let mut d = self.dot(o);
        if d < 0.0 {
            o = -o;
            d = -d;
        }
        if d > 1.0 - 1e-9 {
            return self.nlerp(o, t);
        }
        let theta = d.acos();
        let s = theta.sin();
        let a = ((1.0 - t) * theta).sin() / s;
        let b = (t * theta).sin() / s;
        Quat::new(
            self.x * a + o.x * b,
            self.y * a + o.y * b,
            self.z * a + o.z * b,
            self.w * a + o.w * b,
        )
    }

    /// `true` when this and `o` describe the same rotation (q and -q are equal).
    pub fn approx_eq(self, o: Quat, eps: f64) -> bool {
        let same = (self.x - o.x).abs() <= eps
            && (self.y - o.y).abs() <= eps
            && (self.z - o.z).abs() <= eps
            && (self.w - o.w).abs() <= eps;
        let neg = (self.x + o.x).abs() <= eps
            && (self.y + o.y).abs() <= eps
            && (self.z + o.z).abs() <= eps
            && (self.w + o.w).abs() <= eps;
        same || neg
    }

    #[inline]
    pub fn to_vec4(self) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, self.w)
    }

    #[inline]
    pub fn to_gpu(self) -> [f32; 4] {
        [self.x as f32, self.y as f32, self.z as f32, self.w as f32]
    }
}

impl core::ops::Mul for Quat {
    type Output = Quat;
    /// Hamilton product: `(a * b)` rotates by `b` first, then `a`.
    #[inline]
    fn mul(self, o: Quat) -> Quat {
        Quat::new(
            self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
            self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
        )
    }
}

impl core::ops::Mul<Vec3> for Quat {
    type Output = Vec3;
    /// Rotate a vector.
    #[inline]
    fn mul(self, v: Vec3) -> Vec3 {
        let u = self.xyz();
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }
}

impl core::ops::Neg for Quat {
    type Output = Quat;
    #[inline]
    fn neg(self) -> Quat {
        Quat::new(-self.x, -self.y, -self.z, -self.w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{EPS, FRAC_PI_2, PI};

    #[test]
    fn rotates_like_the_matrix() {
        let axis = Vec3::new(1.0, -2.0, 0.5).normalize();
        let q = Quat::from_axis_angle(axis, 1.3);
        let m = Mat3::from_axis_angle(axis, 1.3);
        let v = Vec3::new(0.2, 3.0, -1.0);
        assert!((q * v).approx_eq(m * v, EPS));
        assert!(q.to_mat3().approx_eq(&m, EPS));
        assert!(Quat::from_mat3(&m).approx_eq(q, EPS));
    }

    #[test]
    fn basics_are_right_handed() {
        assert!((Quat::from_rotation_z(FRAC_PI_2) * Vec3::X).approx_eq(Vec3::Y, EPS));
        assert!((Quat::from_rotation_x(FRAC_PI_2) * Vec3::Y).approx_eq(Vec3::Z, EPS));
        assert!((Quat::from_rotation_y(FRAC_PI_2) * Vec3::Z).approx_eq(Vec3::X, EPS));
    }

    #[test]
    fn composition_order() {
        let a = Quat::from_rotation_x(0.4);
        let b = Quat::from_rotation_y(-1.1);
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert!(((a * b) * v).approx_eq(a * (b * v), EPS));
        assert!((a * a.inverse()).approx_eq(Quat::IDENTITY, EPS));
        assert!((a.conjugate() * (a * v)).approx_eq(v, EPS));
    }

    #[test]
    fn euler_roundtrip() {
        for (x, y, z) in [(0.3, -0.7, 1.9), (-2.5, 1.2, 0.1), (0.0, 0.0, 0.0), (1.0, 1.5, -1.0)] {
            let q = Quat::from_euler_xyz(x, y, z);
            let (rx, ry, rz) = q.to_euler_xyz();
            assert!(Quat::from_euler_xyz(rx, ry, rz).approx_eq(q, 1e-9), "{x} {y} {z}");
            assert!(crate::approx_eq(rx, x, 1e-9) && crate::approx_eq(ry, y, 1e-9) && crate::approx_eq(rz, z, 1e-9));
        }
        // Gimbal lock does not explode and still reproduces the rotation.
        let q = Quat::from_euler_xyz(0.3, FRAC_PI_2, 0.5);
        let (rx, ry, rz) = q.to_euler_xyz();
        assert!(Quat::from_euler_xyz(rx, ry, rz).approx_eq(q, 1e-9));
    }

    #[test]
    fn rotation_arc() {
        let q = Quat::from_rotation_arc(Vec3::X, Vec3::Y);
        assert!((q * Vec3::X).approx_eq(Vec3::Y, EPS));
        let q = Quat::from_rotation_arc(Vec3::X, Vec3::NEG_X);
        assert!((q * Vec3::X).approx_eq(Vec3::NEG_X, EPS));
        assert!(crate::approx_eq(q.angle(), PI, EPS));
        let q = Quat::from_rotation_arc(Vec3::Z, Vec3::Z);
        assert!(q.approx_eq(Quat::IDENTITY, EPS));
    }

    #[test]
    fn axis_angle_roundtrip() {
        let axis = Vec3::new(0.0, 1.0, 1.0).normalize();
        let (a, ang) = Quat::from_axis_angle(axis, 2.0).to_axis_angle();
        assert!(a.approx_eq(axis, EPS));
        assert!(crate::approx_eq(ang, 2.0, EPS));
        // Negative-w representation gives the same axis and angle.
        let (a2, ang2) = (-Quat::from_axis_angle(axis, 2.0)).to_axis_angle();
        assert!(a2.approx_eq(axis, EPS) && crate::approx_eq(ang2, 2.0, EPS));
        assert_eq!(Quat::IDENTITY.to_axis_angle().1, 0.0);
    }

    #[test]
    fn slerp_endpoints_and_midpoint() {
        let a = Quat::IDENTITY;
        let b = Quat::from_rotation_y(FRAC_PI_2);
        assert!(a.slerp(b, 0.0).approx_eq(a, EPS));
        assert!(a.slerp(b, 1.0).approx_eq(b, EPS));
        let mid = a.slerp(b, 0.5);
        assert!(mid.approx_eq(Quat::from_rotation_y(FRAC_PI_4_LOCAL), EPS));
        // Shortest path: interpolating toward -b lands on the same rotation.
        assert!(a.slerp(-b, 0.5).approx_eq(mid, EPS));
        assert!(crate::approx_eq(a.nlerp(b, 0.5).length(), 1.0, EPS));
    }

    const FRAC_PI_4_LOCAL: f64 = FRAC_PI_2 / 2.0;
}
