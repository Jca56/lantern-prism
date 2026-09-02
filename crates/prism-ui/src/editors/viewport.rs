//! The 3D viewport editor: navigation, click and box selection, the gizmo,
//! and the header controls. Drawing happens in `prism-viewport`; this records
//! what to draw and where.

use prism_doc::{ObjectMode, SelectMode};
use prism_math::{Color, Rect, Vec2};
use prism_ops::{UiRequest, ViewInfo};
use prism_viewport::{Camera, Drag, GizmoMode, PickMode, PickPurpose, PickRequest, Shading, ViewColors, ViewPreset, ViewportRequest};

use crate::editors::{EditorCtx, gizmo};
use crate::icons::{Icon, gizmo_icon};
use crate::state::CursorIcon;
use crate::theme::{Metrics, Theme};
use crate::ui::{Sense, Ui};

/// How far (logical px) the pointer may wander between press and release
/// and still count as a click rather than a drag.
const CLICK_SLOP: f64 = 8.0;

/// What an operator sees of this viewport: its camera as plain matrices
/// over `rect`, the body in window pixels.
pub fn view_info(camera: &Camera, rect: Rect) -> ViewInfo {
    let aspect = rect.width() / rect.height().max(1.0);
    ViewInfo::new(rect, camera.view_proj(aspect), camera.position(), camera.forward(), camera.ortho)
}

/// Colors the viewport takes from the theme.
pub fn view_colors(theme: &Theme, m: &Metrics) -> ViewColors {
    ViewColors {
        bg: theme.field.scale_rgb(1.7),
        grid_minor: theme.border_light.fade(0.22),
        grid_major: theme.border_light.fade(0.5),
        axis_x: Color::hex(0xC0483E),
        axis_z: Color::hex(0x3E74C0),
        wire: theme.text_dim.fade(0.75),
        vertex: theme.text,
        selected: theme.selection.scale_rgb(1.25),
        active: theme.focus.scale_rgb(1.15),
        default_object: Color::hex(0xB4B4B8),
        point_size: m.px(7.0),
    }
}

fn pick_mode(ctx: &EditorCtx) -> PickMode {
    let doc = &*ctx.doc;
    let editing = doc.active_object().is_some_and(|o| o.mode == ObjectMode::Edit) && doc.object_mesh(doc.active_object_id()).is_some();
    if !editing {
        return PickMode::Object;
    }
    match doc.scene().map_or(SelectMode::Vertex, |s| s.tool.select_mode) {
        SelectMode::Vertex => PickMode::Vertex,
        SelectMode::Edge => PickMode::Edge,
        SelectMode::Face => PickMode::Face,
    }
}

/// Header controls as icon buttons: shading, grid, frame all, the gizmo;
/// then the wire overlay and the view menu.
pub fn header(ui: &mut Ui, ctx: &mut EditorCtx) {
    let vp = &mut *ctx.viewport;
    let gap = ui.m.gap;
    if ui.icon_button("solid", Icon::Solid, vp.shading == Shading::Solid).clicked {
        vp.shading = Shading::Solid;
    }
    if ui.icon_button("wireframe", Icon::Wire, vp.shading == Shading::Wire).clicked {
        vp.shading = Shading::Wire;
    }
    if ui.icon_button("grid", Icon::Grid, vp.overlays.grid).clicked {
        vp.overlays.grid = !vp.overlays.grid;
    }
    if ui.icon_button("frame", Icon::Frame, false).clicked {
        ctx.requests.push(UiRequest::ViewFrame { selected: false });
    }
    ui.alloc(Vec2::new(gap, 1.0));
    for g in GizmoMode::ALL {
        if ui.icon_button(g.label(), gizmo_icon(g), vp.gizmo == g).clicked {
            vp.gizmo = g;
        }
    }
    ui.alloc(Vec2::new(gap, 1.0));
    ui.toggle("Wire", &mut vp.overlays.wire);
    if let Some(i) = ui.menu_button("View", &["Perspective / Ortho", "Front", "Back", "Right", "Left", "Top", "Bottom", "Frame All", "Frame Selected"]) {
        match i {
            0 => vp.camera.ortho = !vp.camera.ortho,
            1 => vp.camera.set_view(ViewPreset::Front),
            2 => vp.camera.set_view(ViewPreset::Back),
            3 => vp.camera.set_view(ViewPreset::Right),
            4 => vp.camera.set_view(ViewPreset::Left),
            5 => vp.camera.set_view(ViewPreset::Top),
            6 => vp.camera.set_view(ViewPreset::Bottom),
            7 => ctx.requests.push(UiRequest::ViewFrame { selected: false }),
            _ => ctx.requests.push(UiRequest::ViewFrame { selected: true }),
        }
    }
}

