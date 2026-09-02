//! The immediate-mode context: lays widgets out top to bottom (or left to
//! right inside a row), hit-tests them against this frame's input, and draws
//! into the shared `DrawList`.

use prism_math::{Color, Rect, Vec2};
use prism_render::DrawList;
use prism_text::{GlyphQuad, TextEngine, TextMetrics, TextStyle};

use crate::id::WidgetId;
use crate::state::UiState;
use crate::theme::{Metrics, Theme};

/// Pass as a width to take all available room.
pub const FILL: f64 = f64::INFINITY;

/// What kinds of input a widget reacts to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sense {
    pub click: bool,
    pub drag: bool,
    pub focus: bool,
}

impl Sense {
    pub const NONE: Sense = Sense { click: false, drag: false, focus: false };
    pub const CLICK: Sense = Sense { click: true, drag: false, focus: false };
    pub const DRAG: Sense = Sense { click: true, drag: true, focus: false };
    pub const FOCUS: Sense = Sense { click: true, drag: true, focus: true };
}

/// What happened to a widget this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    pub id: WidgetId,
    pub rect: Rect,
    pub hovered: bool,
    /// The pointer went down on this widget and is still down.
    pub held: bool,
    pub pressed: bool,
    pub clicked: bool,
    pub double_clicked: bool,
    pub released: bool,
    pub dragging: bool,
    pub drag_delta: Vec2,
    pub focused: bool,
}

impl Default for WidgetId {
    fn default() -> Self {
        WidgetId::ROOT
    }
}

struct RowState {
    x: f64,
    y: f64,
    max_h: f64,
    remaining: f64,
}

pub struct Ui<'a> {
    pub draw: &'a mut DrawList,
    pub text: &'a mut TextEngine,
    pub theme: &'a Theme,
    pub m: Metrics,
    pub state: &'a mut UiState,
    clip: Rect,
    cursor: Vec2,
    avail_w: f64,
    max_y: f64,
    row: Option<RowState>,
    ids: Vec<WidgetId>,
    layer: usize,
    window: Rect,
    quads: Vec<GlyphQuad>,
}

