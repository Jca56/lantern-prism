//! Sliders and drag-to-change number fields, with click-to-type editing.

use prism_math::{Rect, Vec2};

use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

/// Human formatting: up to `decimals` places, trailing zeros trimmed.
pub fn format_number(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        let t = s.trim_end_matches('0').trim_end_matches('.');
        if t == "-0" { "0".to_owned() } else { t.to_owned() }
    } else {
        s
    }
}

impl Ui<'_> {
    /// Horizontal slider over `min..=max`. The label and the value are drawn
    /// inside. Double-click types a value. Returns `true` when changed.
    pub fn slider(&mut self, label: &str, value: &mut f64, min: f64, max: f64, step: f64) -> bool {
        let id = self.id(label);
        let rect = self.alloc(Vec2::new(FILL, self.m.widget_h));
        if let Some(done) = self.number_editing(id, rect, *value) {
            if let Some(v) = done {
                *value = clamp_step(v, min, max, step);
                return true;
            }
            return false;
        }
        let r = self.interact(id, rect, Sense::DRAG);
        if r.double_clicked {
            self.begin_number_edit(id, *value);
        }
        if r.hovered || r.held {
            self.state.cursor_icon = CursorIcon::EwResize;
        }
        let mut changed = false;
        if r.dragging && max > min {
            let t = ((self.state.pointer.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
            let v = clamp_step(min + t * (max - min), min, max, step);
            if v != *value {
                *value = v;
                changed = true;
            }
        }
        let bg = self.widget_color(&r);
        self.fill(rect, bg);
        let t = if max > min { ((*value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
        let filled = Rect::new(rect.min, Vec2::new(rect.min.x + rect.width() * t, rect.max.y));
        if filled.width() >= 1.0 {
            self.draw.push_clip(filled);
            self.fill(rect, self.theme.accent);
            self.draw.pop_clip();
        }
        let style = self.text_style();
        let inner = rect.shrink(self.m.pad).expand_y(self.m.pad);
        self.text_in_rect(label, &style, inner, self.theme.text);
        let shown = format_number(*value, 3);
        self.text_right(&shown, &style, inner, self.theme.text);
        // Re-draw the covered part of the text in the accent-text color.
        if filled.width() >= 1.0 {
            self.draw.push_clip(filled);
            self.text_in_rect(label, &style, inner, self.theme.accent_text);
            self.text_right(&shown, &style, inner, self.theme.accent_text);
            self.draw.pop_clip();
        }
        changed
    }

    /// Number field: drag horizontally to change by `step` per pixel (Shift
    /// for a tenth), click to type. `range` clamps when given.
    pub fn drag_value(&mut self, label: &str, value: &mut f64, step: f64, range: Option<(f64, f64)>, decimals: usize) -> bool {
        let id = self.id(label);
        let w = if self.in_row() { self.m.px(120.0) } else { FILL };
        let rect = self.alloc(Vec2::new(w, self.m.widget_h));
        let (min, max) = range.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
        if let Some(done) = self.number_editing(id, rect, *value) {
            if let Some(v) = done {
                *value = v.clamp(min, max);
                return true;
            }
            return false;
        }
        let r = self.interact(id, rect, Sense::DRAG);
        if r.pressed {
            *self.state.drag_start(id) = *value;
        }
        if r.hovered || r.held {
            self.state.cursor_icon = CursorIcon::EwResize;
        }
        let mut changed = false;
        if r.held {
            let travelled = self.state.pointer.x - self.state.press_pos.x;
            if travelled.abs() >= 3.0 {
                let fine = if self.state.mods.shift() { 0.1 } else { 1.0 };
                let start = *self.state.drag_start(id);
                let v = (start + travelled * step * fine / self.m.scale).clamp(min, max);
                if v != *value {
                    *value = v;
                    changed = true;
                }
            }
        }
        if r.clicked && (self.state.pointer.x - self.state.press_pos.x).abs() < 3.0 {
            self.begin_number_edit(id, *value);
            self.state.request_rebuild = true;
        }
        let bg = self.widget_color(&r);
        self.fill(rect, bg);
        let style = self.text_style();
        let inner = rect.shrink(self.m.pad).expand_y(self.m.pad);
        if !label.is_empty() {
            self.text_in_rect(label, &style, inner, self.theme.text_dim);
        }
        let shown = format_number(*value, decimals);
        self.text_right(&shown, &style, inner, self.theme.text);
        changed
    }

    /// Integer flavour of [`Self::drag_value`].
    pub fn drag_int(&mut self, label: &str, value: &mut i64, range: Option<(i64, i64)>) -> bool {
        let mut v = *value as f64;
        let range = range.map(|(a, b)| (a as f64, b as f64));
        let changed = self.drag_value(label, &mut v, 0.1, range, 0);
        if changed {
            *value = v.round() as i64;
        }
        changed
    }

    fn begin_number_edit(&mut self, id: crate::WidgetId, value: f64) {
        let text = format_number(value, 6);
        let te = self.state.text_edit(id);
        te.buffer = Some(text.clone());
        te.cursor = text.len();
        te.anchor = 0;
        te.scroll = 0.0;
        self.state.focus = Some(id);
    }

    /// If `id` is being typed into, run the editor and report:
    /// `Some(Some(v))` committed, `Some(None)` still editing or cancelled.
    fn number_editing(&mut self, id: crate::WidgetId, rect: Rect, current: f64) -> Option<Option<f64>> {
        let editing = self.state.text_edit(id).buffer.is_some() && self.state.has_focus(id);
        if !editing {
            if self.state.text_edit(id).buffer.is_some() {
                // Focus moved away: commit what was typed.
                let buf = self.state.text_edit(id).buffer.take().unwrap_or_default();
                return Some(buf.trim().parse::<f64>().ok().filter(|v| *v != current));
            }
            return None;
        }
        let mut buf = self.state.text_edit(id).buffer.take().unwrap_or_default();
        let res = self.text_edit_core(id, rect, &mut buf);
        if res.committed {
            self.state.focus = None;
            self.state.request_rebuild = true;
            return Some(buf.trim().parse::<f64>().ok());
        }
        if res.cancelled {
            self.state.focus = None;
            self.state.request_rebuild = true;
            return Some(None);
        }
        self.state.text_edit(id).buffer = Some(buf);
        Some(None)
    }
}

fn clamp_step(v: f64, min: f64, max: f64, step: f64) -> f64 {
    let v = if step > 0.0 { min + ((v - min) / step).round() * step } else { v };
    v.clamp(min, max)
}

trait ExpandY {
    fn expand_y(self, d: f64) -> Self;
}

impl ExpandY for Rect {
    fn expand_y(self, d: f64) -> Rect {
        Rect::new(Vec2::new(self.min.x, self.min.y - d), Vec2::new(self.max.x, self.max.y + d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting() {
        assert_eq!(format_number(1.5, 3), "1.5");
        assert_eq!(format_number(2.0, 3), "2");
        assert_eq!(format_number(-0.0001, 3), "0");
        assert_eq!(format_number(1.23456, 2), "1.23");
        assert_eq!(format_number(42.0, 0), "42");
    }

    #[test]
    fn stepping() {
        assert!((clamp_step(0.26, 0.0, 1.0, 0.1) - 0.3).abs() < 1e-12);
        assert_eq!(clamp_step(5.0, 0.0, 1.0, 0.0), 1.0);
        assert_eq!(clamp_step(-3.0, 0.0, 10.0, 1.0), 0.0);
    }
}
