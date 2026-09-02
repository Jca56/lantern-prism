//! RGBA color with `f64` channels in `[0, 1]`. Values are **sRGB-encoded**
//! unless a name says `linear`; the GPU side decides how to interpret them.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

    #[inline]
    pub const fn rgba(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// `0xRRGGBB`, opaque.
    #[inline]
    pub const fn hex(rgb: u32) -> Self {
        Self::from_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
    }

    /// `0xRRGGBBAA`.
    #[inline]
    pub const fn hexa(rgba: u32) -> Self {
        Self::from_u8((rgba >> 24) as u8, (rgba >> 16) as u8, (rgba >> 8) as u8, rgba as u8)
    }

    #[inline]
    pub const fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, a as f64 / 255.0)
    }

    #[inline]
    pub fn to_u8(self) -> [u8; 4] {
        let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.r), q(self.g), q(self.b), q(self.a)]
    }

    #[inline]
    pub const fn with_alpha(self, a: f64) -> Self {
        Self { a, ..self }
    }

    /// Scale alpha by `f`.
    #[inline]
    pub fn fade(self, f: f64) -> Self {
        self.with_alpha(self.a * f)
    }

    /// Channel-wise interpolation, not clamped.
    pub fn lerp(self, o: Color, t: f64) -> Color {
        Color::rgba(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
            self.a + (o.a - self.a) * t,
        )
    }

    /// Brighten (`f > 1`) or darken (`f < 1`) the RGB channels, clamped.
    pub fn scale_rgb(self, f: f64) -> Color {
        Color::rgba(
            (self.r * f).clamp(0.0, 1.0),
            (self.g * f).clamp(0.0, 1.0),
            (self.b * f).clamp(0.0, 1.0),
            self.a,
        )
    }

    pub fn srgb_to_linear(c: f64) -> f64 {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }

    pub fn linear_to_srgb(c: f64) -> f64 {
        if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    }

    /// Decode sRGB channels to linear light. Alpha is untouched.
    pub fn to_linear(self) -> Color {
        Color::rgba(
            Self::srgb_to_linear(self.r),
            Self::srgb_to_linear(self.g),
            Self::srgb_to_linear(self.b),
            self.a,
        )
    }

    /// Encode linear-light channels to sRGB. Alpha is untouched.
    pub fn to_srgb(self) -> Color {
        Color::rgba(
            Self::linear_to_srgb(self.r),
            Self::linear_to_srgb(self.g),
            Self::linear_to_srgb(self.b),
            self.a,
        )
    }

    /// RGB multiplied by alpha (what a premultiplied blend state wants).
    pub fn premultiplied(self) -> Color {
        Color::rgba(self.r * self.a, self.g * self.a, self.b * self.a, self.a)
    }

    /// Relative luminance of the *linear* channels (Rec. 709 weights).
    pub fn luminance_linear(self) -> f64 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    #[inline]
    pub fn to_gpu(self) -> [f32; 4] {
        [self.r as f32, self.g as f32, self.b as f32, self.a as f32]
    }

    pub fn approx_eq(self, o: Color, eps: f64) -> bool {
        (self.r - o.r).abs() <= eps
            && (self.g - o.g).abs() <= eps
            && (self.b - o.b).abs() <= eps
            && (self.a - o.a).abs() <= eps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Color::hex(0x1A2B3C);
        assert_eq!(c.to_u8(), [0x1A, 0x2B, 0x3C, 0xFF]);
        let c = Color::hexa(0x1A2B3C80);
        assert_eq!(c.to_u8(), [0x1A, 0x2B, 0x3C, 0x80]);
        assert_eq!(Color::WHITE.to_u8(), [255; 4]);
        assert_eq!(Color::TRANSPARENT.to_u8(), [0; 4]);
    }

    #[test]
    fn transfer_functions() {
        for v in [0.0, 0.001, 0.04, 0.2, 0.5, 0.9, 1.0] {
            let back = Color::linear_to_srgb(Color::srgb_to_linear(v));
            assert!((back - v).abs() < 1e-12, "{v}");
        }
        // Middle grey: sRGB 0.5 is about 21.4% linear light.
        assert!((Color::srgb_to_linear(0.5) - 0.214041).abs() < 1e-5);
        assert!(Color::rgb(0.5, 0.5, 0.5).to_linear().to_srgb().approx_eq(Color::rgb(0.5, 0.5, 0.5), 1e-12));
    }

    #[test]
    fn blending_helpers() {
        let c = Color::rgba(1.0, 0.5, 0.0, 0.5);
        assert_eq!(c.premultiplied(), Color::rgba(0.5, 0.25, 0.0, 0.5));
        assert_eq!(c.fade(0.5).a, 0.25);
        assert_eq!(Color::BLACK.lerp(Color::WHITE, 0.25), Color::rgb(0.25, 0.25, 0.25));
        assert_eq!(Color::rgb(0.5, 0.5, 0.5).scale_rgb(4.0), Color::WHITE);
        assert_eq!(Color::WHITE.luminance_linear(), 1.0);
        assert_eq!(c.to_gpu(), [1.0f32, 0.5, 0.0, 0.5]);
    }
}