pub fn draw(ui: &mut Ui, ctx: &mut EditorCtx) {
    let rect = ui.clip();
    let colors = view_colors(ui.theme, &ui.m);
    let mode = pick_mode(ctx);

    // The gizmo gets first claim on a press; a handle drag starts a transform.
    gizmo::draw(ui, ctx, rect);
    let vp = &mut *ctx.viewport;

    // ---- the pointer on the body ----------------------------------------------
    let r = ui.interact(ui.id("body"), rect, Sense::DRAG);
    let st = &mut *ui.state;
    let over = st.pointer_in_window && rect.contains(st.pointer) && st.popup.is_none_or(|(p, _)| !p.contains(st.pointer));
    let dragged = (st.pointer - st.press_pos).length() > ui.m.px(CLICK_SLOP);

    // A left drag past the slop commits to one thing for the rest of the
    // press, chosen by the modifiers held as it begins: Ctrl draws a selection
    // box, Shift pans, plain orbits. The middle button orbits (Shift pans) as
    // well, for hands that know it.
    if r.held && vp.drag == Drag::None && dragged {
        vp.drag = if st.mods.ctrl() {
            Drag::Box
        } else if st.mods.shift() {
            Drag::Pan
        } else {
            Drag::Orbit
        };
    }
    if st.middle_pressed && rect.contains(st.middle_press_pos) && st.popup.is_none() {
        vp.drag = if st.mods.shift() { Drag::Pan } else { Drag::Orbit };
    }
    let holding = r.held || st.middle_down;
    match vp.drag {
        Drag::Orbit if holding => {
            vp.camera.orbit(st.delta.x, st.delta.y);
            st.cursor_icon = CursorIcon::Grabbing;
        }
        Drag::Pan if holding => {
            vp.camera.pan(st.delta.x, st.delta.y, rect.height());
            st.cursor_icon = CursorIcon::Grabbing;
        }
        _ => {}
    }
    if over && st.wheel.y != 0.0 {
        vp.camera.zoom(st.wheel.y / ui.m.widget_h);
        st.wheel = Vec2::ZERO;
    }

    let (area, camera, pos, radius) = (ctx.area, vp.camera, st.pointer, ui.m.px(14.0));
    let request = move |purpose: PickPurpose, extend: bool, toggle: bool, region: Rect| PickRequest { purpose, area, rect, camera, pos, mode, radius, extend, toggle, region, colors };

    // ---- right click: context menu for what is under the pointer ---------------
    if st.right_pressed && over {
        ctx.picks.push(request(PickPurpose::ContextMenu, false, false, Rect::ZERO));
    }

    // ---- selection: a click picks; a Ctrl-drag boxes (D025) ---------------------
    let region = Rect::new(st.press_pos.min(st.pointer), st.press_pos.max(st.pointer)).intersection(&rect);
    if vp.drag == Drag::Box && r.held {
        let c = ui.theme.selection;
        ui.draw.rect(region, c.fade(0.18));
        ui.draw.stroke_rect(region, ui.m.px(2.0), 0.0, c);
    }
    if r.released {
        match vp.drag {
            // Ctrl+Shift extends the selection, Ctrl+Alt subtracts from it.
            Drag::Box if !region.is_empty() => ctx.picks.push(request(PickPurpose::Box, st.mods.shift(), st.mods.alt(), region)),
            Drag::None if r.clicked && !dragged => ctx.picks.push(request(PickPurpose::Select, st.mods.shift(), st.mods.ctrl(), Rect::ZERO)),
            _ => {}
        }
    }
    if !holding {
        vp.drag = Drag::None;
    }

    // ---- what to draw ----------------------------------------------------------
    ctx.viewports.push(ViewportRequest { area: ctx.area, rect, state: *vp, colors });

    // Overlays on top of the 3D: an inner shadow so the well reads as sunk,
    // and the mode in the corner.
    let d = ui.m.px(15.0);
    ui.draw.rect_gradient(Rect::new(rect.min, Vec2::new(rect.max.x, rect.min.y + d)), Color::BLACK.fade(0.4), Color::TRANSPARENT);
    ui.draw.rect(Rect::new(rect.min, Vec2::new(rect.min.x + d, rect.max.y)), Color::BLACK.fade(0.1));
    let label = match mode {
        PickMode::Object => "Object Mode",
        PickMode::Vertex => "Edit Mode · Vertex",
        PickMode::Edge => "Edit Mode · Edge",
        PickMode::Face => "Edit Mode · Face",
    };
    let label = format!("{label} · {}", vp.gizmo.label());
    let style = ui.text_style();
    let pad = ui.m.pad;
    ui.text_at(&label, &style, Vec2::new(rect.min.x + pad * 2.0, rect.min.y + pad), rect.width(), ui.theme.text_dim);
}
