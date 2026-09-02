//! Labels, headings, buttons, toggles, tabs, separators.

use prism_math::{Rect, Vec2};

use crate::state::CursorIcon;
use crate::ui::{FILL, Response, Sense, Ui};

impl Ui<'_> {
    /// One line of body text.
    pub fn label(&mut self, s: &str) -> Rect {
        let style = self.text_style();
        let w = if self.in_row() { self.measure(s, &style) } else { FILL };
        let r = self.alloc(Vec2::new(w, self.m.widget_h.min(style.line_height() as f64 + self.m.gap)));
        self.text_in_rect(s, &style, r, self.theme.text);
        r
    }

    pub fn label_dim(&mut self, s: &str) -> Rect {
        let style = self.text_style();
        let w = if self.in_row() { self.measure(s, &style) } else { FILL };
        let r = self.alloc(Vec2::new(w, self.m.widget_h.min(style.line_height() as f64 + self.m.gap)));
        self.text_in_rect(s, &style, r, self.theme.text_dim);
        r
    }

    pub fn heading(&mut self, s: &str) -> Rect {
        let style = self.heading_style();
        let r = self.alloc(Vec2::new(FILL, style.line_height() as f64 + self.m.gap));
        self.text_in_rect(s, &style, r, self.theme.text);
        r
    }

    /// Wrapped paragraph.
    pub fn paragraph(&mut self, s: &str) {
        let style = self.text_style();
        let w = self.avail_width();
        let m = self.text.measure_wrapped(s, &style, w as f32);
        let r = self.alloc(Vec2::new(FILL, m.height as f64));
        self.text_at(s, &style, r.min, w, self.theme.text);
    }

    /// Content-sized button. `clicked` in the response fires on release.
    pub fn button(&mut self, label: &str) -> Response {
        let style = self.text_style();
        let w = self.measure(label, &style) + self.m.pad * 2.0;
        self.button_sized(label, Vec2::new(w, self.m.widget_h))
    }

    /// Button spanning the available width.
    pub fn button_wide(&mut self, label: &str) -> Response {
        self.button_sized(label, Vec2::new(FILL, self.m.widget_h))
    }

    pub fn button_sized(&mut self, label: &str, size: Vec2) -> Response {
        let id = self.id(label);
        let rect = self.alloc(size);
        let r = self.interact(id, rect, Sense::CLICK);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        self.button_face(rect, &r);
        let style = self.text_style();
        self.text_centered(label, &style, rect, self.theme.text);
        r
    }

    /// Checkbox with a label. Returns `true` when toggled.
    pub fn toggle(&mut self, label: &str, value: &mut bool) -> bool {
        let id = self.id(label);
        let style = self.text_style();
        let box_size = self.m.px(25.0);
        let w = if self.in_row() { box_size + self.m.gap + self.measure(label, &style) } else { FILL };
        let rect = self.alloc(Vec2::new(w, self.m.widget_h));
        let r = self.interact(id, rect, Sense::CLICK);
        if r.clicked {
            *value = !*value;
        }
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        let bx = Rect::from_center_size(
            Vec2::new(rect.min.x + box_size * 0.5, rect.center().y),
            Vec2::splat(box_size),
        );
        let well = if r.hovered { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(bx, well);
        if *value {
            let inner = bx.shrink(self.m.px(4.0));
            self.fill_shaded(inner, self.theme.accent);
            // Check mark.
            let w = self.m.px(3.0);
            let (x0, y0) = (inner.min.x, inner.min.y);
            let s = inner.width();
            let c = self.theme.accent_text;
            self.draw.line(Vec2::new(x0 + s * 0.22, y0 + s * 0.52), Vec2::new(x0 + s * 0.42, y0 + s * 0.74), w, c);
            self.draw.line(Vec2::new(x0 + s * 0.40, y0 + s * 0.74), Vec2::new(x0 + s * 0.80, y0 + s * 0.28), w, c);
        }
        let text_rect = Rect::new(Vec2::new(bx.max.x + self.m.gap, rect.min.y), rect.max);
        self.text_in_rect(label, &style, text_rect, self.theme.text);
        r.clicked
    }

    /// A row item that highlights when `selected` (lists, menus).
    pub fn selectable(&mut self, label: &str, selected: bool) -> Response {
        let id = self.id(label);
        let rect = self.alloc(Vec2::new(FILL, self.m.widget_h));
        let r = self.interact(id, rect, Sense::CLICK);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        let style = self.text_style();
        if selected {
            self.fill_shaded(rect, self.theme.selection);
            self.text_in_rect_padded(label, &style, rect, self.theme.selection_text);
        } else {
            if r.hovered || r.held {
                let bg = self.theme.hover(self.theme.panel);
                self.fill(rect, bg);
            }
            self.text_in_rect_padded(label, &style, rect, self.theme.text);
        }
        r
    }

    fn text_in_rect_padded(&mut self, s: &str, style: &prism_text::TextStyle, rect: Rect, color: prism_math::Color) {
        let inner = Rect::new(Vec2::new(rect.min.x + self.m.pad, rect.min.y), rect.max);
        self.text_in_rect(s, style, inner, color);
    }

    /// Tab strip. Returns `true` when the selection changed.
    pub fn tabs(&mut self, selected: &mut usize, labels: &[&str]) -> bool {
        let mut changed = false;
        let style = self.text_style();
        let rect = self.alloc(Vec2::new(FILL, self.m.widget_h));
        let n = labels.len().max(1) as f64;
        let w = (rect.width() - self.m.gap * (n - 1.0)) / n;
        for (i, label) in labels.iter().enumerate() {
            let tr = Rect::from_min_size(
                Vec2::new((rect.min.x + i as f64 * (w + self.m.gap)).round(), rect.min.y),
                Vec2::new(w.round(), rect.height()),
            );
            let id = self.id(label).with_index(i);
            let r = self.interact(id, tr, Sense::CLICK);
            if r.clicked && *selected != i {
                *selected = i;
                changed = true;
            }
            if r.hovered {
                self.state.cursor_icon = CursorIcon::Pointer;
            }
            if *selected == i {
                self.raised(tr, self.theme.accent, false);
                self.text_centered(label, &style, tr, self.theme.accent_text);
            } else {
                self.button_face(tr, &r);
                self.text_centered(label, &style, tr, self.theme.text);
            }
        }
        changed
    }

    /// Thin horizontal rule with breathing room.
    pub fn separator(&mut self) {
        let r = self.alloc(Vec2::new(FILL, self.m.gap * 2.0 + self.m.border * 2.0));
        let y = (r.center().y - self.m.border).round();
        self.etched_line(r.min.x, r.max.x, y);
    }
}
