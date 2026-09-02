//! The outliner: every object of the active scene. Click selects (Ctrl or
//! Shift extends), double-click renames, the button opens the add menu.

use prism_core::Id;
use prism_doc::DataKind;
use prism_math::{Rect, Vec2};
use prism_ops::UiRequest;
use prism_props::Value;

use crate::editors::EditorCtx;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

#[derive(Clone, Debug, Default)]
pub struct OutlinerState {
    /// Object being renamed and the text so far.
    pub renaming: Option<(Id, String)>,
}

fn kind_label(k: DataKind) -> &'static str {
    match k {
        DataKind::Empty => "Empty",
        DataKind::Mesh => "Mesh",
        DataKind::Camera => "Camera",
        DataKind::Light => "Light",
    }
}

pub fn draw(ui: &mut Ui, ctx: &mut EditorCtx) {
    ui.row(|ui| {
        let scene = ctx.doc.scene().map_or("No scene", |s| s.name.as_str()).to_owned();
        ui.label(&scene);
        let w = ui.avail_width();
        let bw = ui.m.px(120.0);
        ui.alloc(Vec2::new((w - bw - ui.m.gap).max(0.0), 1.0));
        if ui.button("+ Add").clicked {
            ctx.requests.push(UiRequest::Menu("add".into()));
        }
    });
    ui.separator();

    let ids = ctx.doc.scene_objects();
    let active = ctx.doc.active_object_id();
    let mut click: Option<(Id, bool, bool)> = None;
    let mut rename: Option<(Id, String)> = None;
    let mut commit_rename: Option<String> = None;
    let mut cancel_rename = false;

    ui.scroll_area("objects", None, |ui| {
        for (i, id) in ids.iter().enumerate() {
            let Some(obj) = ctx.doc.objects.get(*id) else {
                continue;
            };
            ui.push_index(i);
            let rect = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
            let is_renaming = ctx.outliner.renaming.as_ref().is_some_and(|(r, _)| r == id);
            if is_renaming {
                let (_, text) = ctx.outliner.renaming.as_mut().expect("renaming");
                let field_id = ui.id("rename");
                ui.state.focus = Some(field_id);
                let r = ui.text_edit_core(field_id, rect, text);
                if r.committed {
                    commit_rename = Some(text.clone());
                } else if r.cancelled || !r.focused {
                    cancel_rename = true;
                }
            } else {
                let r = ui.interact(ui.id("row"), rect, Sense::CLICK);
                if r.hovered {
                    ui.state.cursor_icon = CursorIcon::Pointer;
                }
                let mods = ui.state.mods;
                if r.double_clicked {
                    rename = Some((*id, obj.name.clone()));
                } else if r.clicked {
                    click = Some((*id, mods.ctrl() || mods.shift(), mods.ctrl()));
                }
                let style = ui.text_style();
                if obj.selected {
                    ui.fill(rect, ui.theme.accent);
                } else if r.hovered {
                    let bg = ui.widget_color(&r);
                    ui.fill(rect, bg);
                }
                let (fg, dim) = if obj.selected { (ui.theme.accent_text, ui.theme.accent_text) } else { (ui.theme.text, ui.theme.text_dim) };
                let inner = Rect::new(Vec2::new(rect.min.x + ui.m.pad, rect.min.y), Vec2::new(rect.max.x - ui.m.pad, rect.max.y));
                ui.text_in_rect(&obj.name, &style, inner, fg);
                let mode = if obj.mode == prism_doc::ObjectMode::Edit { " · Edit" } else { "" };
                ui.text_right(&format!("{}{mode}", kind_label(obj.kind)), &style, inner, dim);
                if *id == active {
                    ui.outline(rect, ui.m.border, ui.theme.focus);
                }
            }
            ui.pop_id();
        }
    });

    if let Some((id, extend, toggle)) = click {
        let _ = ctx.run("object.select", &[("id", Value::Id(id)), ("extend", Value::Bool(extend)), ("toggle", Value::Bool(toggle))]);
    }
    if let Some((id, name)) = rename {
        let _ = ctx.run("object.select", &[("id", Value::Id(id))]);
        ctx.outliner.renaming = Some((id, name));
        ui.state.request_rebuild = true;
    }
    if let Some(name) = commit_rename {
        ctx.outliner.renaming = None;
        ui.state.focus = None;
        let _ = ctx.run("object.rename", &[("name", Value::Str(name))]);
        ui.state.request_rebuild = true;
    } else if cancel_rename {
        ctx.outliner.renaming = None;
        ui.state.request_rebuild = true;
    }
}
