//! Transform operators driven by synthetic events against a fixed
//! orthographic view; shared fixtures for the extrude tests too.

use super::*;
use crate::executor::Executor;
use crate::input::Modifiers;
use prism_core::Id;
use prism_doc::Doc;
use prism_math::{Mat4, Rect};
use prism_props::Value;

/// Looking straight down −Z at the origin, orthographic, 800×800 px
/// covering ±5 units: one unit is 80 px, +X is right, +Y is up.
pub(crate) fn top_down_view() -> ViewInfo {
    let rect = Rect::from_xywh(0.0, 0.0, 800.0, 800.0);
    let eye = Vec3::new(0.0, 0.0, 10.0);
    let view = Mat4::look_at(eye, Vec3::ZERO, Vec3::Y);
    let proj = Mat4::orthographic_reverse_z(-5.0, 5.0, -5.0, 5.0, 0.1, 100.0);
    ViewInfo::new(rect, proj * view, eye, Vec3::new(0.0, 0.0, -1.0), true)
}

pub(crate) fn moved(p: Vec2) -> Event {
    Event::PointerMoved(p)
}
pub(crate) fn key(c: char) -> Event {
    Event::Key { key: Key::Char(c), pressed: true, repeat: false, mods: Modifiers::NONE }
}
pub(crate) fn escape() -> Event {
    Event::Key { key: Key::Escape, pressed: true, repeat: false, mods: Modifiers::NONE }
}
pub(crate) fn click() -> Event {
    Event::Button { button: MouseButton::Left, pressed: true, pos: Vec2::ZERO, mods: Modifiers::NONE }
}

pub(crate) fn ctx<'a>(doc: &'a mut Doc, pointer: Vec2) -> Ctx<'a> {
    let mut c = Ctx::new(doc);
    c.view = Some(top_down_view());
    c.pointer = pointer;
    c
}

pub(crate) fn selected_cube() -> (Doc, Executor, Id) {
    let mut doc = Doc::starter();
    let mut ex = Executor::with_builtins();
    let cube = doc.scene_objects()[0];
    ex.run_with("object.select", &[("id", Value::Id(cube))], &mut Ctx::new(&mut doc)).unwrap();
    (doc, ex, cube)
}

#[test]
fn view_maps_pixels_and_world_both_ways() {
    let v = top_down_view();
    let px = v.project(Vec3::new(1.0, 2.0, 0.0)).unwrap();
    assert!((px - Vec2::new(480.0, 240.0)).length() < 1e-6, "{px:?}");
    let w = v.on_view_plane(Vec3::ZERO, px).unwrap();
    assert!((w - Vec3::new(1.0, 2.0, 0.0)).length() < 1e-6, "{w:?}");
    let t = v.on_axis(Vec3::ZERO, Vec3::X, px).unwrap();
    assert!((t - 1.0).abs() < 1e-6, "{t}");
    assert!(v.on_axis(Vec3::ZERO, Vec3::Z, px).is_none(), "axis into the screen is unstable");
    assert!((v.units_per_pixel(Vec3::ZERO) - 1.0 / 80.0).abs() < 1e-9);
}