impl<'a> Ui<'a> {
    /// Start laying out inside `content`, clipped to `clip`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        draw: &'a mut DrawList,
        text: &'a mut TextEngine,
        theme: &'a Theme,
        m: Metrics,
        state: &'a mut UiState,
        content: Rect,
        clip: Rect,
        id: WidgetId,
        layer: usize,
    ) -> Self {
        draw.set_layer(layer);
        draw.push_clip_absolute(clip);
        Self {
            draw,
            text,
            theme,
            m,
            state,
            clip,
            cursor: content.min,
            avail_w: content.width(),
            max_y: content.min.y,
            row: None,
            ids: vec![id],
            layer,
            window: Rect::from_min_size(Vec2::ZERO, Vec2::new(1.0e6, 1.0e6)),
            quads: Vec::new(),
        }
    }

    /// Tell the context how big the window is (popups stay inside it).
    pub fn set_window_rect(&mut self, window: Rect) {
        self.window = window;
    }

    /// Finish: pops the clip pushed by `new`. Returns the content bottom.
    pub fn finish(self) -> f64 {
        self.draw.pop_clip();
        self.max_y
    }

    // ---- identity -------------------------------------------------------

    pub fn id(&self, label: &str) -> WidgetId {
        self.ids.last().copied().unwrap_or(WidgetId::ROOT).with(label)
    }

    pub fn push_id(&mut self, label: &str) {
        let id = self.id(label);
        self.ids.push(id);
    }

    pub fn push_index(&mut self, i: usize) {
        let id = self.ids.last().copied().unwrap_or(WidgetId::ROOT).with_index(i);
        self.ids.push(id);
    }

    pub fn pop_id(&mut self) {
        if self.ids.len() > 1 {
            self.ids.pop();
        }
    }

    // ---- geometry -------------------------------------------------------

    pub fn clip(&self) -> Rect {
        self.clip
    }

    pub fn cursor(&self) -> Vec2 {
        self.cursor
    }

    pub fn avail_width(&self) -> f64 {
        match &self.row {
            Some(r) => r.remaining.max(0.0),
            None => self.avail_w,
        }
    }

    pub fn remaining_height(&self) -> f64 {
        (self.clip.max.y - self.cursor.y).max(0.0)
    }

    pub fn layer(&self) -> usize {
        self.layer
    }

    /// Inside a `row(...)` call: widgets size to content instead of filling.
    pub fn in_row(&self) -> bool {
        self.row.is_some()
    }

    /// Reserve space for a widget. `size.x == FILL` takes the available width.
    pub fn alloc(&mut self, size: Vec2) -> Rect {
        let gap = self.m.gap;
        match &mut self.row {
            Some(row) => {
                let w = if size.x == FILL { row.remaining.max(0.0) } else { size.x };
                let r = Rect::from_min_size(Vec2::new(row.x, row.y), Vec2::new(w, size.y));
                row.x += w + gap;
                row.remaining -= w + gap;
                row.max_h = row.max_h.max(size.y);
                self.max_y = self.max_y.max(r.max.y);
                r
            }
            None => {
                let w = if size.x == FILL { self.avail_w } else { size.x.min(self.avail_w) };
                let r = Rect::from_min_size(self.cursor, Vec2::new(w, size.y));
                self.cursor.y += size.y + gap;
                self.max_y = self.max_y.max(r.max.y);
                r
            }
        }
    }

    /// Lay the widgets declared in `f` left to right on one line.
    pub fn row(&mut self, f: impl FnOnce(&mut Ui)) {
        debug_assert!(self.row.is_none(), "rows do not nest");
        self.row = Some(RowState { x: self.cursor.x, y: self.cursor.y, max_h: 0.0, remaining: self.avail_w });
        f(self);
        let row = self.row.take().expect("row state");
        if row.max_h > 0.0 {
            self.cursor.y += row.max_h + self.m.gap;
        }
    }

    /// A label column followed by whatever `f` declares, on one line.
    pub fn labelled(&mut self, label: &str, f: impl FnOnce(&mut Ui)) {
        let label_w = self.m.label_w.min(self.avail_w * 0.5);
        let r = Rect::from_min_size(self.cursor, Vec2::new(label_w, self.m.widget_h));
        let style = self.text_style();
        self.text_in_rect(label, &style, r, self.theme.text_dim);
        let saved = (self.cursor.x, self.avail_w);
        self.cursor.x += label_w + self.m.gap;
        self.avail_w -= label_w + self.m.gap;
        let before = self.cursor.y;
        f(self);
        if self.cursor.y == before {
            // Nothing was declared; still advance past the label.
            self.cursor.y += self.m.widget_h + self.m.gap;
        }
        self.cursor.x = saved.0;
        self.avail_w = saved.1;
    }

    /// Shift everything after this point right by `amount`.
    pub fn indent(&mut self, amount: f64, f: impl FnOnce(&mut Ui)) {
        let saved = (self.cursor.x, self.avail_w);
        self.cursor.x += amount;
        self.avail_w -= amount;
        f(self);
        self.cursor.x = saved.0;
        self.avail_w = saved.1;
    }

    pub fn space(&mut self, h: f64) {
        self.cursor.y += h;
        self.max_y = self.max_y.max(self.cursor.y);
    }

    /// Move the layout origin (scroll areas).
    pub(crate) fn set_cursor(&mut self, p: Vec2) {
        self.cursor = p;
        self.max_y = self.max_y.max(p.y);
    }

    pub(crate) fn set_avail_width(&mut self, w: f64) {
        self.avail_w = w;
    }

    pub(crate) fn set_clip(&mut self, clip: Rect) {
        self.clip = clip;
    }

    pub(crate) fn set_layer_internal(&mut self, layer: usize) {
        self.layer = layer;
    }

    /// Whole-window rect, for placing popups.
    pub(crate) fn state_window_rect(&self) -> Rect {
        self.window
    }

    // ---- interaction ----------------------------------------------------

    /// Hit-test `rect` for `id` against this frame's input.
    pub fn interact(&mut self, id: WidgetId, rect: Rect, sense: Sense) -> Response {
        let st = &mut *self.state;
        let visible = rect.intersection(&self.clip);
        let blocked = st
            .popup
            .is_some_and(|(prect, layer)| layer > self.layer && prect.contains(st.pointer));
        let over = st.pointer_in_window && !blocked && visible.contains(st.pointer);
        let hovered = over && (st.active.is_none() || st.active == Some(id));
        if hovered {
            st.hot = Some(id);
        }
        let mut r = Response { id, rect, hovered, ..Response::default() };
        let wants = sense.click || sense.drag || sense.focus;
        if wants && st.pressed && !st.press_claimed && !blocked && visible.contains(st.press_pos) {
            st.press_claimed = true;
            st.active = Some(id);
            st.focus = if sense.focus { Some(id) } else { None };
            r.pressed = true;
            r.double_clicked = st.double_click;
        }
        if st.active == Some(id) {
            r.held = st.down;
            if st.released {
                r.released = true;
                r.clicked = sense.click && over;
            }
            if sense.drag && st.down {
                r.dragging = true;
                r.drag_delta = if r.pressed { st.pointer - st.press_pos } else { st.delta };
            }
        }
        r.focused = st.focus == Some(id);
        r
    }

    /// Background color for an interactive widget in its current role.
    pub fn widget_color(&self, r: &Response) -> Color {
        if r.held {
            self.theme.widget_active
        } else if r.hovered {
            self.theme.widget_hover
        } else {
            self.theme.widget
        }
    }

    // ---- text -----------------------------------------------------------

    pub fn text_style(&self) -> TextStyle {
        TextStyle::new(self.m.text_size)
    }

    pub fn heading_style(&self) -> TextStyle {
        TextStyle::new(self.m.heading_size).bold()
    }

    pub fn mono_style(&self) -> TextStyle {
        TextStyle::new(self.m.text_size).mono()
    }

    pub fn measure(&mut self, s: &str, style: &TextStyle) -> f64 {
        self.text.measure(s, style) as f64
    }

    /// Draw `s` with its line box's top-left at `pos`, wrapping at `max_w`.
    pub fn text_at(&mut self, s: &str, style: &TextStyle, pos: Vec2, max_w: f64, color: Color) -> TextMetrics {
        self.quads.clear();
        let m = self.text.place(s, style, pos.x as f32, pos.y as f32, max_w as f32, color.to_gpu(), &mut self.quads);
        self.draw.glyphs(&self.quads);
        m
    }

    /// Draw one line of `s`, left-aligned and vertically centred in `rect`,
    /// clipped to it.
    pub fn text_in_rect(&mut self, s: &str, style: &TextStyle, rect: Rect, color: Color) {
        let lh = style.line_height() as f64;
        let y = (rect.center().y - lh * 0.5).round();
        let clip = rect.intersection(&self.clip);
        self.draw.push_clip(clip);
        self.text_at(s, style, Vec2::new(rect.min.x, y), 1.0e6, color);
        self.draw.pop_clip();
    }

    /// Draw one line of `s` centred both ways in `rect`.
    pub fn text_centered(&mut self, s: &str, style: &TextStyle, rect: Rect, color: Color) {
        let w = self.measure(s, style);
        let lh = style.line_height() as f64;
        let x = (rect.center().x - w * 0.5).round();
        let y = (rect.center().y - lh * 0.5).round();
        let clip = rect.intersection(&self.clip);
        self.draw.push_clip(clip);
        self.text_at(s, style, Vec2::new(x, y), 1.0e6, color);
        self.draw.pop_clip();
    }

    /// Draw one line of `s` right-aligned and vertically centred in `rect`.
    pub fn text_right(&mut self, s: &str, style: &TextStyle, rect: Rect, color: Color) {
        let w = self.measure(s, style);
        let lh = style.line_height() as f64;
        let x = (rect.max.x - w).round();
        let y = (rect.center().y - lh * 0.5).round();
        let clip = rect.intersection(&self.clip);
        self.draw.push_clip(clip);
        self.text_at(s, style, Vec2::new(x, y), 1.0e6, color);
        self.draw.pop_clip();
    }

    // ---- shapes ---------------------------------------------------------

    pub fn fill(&mut self, rect: Rect, color: Color) {
        self.draw.rounded_rect(rect, self.m.radius, color);
    }

    pub fn fill_square(&mut self, rect: Rect, color: Color) {
        self.draw.rect(rect, color);
    }

    pub fn outline(&mut self, rect: Rect, width: f64, color: Color) {
        self.draw.stroke_rect(rect, width, self.m.radius, color);
    }

    pub fn hline(&mut self, y: f64, x0: f64, x1: f64, color: Color) {
        self.draw.hline(x0, x1, y, self.m.border, color);
    }
}
