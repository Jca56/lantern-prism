//! Operator boilerplate shared by the vector types.

/// Implements component-wise `Add/Sub/Mul/Div` (vector–vector and
/// vector–scalar), their `*Assign` forms, `Neg`, `Index`, and `Sum`.
macro_rules! impl_vec_ops {
    ($t:ident { $($f:ident),+ }) => {
        impl core::ops::Add for $t {
            type Output = $t;
            #[inline]
            fn add(self, o: $t) -> $t { $t { $($f: self.$f + o.$f),+ } }
        }
        impl core::ops::Sub for $t {
            type Output = $t;
            #[inline]
            fn sub(self, o: $t) -> $t { $t { $($f: self.$f - o.$f),+ } }
        }
        impl core::ops::Mul for $t {
            type Output = $t;
            #[inline]
            fn mul(self, o: $t) -> $t { $t { $($f: self.$f * o.$f),+ } }
        }
        impl core::ops::Div for $t {
            type Output = $t;
            #[inline]
            fn div(self, o: $t) -> $t { $t { $($f: self.$f / o.$f),+ } }
        }
        impl core::ops::Mul<f64> for $t {
            type Output = $t;
            #[inline]
            fn mul(self, s: f64) -> $t { $t { $($f: self.$f * s),+ } }
        }
        impl core::ops::Mul<$t> for f64 {
            type Output = $t;
            #[inline]
            fn mul(self, v: $t) -> $t { $t { $($f: v.$f * self),+ } }
        }
        impl core::ops::Div<f64> for $t {
            type Output = $t;
            #[inline]
            fn div(self, s: f64) -> $t { let r = 1.0 / s; $t { $($f: self.$f * r),+ } }
        }
        impl core::ops::Neg for $t {
            type Output = $t;
            #[inline]
            fn neg(self) -> $t { $t { $($f: -self.$f),+ } }
        }
        impl core::ops::AddAssign for $t {
            #[inline]
            fn add_assign(&mut self, o: $t) { $(self.$f += o.$f;)+ }
        }
        impl core::ops::SubAssign for $t {
            #[inline]
            fn sub_assign(&mut self, o: $t) { $(self.$f -= o.$f;)+ }
        }
        impl core::ops::MulAssign for $t {
            #[inline]
            fn mul_assign(&mut self, o: $t) { $(self.$f *= o.$f;)+ }
        }
        impl core::ops::MulAssign<f64> for $t {
            #[inline]
            fn mul_assign(&mut self, s: f64) { $(self.$f *= s;)+ }
        }
        impl core::ops::DivAssign<f64> for $t {
            #[inline]
            fn div_assign(&mut self, s: f64) { let r = 1.0 / s; $(self.$f *= r;)+ }
        }
        impl core::iter::Sum for $t {
            fn sum<I: Iterator<Item = $t>>(iter: I) -> $t {
                iter.fold($t::ZERO, |a, b| a + b)
            }
        }
        impl $t {
            /// Every component equal to `v`.
            #[inline]
            pub const fn splat(v: f64) -> $t { $t { $($f: v),+ } }
            /// Component-wise minimum.
            #[inline]
            pub fn min(self, o: $t) -> $t { $t { $($f: self.$f.min(o.$f)),+ } }
            /// Component-wise maximum.
            #[inline]
            pub fn max(self, o: $t) -> $t { $t { $($f: self.$f.max(o.$f)),+ } }
            /// Component-wise absolute value.
            #[inline]
            pub fn abs(self) -> $t { $t { $($f: self.$f.abs()),+ } }
            #[inline]
            pub fn floor(self) -> $t { $t { $($f: self.$f.floor()),+ } }
            #[inline]
            pub fn ceil(self) -> $t { $t { $($f: self.$f.ceil()),+ } }
            #[inline]
            pub fn round(self) -> $t { $t { $($f: self.$f.round()),+ } }
            /// Component-wise clamp between `lo` and `hi`.
            #[inline]
            pub fn clamp(self, lo: $t, hi: $t) -> $t { self.max(lo).min(hi) }
            /// Smallest component.
            #[inline]
            pub fn min_element(self) -> f64 { let mut m = f64::INFINITY; $(m = m.min(self.$f);)+ m }
            /// Largest component.
            #[inline]
            pub fn max_element(self) -> f64 { let mut m = f64::NEG_INFINITY; $(m = m.max(self.$f);)+ m }
            /// Dot product.
            #[inline]
            pub fn dot(self, o: $t) -> f64 { let mut d = 0.0; $(d += self.$f * o.$f;)+ d }
            #[inline]
            pub fn length_squared(self) -> f64 { self.dot(self) }
            #[inline]
            pub fn length(self) -> f64 { self.dot(self).sqrt() }
            #[inline]
            pub fn distance(self, o: $t) -> f64 { (self - o).length() }
            #[inline]
            pub fn distance_squared(self, o: $t) -> f64 { (self - o).length_squared() }
            /// Unit vector in the same direction. Debug-asserts on a zero vector;
            /// use [`Self::try_normalize`] when the input may be degenerate.
            #[inline]
            pub fn normalize(self) -> $t {
                let l = self.length();
                debug_assert!(l > 0.0, "normalize() of a zero-length vector");
                self * (1.0 / l)
            }
            /// `Some(unit)` unless the length is below `EPS`.
            #[inline]
            pub fn try_normalize(self) -> Option<$t> {
                let l = self.length();
                if l > $crate::scalar::EPS { Some(self * (1.0 / l)) } else { None }
            }
            /// Normalize, or return `fallback` for a degenerate input.
            #[inline]
            pub fn normalize_or(self, fallback: $t) -> $t { self.try_normalize().unwrap_or(fallback) }
            /// Normalize, or return `ZERO` for a degenerate input.
            #[inline]
            pub fn normalize_or_zero(self) -> $t { self.normalize_or($t::ZERO) }
            /// Linear interpolation, not clamped.
            #[inline]
            pub fn lerp(self, o: $t, t: f64) -> $t { self + (o - self) * t }
            /// Projection of `self` onto `onto` (which need not be unit length).
            #[inline]
            pub fn project_onto(self, onto: $t) -> $t { onto * (self.dot(onto) / onto.dot(onto)) }
            /// Reflect about a unit `normal`.
            #[inline]
            pub fn reflect(self, normal: $t) -> $t { self - normal * (2.0 * self.dot(normal)) }
            /// `true` when every component is within `eps` of `o`'s.
            #[inline]
            pub fn approx_eq(self, o: $t, eps: f64) -> bool {
                true $(&& (self.$f - o.$f).abs() <= eps)+
            }
            /// `true` when every component is finite.
            #[inline]
            pub fn is_finite(self) -> bool { true $(&& self.$f.is_finite())+ }
        }
    };
}

pub(crate) use impl_vec_ops;
