//! Single-line text editing: caret, selection, keyboard navigation, and
//! horizontal scrolling so the caret stays visible.

use prism_math::{Rect, Vec2};

use crate::event::Key;
use crate::id::WidgetId;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextResponse {
    /// The text changed this frame.
    pub changed: bool,
    /// Enter was pressed.
    pub committed: bool,
    /// Escape was pressed.
    pub cancelled: bool,
    pub focused: bool,
}

fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i].chars().next_back().map_or(0, |c| i - c.len_utf8())
}

fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

/// Pen x at byte `b`, from a cluster-boundary advance table.
fn x_at(adv: &[(u32, f32)], b: usize) -> f64 {
    let b = b as u32;
    match adv.binary_search_by_key(&b, |&(o, _)| o) {
        Ok(i) => adv[i].1 as f64,
        Err(i) => {
            // Inside a ligature: interpolate between neighbours.
            let (b0, x0) = adv.get(i.wrapping_sub(1)).copied().unwrap_or((0, 0.0));
            let (b1, x1) = adv.get(i).copied().unwrap_or((b0, x0));
            if b1 == b0 {
                x0 as f64
            } else {
                x0 as f64 + (x1 - x0) as f64 * (b - b0) as f64 / (b1 - b0) as f64
            }
        }
    }
}

/// Byte offset whose pen x is closest to `x`.
fn byte_at_x(adv: &[(u32, f32)], x: f64) -> usize {
    adv.iter()
        .min_by(|a, b| (a.1 as f64 - x).abs().total_cmp(&(b.1 as f64 - x).abs()))
        .map_or(0, |a| a.0 as usize)
}

