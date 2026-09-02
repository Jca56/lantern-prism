//! The transform gizmo (D024): a 2D overlay projected from the selection's
//! pivot. One mode shows at a time — Move arrows, Rotate rings or Scale
//! handles — each with a free handle in the middle. Hovering lights a handle;
//! pressing one starts the matching transform operator constrained to that
//! axis, which runs as a modal until the button is released.

use prism_math::{Color, Rect, Vec2, Vec3};
use prism_ops::{Flow, ViewInfo};
use prism_props::Value;
use prism_viewport::{GizmoHandle, GizmoMode};

use crate::editors::{EditorCtx, invoke_op};
use crate::event::{Event, MouseButton};
use crate::state::CursorIcon;
use crate::theme::Metrics;
use crate::ui::Ui;

/// Arrow length and ring radius, logical px.
const LENGTH: f64 = 110.0;
/// How close the pointer must be to grab a handle, logical px.
const GRAB: f64 = 16.0;
const STROKE: f64 = 4.0;
const HEAD: f64 = 20.0;
/// Radius of the free handle in the middle.
const CENTER: f64 = 14.0;
/// The free rotation ring sits outside the axis rings.
const OUTER_RING: f64 = 1.3;
const RING_STEPS: usize = 48;

const AXES: [GizmoHandle; 3] = [GizmoHandle::X, GizmoHandle::Y, GizmoHandle::Z];

fn axis_dir(h: GizmoHandle) -> Vec3 {
    match h {
        GizmoHandle::X => Vec3::X,
        GizmoHandle::Y => Vec3::Y,
        GizmoHandle::Z => Vec3::Z,
        GizmoHandle::Free => Vec3::ZERO,
    }
}

fn axis_color(h: GizmoHandle) -> Color {
    match h {
        GizmoHandle::X => Color::hex(0xF0524A),
        GizmoHandle::Y => Color::hex(0x7ED957),
        GizmoHandle::Z => Color::hex(0x4C8DF5),
        GizmoHandle::Free => Color::hex(0xF2F2F4),
    }
}

/// `Axis` prop discriminant of `transform.*` for a handle.
fn axis_index(h: GizmoHandle) -> i64 {
    match h {
        GizmoHandle::Free => 0,
        GizmoHandle::X => 1,
        GizmoHandle::Y => 2,
        GizmoHandle::Z => 3,
    }
}

pub(crate) enum Shape {
    /// From the pivot to the tip.
    Segment(Vec2, Vec2),
    /// A closed loop.
    Ring(Vec<Vec2>),
    Disc(Vec2, f64),
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 <= 1e-12 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    (a + ab * t).distance(p)
}

impl Shape {
    pub(crate) fn distance(&self, p: Vec2) -> f64 {
        match self {
            Shape::Segment(a, b) => seg_dist(p, *a, *b),
            Shape::Ring(pts) => (0..pts.len()).map(|i| seg_dist(p, pts[i], pts[(i + 1) % pts.len()])).fold(f64::INFINITY, f64::min),
            Shape::Disc(c, r) => (c.distance(p) - r).max(0.0),
        }
    }
}

pub(crate) struct Handle {
    pub which: GizmoHandle,
    pub shape: Shape,
}

