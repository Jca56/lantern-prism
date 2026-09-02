//! The operator system end to end: run, undo, redo, rollback, adjust-last,
//! modal flow, keymaps, and the undo budget.

use prism_doc::{Doc, ObjectMode};
use prism_math::{Vec2, Vec3};
use prism_ops::input::{Event, Key, Modifiers};
use prism_ops::{Ctx, Executor, Flow, KeyConfig, OpError, OpResult, Operator, Outcome, Registry, UiRequest};
use prism_props::{Value, props};

fn setup() -> (Doc, Executor) {
    (Doc::starter(), Executor::with_builtins())
}

#[test]
fn add_undo_redo() {
    let (mut doc, mut ex) = setup();
    let n = doc.objects.len();
    let mut ctx = Ctx::new(&mut doc);
    ex.run_with("object.add_primitive", &[("kind", Value::Enum(2)), ("size", Value::F64(1.0))], &mut ctx).unwrap();
    assert_eq!(doc.objects.len(), n + 1);
    assert_eq!(doc.active_object().unwrap().name, "Sphere");
    assert_eq!(ex.history.len(), 1);
    assert!(ex.undo(&mut doc));
    assert_eq!(doc.objects.len(), n);
    assert!(ex.redo(&mut doc));
    assert_eq!(doc.objects.len(), n + 1);
    // Undo through the operator (request routed by the executor).
    let mut ctx = Ctx::new(&mut doc);
    ex.run("ed.undo", None, &mut ctx).unwrap();
    assert_eq!(doc.objects.len(), n);
    assert!(ex.requests.is_empty(), "undo request was consumed, not forwarded");
}

#[test]
fn errors_roll_back_and_poll_gates() {
    let (mut doc, mut ex) = setup();
    let mut ctx = Ctx::new(&mut doc);
    // Nothing selected: delete polls false.
    assert!(matches!(ex.run("object.delete", None, &mut ctx), Err(OpError::Poll(_))));
    assert!(matches!(ex.run("nope.nothing", None, &mut ctx), Err(OpError::Unknown(_))));
    // Selecting a bogus id fails and leaves the document untouched.
    let before = doc.clone();
    let mut ctx = Ctx::new(&mut doc);
    let r = ex.run_with("object.select", &[("id", Value::Id(prism_core::Id(9999)))], &mut ctx);
    assert!(matches!(r, Err(OpError::Failed(_))));
    assert!(doc.objects.ptr_eq(&before.objects));
    assert_eq!(ex.history.len(), 0);
    assert!(ex.last_report.is_some());
}

#[test]
fn edit_mode_extrude_and_adjust_last() {
    let (mut doc, mut ex) = setup();
    let cube = doc.scene_objects()[0];
    let mut ctx = Ctx::new(&mut doc);
    ex.run_with("object.select", &[("id", Value::Id(cube))], &mut ctx).unwrap();
    assert!(matches!(ex.run("mesh.extrude", None, &mut ctx), Err(OpError::Poll(_))), "not in edit mode");
    ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut ctx).unwrap();
    assert_eq!(doc.active_object().unwrap().mode, ObjectMode::Edit);
    let mut ctx = Ctx::new(&mut doc);
    // Extrude with nothing selected: cancelled, nothing recorded.
    let steps = ex.history.len();
    assert_eq!(ex.run("mesh.extrude", None, &mut ctx).unwrap(), Outcome::Cancelled);
    assert_eq!(ex.history.len(), steps);
    ex.run_with("mesh.select_all", &[("action", Value::Enum(1))], &mut ctx).unwrap();
    ex.run_with("mesh.extrude", &[("offset", Value::F64(1.0))], &mut ctx).unwrap();
    let mesh = &doc.object_mesh(cube).unwrap().mesh;
    // Extruding every face of a closed cube keeps the originals and grows a
    // second shell along the vertex normals.
    assert_eq!(mesh.face_count(), 12);
    assert_eq!(mesh.vert_count(), 16);
    let max_y = mesh.verts().map(|v| mesh.position(v).y).fold(f64::MIN, f64::max);
    assert!((max_y - (1.0 + 1.0 / 3f64.sqrt())).abs() < 1e-9, "corner moved along (1,1,1)/√3: {max_y}");

    // Adjust: extrude only the top face by 2 instead.
    ex.undo(&mut doc);
    let mut ctx = Ctx::new(&mut doc);
    {
        let block = ctx.doc.object_mesh_mut(cube).unwrap();
        let top = block.mesh.faces().find(|&f| block.mesh.face_normal(f).approx_eq(Vec3::Y, 1e-9)).unwrap();
        prism_ops::builtin::select::select_faces(&mut block.mesh, &[top]);
    }
    ex.run_with("mesh.extrude", &[("offset", Value::F64(1.0))], &mut ctx).unwrap();
    let top_y = |doc: &Doc| {
        let m = &doc.object_mesh(cube).unwrap().mesh;
        m.verts().map(|v| m.position(v).y).fold(f64::MIN, f64::max)
    };
    assert!((top_y(&doc) - 2.0).abs() < 1e-9);
    {
        let (id, props) = ex.last_step_props().unwrap();
        assert_eq!(id, "mesh.extrude");
        props.set_by_name("offset", Value::F64(2.0)).unwrap();
    }
    let mut ctx = Ctx::new(&mut doc);
    ex.adjust_last(&mut ctx).unwrap();
    assert!((top_y(&doc) - 3.0).abs() < 1e-9, "re-run from the pre-op snapshot with the new offset");
    assert_eq!(doc.object_mesh(cube).unwrap().mesh.face_count(), 10);
    // Undo removes the adjusted step in one go.
    ex.undo(&mut doc);
    assert_eq!(doc.object_mesh(cube).unwrap().mesh.face_count(), 6);
}

