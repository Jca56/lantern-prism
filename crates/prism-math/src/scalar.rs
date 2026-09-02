//! Scalar helpers and the tolerance used by every `approx_eq` in the crate.

pub use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, SQRT_2, TAU};

/// Default tolerance for approximate comparisons. Geometry lives in `f64`, so
/// this is generous for rounding noise and tight for anything real.
pub const EPS: f64 = 1e-9;

/// `true` when `a` and `b` differ by at most `eps`.
#[inline]
pub fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

/// Linear interpolation: `a` at `t = 0`, `b` at `t = 1`. Not clamped.
#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Inverse of [`lerp`]: where does `v` sit between `a` and `b`? Not clamped.
/// Returns `0.0` when `a == b`.
#[inline]
pub fn inv_lerp(a: f64, b: f64, v: f64) -> f64 {
    if a == b { 0.0 } else { (v - a) / (b - a) }
}

/// Map `v` from range `[a0, a1]` onto `[b0, b1]`. Not clamped.
#[inline]
pub fn remap(v: f64, a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    lerp(b0, b1, inv_lerp(a0, a1, v))
}

/// Clamp to `[0, 1]`.
#[inline]
pub fn saturate(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// Hermite smoothstep from `e0` to `e1`.
#[inline]
pub fn smoothstep(e0: f64, e1: f64, v: f64) -> f64 {
    let t = saturate(inv_lerp(e0, e1, v));
    t * t * (3.0 - 2.0 * t)
}

#[inline]
pub fn deg_to_rad(deg: f64) -> f64 {
    deg * (PI / 180.0)
}

#[inline]
pub fn rad_to_deg(rad: f64) -> f64 {
    rad * (180.0 / PI)
}

/// Wrap an angle into `(-PI, PI]`.
#[inline]
pub fn wrap_angle(a: f64) -> f64 {
    let mut r = a % TAU;
    if r > PI {
        r -= TAU;
    } else if r <= -PI {
        r += TAU;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_and_inverse() {
        assert_eq!(lerp(10.0, 20.0, 0.25), 12.5);
        assert_eq!(inv_lerp(10.0, 20.0, 12.5), 0.25);
        assert_eq!(inv_lerp(5.0, 5.0, 7.0), 0.0);
        assert_eq!(remap(0.5, 0.0, 1.0, 100.0, 200.0), 150.0);
    }

    #[test]
    fn angles() {
        assert!(approx_eq(deg_to_rad(180.0), PI, EPS));
        assert!(approx_eq(rad_to_deg(FRAC_PI_2), 90.0, EPS));
        assert!(approx_eq(wrap_angle(3.0 * PI), PI, EPS));
        assert!(approx_eq(wrap_angle(-3.0 * PI), PI, EPS));
        assert!(approx_eq(wrap_angle(0.5), 0.5, EPS));
    }

    #[test]
    fn smooth() {
        assert_eq!(smoothstep(0.0, 1.0, -1.0), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 2.0), 1.0);
        assert_eq!(smoothstep(0.0, 1.0, 0.5), 0.5);
    }
}
