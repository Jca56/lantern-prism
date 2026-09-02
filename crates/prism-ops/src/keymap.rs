//! Keymaps are data: a trigger, an operator id, property overrides. Grouped
//! into named maps; resolution walks the maps for the current context chain
//! and fires the first item whose operator polls true.

use prism_props::Value;

use crate::input::{Event, Key, Modifiers, MouseButton};

#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub key: Option<Key>,
    pub button: Option<MouseButton>,
    pub mods: Modifiers,
    /// Fire on press (else on release).
    pub press: bool,
    /// Only on a double click (buttons).
    pub double: bool,
}

impl Trigger {
    pub const fn key(key: Key, mods: Modifiers) -> Self {
        Self { key: Some(key), button: None, mods, press: true, double: false }
    }

    pub const fn button(button: MouseButton, mods: Modifiers) -> Self {
        Self { key: None, button: Some(button), mods, press: true, double: false }
    }

    pub fn matches(&self, ev: &Event) -> bool {
        match ev {
            Event::Key { key, pressed, mods, .. } => {
                self.key.is_some_and(|k| key_eq(k, *key)) && *pressed == self.press && *mods == self.mods
            }
            Event::Button { button, pressed, mods, .. } => {
                self.button == Some(*button) && *pressed == self.press && *mods == self.mods
            }
            _ => false,
        }
    }
}

