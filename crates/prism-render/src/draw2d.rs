//! The 2D draw list: what `prism-ui` emits and `Pass2d` draws.
//!
//! Everything is a quad. Six vertices each, no index buffer (yet). Colors are
//! converted from sRGB to linear at push time; clipping is per-vertex so the
//! whole list is one draw call regardless of how many clip rects it uses.
//! Layers order the output (popups above panels) without sorting.

use prism_core::impl_pod;
use prism_math::{Color, Rect, Vec2};
use prism_text::GlyphQuad;

/// Vertex format of the 2D pass. Must match `shaders/ui.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Vertex2d {
    pub pos: [f32; 2],
    /// Texel-space atlas coordinates (glyph mode only).
    pub uv: [f32; 2],
    /// Linear RGB, straight alpha.
    pub color: [f32; 4],
    /// SDF rect: center x, center y, half width, half height.
    pub rect: [f32; 4],
    /// radius, mode, stroke width, unused.
    pub params: [f32; 4],
    /// x0, y0, x1, y1 clip in pixels.
    pub clip: [f32; 4],
}
impl_pod!(Vertex2d);

const MODE_FILL: f32 = 0.0;
const MODE_GLYPH: f32 = 1.0;
const MODE_STROKE: f32 = 2.0;
const MODE_PLAIN: f32 = 3.0;
const MODE_SHADOW: f32 = 4.0;

/// "No clip": anything on screen passes.
const OPEN_CLIP: [f32; 4] = [-1.0e6, -1.0e6, 1.0e6, 1.0e6];

#[derive(Default)]
pub struct DrawList {
    layers: Vec<Vec<Vertex2d>>,
    layer: usize,
    clips: Vec<Rect>,
}

impl DrawList {
    pub fn new() -> Self {
        Self { layers: vec![Vec::new()], layer: 0, clips: Vec::new() }
    }

    /// Forget everything; keep allocations.
    pub fn clear(&mut self) {
        for l in &mut self.layers {
            l.clear();
        }
        self.layer = 0;
        self.clips.clear();
    }

    /// Draw into layer `n` (higher layers draw later). Layers are created on demand.
    pub fn set_layer(&mut self, n: usize) {
        while self.layers.len() <= n {
            self.layers.push(Vec::new());
        }
        self.layer = n;
    }

    pub fn layer(&self) -> usize {
        self.layer
    }

    /// Restrict drawing to `r` (intersected with the current clip).
    pub fn push_clip(&mut self, r: Rect) {
        let r = match self.clips.last() {
            Some(c) => c.intersection(&r),
            None => r,
        };
        self.clips.push(r);
    }