props! {
    pub struct CountProps {
        pub target: i64 = 3 => { id: 1 },
    }
}

/// A modal operator that finishes after `target` pointer moves, moving the
/// active object one unit per event; Escape cancels.
struct Nudge;
impl Operator for Nudge {
    const ID: &'static str = "test.nudge";
    const LABEL: &'static str = "Nudge";
    type Props = CountProps;
    type Modal = i64;
    fn exec(ctx: &mut Ctx, p: &CountProps) -> OpResult<Outcome> {
        let id = ctx.doc.active_object_id();
        if let Some(o) = ctx.doc.objects.get_mut(id) {
            o.location.x += p.target as f64;
        }
        Ok(Outcome::Finished)
    }
    fn invoke(_ctx: &mut Ctx, _p: &mut CountProps, _ev: &Event, count: &mut i64) -> OpResult<Flow> {
        *count = 0;
        Ok(Flow::Running)
    }
    fn modal(count: &mut i64, ctx: &mut Ctx, p: &mut CountProps, ev: &Event) -> OpResult<Flow> {
        match ev {
            Event::PointerMoved(_) => {
                *count += 1;
                let id = ctx.doc.active_object_id();
                if let Some(o) = ctx.doc.objects.get_mut(id) {
                    o.location.x += 1.0;
                }
                Ok(if *count >= p.target { Flow::Finished } else { Flow::Running })
            }
            Event::Key { key: Key::Escape, .. } => Ok(Flow::Cancelled),
            _ => Ok(Flow::PassThrough),
        }
    }
}

#[test]
fn modal_lifecycle() {
    let mut doc = Doc::starter();
    let mut reg = Registry::new();
    reg.register::<Nudge>();
    let mut ex = Executor::new(reg);
    let start = Event::PointerMoved(Vec2::ZERO);
    let mut ctx = Ctx::new(&mut doc);
    assert_eq!(ex.invoke("test.nudge", None, &mut ctx, &start).unwrap(), Flow::Running);
    assert!(ex.is_modal());
    assert!(matches!(ex.run("test.nudge", None, &mut ctx), Err(OpError::Busy)));
    for _ in 0..2 {
        assert_eq!(ex.modal_event(&mut ctx, &start).unwrap().unwrap(), Flow::Running);
    }
    assert_eq!(ex.modal_event(&mut ctx, &start).unwrap().unwrap(), Flow::Finished);
    assert!(!ex.is_modal());
    assert_eq!(doc.active_object().unwrap().location.x, 7.0 + 3.0, "camera starts at x=7");
    assert_eq!(ex.history.len(), 1, "one step for the whole modal run");
    // Cancel restores the snapshot from before invoke.
    let mut ctx = Ctx::new(&mut doc);
    ex.invoke("test.nudge", None, &mut ctx, &start).unwrap();
    ex.modal_event(&mut ctx, &start);
    let esc = Event::Key { key: Key::Escape, pressed: true, repeat: false, mods: Modifiers::NONE };
    assert_eq!(ex.modal_event(&mut ctx, &esc).unwrap().unwrap(), Flow::Cancelled);
    assert_eq!(doc.active_object().unwrap().location.x, 10.0);
    assert_eq!(ex.history.len(), 1);
}

