//! Small procedural icons drawn with lines and rects, so the UI needs no
//! icon font. Each draws centred in a rect with a stroke width.

use prism_math::{Color, Rect, Vec2};
use prism_render::DrawList;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Vertex,
    Edge,
    Face,
    Solid,
    Wire,
    Grid,
    Frame,
    EditMode,
    Plus,
    Camera,
    Move,
    Rotate,
    Scale,
}

pub fn draw(d: &mut DrawList, rect: Rect, icon: Icon, color: Color, stroke: f64) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.28; // half extent of the glyph
    let p = |x: f64, y: f64| c + Vec2::new(x * s, y * s);
    match icon {
        Icon::Vertex => {
            d.rounded_rect(Rect::from_center_size(c, Vec2::splat(s * 0.9)), s * 0.45, color);
        }
        Icon::Edge => {
            d.line(p(-1.0, 0.8), p(1.0, -0.8), stroke, color);
            d.rounded_rect(Rect::from_center_size(p(-1.0, 0.8), Vec2::splat(s * 0.6)), s * 0.3, color);
            d.rounded_rect(Rect::from_center_size(p(1.0, -0.8), Vec2::splat(s * 0.6)), s * 0.3, color);
        }
        Icon::Face => {
            d.rounded_rect(Rect::from_center_size(c, Vec2::splat(s * 2.0)), stroke, color.fade(0.55));
            d.stroke_rect(Rect::from_center_size(c, Vec2::splat(s * 2.0)), stroke, stroke, color);
        }
        Icon::Solid => {
            // A shaded cube: front square plus two offset edges.
            let f = Rect::from_center_size(p(-0.25, 0.25), Vec2::splat(s * 1.5));
            d.rounded_rect(f, stroke, color.fade(0.6));
            d.stroke_rect(f, stroke, stroke, color);
            d.line(f.min, f.min + Vec2::new(s * 0.5, -s * 0.5), stroke, color);
            d.line(Vec2::new(f.max.x, f.min.y), Vec2::new(f.max.x, f.min.y) + Vec2::new(s * 0.5, -s * 0.5), stroke, color);
            d.line(f.min + Vec2::new(s * 0.5, -s * 0.5), Vec2::new(f.max.x, f.min.y) + Vec2::new(s * 0.5, -s * 0.5), stroke, color);
            d.line(Vec2::new(f.max.x, f.min.y) + Vec2::new(s * 0.5, -s * 0.5), f.max + Vec2::new(s * 0.5, -s * 0.5), stroke, color);
        }
        Icon::Wire => {
            let f = Rect::from_center_size(p(-0.25, 0.25), Vec2::splat(s * 1.5));
            d.stroke_rect(f, stroke, stroke, color);
            let o = Vec2::new(s * 0.5, -s * 0.5);
            d.stroke_rect(f.translate(o), stroke, stroke, color);
            for corner in [f.min, Vec2::new(f.max.x, f.min.y), f.max, Vec2::new(f.min.x, f.max.y)] {
                d.line(corner, corner + o, stroke, color);
            }
        }
        Icon::Grid => {
            for i in -1..=1 {
                let t = i as f64 * 0.7;
                d.line(p(-1.0, t), p(1.0, t), stroke, color);
                d.line(p(t, -1.0), p(t, 1.0), stroke, color);
            }
        }
        Icon::Frame => {
            let l = s * 0.7;
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let corner = p(sx, sy);
                d.line(corner, corner - Vec2::new(sx * l, 0.0), stroke, color);
                d.line(corner, corner - Vec2::new(0.0, sy * l), stroke, color);
            }
            d.rounded_rect(Rect::from_center_size(c, Vec2::splat(s * 0.5)), s * 0.25, color);
        }
        Icon::EditMode => {
            // A vertex being pulled: dot plus arrow.
            d.rounded_rect(Rect::from_center_size(p(-0.6, 0.6), Vec2::splat(s * 0.7)), s * 0.35, color);
            d.line(p(-0.3, 0.3), p(0.9, -0.9), stroke, color);
            d.line(p(0.9, -0.9), p(0.2, -0.9), stroke, color);
            d.line(p(0.9, -0.9), p(0.9, -0.2), stroke, color);
        }
        Icon::Plus => {
            d.line(p(-1.0, 0.0), p(1.0, 0.0), stroke, color);
            d.line(p(0.0, -1.0), p(0.0, 1.0), stroke, color);
        }
        Icon::Move => {
            // Four-way arrows.
            let h = s * 0.45;
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let tip = p(dx, dy);
                d.line(c, tip, stroke, color);
                let back = tip - Vec2::new(dx, dy) * h;
                let side = Vec2::new(dy, dx) * h * 0.8;
                d.line(tip, back + side, stroke, color);
                d.line(tip, back - side, stroke, color);
            }
        }
        Icon::Rotate => {
            // Three quarters of a ring, ending in an arrowhead.
            let r = s * 0.95;
            let (a0, a1) = (-0.3f64, 4.4f64);
            let pts: Vec<Vec2> = (0..=24).map(|i| c + Vec2::from_angle(a0 + (a1 - a0) * i as f64 / 24.0) * r).collect();
            d.polyline(&pts, stroke, color, false);
            let end = pts[24];
            let tangent = Vec2::from_angle(a1 + core::f64::consts::FRAC_PI_2);
            let h = s * 0.55;
            d.line(end, end - tangent * h + tangent.perp() * h * 0.7, stroke, color);
            d.line(end, end - tangent * h - tangent.perp() * h * 0.7, stroke, color);
        }
        Icon::Scale => {
            // A small square growing toward a corner bracket.
            d.stroke_rect(Rect::new(p(-1.0, 0.0), p(0.0, 1.0)), stroke, stroke, color);
            d.line(p(-0.2, 0.2), p(0.9, -0.9), stroke, color);
            d.line(p(0.9, -0.9), p(0.1, -0.9), stroke, color);
            d.line(p(0.9, -0.9), p(0.9, -0.1), stroke, color);
        }
        Icon::Camera => {
            let body = Rect::from_center_size(p(-0.2, 0.1), Vec2::new(s * 1.6, s * 1.1));
            d.stroke_rect(body, stroke, stroke, color);
            d.line(Vec2::new(body.max.x, body.min.y + body.height() * 0.3), p(1.0, -0.5), stroke, color);
            d.line(p(1.0, -0.5), p(1.0, 0.7), stroke, color);
            d.line(p(1.0, 0.7), Vec2::new(body.max.x, body.max.y - body.height() * 0.3), stroke, color);
        }
    }
}
