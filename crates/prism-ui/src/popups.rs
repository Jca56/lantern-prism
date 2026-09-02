//! Shell-level popups: named menus, the command palette, and the path
//! dialog. Drawn on layer 1 over everything; Escape or an outside press
//! closes them.

use prism_math::{Rect, Vec2};
use prism_props::Value;

use crate::event::Key;
use crate::id::WidgetId;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub op: String,
    pub overrides: Vec<(String, Value)>,
}

impl MenuItem {
    fn new(label: &str, op: &str, overrides: Vec<(&str, Value)>) -> Self {
        Self { label: label.into(), op: op.into(), overrides: overrides.into_iter().map(|(k, v)| (k.to_owned(), v)).collect() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Popup {
    Menu { title: String, items: Vec<MenuItem>, pos: Vec2 },
    Palette { query: String, selected: usize },
    Path { op: String, save: bool, text: String },
}

/// An operator the popup wants run: `(op, overrides)`.
pub type Command = (String, Vec<(String, Value)>);

/// The menus Prism knows by name.
pub fn menu(name: &str, pos: Vec2) -> Option<Popup> {
    let items = match name {
        "add" => vec![
            MenuItem::new("Plane", "object.add_primitive", vec![("kind", Value::Enum(0))]),
            MenuItem::new("Cube", "object.add_primitive", vec![("kind", Value::Enum(1))]),
            MenuItem::new("UV Sphere", "object.add_primitive", vec![("kind", Value::Enum(2))]),
            MenuItem::new("Cylinder", "object.add_primitive", vec![("kind", Value::Enum(3))]),
            MenuItem::new("Grid", "object.add_primitive", vec![("kind", Value::Enum(4)), ("segments", Value::I64(10))]),
            MenuItem::new("Circle", "object.add_primitive", vec![("kind", Value::Enum(5))]),
        ],
        "mesh_delete" => vec![
            MenuItem::new("Vertices", "mesh.delete", vec![("kind", Value::Enum(0))]),
            MenuItem::new("Edges", "mesh.delete", vec![("kind", Value::Enum(1))]),
            MenuItem::new("Faces", "mesh.delete", vec![("kind", Value::Enum(2))]),
            MenuItem::new("Only Faces", "mesh.delete", vec![("kind", Value::Enum(3))]),
            MenuItem::new("Dissolve Vertices", "mesh.dissolve", vec![("kind", Value::Enum(0))]),
            MenuItem::new("Dissolve Edges", "mesh.dissolve", vec![("kind", Value::Enum(1))]),
            MenuItem::new("Dissolve Faces", "mesh.dissolve", vec![("kind", Value::Enum(2))]),
        ],
        _ => return None,
    };
    let title = match name {
        "add" => "Add",
        "mesh_delete" => "Delete",
        _ => name,
    };
    Some(Popup::Menu { title: title.into(), items, pos })
}

/// Draw the popup. Returns commands to run and whether it should close.
pub fn draw(ui: &mut Ui, popup: &mut Popup, window: Rect, palette_entries: &[(String, String)]) -> (Vec<Command>, bool) {
    let mut commands = Vec::new();
    let mut close = false;
    if ui.state.take_key(|k| k.key == Key::Escape).is_some() {
        close = true;
    }
    let layer = 1;
    let m = ui.m;
    let rect = match popup {
        Popup::Menu { items, pos, .. } => {
            let w = m.px(260.0);
            let h = m.widget_h * (items.len() as f64 + 1.0) + m.gap * 3.0;
            let x = pos.x.min(window.max.x - w).max(window.min.x);
            let y = pos.y.min(window.max.y - h).max(window.min.y);
            Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
        }
        Popup::Palette { .. } => {
            let w = m.px(600.0).min(window.width() - m.pad * 2.0);
            let h = m.widget_h * 13.0 + m.gap * 4.0;
            Rect::from_min_size(Vec2::new((window.center().x - w * 0.5).round(), window.min.y + m.px(80.0)), Vec2::new(w, h))
        }
        Popup::Path { .. } => {
            let w = m.px(700.0).min(window.width() - m.pad * 2.0);
            let h = m.widget_h * 3.0 + m.gap * 4.0;
            Rect::from_min_size((window.center() - Vec2::new(w, h) * 0.5).round(), Vec2::new(w, h))
        }
    };
    ui.state.keep_popup(rect, layer);
    if ui.state.pressed && !rect.contains(ui.state.press_pos) {
        close = true;
        ui.state.press_claimed = true;
    }

    let saved_layer = ui.layer();
    let saved_clip = ui.clip();
    ui.draw.set_layer(layer);
    ui.set_layer_internal(layer);
    ui.set_clip(rect);
    ui.draw.push_clip_absolute(rect.expand(m.px(20.0)));
    ui.floating_panel(rect, ui.theme.header);
    ui.draw.pop_clip();
    ui.draw.push_clip_absolute(rect);
    ui.push_id("popup");
    let content = rect.shrink(m.gap);
    ui.set_cursor(content.min);
    ui.set_avail_width(content.width());

    match popup {
        Popup::Menu { title, items, .. } => {
            ui.label_dim(title);
            for (i, item) in items.iter().enumerate() {
                ui.push_index(i);
                if ui.selectable(&item.label, false).clicked {
                    commands.push((item.op.clone(), item.overrides.clone()));
                    close = true;
                }
                ui.pop_id();
            }
        }
        Popup::Palette { query, selected } => {
            let field = ui.id("query");
            if ui.state.focus.is_none() {
                ui.state.focus = Some(field);
            }
            let field_rect = ui.alloc(Vec2::new(FILL, m.widget_h));
            let r = ui.text_edit_core(field, field_rect, query);
            let n = palette_entries.len();
            if ui.state.take_key(|k| k.key == Key::ArrowDown).is_some() && n > 0 {
                *selected = (*selected + 1).min(n - 1);
            }
            if ui.state.take_key(|k| k.key == Key::ArrowUp).is_some() {
                *selected = selected.saturating_sub(1);
            }
            if r.changed {
                *selected = 0;
            }
            if r.committed
                && let Some((id, _)) = palette_entries.get(*selected)
            {
                commands.push((id.clone(), Vec::new()));
                close = true;
            }
            ui.space(m.gap);
            for (i, (id, label)) in palette_entries.iter().enumerate().take(11) {
                ui.push_index(i);
                let rect = ui.alloc(Vec2::new(FILL, m.widget_h));
                let rr = ui.interact(ui.id("entry"), rect, Sense::CLICK);
                if rr.hovered {
                    ui.state.cursor_icon = CursorIcon::Pointer;
                }
                let style = ui.text_style();
                let inner = Rect::new(Vec2::new(rect.min.x + m.pad, rect.min.y), Vec2::new(rect.max.x - m.pad, rect.max.y));
                if i == *selected {
                    ui.fill_shaded(rect, ui.theme.selection);
                    ui.text_in_rect(label, &style, inner, ui.theme.selection_text);
                    ui.text_right(id, &style, inner, ui.theme.selection_text);
                } else {
                    if rr.hovered {
                        let bg = ui.theme.hover(ui.theme.header);
                        ui.fill(rect, bg);
                    }
                    ui.text_in_rect(label, &style, inner, ui.theme.text);
                    ui.text_right(id, &style, inner, ui.theme.text_dim);
                }
                if rr.clicked {
                    commands.push((id.clone(), Vec::new()));
                    close = true;
                }
                ui.pop_id();
            }
        }
        Popup::Path { op, save, text } => {
            ui.label_dim(if *save { "Save as" } else { "Open" });
            let field = ui.id("path");
            if ui.state.focus.is_none() {
                ui.state.focus = Some(field);
            }
            let field_rect = ui.alloc(Vec2::new(FILL, m.widget_h));
            let r = ui.text_edit_core(field, field_rect, text);
            let mut go = r.committed;
            ui.row(|ui| {
                if ui.button(if *save { "Save" } else { "Open" }).clicked {
                    go = true;
                }
                if ui.button("Cancel").clicked {
                    close = true;
                }
            });
            if go && !text.trim().is_empty() {
                commands.push((op.clone(), vec![("path".to_owned(), Value::Str(text.trim().to_owned()))]));
                close = true;
            }
        }
    }

    ui.pop_id();
    ui.draw.pop_clip();
    ui.set_clip(saved_clip);
    ui.set_layer_internal(saved_layer);
    ui.draw.set_layer(saved_layer);
    if close {
        ui.state.focus = None;
        ui.state.request_rebuild = true;
    }
    let _ = WidgetId::ROOT;
    (commands, close)
}
