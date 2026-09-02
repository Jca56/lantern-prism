//! Extrude the selected faces and, interactively, drag them out along their
//! normal (D024). `exec` (no view, or Adjust Last Operation) extrudes by
//! `offset` directly.

use prism_core::Id;
use prism_math::{Mat4, Quat, Vec2, Vec3};
use prism_mesh::{Mesh, VertH};
use prism_props::props;

use crate::builtin::mesh::{edit_mesh, in_edit_mode};
use crate::builtin::select;
use crate::builtin::transform::{Control, INTERACTIVE, control, pointer_of, starts_drag};
use crate::context::{Ctx, Flow, Outcome};
use crate::input::Event;
use crate::operator::{OpFlags, OpResult, Operator};
use crate::registry::Registry;

props! {
    pub struct ExtrudeProps {
        /// Distance along the average normal of the selected faces.
        pub offset: f64 = 1.0 => { id: 1, soft: -10.0..=10.0, subtype: Distance },
    }
}

/// A new vertex, where it started, and the unit direction it moves along.
type Moved = (VertH, Vec3, Vec3);

/// Duplicate the selected faces; the new region ends up selected. `None`
/// when nothing was selected.
fn extrude(m: &mut Mesh) -> OpResult<Option<(Vec<Moved>, usize)>> {
    let faces = select::selected_faces(m);
    if faces.is_empty() {
        return Ok(None);
    }
    let r = m.extrude_faces(&faces)?;
    // Each new vertex moves along the average normal of the new faces around
    // it, so a flat region moves as a slab and a closed shell grows.
    let normals: Vec<Vec3> = r.faces.iter().map(|&f| m.face_normal(f)).collect();
    let moved = r
        .verts
        .iter()
        .map(|&v| {
            let mut n = Vec3::ZERO;
            for (i, &f) in r.faces.iter().enumerate() {
                if m.verts_of_face(f).any(|x| x == v) {
                    n += normals[i];
                }
            }
            (v, m.position(v), n.normalize_or_zero())
        })
        .collect();
    select::select_faces(m, &r.faces);
    Ok(Some((moved, r.faces.len())))
}

fn place(m: &mut Mesh, moved: &[Moved], offset: f64) {
    for &(v, p0, n) in moved {
        m.set_position(v, p0 + n * offset);
    }
}

#[derive(Default)]
pub struct ExtrudeModal {
    mesh: Id,
    moved: Vec<Moved>,
    faces: usize,
    /// World-space line the pointer drags along.
    pivot: Vec3,
    axis: Vec3,
    /// World units per mesh unit along `axis` (the object may be scaled).
    world_per_local: f64,
    start: Vec2,
    release_confirm: bool,
}

