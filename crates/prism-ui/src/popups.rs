//! Shell-level popups: named menus, the command palette, the path dialog,
//! and the right-click context menu. Drawn above everything; Escape or an
//! outside press closes them. Menus let that press fall through to whatever
//! is underneath (one click dismisses *and* selects); dialogs swallow it.

use prism_doc::{Doc, ObjectMode};
use prism_math::{Rect, Vec2};
use prism_ops::{Ctx, Executor, UiRequest, ViewInfo};
use prism_props::Value;

use crate::context_menu::{ContextMenu, Item, Tint, Width};
use crate::editors::{record_edit, run_op};
use crate::event::Key;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub enum Popup {
    Menu { title: String, items: Vec<MenuItem>, pos: Vec2 },
    Palette { query: String, selected: usize },
    Path { op: String, save: bool, text: String },
    Context(ContextMenu),
}

/// What the shell does after a popup frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopupResult {
    pub close: bool,
    /// Rebuild the context menu (a tool changed state it displays).
    pub refresh: bool,
}

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

struct Host<'a> {
    doc: &'a mut Doc,
    exec: &'a mut Executor,
    requests: &'a mut Vec<UiRequest>,
    pointer: Vec2,
    /// The 3D view the menu was opened over, for interactive operators.
    view: Option<ViewInfo>,
}

impl Host<'_> {
    fn run(&mut self, op: &str, overrides: &[(String, Value)]) {
        let _ = run_op(self.doc, self.exec, self.pointer, self.view, op, overrides, self.requests);
    }
}

/// Draw the popup and act on it.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ui: &mut Ui,
    popup: &mut Popup,
    window: Rect,
    palette_entries: &[(String, String)],
    doc: &mut Doc,
    exec: &mut Executor,
    requests: &mut Vec<UiRequest>,
    pointer: Vec2,
    view: Option<ViewInfo>,
) -> PopupResult {
    let mut host = Host { doc, exec, requests, pointer, view };
    if let Popup::Context(menu) = popup {
        return draw_context(ui, menu, window, &mut host);
    }
    let mut out = PopupResult::default();
    if ui.state.take_key(|k| k.key == Key::Escape).is_some() {
        out.close = true;
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
        Popup::Context(_) => unreachable!(),
    };
    ui.state.keep_popup(rect, layer);
    if ui.state.pressed && !rect.contains(ui.state.press_pos) {
        out.close = true;
        if !matches!(popup, Popup::Menu { .. }) {
            ui.state.press_claimed = true;
        }
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
                    host.run(&item.op, &item.overrides);
                    out.close = true;
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
                host.run(id, &[]);
                out.close = true;
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
                    host.run(id, &[]);
                    out.close = true;
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
                    out.close = true;
                }
            });
            if go && !text.trim().is_empty() {
                host.run(op, &[("path".to_owned(), Value::Str(text.trim().to_owned()))]);
                out.close = true;
            }
        }
        Popup::Context(_) => unreachable!(),
    }

    ui.pop_id();
    ui.draw.pop_clip();
    ui.set_clip(saved_clip);
    ui.set_layer_internal(saved_layer);
    ui.draw.set_layer(saved_layer);
    if out.close {
        ui.state.focus = None;
        ui.state.request_rebuild = true;
    }
    out
}

