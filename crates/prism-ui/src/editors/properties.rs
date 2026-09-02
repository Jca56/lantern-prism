//! Properties: the active object, its data, the scene, and the undo history,
//! plus "Adjust Last Operation". Every panel is generated from `props!`.

use prism_doc::DataKind;
use prism_math::Vec2;
use prism_ops::Ctx;

use crate::editors::EditorCtx;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

#[derive(Clone, Debug, Default)]
pub struct PropertiesState {
    pub tab: usize,
}

const TABS: [&str; 4] = ["Object", "Data", "Scene", "History"];

pub fn draw(ui: &mut Ui, ctx: &mut EditorCtx) {
    ui.tabs(&mut ctx.properties.tab, &TABS);
    ui.space(ui.m.gap);
    let tab = ctx.properties.tab;
    ui.scroll_area("props", None, |ui| {
        match tab {
            0 => object_tab(ui, ctx),
            1 => data_tab(ui, ctx),
            2 => scene_tab(ui, ctx),
            _ => history_tab(ui, ctx),
        }
        ui.separator();
        adjust_last(ui, ctx);
    });
}

fn object_tab(ui: &mut Ui, ctx: &mut EditorCtx) {
    let id = ctx.doc.active_object_id();
    let Some(obj) = ctx.doc.objects.get(id).cloned() else {
        ui.label_dim("No active object");
        return;
    };
    ui.heading(&obj.name);
    let before = ctx.doc.clone();
    let dragging = ui.state.down;
    let mut changed = false;
    if let Some(o) = ctx.doc.objects.get_mut(id) {
        changed = ui.props_panel(o);
    }
    if changed {
        ctx.record_edit(before, "Edit Object", dragging);
    }
}

fn data_tab(ui: &mut Ui, ctx: &mut EditorCtx) {
    let id = ctx.doc.active_object_id();
    let Some(obj) = ctx.doc.objects.get(id).cloned() else {
        ui.label_dim("No active object");
        return;
    };
    let before = ctx.doc.clone();
    let dragging = ui.state.down;
    let mut changed = false;
    match obj.kind {
        DataKind::Mesh => {
            if let Some(block) = ctx.doc.meshes.get_mut(obj.data) {
                let m = &block.mesh;
                ui.heading(&format!("Mesh · {}", block.props.name));
                ui.label_dim(&format!("{} vertices, {} edges, {} faces", m.vert_count(), m.edge_count(), m.face_count()));
                changed |= ui.props_panel(&mut block.props);
            }
            let mut mats: Vec<prism_core::Id> = ctx.doc.meshes.get(obj.data).map(|b| b.props.materials.clone()).unwrap_or_default();
            if !mats.is_empty() {
                ui.heading("Materials");
                for mid in mats.drain(..) {
                    if let Some(mat) = ctx.doc.materials.get_mut(mid) {
                        let name = mat.name.clone();
                        ui.collapsing(&name, |ui| changed |= ui.props_panel(mat));
                    }
                }
            }
        }
        DataKind::Camera => {
            if let Some(c) = ctx.doc.cameras.get_mut(obj.data) {
                ui.heading("Camera");
                changed |= ui.props_panel(c);
            }
        }
        DataKind::Light => {
            if let Some(l) = ctx.doc.lights.get_mut(obj.data) {
                ui.heading("Light");
                changed |= ui.props_panel(l);
            }
        }
        DataKind::Empty => {
            ui.label_dim("Empty object: no data");
        }
    }
    if changed {
        ctx.record_edit(before, "Edit Data", dragging);
    }
}

fn scene_tab(ui: &mut Ui, ctx: &mut EditorCtx) {
    let sid = ctx.doc.active_scene;
    let before = ctx.doc.clone();
    let dragging = ui.state.down;
    let mut changed = false;
    if let Some(s) = ctx.doc.scenes.get_mut(sid) {
        ui.heading("Scene");
        changed = ui.props_panel(s);
    }
    if changed {
        ctx.record_edit(before, "Edit Scene", dragging);
    }
}

fn history_tab(ui: &mut Ui, ctx: &mut EditorCtx) {
    let (labels, cursor) = {
        let (l, c) = ctx.exec.history.labels();
        (l.iter().map(|s| s.to_string()).collect::<Vec<_>>(), c)
    };
    ui.heading(&format!("Undo History · {} steps", labels.len()));
    let stats = ctx.exec.history.stats();
    ui.label_dim(&format!("{:.1} MB of mesh storage across snapshots", stats.unique_mesh_bytes as f64 / 1.0e6));
    let mut jump: Option<usize> = None;
    let rect = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
    let r = ui.interact(ui.id("origin"), rect, Sense::CLICK);
    let style = ui.text_style();
    if cursor == 0 {
        ui.fill_shaded(rect, ui.theme.selection);
        ui.text_in_rect("  Original", &style, rect, ui.theme.selection_text);
    } else {
        if r.hovered {
            let bg = ui.theme.hover(ui.theme.panel);
            ui.fill(rect, bg);
        }
        ui.text_in_rect("  Original", &style, rect, ui.theme.text_dim);
    }
    if r.clicked {
        jump = Some(0);
    }
    for (i, label) in labels.iter().enumerate() {
        ui.push_index(i);
        let rect = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
        let r = ui.interact(ui.id("step"), rect, Sense::CLICK);
        if r.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        let applied = i < cursor;
        if i + 1 == cursor {
            ui.fill_shaded(rect, ui.theme.selection);
            ui.text_in_rect(&format!("  {label}"), &style, rect, ui.theme.selection_text);
        } else {
            if r.hovered {
                let bg = ui.theme.hover(ui.theme.panel);
                ui.fill(rect, bg);
            }
            ui.text_in_rect(&format!("  {label}"), &style, rect, if applied { ui.theme.text } else { ui.theme.text_dim });
        }
        if r.clicked {
            jump = Some(i + 1);
        }
        ui.pop_id();
    }
    if let Some(target) = jump {
        while ctx.exec.history.cursor() > target && ctx.exec.undo(ctx.doc) {}
        while ctx.exec.history.cursor() < target && ctx.exec.redo(ctx.doc) {}
        ui.state.request_rebuild = true;
    }
}

fn adjust_last(ui: &mut Ui, ctx: &mut EditorCtx) {
    let Some((op_id, label)) = ctx.exec.history.last().map(|s| (s.op_id.clone(), s.label.clone())) else {
        return;
    };
    if op_id == "ui.edit" {
        return;
    }
    let dragging = ui.state.down;
    let mut changed = false;
    ui.collapsing(&format!("Adjust Last Operation · {label}"), |ui| {
        if let Some((_, props)) = ctx.exec.last_step_props() {
            changed = ui.props_panel(&mut **props);
        }
    });
    if changed {
        let mut op_ctx = Ctx::new(ctx.doc);
        let _ = ctx.exec.adjust_last(&mut op_ctx);
        ctx.requests.append(&mut ctx.exec.requests);
        let _ = dragging;
        ui.state.request_rebuild = true;
    }
}