    /// Clip to `r` regardless of the enclosing clips (popups escape their region).
    pub fn push_clip_absolute(&mut self, r: Rect) {
        self.clips.push(r);
    }

    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }

    pub fn current_clip(&self) -> Option<Rect> {
        self.clips.last().copied()
    }

    fn clip4(&self) -> [f32; 4] {
        match self.clips.last() {
            Some(c) => [c.min.x as f32, c.min.y as f32, c.max.x as f32, c.max.y as f32],
            None => OPEN_CLIP,
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.layers.iter().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.vertex_count() == 0
    }

    /// All vertices, layer by layer, ready for upload.
    pub fn vertices(&self) -> impl Iterator<Item = &Vertex2d> {
        self.layers.iter().flatten()
    }

    fn push_quad(&mut self, corners: [Vec2; 4], uvs: [[f32; 2]; 4], color: [f32; 4], rect: [f32; 4], params: [f32; 4]) {
        self.push_quad_colors(corners, uvs, [color; 4], rect, params);
    }

    /// Corners are TL, TR, BR, BL, each with its own color (linear).
    fn push_quad_colors(&mut self, corners: [Vec2; 4], uvs: [[f32; 2]; 4], colors: [[f32; 4]; 4], rect: [f32; 4], params: [f32; 4]) {
        let clip = self.clip4();
        let v = |i: usize| Vertex2d { pos: corners[i].to_gpu(), uv: uvs[i], color: colors[i], rect, params, clip };
        // Corners are TL, TR, BR, BL. Two triangles: (TL, BL, BR) + (TL, BR, TR).
        let out = &mut self.layers[self.layer];
        out.extend_from_slice(&[v(0), v(3), v(2), v(0), v(2), v(1)]);
    }

    fn rect_quad(&mut self, r: Rect, color: Color, params: [f32; 4]) {
        if r.is_empty() || color.a <= 0.0 {
            return;
        }
        let c = r.center();
        let h = r.size() * 0.5;
        let corners = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)];
        self.push_quad(
            corners,
            [[0.0; 2]; 4],
            color.to_linear().to_gpu(),
            [c.x as f32, c.y as f32, h.x as f32, h.y as f32],
            params,
        );
    }

    fn gradient_quad(&mut self, r: Rect, top: Color, bottom: Color, params: [f32; 4]) {
        if r.is_empty() || (top.a <= 0.0 && bottom.a <= 0.0) {
            return;
        }
        let c = r.center();
        let h = r.size() * 0.5;
        let corners = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)];
        let t = top.to_linear().to_gpu();
        let b = bottom.to_linear().to_gpu();
        self.push_quad_colors(corners, [[0.0; 2]; 4], [t, t, b, b], [c.x as f32, c.y as f32, h.x as f32, h.y as f32], params);
    }

    /// Axis-aligned filled rectangle with hard edges.
    pub fn rect(&mut self, r: Rect, color: Color) {
        self.rect_quad(r, color, [0.0, MODE_PLAIN, 0.0, 0.0]);
    }

    /// Hard-edged rectangle shaded from `top` to `bottom`.
    pub fn rect_gradient(&mut self, r: Rect, top: Color, bottom: Color) {
        self.gradient_quad(r, top, bottom, [0.0, MODE_PLAIN, 0.0, 0.0]);
    }

    /// Rounded rectangle shaded from `top` to `bottom`.
    pub fn rounded_rect_gradient(&mut self, r: Rect, radius: f64, top: Color, bottom: Color) {
        let radius = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
        self.gradient_quad(r, top, bottom, [radius as f32, MODE_FILL, 0.0, 0.0]);
    }

    /// Soft shadow fading out over `blur` pixels beyond the rounded rect.
    pub fn shadow(&mut self, r: Rect, radius: f64, blur: f64, color: Color) {
        if r.is_empty() || color.a <= 0.0 {
            return;
        }
        let radius = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
        let c = r.center();
        let h = r.size() * 0.5;
        let q = r.expand(blur);
        let corners = [q.min, Vec2::new(q.max.x, q.min.y), q.max, Vec2::new(q.min.x, q.max.y)];
        self.push_quad(
            corners,
            [[0.0; 2]; 4],
            color.to_linear().to_gpu(),
            [c.x as f32, c.y as f32, h.x as f32, h.y as f32],
            [radius as f32, MODE_SHADOW, blur as f32, 0.0],
        );
    }

    /// Filled rectangle with anti-aliased rounded corners.
    pub fn rounded_rect(&mut self, r: Rect, radius: f64, color: Color) {
        let radius = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
        self.rect_quad(r, color, [radius as f32, MODE_FILL, 0.0, 0.0]);
    }

    /// Inner stroke of `width` along the (rounded) rectangle's edge.
    pub fn stroke_rect(&mut self, r: Rect, width: f64, radius: f64, color: Color) {
        let radius = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
        self.rect_quad(r, color, [radius as f32, MODE_STROKE, width as f32, 0.0]);
    }

    /// Line segment as a hard-edged quad of `width`.
    pub fn line(&mut self, a: Vec2, b: Vec2, width: f64, color: Color) {
        let Some(dir) = (b - a).try_normalize() else {
            return;
        };
        let n = dir.perp() * (width * 0.5);
        let corners = [a + n, b + n, b - n, a - n];
        self.push_quad(corners, [[0.0; 2]; 4], color.to_linear().to_gpu(), [0.0; 4], [0.0, MODE_PLAIN, 0.0, 0.0]);
    }

    /// Filled triangle with hard edges.
    pub fn triangle(&mut self, a: Vec2, b: Vec2, c: Vec2, color: Color) {
        if color.a <= 0.0 {
            return;
        }
        let col = color.to_linear().to_gpu();
        let clip = self.clip4();
        let v = |p: Vec2| Vertex2d { pos: p.to_gpu(), uv: [0.0; 2], color: col, rect: [0.0; 4], params: [0.0, MODE_PLAIN, 0.0, 0.0], clip };
        self.layers[self.layer].extend_from_slice(&[v(a), v(b), v(c)]);
    }

    /// Connected segments of `width` with mitred joints (clamped, so sharp
    /// corners do not spike). `closed` joins the last point back to the first.
    pub fn polyline(&mut self, pts: &[Vec2], width: f64, color: Color, closed: bool) {
        let n = pts.len();
        if n < 2 || color.a <= 0.0 {
            return;
        }
        let hw = width * 0.5;
        let dir = |i: usize, j: usize| (pts[j % n] - pts[i % n]).normalize_or_zero();
        // Offset at a point: along the bisector of its two edges, long enough
        // to meet both offset lines, but never more than twice the half width.
        let offset = |i: usize| -> Vec2 {
            let prev = if i == 0 { if closed { dir(n - 1, 0) } else { dir(0, 1) } } else { dir(i - 1, i) };
            let next = if i + 1 == n { if closed { dir(n - 1, 0) } else { dir(n - 2, n - 1) } } else { dir(i, i + 1) };
            let bisector = (prev + next).normalize_or(next).perp();
            let cos_half = bisector.dot(next.perp()).abs().max(0.5);
            bisector * (hw / cos_half)
        };
        let col = color.to_linear().to_gpu();
        let clip = self.clip4();
        let v = |p: Vec2| Vertex2d { pos: p.to_gpu(), uv: [0.0; 2], color: col, rect: [0.0; 4], params: [0.0, MODE_PLAIN, 0.0, 0.0], clip };
        let segs = if closed { n } else { n - 1 };
        let out = &mut self.layers[self.layer];
        for i in 0..segs {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let (oa, ob) = (offset(i), offset((i + 1) % n));
            out.extend_from_slice(&[v(a + oa), v(a - oa), v(b - ob), v(a + oa), v(b - ob), v(b + ob)]);
        }
    }

    /// Horizontal or vertical 1px-style separator, snapped to pixels.
    pub fn hline(&mut self, x0: f64, x1: f64, y: f64, width: f64, color: Color) {
        self.rect(Rect::from_xywh(x0, y, x1 - x0, width), color);
    }

    pub fn vline(&mut self, x: f64, y0: f64, y1: f64, width: f64, color: Color) {
        self.rect(Rect::from_xywh(x, y0, width, y1 - y0), color);
    }

    /// Glyph quads from `prism-text`. Their colors are treated as sRGB and
    /// converted here, like every other color in the list.
    pub fn glyphs(&mut self, quads: &[GlyphQuad]) {
        for q in quads {
            let color = if q.is_color {
                [1.0, 1.0, 1.0, q.color[3]]
            } else {
                Color::rgba(q.color[0] as f64, q.color[1] as f64, q.color[2] as f64, q.color[3] as f64)
                    .to_linear()
                    .to_gpu()
            };
            let (x0, y0, x1, y1) = (q.x as f64, q.y as f64, (q.x + q.w) as f64, (q.y + q.h) as f64);
            let corners = [Vec2::new(x0, y0), Vec2::new(x1, y0), Vec2::new(x1, y1), Vec2::new(x0, y1)];
            let [u0, v0] = q.uv_min;
            let [u1, v1] = q.uv_max;
            self.push_quad(corners, [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], color, [0.0; 4], [0.0, MODE_GLYPH, 0.0, 0.0]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_layout_matches_shader() {
        assert_eq!(size_of::<Vertex2d>(), 80);
        assert_eq!(core::mem::offset_of!(Vertex2d, uv), 8);
        assert_eq!(core::mem::offset_of!(Vertex2d, color), 16);
        assert_eq!(core::mem::offset_of!(Vertex2d, rect), 32);
        assert_eq!(core::mem::offset_of!(Vertex2d, params), 48);
        assert_eq!(core::mem::offset_of!(Vertex2d, clip), 64);
    }

    #[test]
    fn quads_layers_and_clips() {
        let mut d = DrawList::new();
        d.rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::WHITE);
        assert_eq!(d.vertex_count(), 6);
        let v: Vec<_> = d.vertices().collect();
        assert_eq!(v[0].pos, [0.0, 0.0]);
        assert_eq!(v[2].pos, [10.0, 10.0]);
        assert_eq!(v[0].clip, OPEN_CLIP);
        assert_eq!(v[0].params[1], MODE_PLAIN);
        assert_eq!(v[0].rect, [5.0, 5.0, 5.0, 5.0]);

        d.push_clip(Rect::from_xywh(0.0, 0.0, 100.0, 100.0));
        d.push_clip(Rect::from_xywh(50.0, 50.0, 100.0, 100.0));
        assert_eq!(d.current_clip(), Some(Rect::from_xywh(50.0, 50.0, 50.0, 50.0)));
        d.set_layer(2);
        d.rounded_rect(Rect::from_xywh(0.0, 0.0, 10.0, 4.0), 50.0, Color::RED);
        let last = *d.vertices().last().unwrap();
        assert_eq!(last.clip, [50.0, 50.0, 100.0, 100.0]);
        assert_eq!(last.params[0], 2.0, "radius clamps to half the short side");
        assert_eq!(last.params[1], MODE_FILL);
        assert_eq!(last.color, [1.0, 0.0, 0.0, 1.0]);
        d.pop_clip();
        d.pop_clip();
        assert_eq!(d.current_clip(), None);
        // Layer 2 vertices come after layer 0's.
        assert_eq!(d.vertices().nth(6).unwrap().params[1], MODE_FILL);

        // Empty and invisible things emit nothing.
        d.rect(Rect::ZERO, Color::WHITE);
        d.rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::TRANSPARENT);
        d.line(Vec2::ZERO, Vec2::ZERO, 2.0, Color::WHITE);
        assert_eq!(d.vertex_count(), 12);
        d.clear();
        assert!(d.is_empty());
        assert_eq!(d.layer(), 0);
    }

    #[test]
    fn colors_go_linear() {
        let mut d = DrawList::new();
        d.rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::rgb(0.5, 0.5, 0.5));
        let c = d.vertices().next().unwrap().color;
        assert!((c[0] - 0.214).abs() < 0.01, "{c:?}");
        let q = GlyphQuad {
            x: 1.0, y: 2.0, w: 3.0, h: 4.0,
            uv_min: [10.0, 20.0], uv_max: [13.0, 24.0],
            color: [1.0, 1.0, 1.0, 0.5], is_color: false,
        };
        d.glyphs(&[q]);
        let g = *d.vertices().last().unwrap();
        assert_eq!(g.params[1], MODE_GLYPH);
        assert_eq!(g.pos, [4.0, 2.0]);
        assert_eq!(g.uv, [13.0, 20.0]);
        assert_eq!(g.color[3], 0.5);
    }

    #[test]
    fn gradients_and_shadows() {
        let mut d = DrawList::new();
        d.rounded_rect_gradient(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), 2.0, Color::WHITE, Color::BLACK);
        let v: Vec<_> = d.vertices().collect();
        assert_eq!(v[0].color, [1.0, 1.0, 1.0, 1.0], "top-left is the top color");
        assert_eq!(v[2].color, [0.0, 0.0, 0.0, 1.0], "bottom-right is the bottom color");
        assert_eq!(v[0].params[1], MODE_FILL);
        d.shadow(Rect::from_xywh(10.0, 10.0, 10.0, 10.0), 3.0, 4.0, Color::BLACK);
        let last = *d.vertices().last().unwrap();
        assert_eq!(last.params[1], MODE_SHADOW);
        assert_eq!(last.params[2], 4.0);
        assert_eq!(last.pos, [24.0, 6.0], "quad grows by the blur");
        assert_eq!(last.rect, [15.0, 15.0, 5.0, 5.0], "SDF rect stays the original");
    }

    #[test]
    fn lines_have_width() {
        let mut d = DrawList::new();
        d.line(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 2.0, Color::WHITE);
        let v: Vec<_> = d.vertices().collect();
        assert_eq!(v[0].pos, [0.0, 1.0]);
        assert_eq!(v[1].pos, [0.0, -1.0]);
        assert_eq!(v[2].pos, [10.0, -1.0]);
    }
}