#[test]
fn move_follows_the_pointer_constrains_and_cancels() {
    let (mut doc, mut ex, cube) = selected_cube();
    let centre = Vec2::new(400.0, 400.0);
    let loc = |doc: &Doc| doc.objects.get(cube).unwrap().location;
    let steps = ex.history.len();
    let mut c = ctx(&mut doc, centre);
    assert_eq!(ex.invoke_with("transform.translate", &[], &mut c, &key('g')).unwrap(), Flow::Running);
    ex.modal_event(&mut c, &moved(Vec2::new(480.0, 400.0)));
    assert!((loc(c.doc) - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6);
    ex.modal_event(&mut c, &key('x'));
    ex.modal_event(&mut c, &moved(Vec2::new(480.0, 320.0)));
    assert!((loc(c.doc) - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6, "constrained to X");
    ex.modal_event(&mut c, &key('x'));
    assert!((loc(c.doc) - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-6, "free again: {:?}", loc(c.doc));
    assert!(ex.last_report.as_deref().unwrap().starts_with("Move"));
    assert_eq!(ex.modal_event(&mut c, &escape()), Some(Ok(Flow::Cancelled)));
    assert_eq!(loc(c.doc), Vec3::ZERO, "cancel restores");
    assert_eq!(ex.history.len(), steps);
    assert!(!ex.is_modal());

    // Confirm with a click, then adjust the recorded delta.
    let mut c = ctx(&mut doc, centre);
    ex.invoke_with("transform.translate", &[], &mut c, &key('g')).unwrap();
    ex.modal_event(&mut c, &moved(Vec2::new(480.0, 320.0)));
    assert_eq!(ex.modal_event(&mut c, &click()), Some(Ok(Flow::Finished)));
    assert_eq!(ex.history.len(), steps + 1);
    assert!((loc(&doc) - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-6);
    ex.last_step_props().unwrap().1.set_by_name("delta", Value::Vec3(Vec3::new(2.0, 0.0, 0.0))).unwrap();
    ex.adjust_last(&mut Ctx::new(&mut doc)).unwrap();
    assert!((loc(&doc) - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-6);
    ex.undo(&mut doc);
    assert_eq!(loc(&doc), Vec3::ZERO);
}

#[test]
fn drag_started_by_a_press_confirms_on_release() {
    let (mut doc, mut ex, cube) = selected_cube();
    let press = Event::Button { button: MouseButton::Left, pressed: true, pos: Vec2::new(400.0, 400.0), mods: Modifiers::NONE };
    let mut c = ctx(&mut doc, Vec2::new(400.0, 400.0));
    ex.invoke_with("transform.translate", &[("axis", Value::Enum(1))], &mut c, &press).unwrap();
    ex.modal_event(&mut c, &moved(Vec2::new(480.0, 480.0)));
    let release = Event::Button { button: MouseButton::Left, pressed: false, pos: Vec2::new(480.0, 480.0), mods: Modifiers::NONE };
    assert_eq!(ex.modal_event(&mut c, &release), Some(Ok(Flow::Finished)));
    let loc = doc.objects.get(cube).unwrap().location;
    assert!((loc - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6, "X-constrained from the start: {loc:?}");
}

#[test]
fn rotate_sweeps_about_the_view_axis_and_scale_uses_distance() {
    let (mut doc, mut ex, cube) = selected_cube();
    // Start to the right of the pivot, sweep a quarter turn counter-clockwise.
    let mut c = ctx(&mut doc, Vec2::new(480.0, 400.0));
    ex.invoke_with("transform.rotate", &[], &mut c, &key('r')).unwrap();
    ex.modal_event(&mut c, &moved(Vec2::new(456.0, 320.0)));
    ex.modal_event(&mut c, &moved(Vec2::new(400.0, 320.0)));
    assert_eq!(ex.modal_event(&mut c, &click()), Some(Ok(Flow::Finished)));
    let rot = doc.objects.get(cube).unwrap().rotation;
    assert!((rot.z - PI / 2.0).abs() < 1e-6 && rot.x.abs() < 1e-9 && rot.y.abs() < 1e-9, "{rot:?}");
    ex.undo(&mut doc);

    let mut c = ctx(&mut doc, Vec2::new(480.0, 400.0));
    ex.invoke_with("transform.scale", &[], &mut c, &key('s')).unwrap();
    ex.modal_event(&mut c, &moved(Vec2::new(560.0, 400.0)));
    assert_eq!(doc.objects.get(cube).unwrap().scale, Vec3::splat(2.0));
    let mut c = ctx(&mut doc, Vec2::new(560.0, 400.0));
    ex.modal_event(&mut c, &key('y'));
    assert_eq!(c.doc.objects.get(cube).unwrap().scale, Vec3::new(1.0, 2.0, 1.0));
    ex.modal_event(&mut c, &escape());
    assert_eq!(doc.objects.get(cube).unwrap().scale, Vec3::ONE);
}

#[test]
fn edit_mode_moves_the_selected_vertices() {
    let (mut doc, mut ex, cube) = selected_cube();
    ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
    ex.run_with("mesh.select_all", &[("action", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
    let max_x = |doc: &Doc| {
        let m = &doc.object_mesh(cube).unwrap().mesh;
        m.verts().map(|v| m.position(v).x).fold(f64::MIN, f64::max)
    };
    assert!((max_x(&doc) - 1.0).abs() < 1e-9);
    let mut c = ctx(&mut doc, Vec2::new(400.0, 400.0));
    ex.invoke_with("transform.translate", &[], &mut c, &key('g')).unwrap();
    ex.modal_event(&mut c, &moved(Vec2::new(480.0, 400.0)));
    assert_eq!(ex.modal_event(&mut c, &click()), Some(Ok(Flow::Finished)));
    assert!((max_x(&doc) - 2.0).abs() < 1e-9, "{}", max_x(&doc));
    ex.undo(&mut doc);
    assert!((max_x(&doc) - 1.0).abs() < 1e-9);
}

#[test]
fn needs_a_view_and_a_selection() {
    let mut doc = Doc::starter();
    let mut ex = Executor::with_builtins();
    let mut c = Ctx::new(&mut doc);
    assert!(!ex.registry.get("transform.translate").unwrap().poll(&c), "nothing selected");
    let cube = c.doc.scene_objects()[0];
    ex.run_with("object.select", &[("id", Value::Id(cube))], &mut c).unwrap();
    assert_eq!(ex.invoke_with("transform.translate", &[], &mut c, &key('g')).unwrap(), Flow::Cancelled);
    assert!(ex.last_report.as_deref().unwrap().contains("viewport"));
    assert!(!ex.is_modal());
}