/// Screen-space handles for `mode` around `pivot`. `None` when the pivot is
/// off screen. Sizes are constant in pixels, so the gizmo never shrinks with
/// distance.
pub(crate) fn handles(mode: GizmoMode, view: &ViewInfo, pivot: Vec3, m: &Metrics) -> Option<Vec<Handle>> {
    let c = view.project(pivot)?;
    let upp = view.units_per_pixel(pivot);
    if upp <= 0.0 {
        return None;
    }
    let len = m.px(LENGTH) * upp;
    let mut out = Vec::with_capacity(4);
    match mode {
        GizmoMode::Move | GizmoMode::Scale => {
            for h in AXES {
                if let Some(tip) = view.project(pivot + axis_dir(h) * len) {
                    out.push(Handle { which: h, shape: Shape::Segment(c, tip) });
                }
            }
            out.push(Handle { which: GizmoHandle::Free, shape: Shape::Disc(c, m.px(CENTER)) });
        }
        GizmoMode::Rotate => {
            for h in AXES {
                let (u, v) = axis_dir(h).orthonormal_basis();
                let pts: Option<Vec<Vec2>> = (0..RING_STEPS)
                    .map(|i| {
                        let t = i as f64 / RING_STEPS as f64 * core::f64::consts::TAU;
                        view.project(pivot + (u * t.cos() + v * t.sin()) * len)
                    })
                    .collect();
                if let Some(pts) = pts {
                    out.push(Handle { which: h, shape: Shape::Ring(pts) });
                }
            }
            let r = m.px(LENGTH) * OUTER_RING;
            let pts = (0..64).map(|i| c + Vec2::from_angle(i as f64 / 64.0 * core::f64::consts::TAU) * r).collect();
            out.push(Handle { which: GizmoHandle::Free, shape: Shape::Ring(pts) });
        }
    }
    Some(out)
}

