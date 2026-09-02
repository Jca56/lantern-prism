//! The 3D viewport editor: navigation, click selection, header controls.
//! Drawing happens in `prism-viewport`; this records what to draw and where.

use prism_doc::{ObjectMode, SelectMode};
use prism_math::{Color, Rect, Vec2};
use prism_ops::UiRequest;
use prism_viewport::{PickMode, PickRequest, Shading, ViewColors, ViewPreset, ViewportRequest};

use crate::editors::EditorCtx;
use crate::state::CursorIcon;
use crate::theme::{Metrics, Theme};
use crate::ui::{Sense, Ui};

/// How far (logical px) the pointer may wander between press and release
/// and still count as a click rather than a drag.
const CLICK_SLOP: f64 = 8.0;

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

/// Header controls: shading, overlays, view presets, frame.
pub fn header(ui: &mut Ui, ctx: &mut EditorCtx) {
    let vp = &mut *ctx.viewport;
    let mut shading = if vp.shading == Shading::Wire { 1 } else { 0 };
    if ui.dropdown("shading", &mut shading, &["Solid", "Wireframe"]) {
        vp.shading = if shading == 1 { Shading::Wire } else { Shading::Solid };
    }
    ui.toggle("Wire", &mut vp.overlays.wire);
    ui.toggle("Grid", &mut vp.overlays.grid);
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
    let vp = &mut *ctx.viewport;

    // ---- navigation -------------------------------------------------------
    let r = ui.interact(ui.id("body"), rect, Sense::DRAG);
    let st = &mut *ui.state;
    let over = st.pointer_in_window && rect.contains(st.pointer) && st.popup.is_none_or(|(p, _)| !p.contains(st.pointer));
    let alt = st.mods.alt();
    // Middle drag: orbit, Shift for pan.
    if st.middle_pressed && rect.contains(st.middle_press_pos) && st.popup.is_none() {
        vp.nav = if st.mods.shift() { prism_viewport::Nav::Pan } else { prism_viewport::Nav::Orbit };
    }
    if !(st.middle_down || (r.held && alt)) {
        vp.nav = prism_viewport::Nav::None;
    }
    if r.pressed && alt {
        vp.nav = if st.mods.shift() { prism_viewport::Nav::Pan } else { prism_viewport::Nav::Orbit };
    }
    let dragging_nav = (st.middle_down || (r.held && alt)) && vp.nav != prism_viewport::Nav::None;
    if dragging_nav {
        let d = st.delta;
        match vp.nav {
            prism_viewport::Nav::Orbit => vp.camera.orbit(d.x, d.y),
            prism_viewport::Nav::Pan => vp.camera.pan(d.x, d.y, rect.height()),
            prism_viewport::Nav::None => {}
        }
        st.cursor_icon = CursorIcon::Grabbing;
    }
    if over && st.wheel.y != 0.0 {
        vp.camera.zoom(st.wheel.y / ui.m.widget_h);
        st.wheel = Vec2::ZERO;
    }

    // ---- right click: context menu for what is under the pointer -------------
    if st.right_pressed && over {
        ctx.picks.push(PickRequest {
            purpose: prism_viewport::PickPurpose::ContextMenu,
            area: ctx.area,
            rect,
            camera: vp.camera,
            pos: st.pointer,
            mode,
            radius: ui.m.px(14.0),
            extend: false,
            toggle: false,
            colors,
        });
    }

    // ---- click selection ----------------------------------------------------
    if r.clicked && !alt && (st.pointer - st.press_pos).length() <= ui.m.px(CLICK_SLOP) {
        ctx.picks.push(PickRequest {
            purpose: prism_viewport::PickPurpose::Select,
            area: ctx.area,
            rect,
            camera: vp.camera,
            pos: st.pointer,
            mode,
            radius: ui.m.px(14.0),
            extend: st.mods.shift(),
            toggle: st.mods.ctrl(),
            colors,
        });
    }

    // ---- what to draw --------------------------------------------------------
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
    let style = ui.text_style();
    let pad = ui.m.pad;
    ui.text_at(label, &style, Vec2::new(rect.min.x + pad * 2.0, rect.min.y + pad), rect.width(), ui.theme.text_dim);
}
