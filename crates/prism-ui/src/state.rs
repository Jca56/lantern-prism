//! Cross-frame UI state: pointer and keyboard input gathered for one rebuild,
//! which widget is hot/active/focused, open popups, and per-widget memory.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use prism_math::{Rect, Vec2};

use crate::event::{Event, Key, Modifiers, MouseButton};
use crate::id::WidgetId;

/// What the pointer should look like, decided by whatever is under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    EwResize,
    NsResize,
    Grabbing,
}

/// A key press delivered to the focused widget this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyPress {
    pub key: Key,
    pub mods: Modifiers,
    pub repeat: bool,
}

/// Caret and selection of a text field, in byte offsets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEdit {
    pub cursor: usize,
    pub anchor: usize,
    /// Horizontal scroll so the caret stays visible.
    pub scroll: f64,
    /// While editing a number field: the text being typed.
    pub buffer: Option<String>,
}

impl TextEdit {
    pub fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }
    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScrollMem {
    pub offset: f64,
    /// Content height measured last frame.
    pub content: f64,
}

pub struct UiState {
    // ---- pointer ----
    pub pointer: Vec2,
    pub pointer_in_window: bool,
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
    pub press_pos: Vec2,
    pub right_pressed: bool,
    pub double_click: bool,
    /// Some widget took this frame's press. Unclaimed presses clear focus.
    pub press_claimed: bool,
    /// Pointer movement since the previous rebuild.
    pub delta: Vec2,
    /// Wheel movement this frame, in pixels (y > 0 = content scrolls up).
    pub wheel: Vec2,
    // ---- keyboard ----
    pub mods: Modifiers,
    pub keys: Vec<KeyPress>,
    pub text_input: String,
    // ---- widget roles ----
    pub hot: Option<WidgetId>,
    pub active: Option<WidgetId>,
    pub focus: Option<WidgetId>,
    /// A popup (menu, dropdown list) currently open: its rect and layer.
    pub popup: Option<(Rect, usize)>,
    popup_seen: bool,
    pub cursor_icon: CursorIcon,
    /// Set by widgets that want another rebuild right after this one
    /// (e.g. a popup that just closed and needs the underlying UI to redraw).
    pub request_rebuild: bool,
    last_click: Option<(Instant, Vec2)>,
    mem: HashMap<WidgetId, Mem>,
}