/// The nearest handle within reach of `p`.
fn nearest(handles: &[Handle], p: Vec2, grab: f64) -> Option<usize> {
    handles
        .iter()
        .enumerate()
        .map(|(i, h)| (i, h.shape.distance(p)))
        .filter(|(_, d)| *d <= grab)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Interact with and draw the gizmo for the selection in this viewport.
pub fn draw(ui: &mut Ui, ctx: &mut EditorCtx, rect: Rect) {
    let Some(view) = ctx.view else {
        return;
    };
    let Some(pivot) = prism_ops::builtin::transform::pivot(ctx.doc) else {
        return;
    };
    let mode = ctx.viewport.gizmo;
    let Some(handles) = handles(mode, &view, pivot, &ui.m) else {
        return;
    };
    let gid = ui.id("gizmo");
    let grab = ui.m.px(GRAB);
    let modal = ctx.exec.is_modal();
    let st = &mut *ui.state;
    let clear = |p: Vec2| rect.contains(p) && st.popup.is_none_or(|(r, _)| !r.contains(p));

    // Hover: only while nothing else is held and no transform is running.
    let hover = if !modal && st.pointer_in_window && st.active.is_none() && clear(st.pointer) { nearest(&handles, st.pointer, grab) } else { None };
    if hover.is_some() {
        st.cursor_icon = CursorIcon::Pointer;
    }

    // Press on a handle: start its transform as a drag (confirms on release).
    if !modal
        && st.pressed
        && !st.press_claimed
        && clear(st.press_pos)
        && let Some(i) = nearest(&handles, st.press_pos, grab)
    {
        st.press_claimed = true;
        st.active = Some(gid);
        let which = handles[i].which;
        let (op, overrides) = match mode {
            GizmoMode::Move => ("transform.translate", vec![("axis", Value::Enum(axis_index(which)))]),
            GizmoMode::Scale => ("transform.scale", vec![("axis", Value::Enum(axis_index(which)))]),
            GizmoMode::Rotate => ("transform.rotate", vec![("axis", Value::Vec3(axis_dir(which)))]),
        };
        let press = Event::Button { button: MouseButton::Left, pressed: true, pos: st.press_pos, mods: st.mods };
        let flow = invoke_op(ctx.doc, ctx.exec, st.press_pos, Some(view), op, &overrides, ctx.requests, &press);
        ctx.viewport.gizmo_drag = matches!(flow, Ok(Flow::Running)).then_some(which);
        st.request_rebuild = true;
    }
    if !ctx.exec.is_modal() {
        ctx.viewport.gizmo_drag = None;
    }

    // ---- draw ---------------------------------------------------------------
    let active = ctx.viewport.gizmo_drag;
    let m = ui.m;
    let stroke = m.px(STROKE);
    for (i, h) in handles.iter().enumerate() {
        let lit = hover == Some(i) || active == Some(h.which);
        let mut color = if lit { Color::hex(0xFFFFFF) } else { axis_color(h.which) };
        if active.is_some() && active != Some(h.which) {
            color = color.fade(0.3);
        }
        match &h.shape {
            Shape::Segment(a, b) => {
                let dir = (*b - *a).normalize_or_zero();
                match mode {
                    GizmoMode::Move => {
                        let head = m.px(HEAD);
                        let base = *b - dir * head;
                        let side = dir.perp() * (head * 0.45);
                        ui.draw.line(*a, base + dir * (head * 0.2), stroke, color);
                        ui.draw.triangle(*b, base + side, base - side, color);
                    }
                    _ => {
                        let s = m.px(CENTER);
                        ui.draw.line(*a, *b - dir * (s * 0.5), stroke, color);
                        ui.draw.rounded_rect(Rect::from_center_size(*b, Vec2::splat(s)), m.px(2.0), color);
                    }
                }
            }
            Shape::Ring(pts) => ui.draw.polyline(pts, stroke, color, true),
            Shape::Disc(c, r) => {
                let r_rect = Rect::from_center_size(*c, Vec2::splat(*r * 2.0));
                ui.draw.rounded_rect(r_rect, *r, color.fade(if lit { 0.9 } else { 0.45 }));
                ui.draw.stroke_rect(r_rect, m.px(2.0), *r, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use prism_math::Mat4;

    /// Looking down −Z, orthographic, 800×800 px over ±5 units: 80 px per unit.
    fn view() -> ViewInfo {
        let rect = Rect::from_xywh(0.0, 0.0, 800.0, 800.0);
        let eye = Vec3::new(0.0, 0.0, 10.0);
        let vp = Mat4::orthographic_reverse_z(-5.0, 5.0, -5.0, 5.0, 0.1, 100.0) * Mat4::look_at(eye, Vec3::ZERO, Vec3::Y);
        ViewInfo::new(rect, vp, eye, Vec3::new(0.0, 0.0, -1.0), true)
    }

    #[test]
    fn move_handles_are_screen_sized_and_grabbable() {
        let m = Theme::default().metrics(1.0);
        let hs = handles(GizmoMode::Move, &view(), Vec3::ZERO, &m).unwrap();
        assert_eq!(hs.len(), 4);
        let Shape::Segment(a, b) = &hs[0].shape else { panic!("X is an arrow") };
        assert!((*a - Vec2::new(400.0, 400.0)).length() < 1e-6);
        assert!((*b - Vec2::new(510.0, 400.0)).length() < 1e-6, "110 px long regardless of zoom: {b:?}");
        // Y points up on screen; Z points at the camera, so it collapses.
        let Shape::Segment(_, y_tip) = &hs[1].shape else { panic!() };
        assert!((*y_tip - Vec2::new(400.0, 290.0)).length() < 1e-6);
        assert_eq!(nearest(&hs, Vec2::new(470.0, 406.0), 16.0).map(|i| hs[i].which), Some(GizmoHandle::X));
        assert_eq!(nearest(&hs, Vec2::new(404.0, 396.0), 16.0).map(|i| hs[i].which), Some(GizmoHandle::Free), "the disc wins near the pivot");
        assert_eq!(nearest(&hs, Vec2::new(600.0, 600.0), 16.0), None);
    }

    #[test]
    fn rotate_rings_and_distances() {
        let m = Theme::default().metrics(1.0);
        let hs = handles(GizmoMode::Rotate, &view(), Vec3::ZERO, &m).unwrap();
        assert_eq!(hs.len(), 4);
        // The Z ring lies in the view plane: a 110 px circle around the pivot.
        let z = hs.iter().find(|h| h.which == GizmoHandle::Z).unwrap();
        assert!(z.shape.distance(Vec2::new(510.0, 400.0)) < 1.0);
        assert!(z.shape.distance(Vec2::new(400.0, 400.0)) > 100.0);
        assert!(seg_dist(Vec2::new(0.0, 5.0), Vec2::ZERO, Vec2::new(10.0, 0.0)) - 5.0 < 1e-9);
        assert!(seg_dist(Vec2::new(20.0, 0.0), Vec2::ZERO, Vec2::new(10.0, 0.0)) - 10.0 < 1e-9, "clamped to the end");
    }
}