impl Ui<'_> {
    /// Editable single-line text. Click to focus, drag to select.
    pub fn text_field(&mut self, label: &str, value: &mut String) -> TextResponse {
        let id = self.id(label);
        let rect = self.alloc(Vec2::new(FILL, self.m.widget_h));
        self.text_edit_core(id, rect, value)
    }

    /// The editor behind text fields and number typing. Draws into `rect`.
    pub(crate) fn text_edit_core(&mut self, id: WidgetId, rect: Rect, value: &mut String) -> TextResponse {
        let r = self.interact(id, rect, Sense::FOCUS);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Text;
        }
        let style = self.text_style();
        let inner = Rect::new(Vec2::new(rect.min.x + self.m.pad, rect.min.y), Vec2::new(rect.max.x - self.m.pad, rect.max.y));
        let mut adv = Vec::new();
        self.text.advances(value, &style, &mut adv);

        let mut out = TextResponse { focused: r.focused, ..TextResponse::default() };
        let mut scroll = self.state.text_edit(id).scroll;

        if r.focused {
            // Pointer: place caret / extend selection.
            if r.pressed {
                let b = byte_at_x(&adv, self.state.pointer.x - inner.min.x + scroll);
                let shift = self.state.mods.shift();
                let te = self.state.text_edit(id);
                te.cursor = b;
                if !shift {
                    te.anchor = b;
                }
                if r.double_clicked {
                    te.anchor = 0;
                    te.cursor = value.len();
                }
            } else if r.dragging {
                let b = byte_at_x(&adv, self.state.pointer.x - inner.min.x + scroll);
                self.state.text_edit(id).cursor = b;
            }
            // Keyboard.
            let keys: Vec<_> = self.state.keys.drain(..).collect();
            for k in keys {
                let te = self.state.text_edit(id);
                let (s0, s1) = te.selection();
                match k.key {
                    Key::ArrowLeft => {
                        te.cursor = if te.has_selection() && !k.mods.shift() { s0 } else { prev_boundary(value, te.cursor) };
                        if k.mods.ctrl() {
                            te.cursor = 0;
                        }
                        if !k.mods.shift() {
                            te.anchor = te.cursor;
                        }
                    }
                    Key::ArrowRight => {
                        te.cursor = if te.has_selection() && !k.mods.shift() { s1 } else { next_boundary(value, te.cursor) };
                        if k.mods.ctrl() {
                            te.cursor = value.len();
                        }
                        if !k.mods.shift() {
                            te.anchor = te.cursor;
                        }
                    }
                    Key::Home => {
                        te.cursor = 0;
                        if !k.mods.shift() {
                            te.anchor = 0;
                        }
                    }
                    Key::End => {
                        te.cursor = value.len();
                        if !k.mods.shift() {
                            te.anchor = value.len();
                        }
                    }
                    Key::Backspace => {
                        if te.has_selection() {
                            value.replace_range(s0..s1, "");
                            te.cursor = s0;
                        } else if te.cursor > 0 {
                            let p = prev_boundary(value, te.cursor);
                            value.replace_range(p..te.cursor, "");
                            te.cursor = p;
                        }
                        te.anchor = te.cursor;
                        out.changed = true;
                    }
                    Key::Delete => {
                        if te.has_selection() {
                            value.replace_range(s0..s1, "");
                            te.cursor = s0;
                        } else if te.cursor < value.len() {
                            let n = next_boundary(value, te.cursor);
                            value.replace_range(te.cursor..n, "");
                        }
                        te.anchor = te.cursor;
                        out.changed = true;
                    }
                    Key::Enter => out.committed = true,
                    Key::Escape => out.cancelled = true,
                    Key::Char('a') if k.mods.ctrl() => {
                        te.anchor = 0;
                        te.cursor = value.len();
                    }
                    _ => {}
                }
            }
            // Typed text.
            if !self.state.text_input.is_empty() {
                let typed: String = self.state.text_input.chars().filter(|c| !c.is_control()).collect();
                if !typed.is_empty() {
                    let te = self.state.text_edit(id);
                    let (s0, s1) = te.selection();
                    value.replace_range(s0..s1, &typed);
                    te.cursor = s0 + typed.len();
                    te.anchor = te.cursor;
                    out.changed = true;
                }
                self.state.text_input.clear();
            }
            if out.changed {
                self.text.advances(value, &style, &mut adv);
            }
            // Keep the caret in view.
            let te = self.state.text_edit(id);
            te.cursor = te.cursor.min(value.len());
            te.anchor = te.anchor.min(value.len());
            let cx = x_at(&adv, te.cursor);
            let w = inner.width().max(1.0);
            if cx - scroll > w - 2.0 {
                scroll = cx - w + 2.0;
            }
            if cx - scroll < 0.0 {
                scroll = cx;
            }
            scroll = scroll.max(0.0);
            te.scroll = scroll;
        }

        // Draw.
        let bg = if r.focused { self.theme.widget_active } else { self.widget_color(&r) };
        self.fill(rect, bg);
        if r.focused {
            self.outline(rect, self.m.border, self.theme.focus);
        }
        let clip = inner.intersection(&self.clip());
        self.draw.push_clip(clip);
        let lh = style.line_height() as f64;
        let ty = (rect.center().y - lh * 0.5).round();
        let origin = Vec2::new(inner.min.x - scroll, ty);
        if r.focused {
            let te = self.state.text_edit(id).clone();
            if te.has_selection() {
                let (s0, s1) = te.selection();
                let x0 = origin.x + x_at(&adv, s0);
                let x1 = origin.x + x_at(&adv, s1);
                self.draw.rect(Rect::new(Vec2::new(x0, ty), Vec2::new(x1, ty + lh)), self.theme.selection);
            }
        }
        self.text_at(value, &style, origin, 1.0e6, self.theme.text);
        if r.focused {
            let cx = (origin.x + x_at(&adv, self.state.text_edit(id).cursor)).round();
            let cw = self.m.px(2.0);
            self.draw.rect(Rect::new(Vec2::new(cx, ty), Vec2::new(cx + cw, ty + lh)), self.theme.text);
        }
        self.draw.pop_clip();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries() {
        let s = "aé🦊";
        assert_eq!(next_boundary(s, 0), 1);
        assert_eq!(next_boundary(s, 1), 3);
        assert_eq!(next_boundary(s, 3), 7);
        assert_eq!(next_boundary(s, 7), 7);
        assert_eq!(prev_boundary(s, 7), 3);
        assert_eq!(prev_boundary(s, 0), 0);
    }

    #[test]
    fn advance_lookup() {
        let adv = [(0u32, 0.0f32), (1, 10.0), (3, 30.0)];
        assert_eq!(x_at(&adv, 1), 10.0);
        assert_eq!(x_at(&adv, 2), 20.0, "interpolates inside a cluster");
        assert_eq!(x_at(&adv, 9), 30.0, "clamps past the end");
        assert_eq!(byte_at_x(&adv, 12.0), 1);
        assert_eq!(byte_at_x(&adv, 26.0), 3);
        assert_eq!(byte_at_x(&adv, -5.0), 0);
    }
}