pub struct Extrude;
impl Operator for Extrude {
    const ID: &'static str = "mesh.extrude";
    const LABEL: &'static str = "Extrude Faces";
    const FLAGS: OpFlags = INTERACTIVE;
    type Props = ExtrudeProps;
    type Modal = ExtrudeModal;
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &ExtrudeProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let Some((moved, n)) = extrude(&mut block.mesh)? else {
            return Ok(Outcome::Cancelled);
        };
        place(&mut block.mesh, &moved, p.offset);
        ctx.report(format!("Extruded {n} face(s)"));
        Ok(Outcome::Finished)
    }
    fn invoke(ctx: &mut Ctx, p: &mut ExtrudeProps, event: &Event, m: &mut ExtrudeModal) -> OpResult<Flow> {
        if ctx.view.is_none() {
            return Self::exec(ctx, p).map(Flow::from);
        }
        let id = ctx.doc.active_object_id();
        let to_world = ctx.doc.object_matrix(id);
        let Some(mesh) = ctx.doc.objects.get(id).map(|o| o.data) else {
            return Ok(Flow::Cancelled);
        };
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Flow::Cancelled);
        };
        let Some((moved, faces)) = extrude(&mut block.mesh)? else {
            ctx.report("Extrude: nothing selected");
            return Ok(Flow::Cancelled);
        };
        let n = moved.len().max(1) as f64;
        let n_local = moved.iter().fold(Vec3::ZERO, |s, (_, _, n)| s + *n) / n;
        let n_world = to_world.transform_vector(n_local);
        m.world_per_local = n_world.length().max(1e-9);
        m.axis = n_world.normalize_or(Vec3::Y);
        m.pivot = moved.iter().fold(Vec3::ZERO, |s, (_, p, _)| s + to_world.transform_point(*p)) / n;
        m.mesh = mesh;
        m.moved = moved;
        m.faces = faces;
        m.start = pointer_of(event).unwrap_or(ctx.pointer);
        m.release_confirm = starts_drag(event);
        p.offset = 0.0;
        ctx.report("Extrude  +0.000");
        Ok(Flow::Running)
    }
    fn modal(m: &mut ExtrudeModal, ctx: &mut Ctx, p: &mut ExtrudeProps, event: &Event) -> OpResult<Flow> {
        match control(event, m.release_confirm) {
            Control::Move(pixel) => {
                if let Some(view) = ctx.view
                    && let (Some(t0), Some(t1)) = (view.on_axis(m.pivot, m.axis, m.start), view.on_axis(m.pivot, m.axis, pixel))
                {
                    p.offset = (t1 - t0) / m.world_per_local;
                    if let Some(block) = ctx.doc.meshes.get_mut(m.mesh) {
                        place(&mut block.mesh, &m.moved, p.offset);
                    }
                }
                ctx.report(format!("Extrude  {:+.3}", p.offset));
                Ok(Flow::Running)
            }
            Control::Confirm => {
                ctx.report(format!("Extruded {} face(s)", m.faces));
                Ok(Flow::Finished)
            }
            Control::Cancel => Ok(Flow::Cancelled),
            Control::Axis(_) | Control::Ignore => Ok(Flow::Running),
        }
    }
}

props! {
    pub struct ExtrudeToCursorProps {
        /// Where the new region's centre lands, in world space.
        pub target: Vec3 = Vec3::ZERO => { id: 1, subtype: Translation },
        /// Turn the new region to face the way it travelled, so a chain of
        /// extrusions bends like a trunk.
        pub rotate: bool = true => { id: 2 },
    }
}

/// Extrude the selected faces so they land under the pointer (D030). Bound
/// to Ctrl+right-click in edit mode, as in Blender.
pub struct ExtrudeToCursor;
impl Operator for ExtrudeToCursor {
    const ID: &'static str = "mesh.extrude_to_cursor";
    const LABEL: &'static str = "Extrude to Cursor";
    type Props = ExtrudeToCursorProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &ExtrudeToCursorProps) -> OpResult<Outcome> {
        let id = ctx.doc.active_object_id();
        let to_world = ctx.doc.object_matrix(id);
        let to_local = to_world.inverse().unwrap_or(Mat4::IDENTITY);
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let Some((moved, n)) = extrude(&mut block.mesh)? else {
            ctx.report("Extrude to Cursor: select faces first");
            return Ok(Outcome::Cancelled);
        };
        let count = moved.len().max(1) as f64;
        let centre = moved.iter().fold(Vec3::ZERO, |s, (_, b, _)| s + to_world.transform_point(*b)) / count;
        let normal = to_world.transform_vector(moved.iter().fold(Vec3::ZERO, |s, (_, _, nn)| s + *nn) / count).normalize_or(Vec3::Y);
        let travel = p.target - centre;
        let dir = travel.normalize_or(normal);
        let turn = if p.rotate { Quat::from_rotation_arc(normal, dir) } else { Quat::from_axis_angle(Vec3::Y, 0.0) };
        for &(v, base, _) in &moved {
            let carried = to_world.transform_point(base) + travel;
            let world = p.target + turn * (carried - p.target);
            block.mesh.set_position(v, to_local.transform_point(world));
        }
        ctx.report(format!("Extruded {n} face(s) to cursor"));
        Ok(Outcome::Finished)
    }
}

