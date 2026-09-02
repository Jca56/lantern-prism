//! Dropdowns and menus: a button that opens a popup list on the layer above.

use prism_math::{Rect, Vec2};

use crate::event::Key;
use crate::id::WidgetId;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

/// Outcome of a popup list this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopupResult {
    pub picked: Option<usize>,
    pub closed: bool,
}

impl Ui<'_> {
    /// Pick one of `options`. Returns `true` when the selection changed.
    pub fn dropdown(&mut self, label: &str, selected: &mut usize, options: &[&str]) -> bool {
        let id = self.id(label);
        let w = if self.in_row() {
            let style = self.text_style();
            let widest = options.iter().map(|o| self.measure(o, &style)).fold(0.0, f64::max);
            widest + self.m.pad * 3.0 + self.m.px(15.0)
        } else {
            FILL
        };
        let rect = self.alloc(Vec2::new(w, self.m.widget_h));
        let r = self.interact(id, rect, Sense::CLICK);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        let open = *self.state.open(id);
        let well = if r.hovered || open { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(rect, well);
        if open {
            self.outline(rect, self.m.border, self.theme.focus);
        }
        let style = self.text_style();
        let inner = Rect::new(Vec2::new(rect.min.x + self.m.pad, rect.min.y), Vec2::new(rect.max.x - self.m.pad, rect.max.y));
        let current = options.get(*selected).copied().unwrap_or("");
        self.text_in_rect(current, &style, inner, self.theme.text);
        self.draw_chevron(rect);

        if r.clicked {
            *self.state.open(id) = !open;
            self.state.request_rebuild = true;
        }
        if *self.state.open(id) {
            let res = self.popup_list(id, rect, options, Some(*selected));
            if let Some(i) = res.picked {
                let changed = *selected != i;
                *selected = i;
                *self.state.open(id) = false;
                return changed;
            }
            if res.closed {
                *self.state.open(id) = false;
            }
        }
        false
    }

    /// A button that opens a menu. Returns the picked item index.
    pub fn menu_button(&mut self, label: &str, items: &[&str]) -> Option<usize> {
        let id = self.id(label);
        let style = self.text_style();
        let w = self.measure(label, &style) + self.m.pad * 2.0;
        let rect = self.alloc(Vec2::new(w, self.m.widget_h));
        let r = self.interact(id, rect, Sense::CLICK);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        let open_now = *self.state.open(id);
        let base = self.widget_color(&r);
        self.raised(rect, base, open_now || r.held);
        self.text_centered(label, &style, rect, self.theme.text);
        if r.clicked {
            *self.state.open(id) = !open_now;
            self.state.request_rebuild = true;
        }
        if *self.state.open(id) {
            let res = self.popup_list(id, rect, items, None);
            if res.picked.is_some() || res.closed {
                *self.state.open(id) = false;
            }
            return res.picked;
        }
        None
    }

    fn draw_chevron(&mut self, rect: Rect) {
        let s = self.m.px(6.0);
        let c = Vec2::new(rect.max.x - self.m.pad - s, rect.center().y - s * 0.25);
        let w = self.m.px(2.0);
        let col = self.theme.text_dim;
        self.draw.line(Vec2::new(c.x - s, c.y - s * 0.5), Vec2::new(c.x, c.y + s * 0.5), w, col);
        self.draw.line(Vec2::new(c.x, c.y + s * 0.5), Vec2::new(c.x + s, c.y - s * 0.5), w, col);
    }

    /// A list popup anchored below (or above) `anchor`, drawn one layer up.
    /// Closes on outside press or Escape.
    pub fn popup_list(&mut self, id: WidgetId, anchor: Rect, items: &[&str], selected: Option<usize>) -> PopupResult {
        let style = self.text_style();
        let item_h = self.m.widget_h;
        let widest = items.iter().map(|o| self.measure(o, &style)).fold(0.0, f64::max);
        let w = (widest + self.m.pad * 3.0).max(anchor.width());
        let h = item_h * items.len() as f64 + self.m.gap * 2.0;
        let window = self.state_window_rect();
        let below = anchor.max.y + self.m.gap;
        let y = if below + h <= window.max.y { below } else { (anchor.min.y - self.m.gap - h).max(window.min.y) };
        let x = anchor.min.x.min(window.max.x - w).max(window.min.x);
        let rect = Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h));
        let layer = self.layer() + 1;
        self.state.keep_popup(rect, layer);

        let mut out = PopupResult::default();
        if self.state.take_key(|k| k.key == Key::Escape).is_some() {
            out.closed = true;
        }
        if self.state.pressed && !rect.contains(self.state.press_pos) && !anchor.contains(self.state.press_pos) {
            out.closed = true;
            self.state.request_rebuild = true;
        }

        // Draw and interact on the popup layer with the popup as clip.
        let saved_layer = self.layer();
        let saved_clip = self.clip();
        self.draw.set_layer(layer);
        self.set_layer_internal(layer);
        self.set_clip(rect);
        self.draw.push_clip_absolute(rect.expand(self.m.px(20.0)));
        self.floating_panel(rect, self.theme.header);
        self.draw.pop_clip();
        self.draw.push_clip_absolute(rect);
        let mut y = rect.min.y + self.m.gap;
        for (i, item) in items.iter().enumerate() {
            let ir = Rect::from_min_size(Vec2::new(rect.min.x, y), Vec2::new(w, item_h));
            let iid = id.with("item").with_index(i);
            let r = self.interact(iid, ir, Sense::CLICK);
            if r.hovered {
                self.state.cursor_icon = CursorIcon::Pointer;
            }
            let is_sel = selected == Some(i);
            if r.hovered || r.held {
                let bg = self.theme.hover(self.theme.header);
                self.fill(ir.shrink(self.m.border), bg);
            }
            let inner = Rect::new(Vec2::new(ir.min.x + self.m.pad, ir.min.y), Vec2::new(ir.max.x - self.m.pad, ir.max.y));
            let color = if is_sel { self.theme.accent } else { self.theme.text };
            self.text_in_rect(item, &style, inner, color);
            if r.clicked {
                out.picked = Some(i);
                self.state.request_rebuild = true;
            }
            y += item_h;
        }
        self.draw.pop_clip();
        self.set_clip(saved_clip);
        self.set_layer_internal(saved_layer);
        self.draw.set_layer(saved_layer);
        out
    }
}