enum Mem {
    Text(TextEdit),
    Scroll(ScrollMem),
    Open(bool),
    DragStart(f64),
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl UiState {
    pub fn new() -> Self {
        Self {
            pointer: Vec2::new(-1.0, -1.0),
            pointer_in_window: false,
            down: false,
            pressed: false,
            released: false,
            press_pos: Vec2::ZERO,
            right_pressed: false,
            double_click: false,
            press_claimed: false,
            delta: Vec2::ZERO,
            wheel: Vec2::ZERO,
            mods: Modifiers::NONE,
            keys: Vec::new(),
            text_input: String::new(),
            hot: None,
            active: None,
            focus: None,
            popup: None,
            popup_seen: false,
            cursor_icon: CursorIcon::Default,
            request_rebuild: false,
            last_click: None,
            mem: HashMap::new(),
        }
    }

    /// Fold this frame's events into the state. `line_px` converts wheel
    /// notches to pixels.
    pub fn begin_frame(&mut self, events: &[Event], line_px: f64) {
        self.pressed = false;
        self.released = false;
        self.right_pressed = false;
        self.double_click = false;
        self.press_claimed = false;
        self.wheel = Vec2::ZERO;
        self.keys.clear();
        self.text_input.clear();
        self.hot = None;
        self.cursor_icon = CursorIcon::Default;
        self.request_rebuild = false;
        self.popup_seen = false;
        let start = self.pointer;
        for ev in events {
            match ev {
                Event::PointerMoved(p) => {
                    self.pointer = *p;
                    self.pointer_in_window = true;
                }
                Event::PointerLeft => self.pointer_in_window = false,
                Event::Button { button: MouseButton::Left, pressed: true, pos, .. } => {
                    self.down = true;
                    self.pressed = true;
                    self.press_pos = *pos;
                    let now = Instant::now();
                    self.double_click = self
                        .last_click
                        .is_some_and(|(t, p)| now - t < Duration::from_millis(400) && p.distance(*pos) < 6.0);
                    self.last_click = if self.double_click { None } else { Some((now, *pos)) };
                }
                Event::Button { button: MouseButton::Left, pressed: false, .. } => {
                    self.down = false;
                    self.released = true;
                }
                Event::Button { button: MouseButton::Right, pressed: true, .. } => self.right_pressed = true,
                Event::Wheel { delta, .. } => self.wheel += delta.to_pixels(line_px),
                Event::Modifiers(m) => self.mods = *m,
                Event::Key { key, pressed: true, repeat, mods } => {
                    self.keys.push(KeyPress { key: *key, mods: *mods, repeat: *repeat });
                }
                Event::Text(t) => self.text_input.push_str(t),
                Event::Focus(false) => {
                    self.down = false;
                    self.active = None;
                }
                _ => {}
            }
        }
        self.delta = self.pointer - start;
    }

    /// Settle roles after every widget has run.
    pub fn end_frame(&mut self) {
        if self.pressed && !self.press_claimed {
            self.focus = None;
        }
        if self.released {
            self.active = None;
        }
        if !self.popup_seen {
            self.popup = None;
        }
    }

    /// Called by a popup while it is open so it survives the frame.
    pub fn keep_popup(&mut self, rect: Rect, layer: usize) {
        self.popup = Some((rect, layer));
        self.popup_seen = true;
    }

    /// Consume a key press matching `pred`, if one arrived this frame.
    pub fn take_key(&mut self, pred: impl Fn(&KeyPress) -> bool) -> Option<KeyPress> {
        let i = self.keys.iter().position(pred)?;
        Some(self.keys.remove(i))
    }

    pub fn is_active(&self, id: WidgetId) -> bool {
        self.active == Some(id)
    }

    pub fn has_focus(&self, id: WidgetId) -> bool {
        self.focus == Some(id)
    }

    // ---- per-widget memory ----

    pub fn text_edit(&mut self, id: WidgetId) -> &mut TextEdit {
        match self.mem.entry(id).or_insert_with(|| Mem::Text(TextEdit::default())) {
            Mem::Text(t) => t,
            other => {
                *other = Mem::Text(TextEdit::default());
                match other {
                    Mem::Text(t) => t,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn scroll(&mut self, id: WidgetId) -> &mut ScrollMem {
        match self.mem.entry(id).or_insert_with(|| Mem::Scroll(ScrollMem::default())) {
            Mem::Scroll(s) => s,
            other => {
                *other = Mem::Scroll(ScrollMem::default());
                match other {
                    Mem::Scroll(s) => s,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn open(&mut self, id: WidgetId) -> &mut bool {
        match self.mem.entry(id).or_insert(Mem::Open(false)) {
            Mem::Open(b) => b,
            other => {
                *other = Mem::Open(false);
                match other {
                    Mem::Open(b) => b,
                    _ => unreachable!(),
                }
            }
        }
    }

    /// Like [`Self::open`], but a fresh slot starts as `default`.
    pub fn open_default(&mut self, id: WidgetId, default: bool) -> bool {
        match self.mem.entry(id).or_insert(Mem::Open(default)) {
            Mem::Open(b) => *b,
            other => {
                *other = Mem::Open(default);
                default
            }
        }
    }

    /// Value a drag started from (for number drags).
    pub fn drag_start(&mut self, id: WidgetId) -> &mut f64 {
        match self.mem.entry(id).or_insert(Mem::DragStart(0.0)) {
            Mem::DragStart(v) => v,
            other => {
                *other = Mem::DragStart(0.0);
                match other {
                    Mem::DragStart(v) => v,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn forget(&mut self, id: WidgetId) {
        self.mem.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::WheelDelta;

    #[test]
    fn ingests_events() {
        let mut s = UiState::new();
        let p = Vec2::new(10.0, 20.0);
        s.begin_frame(
            &[
                Event::PointerMoved(p),
                Event::Button { button: MouseButton::Left, pressed: true, pos: p, mods: Modifiers::NONE },
                Event::Wheel { delta: WheelDelta::Lines(Vec2::new(0.0, 1.0)), pos: p, mods: Modifiers::NONE },
                Event::Key { key: Key::Enter, pressed: true, repeat: false, mods: Modifiers::NONE },
                Event::Text("a".into()),
            ],
            40.0,
        );
        assert!(s.pressed && s.down && !s.released);
        assert_eq!(s.press_pos, p);
        assert_eq!(s.wheel, Vec2::new(0.0, 40.0));
        assert_eq!(s.text_input, "a");
        assert!(s.take_key(|k| k.key == Key::Enter).is_some());
        assert!(s.take_key(|k| k.key == Key::Enter).is_none());
        s.active = Some(WidgetId::ROOT);
        s.end_frame();
        assert!(s.is_active(WidgetId::ROOT), "still held");
        s.begin_frame(&[Event::Button { button: MouseButton::Left, pressed: false, pos: p, mods: Modifiers::NONE }], 40.0);
        assert!(s.released && !s.down);
        s.end_frame();
        assert_eq!(s.active, None);
    }

    #[test]
    fn double_click_and_popup_lifetime() {
        let mut s = UiState::new();
        let p = Vec2::ZERO;
        let press = Event::Button { button: MouseButton::Left, pressed: true, pos: p, mods: Modifiers::NONE };
        s.begin_frame(std::slice::from_ref(&press), 1.0);
        assert!(!s.double_click);
        s.begin_frame(&[press], 1.0);
        assert!(s.double_click);
        s.keep_popup(Rect::ZERO, 1);
        s.end_frame();
        assert!(s.popup.is_some());
        s.begin_frame(&[], 1.0);
        s.end_frame();
        assert!(s.popup.is_none(), "not kept → closed");
    }

    #[test]
    fn memory_slots() {
        let mut s = UiState::new();
        let id = WidgetId::ROOT.with("x");
        s.text_edit(id).cursor = 3;
        assert_eq!(s.text_edit(id).cursor, 3);
        *s.open(id) = true; // slot repurposed
        assert!(*s.open(id));
        s.scroll(id).offset = 9.0;
        assert_eq!(s.scroll(id).offset, 9.0);
        s.forget(id);
        assert_eq!(s.scroll(id).offset, 0.0);
    }
}
