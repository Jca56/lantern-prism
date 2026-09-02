//! Vertical scroll areas with a draggable scrollbar.

use prism_math::{Rect, Vec2};

use crate::ui::{FILL, Sense, Ui};

impl Ui<'_> {
    /// A vertically scrolling region. `height` of `None` takes the remaining
    /// height. Content height is measured as `f` lays out, so the scrollbar
    /// is right from the second frame on.
    pub fn scroll_area(&mut self, label: &str, height: Option<f64>, f: impl FnOnce(&mut Ui)) {
        let id = self.id(label);
        let h = height.unwrap_or_else(|| self.remaining_height()).max(0.0);
        let viewport = self.alloc(Vec2::new(FILL, h));
        let bar_w = self.m.scrollbar_w;
        let content_w = (viewport.width() - bar_w - self.m.gap).max(0.0);

        let mem = self.state.scroll(id).clone();
        let max_off = (mem.content - viewport.height()).max(0.0);
        let mut offset = mem.offset.clamp(0.0, max_off);

        // Wheel over the viewport.
        let over = self.state.pointer_in_window && viewport.intersection(&self.clip()).contains(self.state.pointer);
        let popup_blocks = self
            .state
            .popup
            .is_some_and(|(r, layer)| layer > self.layer() && r.contains(self.state.pointer));
        if over && !popup_blocks && self.state.wheel.y != 0.0 {
            offset = (offset - self.state.wheel.y).clamp(0.0, max_off);
            self.state.wheel = Vec2::ZERO;
        }

        // Scrollbar.
        let track = Rect::new(Vec2::new(viewport.max.x - bar_w, viewport.min.y), viewport.max);
        if max_off > 0.0 {
            let ratio = viewport.height() / mem.content.max(1.0);
            let thumb_h = (track.height() * ratio).max(self.m.widget_h).min(track.height());
            let travel = track.height() - thumb_h;
            let thumb_y = track.min.y + travel * (offset / max_off);
            let thumb = Rect::from_min_size(Vec2::new(track.min.x, thumb_y), Vec2::new(bar_w, thumb_h));
            let r = self.interact(id.with("thumb"), thumb, Sense::DRAG);
            if r.dragging && travel > 0.0 && !r.pressed {
                offset = (offset + r.drag_delta.y / travel * max_off).clamp(0.0, max_off);
            }
            self.draw.rounded_rect(track, bar_w * 0.5, self.theme.field);
            let thumb = Rect::from_min_size(Vec2::new(track.min.x, track.min.y + travel * (offset / max_off)), Vec2::new(bar_w, thumb_h));
            let base = if r.held { self.theme.accent } else if r.hovered { self.theme.hover(self.theme.widget) } else { self.theme.widget };
            let (t, b) = (self.theme.top(base), self.theme.bottom(base));
            self.draw.rounded_rect_gradient(thumb.shrink(self.m.border), bar_w * 0.5, t, b);
            self.draw.stroke_rect(thumb, self.m.border, bar_w * 0.5, self.theme.border_dark);
        }

        // Lay out the content shifted by the offset, clipped to the viewport.
        let saved_clip = self.clip();
        let saved_avail = self.avail_width();
        let content_clip = Rect::new(viewport.min, Vec2::new(viewport.min.x + content_w, viewport.max.y)).intersection(&saved_clip);
        self.set_clip(content_clip);
        self.draw.push_clip(content_clip);
        let start = Vec2::new(viewport.min.x, viewport.min.y - offset);
        let after_cursor = self.cursor();
        self.set_cursor(start);
        self.set_avail_width(content_w);
        self.push_id(label);
        f(self);
        self.pop_id();
        let content_h = (self.cursor().y - start.y - self.m.gap).max(0.0);
        self.draw.pop_clip();
        self.set_clip(saved_clip);
        self.set_avail_width(saved_avail);
        self.set_cursor(after_cursor);

        let mem = self.state.scroll(id);
        mem.offset = offset;
        mem.content = content_h;
    }
}