#[test]
fn keymap_drives_operators() {
    let (mut doc, mut ex) = setup();
    let keys = KeyConfig::default_prism();
    let cube = doc.scene_objects()[0];
    let mut ctx = Ctx::new(&mut doc);
    ex.run_with("object.select", &[("id", Value::Id(cube))], &mut ctx).unwrap();
    let tab = Event::Key { key: Key::Tab, pressed: true, repeat: false, mods: Modifiers::NONE };
    let item = keys.resolve(&["object", "window"], &tab, |op| ex.registry.get(op).is_some_and(|i| i.poll(&ctx))).unwrap();
    ex.run_with(&item.op, &item.overrides(), &mut ctx).unwrap();
    assert_eq!(doc.active_object().unwrap().mode, ObjectMode::Edit);
    // Shift+A in object mode asks the UI for the add menu.
    let mut ctx = Ctx::new(&mut doc);
    let shift_a = Event::Key { key: Key::Char('A'), pressed: true, repeat: false, mods: Modifiers::SHIFT };
    let item = keys.resolve(&["object"], &shift_a, |_| true).unwrap();
    ex.run_with(&item.op, &item.overrides(), &mut ctx).unwrap();
    assert_eq!(ex.requests, vec![UiRequest::Menu("add".into())]);
}

#[test]
fn ten_thousand_undo_steps_stay_in_budget() {
    let (mut doc, mut ex) = setup();
    ex.history.set_budget(10_000);
    let cube = doc.scene_objects()[0];
    let mut ctx = Ctx::new(&mut doc);
    ex.run_with("object.select", &[("id", Value::Id(cube))], &mut ctx).unwrap();
    let base = ctx.doc.objects.get(cube).unwrap().location;
    for _ in 0..10_000 {
        ex.run_with("object.translate", &[("delta", Value::Vec3(Vec3::new(0.001, 0.0, 0.0)))], &mut ctx).unwrap();
    }
    let stats = ex.history.stats();
    assert_eq!(stats.steps, 10_000);
    assert_eq!(stats.cursor, 10_000);
    // Moving objects never copies mesh storage: all 20 000 snapshots share it.
    let mut one = std::collections::HashSet::new();
    doc.object_mesh(cube).unwrap().mesh.chunk_ptrs(&mut one);
    assert_eq!(stats.unique_mesh_bytes, one.len() * prism_core::CHUNK * 8);
    // And the whole history unwinds back to the start.
    while ex.undo(&mut doc) {}
    assert_eq!(doc.objects.get(cube).unwrap().location, base);
    assert!(ex.history.can_redo());
}

#[test]
fn mesh_edit_steps_cost_only_touched_chunks() {
    let (mut doc, mut ex) = setup();
    let big = doc.add_mesh("Big", prism_mesh::primitives::grid(10.0, 10.0, 120, 120));
    let obj = doc.add_object("Big", prism_doc::DataKind::Mesh, big);
    let mut ctx = Ctx::new(&mut doc);
    ex.run_with("object.select", &[("id", Value::Id(obj))], &mut ctx).unwrap();
    ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut ctx).unwrap();
    let before = ex.history.stats().unique_mesh_bytes;
    // 200 selection flips: each rewrites the select layers (bool, one byte
    // per element) but never the positions or topology.
    for i in 0..200 {
        ex.run_with("mesh.select_all", &[("action", Value::Enum(if i % 2 == 0 { 1 } else { 2 }))], &mut ctx).unwrap();
    }
    let after = ex.history.stats().unique_mesh_bytes;
    let grown = after - before;
    // Select layers: ~14.6k verts + ~29k edges + ~14.4k faces of bools ≈ 58
    // chunks per step at 8 KB accounting each.
    assert!(grown < 200 * 70 * prism_core::CHUNK * 8, "grew {grown} bytes");
    assert!(grown > 200 * 30 * prism_core::CHUNK * 8, "sharing is too good to be true: {grown}");
}
