//! Window-system-independent input events. `prism-app` translates winit
//! events into these; everything above never sees winit.

use prism_math::Vec2;

/// Keyboard modifier state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Modifiers = Modifiers(0);
    pub const SHIFT: Modifiers = Modifiers(1);
    pub const CTRL: Modifiers = Modifiers(2);
    pub const ALT: Modifiers = Modifiers(4);
    pub const SUPER: Modifiers = Modifiers(8);

    pub const fn contains(self, o: Modifiers) -> bool {
        self.0 & o.0 == o.0
    }
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }
    pub const fn ctrl(self) -> bool {
        self.contains(Self::CTRL)
    }
    pub const fn alt(self) -> bool {
        self.contains(Self::ALT)
    }
    pub const fn super_key(self) -> bool {
        self.contains(Self::SUPER)
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for Modifiers {
    type Output = Modifiers;
    fn bitor(self, o: Modifiers) -> Modifiers {
        Modifiers(self.0 | o.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelDelta {
    /// Notches; `y > 0` scrolls content up (wheel away from the user).
    Lines(Vec2),
    /// Physical pixels from a touchpad or high-resolution wheel.
    Pixels(Vec2),
}

impl WheelDelta {
    /// Approximate pixel distance, treating one line as `line_px`.
    pub fn to_pixels(self, line_px: f64) -> Vec2 {
        match self {
            WheelDelta::Lines(l) => l * line_px,
            WheelDelta::Pixels(p) => p,
        }
    }
}

/// Logical key identity, layout-aware (what the key *means*).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Escape,
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Space,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Shift,
    Control,
    Alt,
    Super,
    CapsLock,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// New surface size in physical pixels.
    Resized { width: u32, height: u32 },
    ScaleFactor(f64),
    Focus(bool),
    CloseRequested,
    /// Pointer position in physical pixels.
    PointerMoved(Vec2),
    PointerLeft,
    Button { button: MouseButton, pressed: bool, pos: Vec2, mods: Modifiers },
    Wheel { delta: WheelDelta, pos: Vec2, mods: Modifiers },
    Key { key: Key, pressed: bool, repeat: bool, mods: Modifiers },
    /// Committed text from a key press (already dead-key/IME resolved).
    Text(String),
    Modifiers(Modifiers),
}

impl Event {
    /// Does this event mean "something may look different now"?
    pub fn wants_redraw(&self) -> bool {
        matches!(self, Event::Resized { .. } | Event::ScaleFactor(_) | Event::Focus(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers() {
        let m = Modifiers::CTRL | Modifiers::SHIFT;
        assert!(m.ctrl() && m.shift() && !m.alt() && !m.super_key());
        assert!(Modifiers::NONE.is_empty());
        assert!(m.contains(Modifiers::CTRL));
        assert!(!m.contains(Modifiers::ALT));
    }

    #[test]
    fn wheel_to_pixels() {
        assert_eq!(WheelDelta::Lines(Vec2::new(0.0, 2.0)).to_pixels(40.0), Vec2::new(0.0, 80.0));
        assert_eq!(WheelDelta::Pixels(Vec2::new(3.0, 4.0)).to_pixels(40.0), Vec2::new(3.0, 4.0));
        assert!(Event::Resized { width: 1, height: 1 }.wants_redraw());
        assert!(!Event::PointerLeft.wants_redraw());
    }
}
