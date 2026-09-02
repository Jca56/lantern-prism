//! winit → `prism_ui::Event`.

use prism_math::Vec2;
use prism_ui::{Event, Key, Modifiers, MouseButton, WheelDelta};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key as WKey, NamedKey};

/// Translate one window event. Tracks modifier and pointer state in place so
/// button/wheel/key events carry them. Returns `None` for events the UI does
/// not care about.
pub fn window_event(ev: &WindowEvent, mods: &mut Modifiers, pointer: &mut Vec2) -> Option<Event> {
    Some(match ev {
        WindowEvent::Resized(s) => Event::Resized { width: s.width, height: s.height },
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => Event::ScaleFactor(*scale_factor),
        WindowEvent::Focused(f) => Event::Focus(*f),
        WindowEvent::CloseRequested => Event::CloseRequested,
        WindowEvent::CursorMoved { position, .. } => {
            *pointer = Vec2::new(position.x, position.y);
            Event::PointerMoved(*pointer)
        }
        WindowEvent::CursorLeft { .. } => Event::PointerLeft,
        WindowEvent::MouseInput { state, button, .. } => Event::Button {
            button: match button {
                winit::event::MouseButton::Left => MouseButton::Left,
                winit::event::MouseButton::Right => MouseButton::Right,
                winit::event::MouseButton::Middle => MouseButton::Middle,
                winit::event::MouseButton::Back => MouseButton::Back,
                winit::event::MouseButton::Forward => MouseButton::Forward,
                winit::event::MouseButton::Other(n) => MouseButton::Other(*n),
            },
            pressed: *state == ElementState::Pressed,
            pos: *pointer,
            mods: *mods,
        },
        WindowEvent::MouseWheel { delta, .. } => Event::Wheel {
            delta: match delta {
                MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines(Vec2::new(*x as f64, *y as f64)),
                MouseScrollDelta::PixelDelta(p) => WheelDelta::Pixels(Vec2::new(p.x, p.y)),
            },
            pos: *pointer,
            mods: *mods,
        },
        WindowEvent::ModifiersChanged(m) => {
            let s = m.state();
            let mut out = Modifiers::NONE;
            if s.shift_key() {
                out = out | Modifiers::SHIFT;
            }
            if s.control_key() {
                out = out | Modifiers::CTRL;
            }
            if s.alt_key() {
                out = out | Modifiers::ALT;
            }
            if s.super_key() {
                out = out | Modifiers::SUPER;
            }
            *mods = out;
            Event::Modifiers(out)
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let key = translate_key(&event.logical_key);
            let pressed = event.state == ElementState::Pressed;
            // Text arrives alongside the press; the UI gets both, key first.
            // (Phase 2 routes `Text` to whatever has keyboard focus.)
            Event::Key { key, pressed, repeat: event.repeat, mods: *mods }
        }
        _ => return None,
    })
}

/// The committed text of a key press, if any (separate from the key event).
pub fn key_text(ev: &WindowEvent) -> Option<Event> {
    if let WindowEvent::KeyboardInput { event, .. } = ev
        && event.state == ElementState::Pressed
        && let Some(t) = &event.text
        && !t.chars().all(char::is_control)
    {
        return Some(Event::Text(t.to_string()));
    }
    None
}

fn translate_key(k: &WKey) -> Key {
    match k {
        WKey::Character(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => Key::Unknown,
            }
        }
        WKey::Named(n) => match n {
            NamedKey::Escape => Key::Escape,
            NamedKey::Enter => Key::Enter,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Insert => Key::Insert,
            NamedKey::Space => Key::Space,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Shift => Key::Shift,
            NamedKey::Control => Key::Control,
            NamedKey::Alt => Key::Alt,
            NamedKey::Super => Key::Super,
            NamedKey::CapsLock => Key::CapsLock,
            NamedKey::F1 => Key::F(1),
            NamedKey::F2 => Key::F(2),
            NamedKey::F3 => Key::F(3),
            NamedKey::F4 => Key::F(4),
            NamedKey::F5 => Key::F(5),
            NamedKey::F6 => Key::F(6),
            NamedKey::F7 => Key::F(7),
            NamedKey::F8 => Key::F(8),
            NamedKey::F9 => Key::F(9),
            NamedKey::F10 => Key::F(10),
            NamedKey::F11 => Key::F(11),
            NamedKey::F12 => Key::F(12),
            _ => Key::Unknown,
        },
        _ => Key::Unknown,
    }
}
