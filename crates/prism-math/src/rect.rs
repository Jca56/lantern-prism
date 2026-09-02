//! 2D axis-aligned rectangle for UI layout. **Y grows downward** (screen
//! convention): `min.y` is the top edge and `max.y` the bottom.

use crate::Vec2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const ZERO: Rect = Rect { min: Vec2::ZERO, max: Vec2::ZERO };

    #[inline]
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    #[inline]
    pub fn from_min_size(min: Vec2, size: Vec2) -> Self {
        Self { min, max: min + size }
    }

    #[inline]
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        let h = size * 0.5;
        Self { min: center - h, max: center + h }
    }

    #[inline]
    pub fn from_xywh(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
    }

    #[inline]
    pub fn left(&self) -> f64 {
        self.min.x
    }
    #[inline]
    pub fn right(&self) -> f64 {
        self.max.x
    }
    #[inline]
    pub fn top(&self) -> f64 {
        self.min.y
    }
    #[inline]
    pub fn bottom(&self) -> f64 {
        self.max.y
    }
    #[inline]
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    #[inline]
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
    #[inline]
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }
    #[inline]
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }
    #[inline]
    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }

    /// Zero or negative extent on either axis.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }

    /// Half-open on the max edges: a point exactly on `max` is outside, so
    /// adjacent rects never both claim the shared pixel row.
    #[inline]
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x < self.max.x && p.y >= self.min.y && p.y < self.max.y
    }

    #[inline]
    pub fn contains_rect(&self, o: &Rect) -> bool {
        o.min.x >= self.min.x && o.min.y >= self.min.y && o.max.x <= self.max.x && o.max.y <= self.max.y
    }

    #[inline]
    pub fn intersects(&self, o: &Rect) -> bool {
        self.min.x < o.max.x && o.min.x < self.max.x && self.min.y < o.max.y && o.min.y < self.max.y
    }

    /// Overlap; may be empty (check with [`Self::is_empty`]).
    #[inline]
    pub fn intersection(&self, o: &Rect) -> Rect {
        Rect::new(self.min.max(o.min), self.max.min(o.max))
    }

    #[inline]
    pub fn union(&self, o: &Rect) -> Rect {
        Rect::new(self.min.min(o.min), self.max.max(o.max))
    }

    /// Grow by `d` on every side (negative shrinks).
    #[inline]
    pub fn expand(&self, d: f64) -> Rect {
        Rect::new(self.min - Vec2::splat(d), self.max + Vec2::splat(d))
    }

    #[inline]
    pub fn shrink(&self, d: f64) -> Rect {
        self.expand(-d)
    }

    #[inline]
    pub fn translate(&self, d: Vec2) -> Rect {
        Rect::new(self.min + d, self.max + d)
    }

    /// Snap edges to whole pixels (outward on `min`, inward on `max` never
    /// happens: both round to nearest so adjacent rects stay seamless).
    #[inline]
    pub fn round(&self) -> Rect {
        Rect::new(self.min.round(), self.max.round())
    }

    /// Split at absolute `x`; returns `(left, right)`.
    pub fn split_x(&self, x: f64) -> (Rect, Rect) {
        let x = x.clamp(self.min.x, self.max.x);
        (
            Rect::new(self.min, Vec2::new(x, self.max.y)),
            Rect::new(Vec2::new(x, self.min.y), self.max),
        )
    }

    /// Split at absolute `y`; returns `(top, bottom)`.
    pub fn split_y(&self, y: f64) -> (Rect, Rect) {
        let y = y.clamp(self.min.y, self.max.y);
        (
            Rect::new(self.min, Vec2::new(self.max.x, y)),
            Rect::new(Vec2::new(self.min.x, y), self.max),
        )
    }

    /// Cut `w` off the left: `(taken, remainder)`.
    pub fn take_left(&self, w: f64) -> (Rect, Rect) {
        self.split_x(self.min.x + w)
    }
    /// Cut `w` off the right: `(taken, remainder)`.
    pub fn take_right(&self, w: f64) -> (Rect, Rect) {
        let (rest, taken) = self.split_x(self.max.x - w);
        (taken, rest)
    }
    /// Cut `h` off the top: `(taken, remainder)`.
    pub fn take_top(&self, h: f64) -> (Rect, Rect) {
        self.split_y(self.min.y + h)
    }
    /// Cut `h` off the bottom: `(taken, remainder)`.
    pub fn take_bottom(&self, h: f64) -> (Rect, Rect) {
        let (rest, taken) = self.split_y(self.max.y - h);
        (taken, rest)
    }

    /// Nearest point inside (or on the edge of) the rect.
    #[inline]
    pub fn clamp_point(&self, p: Vec2) -> Vec2 {
        p.clamp(self.min, self.max)
    }

    pub fn approx_eq(&self, o: &Rect, eps: f64) -> bool {
        self.min.approx_eq(o.min, eps) && self.max.approx_eq(o.max, eps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry() {
        let r = Rect::from_xywh(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
        assert_eq!(r.size(), Vec2::new(100.0, 50.0));
        assert_eq!(r.center(), Vec2::new(60.0, 45.0));
        assert_eq!(r.area(), 5000.0);
        assert!(r.contains(Vec2::new(10.0, 20.0)));
        assert!(!r.contains(Vec2::new(110.0, 20.0)), "max edge is exclusive");
        assert!(!r.is_empty());
        assert!(Rect::ZERO.is_empty());
    }

    #[test]
    fn set_ops() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let b = Rect::from_xywh(5.0, 5.0, 10.0, 10.0);
        assert!(a.intersects(&b));
        assert_eq!(a.intersection(&b), Rect::from_xywh(5.0, 5.0, 5.0, 5.0));
        assert_eq!(a.union(&b), Rect::from_xywh(0.0, 0.0, 15.0, 15.0));
        let c = Rect::from_xywh(20.0, 20.0, 1.0, 1.0);
        assert!(!a.intersects(&c));
        assert!(a.intersection(&c).is_empty());
        assert_eq!(a.expand(5.0), Rect::from_xywh(-5.0, -5.0, 20.0, 20.0));
        assert_eq!(a.shrink(2.0), Rect::from_xywh(2.0, 2.0, 6.0, 6.0));
        assert!(a.contains_rect(&a.shrink(1.0)));
    }

    #[test]
    fn layout_cuts() {
        let r = Rect::from_xywh(0.0, 0.0, 100.0, 60.0);
        let (header, body) = r.take_top(20.0);
        assert_eq!(header, Rect::from_xywh(0.0, 0.0, 100.0, 20.0));
        assert_eq!(body, Rect::from_xywh(0.0, 20.0, 100.0, 40.0));
        let (side, rest) = body.take_right(30.0);
        assert_eq!(side, Rect::from_xywh(70.0, 20.0, 30.0, 40.0));
        assert_eq!(rest, Rect::from_xywh(0.0, 20.0, 70.0, 40.0));
        let (l, rr) = r.split_x(500.0); // clamped
        assert_eq!(l, r);
        assert!(rr.is_empty());
        let (t, b) = r.take_bottom(15.0);
        assert_eq!(t.top(), 45.0);
        assert_eq!(b.bottom(), 45.0);
        let (lf, _) = r.take_left(25.0);
        assert_eq!(lf.width(), 25.0);
    }
}