/// The context menu: content on layer 2 measured as it lays out, then its
/// panel painted on layer 1 underneath; tool strip floating to the left;
/// one open submenu to the right.
fn draw_context(ui: &mut Ui, menu: &mut ContextMenu, window: Rect, host: &mut Host) -> PopupResult {
    let mut out = PopupResult::default();
    if ui.state.take_key(|k| k.key == Key::Escape).is_some() {
        out.close = true;
    }
    let m = ui.m;
    let width = match menu.width {
        Width::Narrow => m.px(300.0),
        Width::Wide => m.px(440.0),
    };
    let strip_w = m.widget_h;
    let strip_h = menu.tools.len() as f64 * (m.widget_h + m.gap);
    let est_h = if menu.height > 0.0 { menu.height } else { m.widget_h * 8.0 };
    let min_x = window.min.x + strip_w + m.gap * 2.0;
    let x = menu.pos.x.clamp(min_x, (window.max.x - width).max(min_x));
    let y = menu.pos.y.clamp(window.min.y, (window.max.y - est_h).max(window.min.y));
    let panel = Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, est_h));
    let strip = Rect::from_min_size(Vec2::new(panel.min.x - m.gap - strip_w, panel.min.y), Vec2::new(strip_w, strip_h.max(1.0)));
    let sub_w = m.px(300.0);
    // The submenu opens beside the panel, on the left when the right is full.
    let sub_x = if panel.max.x + m.gap + sub_w <= window.max.x { panel.max.x + m.gap } else { strip.min.x - m.gap - sub_w };
    let sub_area = Rect::from_min_size(Vec2::new(sub_x, panel.min.y), Vec2::new(sub_w, est_h));
    let mut hit = panel.union(&strip);
    if menu.open_sub.is_some() {
        hit = hit.union(&sub_area);
    }
    ui.state.keep_popup(hit, 1);
    // A press outside closes the menu but is left unclaimed, so the editor
    // underneath still gets it: one click selects, one right-click opens the
    // menu for the new thing.
    let outside_left = ui.state.pressed && !hit.contains(ui.state.press_pos);
    let outside_right = ui.state.right_pressed && !hit.contains(ui.state.pointer);
    if outside_left || outside_right {
        out.close = true;
    }

    let saved_layer = ui.layer();
    let saved_clip = ui.clip();
    ui.draw.set_layer(2);
    ui.set_layer_internal(2);
    ui.set_clip(window);
    ui.draw.push_clip_absolute(window);
    ui.push_id("context");

    // ---- content -------------------------------------------------------------
    let content = panel.shrink(m.pad);
    ui.set_cursor(content.min);
    ui.set_avail_width(content.width());
    let title = menu.title.clone();
    ui.heading(&title);
    if menu.tabs.len() > 1 {
        let labels: Vec<&str> = menu.tabs.iter().map(|t| t.label.as_str()).collect();
        let mut tab = menu.tab;
        if ui.tabs(&mut tab, &labels) {
            menu.tab = tab;
            menu.open_sub = None;
        }
        ui.space(m.gap);
    }
    let tab = menu.tab.min(menu.tabs.len().saturating_sub(1));
    let mut sub_anchor: Option<(usize, f64)> = None;
    let mut new_open_sub = menu.open_sub;
    if let Some(t) = menu.tabs.get_mut(tab) {
        for (i, item) in t.items.iter_mut().enumerate() {
            ui.push_index(i);
            match item {
                Item::Header(h) => {
                    ui.label_dim(h);
                }
                Item::Separator => ui.separator(),
                Item::Action { label, op, overrides } => {
                    if ui.selectable(label, false).clicked {
                        host.run(op, overrides);
                        out.close = true;
                    }
                }
                Item::Sub { label, .. } => {
                    let rect = ui.alloc(Vec2::new(FILL, m.widget_h));
                    let r = ui.interact(ui.id("sub"), rect, Sense::CLICK);
                    let open = menu.open_sub == Some(i);
                    if r.hovered || open {
                        let bg = if open { ui.theme.hover(ui.theme.header) } else { ui.theme.hover(ui.theme.panel) };
                        ui.fill(rect, bg);
                    }
                    if r.hovered {
                        ui.state.cursor_icon = CursorIcon::Pointer;
                        new_open_sub = Some(i);
                    }
                    if r.clicked {
                        new_open_sub = if open { None } else { Some(i) };
                    }
                    let style = ui.text_style();
                    let inner = Rect::new(Vec2::new(rect.min.x + m.pad, rect.min.y), Vec2::new(rect.max.x - m.pad, rect.max.y));
                    ui.text_in_rect(label, &style, inner, ui.theme.text);
                    ui.text_right("▸", &style, inner, ui.theme.text_dim);
                    if open {
                        sub_anchor = Some((i, rect.min.y));
                    }
                }
                Item::OpPanel { op, label, props, applied } => {
                    ui.label_dim(label);
                    let changed = ui.props_panel(&mut **props);
                    let mut apply = false;
                    ui.row(|ui| {
                        if ui.button(if *applied { "Apply Again" } else { "Apply" }).clicked {
                            apply = true;
                        }
                    });
                    if apply || (changed && *applied) {
                        let mut ctx = Ctx::new(host.doc);
                        ctx.pointer = host.pointer;
                        ctx.view = host.view;
                        let ok = if *applied && !apply {
                            match host.exec.last_step_props() {
                                Some((id, step_props)) if id == op.as_str() => {
                                    *step_props = props.clone();
                                    host.exec.adjust_last(&mut ctx).is_ok()
                                }
                                _ => host.exec.run(op, Some(props.clone()), &mut ctx).is_ok(),
                            }
                        } else {
                            host.exec.run(op, Some(props.clone()), &mut ctx).is_ok()
                        };
                        host.requests.append(&mut host.exec.requests);
                        if ok {
                            *applied = true;
                        }
                        ui.state.request_rebuild = true;
                    }
                    ui.separator();
                }
                Item::ObjectProps(id) => {
                    let before = host.doc.clone();
                    let dragging = ui.state.down;
                    let mut changed = false;
                    if let Some(o) = host.doc.objects.get_mut(*id) {
                        changed = ui.props_panel(o);
                    }
                    if changed {
                        record_edit(host.exec, host.doc, before, "Edit Object", dragging);
                    }
                }
            }
            ui.pop_id();
        }
    }
    let bottom = ui.cursor().y + m.pad;
    let measured = (bottom - panel.min.y).max(m.widget_h * 2.0);
    if (measured - menu.height).abs() > 0.5 {
        menu.height = measured;
        ui.state.request_rebuild = true;
    }
    let panel_rect = Rect::from_min_size(panel.min, Vec2::new(width, measured));
    menu.open_sub = new_open_sub;

    // ---- submenu ---------------------------------------------------------------
    if let (Some(si), Some((_, anchor_y))) = (menu.open_sub, sub_anchor)
        && let Some(Item::Sub { items, .. }) = menu.tabs.get(tab).and_then(|t| t.items.get(si)).cloned()
    {
        let sub_y = anchor_y.min(window.max.y - m.widget_h * (items.len() as f64 + 0.5) - m.pad * 2.0).max(window.min.y);
        let sub_rect = Rect::from_min_size(Vec2::new(sub_x, sub_y), Vec2::new(sub_w, m.widget_h * items.len() as f64 + m.gap * (items.len() as f64 - 1.0).max(0.0) + m.pad * 2.0));
        ui.set_cursor(sub_rect.min + Vec2::splat(m.pad));
        ui.set_avail_width(sub_rect.width() - m.pad * 2.0);
        ui.push_id("sub");
        for (j, item) in items.iter().enumerate() {
            ui.push_index(j);
            if let Item::Action { label, op, overrides } = item
                && ui.selectable(label, false).clicked
            {
                host.run(op, overrides);
                out.close = true;
            }
            ui.pop_id();
        }
        ui.pop_id();
        ui.draw.set_layer(1);
        ui.floating_panel(sub_rect, ui.theme.header);
        ui.draw.set_layer(2);
    }

    // ---- tool strip -------------------------------------------------------------
    let mut ty = strip.min.y;
    for (i, t) in menu.tools.iter().enumerate() {
        let rect = Rect::from_min_size(Vec2::new(strip.min.x, ty), Vec2::splat(strip_w));
        let lit = t.active.then(|| match t.tint {
            Tint::Accent => (ui.theme.accent, ui.theme.accent_text),
            Tint::Mode { edit } => (ui.theme.mode_color(edit), ui.theme.mode_text(edit)),
        });
        let r = ui.icon_button_in(ui.id("tool").with_index(i), rect, t.icon, lit);
        if r.clicked {
            host.run(&t.op, &t.overrides);
            out.refresh = true;
            ui.state.request_rebuild = true;
        }
        ty += strip_w + m.gap;
    }

    // ---- panel underneath, outlined in the mode colour ---------------------------
    let edit = host.doc.active_object().is_some_and(|o| o.mode == ObjectMode::Edit);
    ui.draw.set_layer(1);
    ui.floating_panel(panel_rect, ui.theme.header);
    let outline = m.px(2.0);
    ui.draw.stroke_rect(panel_rect, outline, m.radius, ui.theme.mode_color(edit));
    ui.draw.set_layer(saved_layer);

    ui.pop_id();
    ui.draw.pop_clip();
    ui.set_clip(saved_clip);
    ui.set_layer_internal(saved_layer);
    if out.close {
        ui.state.focus = None;
        ui.state.request_rebuild = true;
    }
    out
}