/// Letters match case-insensitively so Shift+A binds without knowing
/// whether the platform reports `a` or `A`.
fn key_eq(a: Key, b: Key) -> bool {
    match (a, b) {
        (Key::Char(x), Key::Char(y)) => x.eq_ignore_ascii_case(&y),
        _ => a == b,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyItem {
    pub trigger: Trigger,
    pub op: String,
    pub overrides: Vec<(String, Value)>,
}

impl KeyItem {
    pub fn new(trigger: Trigger, op: &str) -> Self {
        Self { trigger, op: op.to_owned(), overrides: Vec::new() }
    }

    pub fn with(mut self, field: &str, v: Value) -> Self {
        self.overrides.push((field.to_owned(), v));
        self
    }

    /// `(name, value)` pairs as the executor wants them.
    pub fn overrides(&self) -> Vec<(&str, Value)> {
        self.overrides.iter().map(|(n, v)| (n.as_str(), v.clone())).collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyMap {
    pub name: String,
    pub items: Vec<KeyItem>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyConfig {
    pub maps: Vec<KeyMap>,
}

/// Context names the shell passes, most specific first.
pub const CTX_WINDOW: &str = "window";
pub const CTX_OBJECT: &str = "object";
pub const CTX_MESH: &str = "mesh";
pub const CTX_OUTLINER: &str = "outliner";

impl KeyConfig {
    pub fn map(&self, name: &str) -> Option<&KeyMap> {
        self.maps.iter().find(|m| m.name == name)
    }

    pub fn map_mut(&mut self, name: &str) -> &mut KeyMap {
        if let Some(i) = self.maps.iter().position(|m| m.name == name) {
            &mut self.maps[i]
        } else {
            self.maps.push(KeyMap { name: name.to_owned(), items: Vec::new() });
            self.maps.last_mut().expect("just pushed")
        }
    }

    pub fn bind(&mut self, map: &str, item: KeyItem) {
        self.map_mut(map).items.push(item);
    }

    /// First item in `contexts` order whose trigger matches `ev` and whose
    /// operator `poll`s true.
    pub fn resolve<'a>(&'a self, contexts: &[&str], ev: &Event, mut poll: impl FnMut(&str) -> bool) -> Option<&'a KeyItem> {
        for name in contexts {
            let Some(map) = self.map(name) else {
                continue;
            };
            for item in &map.items {
                if item.trigger.matches(ev) && poll(&item.op) {
                    return Some(item);
                }
            }
        }
        None
    }

    /// The bindings Prism ships with.
    pub fn default_prism() -> KeyConfig {
        use Key::*;
        let ctrl = Modifiers::CTRL;
        let shift = Modifiers::SHIFT;
        let none = Modifiers::NONE;
        let mut k = KeyConfig::default();
        // Window-wide.
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('z'), ctrl), "ed.undo"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('z'), ctrl | shift), "ed.redo"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('y'), ctrl), "ed.redo"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('s'), ctrl), "wm.save"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('s'), ctrl | shift), "wm.save_as"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('o'), ctrl), "wm.open"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('n'), ctrl), "wm.new"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('q'), ctrl), "wm.quit"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(F(3), none), "wm.palette"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Space, ctrl), "wm.palette"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Home, none), "view3d.frame_all"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('.'), none), "view3d.frame_selected"));
        // Object mode.
        k.bind(CTX_OBJECT, KeyItem::new(Trigger::key(Char('a'), shift), "wm.call_menu").with("menu", Value::Str("add".into())));
        k.bind(CTX_OBJECT, KeyItem::new(Trigger::key(Char('x'), none), "object.delete"));
        k.bind(CTX_OBJECT, KeyItem::new(Trigger::key(Delete, none), "object.delete"));
        k.bind(CTX_OBJECT, KeyItem::new(Trigger::key(Char('a'), none), "object.select_all").with("action", Value::Enum(0)));
        k.bind(CTX_OBJECT, KeyItem::new(Trigger::key(Char('a'), Modifiers::ALT), "object.select_all").with("action", Value::Enum(2)));
        k.bind(CTX_OBJECT, KeyItem::new(Trigger::key(Char('d'), shift), "object.duplicate"));
        k.bind(CTX_OBJECT, KeyItem::new(Trigger::key(Tab, none), "object.mode_set").with("mode", Value::Enum(1)).with("toggle", Value::Bool(true)));
        // Mesh edit mode.
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Tab, none), "object.mode_set").with("mode", Value::Enum(0)));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('a'), none), "mesh.select_all").with("action", Value::Enum(0)));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('a'), Modifiers::ALT), "mesh.select_all").with("action", Value::Enum(2)));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('e'), none), "mesh.extrude"));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('x'), none), "wm.call_menu").with("menu", Value::Str("mesh_delete".into())));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Delete, none), "mesh.delete").with("kind", Value::Enum(2)));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('m'), none), "mesh.merge_by_distance"));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('1'), none), "mesh.select_mode").with("mode", Value::Enum(0)));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('2'), none), "mesh.select_mode").with("mode", Value::Enum(1)));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('3'), none), "mesh.select_mode").with("mode", Value::Enum(2)));
        k.bind(CTX_MESH, KeyItem::new(Trigger::key(Char('n'), shift), "mesh.normals_make_consistent"));
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_math::Vec2;

    fn key(k: Key, mods: Modifiers) -> Event {
        Event::Key { key: k, pressed: true, repeat: false, mods }
    }

    #[test]
    fn resolution_order_and_poll() {
        let k = KeyConfig::default_prism();
        let undo = k.resolve(&[CTX_MESH, CTX_WINDOW], &key(Key::Char('z'), Modifiers::CTRL), |_| true).unwrap();
        assert_eq!(undo.op, "ed.undo");
        // Uppercase from a shifted key still matches.
        let add = k.resolve(&[CTX_OBJECT, CTX_WINDOW], &key(Key::Char('A'), Modifiers::SHIFT), |_| true).unwrap();
        assert_eq!(add.op, "wm.call_menu");
        assert_eq!(add.overrides()[0].0, "menu");
        // Tab means different things per context; the first context wins.
        let tab = key(Key::Tab, Modifiers::NONE);
        assert_eq!(k.resolve(&[CTX_MESH, CTX_OBJECT], &tab, |_| true).unwrap().overrides[0].1, Value::Enum(0));
        assert_eq!(k.resolve(&[CTX_OBJECT, CTX_MESH], &tab, |_| true).unwrap().overrides[0].1, Value::Enum(1));
        // Poll rejects: falls through to nothing.
        assert!(k.resolve(&[CTX_OBJECT], &tab, |_| false).is_none());
        // Modifiers must match exactly; releases do not fire.
        assert!(k.resolve(&[CTX_WINDOW], &key(Key::Char('z'), Modifiers::NONE), |_| true).is_none());
        let release = Event::Key { key: Key::Char('z'), pressed: false, repeat: false, mods: Modifiers::CTRL };
        assert!(k.resolve(&[CTX_WINDOW], &release, |_| true).is_none());
        let click = Event::Button { button: MouseButton::Left, pressed: true, pos: Vec2::ZERO, mods: Modifiers::NONE };
        assert!(Trigger::button(MouseButton::Left, Modifiers::NONE).matches(&click));
    }
}
