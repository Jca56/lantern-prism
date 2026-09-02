//! Theme: colors and logical sizes, described with `props!` so the
//! Preferences editor edits it live. Sizes are logical pixels in multiples of
//! five; [`Metrics`] is the theme scaled to physical pixels for one frame.
//!
//! The look: near-black panels with a little vertical gradient, raised
//! controls (light top edge, dark bottom edge), recessed fields, a bright
//! amber accent for "on", blue for selection, and a saturated color per
//! object kind so they carry against the dark ground.
//! `gradient` is the depth knob; zero is flat.

use prism_math::Color;
use prism_props::props;

props! {
    /// Colors and sizes of the editor UI.
    pub struct Theme {
        /// Window background, title bar, gaps between areas.
        pub bg: Color = Color::hex(0x0F0F11) => { id: 1 },
        /// Area header bars (shaded by `gradient`).
        pub header: Color = Color::hex(0x2B2B2F) => { id: 2 },
        /// Area bodies.
        pub panel: Color = Color::hex(0x1B1B1E) => { id: 3 },
        /// Raised controls: buttons, tabs, thumbs.
        pub widget: Color = Color::hex(0x37373C) => { id: 4 },
        /// Recessed controls: fields, sliders tracks, the viewport well.
        pub field: Color = Color::hex(0x131315) => { id: 5 },
        pub text: Color = Color::hex(0xF2F2F4) => { id: 6 },
        pub text_dim: Color = Color::hex(0xA0A0A8) => { id: 7 },
        /// "On": active tabs, checked toggles, slider fill.
        pub accent: Color = Color::hex(0xFFB733) => { id: 8 },
        /// Text on top of `accent`.
        pub accent_text: Color = Color::hex(0x1A1508) => { id: 9 },
        /// Selected rows and items.
        pub selection: Color = Color::hex(0x4A82DC) => { id: 10 },
        pub selection_text: Color = Color::hex(0xFFFFFF) => { id: 11 },
        /// Outline of the focused area and focused field.
        pub focus: Color = Color::hex(0x6EA6FF) => { id: 12 },
        /// The close button while hovered.
        pub close: Color = Color::hex(0xE0473A) => { id: 13 },
        /// Dark lines: panel edges, control outlines.
        pub border_dark: Color = Color::hex(0x060608) => { id: 14 },
        /// Light lines: bevels, etched separators.
        pub border_light: Color = Color::hex(0x4E4E55) => { id: 15 },
        /// Object kind colors (outliner strips, later viewport tints).
        pub mesh_color: Color = Color::hex(0xFF9A3C) => { id: 16 },
        pub light_color: Color = Color::hex(0xFFD84F) => { id: 17 },
        pub camera_color: Color = Color::hex(0x5CA3F5) => { id: 18 },
        pub empty_color: Color = Color::hex(0xA8A8B0) => { id: 19 },
        /// Strength of the vertical shading on headers and controls. 0 = flat.
        pub gradient: f64 = 0.12 => { id: 20, hard: 0.0..=0.4, subtype: Factor },

        /// Body text size.
        pub text_size: f64 = 25.0 => { id: 30, hard: 10.0..=80.0, step: 5.0, subtype: Pixels },
        pub heading_size: f64 = 30.0 => { id: 31, hard: 10.0..=100.0, step: 5.0, subtype: Pixels },
        pub header_height: f64 = 45.0 => { id: 32, hard: 20.0..=120.0, step: 5.0, subtype: Pixels },
        /// Height of buttons, fields, sliders.
        pub widget_height: f64 = 45.0 => { id: 33, hard: 20.0..=120.0, step: 5.0, subtype: Pixels },
        /// Inner padding of widgets and panels.
        pub padding: f64 = 10.0 => { id: 34, hard: 0.0..=40.0, step: 5.0, subtype: Pixels },
        /// Space between widgets.
        pub gap: f64 = 5.0 => { id: 35, hard: 0.0..=40.0, step: 5.0, subtype: Pixels },
        pub radius: f64 = 5.0 => { id: 36, hard: 0.0..=30.0, step: 5.0, subtype: Pixels },
        /// Width of the label column in property panels.
        pub label_width: f64 = 200.0 => { id: 37, hard: 50.0..=500.0, step: 10.0, subtype: Pixels },
        pub scrollbar_width: f64 = 15.0 => { id: 38, hard: 5.0..=40.0, step: 5.0, subtype: Pixels },
        /// Gap between areas; also the drag handle.
        pub separator: f64 = 5.0 => { id: 39, hard: 0.0..=20.0, step: 5.0, subtype: Pixels },
    }
}

impl Theme {
    /// Lighter end of a shaded control.
    pub fn top(&self, base: Color) -> Color {
        base.scale_rgb(1.0 + self.gradient)
    }

    /// Darker end of a shaded control.
    pub fn bottom(&self, base: Color) -> Color {
        base.scale_rgb(1.0 - self.gradient)
    }

    /// The bright edge along the top of a raised control.
    pub fn highlight(&self, base: Color) -> Color {
        base.scale_rgb(1.0 + self.gradient * 3.0)
    }

    /// The dark edge along the bottom of a raised control (and the inner
    /// shadow at the top of a recessed one).
    pub fn shade(&self, base: Color) -> Color {
        base.scale_rgb(1.0 - self.gradient * 3.5)
    }

    /// The colour of the current mode: blue in Object mode, gold in Edit mode.
    /// Borders (focused area, context menu) and the mode buttons wear it.
    pub fn mode_color(&self, edit: bool) -> Color {
        if edit { self.accent } else { self.focus }
    }

    /// Text or icon ink on top of [`Self::mode_color`].
    pub fn mode_text(&self, edit: bool) -> Color {
        if edit { self.accent_text } else { self.selection_text }
    }

    /// A hovered control: a little brighter.
    pub fn hover(&self, base: Color) -> Color {
        base.scale_rgb(1.14)
    }

    /// Kind color for an object.
    pub fn kind_color(&self, kind: prism_doc::DataKind) -> Color {
        match kind {
            prism_doc::DataKind::Mesh => self.mesh_color,
            prism_doc::DataKind::Light => self.light_color,
            prism_doc::DataKind::Camera => self.camera_color,
            prism_doc::DataKind::Empty => self.empty_color,
        }
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
    fn theme_is_reflected_and_shades() {
        let info = Theme::info();
        assert_eq!(info.field("accent").unwrap().id, 8);
        assert!(info.field("text_size").unwrap().hard.is_some());
        assert_eq!(info.fields.len(), 30);
        let t = Theme::default();
        assert!(t.top(t.widget).r > t.widget.r && t.bottom(t.widget).r < t.widget.r);
        assert!(t.highlight(t.widget).r > t.top(t.widget).r);
        let flat = Theme { gradient: 0.0, ..Theme::default() };
        assert_eq!(flat.top(flat.widget), flat.widget);
    }
}
