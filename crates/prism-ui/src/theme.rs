//! Theme: colors and logical sizes, described with `props!` so the
//! Preferences editor edits it live. Sizes are logical pixels in multiples of
//! five; [`Metrics`] is the theme scaled to physical pixels for one frame.

use prism_math::Color;
use prism_props::props;

props! {
    /// Colors and sizes of the editor UI.
    pub struct Theme {
        /// Window background, visible between areas.
        pub bg: Color = Color::hex(0x141414) => { id: 1 },
        /// Area header bars.
        pub header: Color = Color::hex(0x1E1E1E) => { id: 2 },
        /// Area bodies.
        pub panel: Color = Color::hex(0x202020) => { id: 3 },
        /// Hairlines and outlines.
        pub border: Color = Color::hex(0x303030) => { id: 4 },
        pub text: Color = Color::hex(0xE6E6E6) => { id: 5 },
        pub text_dim: Color = Color::hex(0x909090) => { id: 6 },
        /// Buttons, fields, sliders at rest.
        pub widget: Color = Color::hex(0x2A2A2A) => { id: 7 },
        pub widget_hover: Color = Color::hex(0x353535) => { id: 8 },
        pub widget_active: Color = Color::hex(0x404040) => { id: 9 },
        /// Filled part of sliders, checked toggles, selected items.
        pub accent: Color = Color::hex(0xC8C8C8) => { id: 10 },
        /// Text drawn on top of `accent`.
        pub accent_text: Color = Color::hex(0x141414) => { id: 11 },
        /// Text selection background.
        pub selection: Color = Color::hex(0x4A4A4A) => { id: 12 },
        /// Border of the area that has keyboard focus.
        pub focus: Color = Color::hex(0xC8C8C8) => { id: 13 },
        /// The close button while hovered.
        pub close: Color = Color::hex(0xB03A3A) => { id: 14 },

        /// Body text size.
        pub text_size: f64 = 25.0 => { id: 20, hard: 10.0..=80.0, step: 5.0, subtype: Pixels },
        pub heading_size: f64 = 30.0 => { id: 21, hard: 10.0..=100.0, step: 5.0, subtype: Pixels },
        pub header_height: f64 = 45.0 => { id: 22, hard: 20.0..=120.0, step: 5.0, subtype: Pixels },
        /// Height of buttons, fields, sliders.
        pub widget_height: f64 = 45.0 => { id: 23, hard: 20.0..=120.0, step: 5.0, subtype: Pixels },
        /// Inner padding of widgets and panels.
        pub padding: f64 = 10.0 => { id: 24, hard: 0.0..=40.0, step: 5.0, subtype: Pixels },
        /// Space between widgets.
        pub gap: f64 = 5.0 => { id: 25, hard: 0.0..=40.0, step: 5.0, subtype: Pixels },
        pub radius: f64 = 5.0 => { id: 26, hard: 0.0..=30.0, step: 5.0, subtype: Pixels },
        /// Width of the label column in property panels.
        pub label_width: f64 = 200.0 => { id: 27, hard: 50.0..=500.0, step: 10.0, subtype: Pixels },
        pub scrollbar_width: f64 = 15.0 => { id: 28, hard: 5.0..=40.0, step: 5.0, subtype: Pixels },
        /// Gap between areas; also the drag handle.
        pub separator: f64 = 5.0 => { id: 29, hard: 0.0..=20.0, step: 5.0, subtype: Pixels },
    }
}

/// The theme's sizes in physical pixels for the current scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub scale: f64,
    pub text_size: f32,
    pub heading_size: f32,
    pub header_h: f64,
    pub widget_h: f64,
    pub pad: f64,
    pub gap: f64,
    pub radius: f64,
    pub border: f64,
    pub label_w: f64,
    pub scrollbar_w: f64,
    pub sep: f64,
    /// Half-width of the separator's grab zone on each side of the gap.
    pub sep_grab: f64,
    /// Width of the focused-area outline.
    pub focus_border: f64,
}

impl Theme {
    /// Scale to physical pixels. `scale` is window scale × UI scale.
    pub fn metrics(&self, scale: f64) -> Metrics {
        let px = |v: f64| (v * scale).round().max(1.0);
        Metrics {
            scale,
            text_size: (self.text_size * scale).round().max(4.0) as f32,
            heading_size: (self.heading_size * scale).round().max(4.0) as f32,
            header_h: px(self.header_height),
            widget_h: px(self.widget_height),
            pad: (self.padding * scale).round(),
            gap: (self.gap * scale).round(),
            radius: (self.radius * scale).round(),
            border: px(1.0),
            label_w: px(self.label_width),
            scrollbar_w: px(self.scrollbar_width),
            sep: (self.separator * scale).round(),
            sep_grab: px(5.0),
            focus_border: px(2.0),
        }
    }
}

impl Metrics {
    /// Round a logical size to physical pixels.
    #[inline]
    pub fn px(&self, logical: f64) -> f64 {
        (logical * self.scale).round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_props::ReflectStatic;

    #[test]
    fn metrics_scale_and_round() {
        let t = Theme::default();
        let m = t.metrics(1.4);
        assert_eq!(m.widget_h, 63.0);
        assert_eq!(m.text_size, 35.0);
        assert_eq!(m.border, 1.0);
        assert_eq!(m.px(10.0), 14.0);
        let m1 = t.metrics(1.0);
        assert_eq!(m1.header_h, 45.0);
        assert_eq!(m1.gap, 5.0);
    }

    #[test]
    fn theme_is_reflected() {
        let info = Theme::info();
        assert_eq!(info.field("accent").unwrap().id, 10);
        assert!(info.field("text_size").unwrap().hard.is_some());
        assert_eq!(info.fields.len(), 24);
    }
}