pub fn register(r: &mut Registry) {
    r.register::<Extrude>();
    r.register::<ExtrudeToCursor>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::transform::tests::{click, ctx, escape, key, moved, selected_cube};
    use prism_doc::Doc;
    use prism_props::Value;

    #[test]
    fn extrude_to_cursor_lands_on_the_target_and_turns_toward_it() {
        let (mut doc, mut ex, cube) = selected_cube();
        ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
        {
            let block = doc.object_mesh_mut(cube).unwrap();
            let top = block.mesh.faces().find(|&f| block.mesh.face_normal(f).approx_eq(Vec3::Y, 1e-9)).unwrap();
            select::select_faces(&mut block.mesh, &[top]);
        }
        // The top face (centre (0,1,0), facing +Y) goes to (2,3,0): up and to
        // the right, so the new face ends up facing that way too.
        ex.run_with("mesh.extrude_to_cursor", &[("target", Value::Vec3(Vec3::new(2.0, 3.0, 0.0)))], &mut Ctx::new(&mut doc)).unwrap();
        let m = &doc.object_mesh(cube).unwrap().mesh;
        assert_eq!(m.face_count(), 10);
        let f = select::selected_faces(m)[0];
        let centre = m.face_positions(f).iter().fold(Vec3::ZERO, |s, p| s + *p) / 4.0;
        assert!((centre - Vec3::new(2.0, 3.0, 0.0)).length() < 1e-9, "{centre:?}");
        let n = m.face_normal(f);
        let want = Vec3::new(2.0, 2.0, 0.0).normalize();
        assert!((n - want).length() < 1e-9, "turned to face its travel: {n:?}");
        // Chain: the new face is selected, so another click keeps building.
        ex.run_with("mesh.extrude_to_cursor", &[("target", Value::Vec3(Vec3::new(2.0, 5.0, 0.0))), ("rotate", Value::Bool(false))], &mut Ctx::new(&mut doc)).unwrap();
        let m = &doc.object_mesh(cube).unwrap().mesh;
        assert_eq!(m.face_count(), 14);
        let f = select::selected_faces(m)[0];
        assert!((m.face_normal(f) - want).length() < 1e-9, "rotate off keeps the orientation");
        ex.undo(&mut doc);
        ex.undo(&mut doc);
        assert_eq!(doc.object_mesh(cube).unwrap().mesh.face_count(), 6);
    }

    #[test]
    fn interactive_extrude_drags_along_the_normal() {
        let (mut doc, mut ex, cube) = selected_cube();
        ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
        {
            let block = doc.object_mesh_mut(cube).unwrap();
            let top = block.mesh.faces().find(|&f| block.mesh.face_normal(f).approx_eq(Vec3::Y, 1e-9)).unwrap();
            select::select_faces(&mut block.mesh, &[top]);
        }
        let top_y = |doc: &Doc| {
            let m = &doc.object_mesh(cube).unwrap().mesh;
            m.verts().map(|v| m.position(v).y).fold(f64::MIN, f64::max)
        };
        let faces = |doc: &Doc| doc.object_mesh(cube).unwrap().mesh.face_count();
        let steps = ex.history.len();

        // Looking down −Z, +Y is up on screen: dragging up 80 px is one unit.
        let mut c = ctx(&mut doc, Vec2::new(400.0, 400.0));
        assert_eq!(ex.invoke_with("mesh.extrude", &[], &mut c, &key('e')).unwrap(), Flow::Running);
        assert_eq!(faces(c.doc), 10, "geometry exists as soon as the drag starts");
        ex.modal_event(&mut c, &moved(Vec2::new(400.0, 320.0)));
        assert!((top_y(c.doc) - 2.0).abs() < 1e-9, "{}", top_y(c.doc));
        assert_eq!(ex.modal_event(&mut c, &escape()), Some(Ok(Flow::Cancelled)));
        assert_eq!(faces(&doc), 6, "cancel removes the whole extrusion");
        assert_eq!(ex.history.len(), steps);

        let mut c = ctx(&mut doc, Vec2::new(400.0, 400.0));
        ex.invoke_with("mesh.extrude", &[], &mut c, &key('e')).unwrap();
        ex.modal_event(&mut c, &moved(Vec2::new(400.0, 320.0)));
        assert_eq!(ex.modal_event(&mut c, &click()), Some(Ok(Flow::Finished)));
        assert_eq!(faces(&doc), 10);
        assert_eq!(ex.history.len(), steps + 1);
        ex.last_step_props().unwrap().1.set_by_name("offset", Value::F64(2.0)).unwrap();
        ex.adjust_last(&mut Ctx::new(&mut doc)).unwrap();
        assert!((top_y(&doc) - 3.0).abs() < 1e-9, "adjust replays with the new offset");
        ex.undo(&mut doc);
        assert_eq!(faces(&doc), 6);
    }
}
