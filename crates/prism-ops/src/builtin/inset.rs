//! Inset the selected faces (Phase 7, D027): the region shrinks inward behind
//! a rim of quads. Interactive like extrude: drag away from where you started
//! to widen the rim, click to confirm.

use prism_core::Id;
use prism_math::Vec2;
use prism_mesh::ops::{InsetResult, InsetVert};
use prism_mesh::Mesh;
use prism_props::props;

use crate::builtin::mesh::{edit_mesh, in_edit_mode};
use crate::builtin::select;
use crate::builtin::transform::{Control, INTERACTIVE, control, pointer_of, starts_drag};
use crate::context::{Ctx, Flow, Outcome};
use crate::input::Event;
use crate::operator::{OpFlags, OpResult, Operator};
use crate::registry::Registry;

props! {
    pub struct InsetProps {
        /// Width of the rim.
        pub thickness: f64 = 0.2 => { id: 1, soft: 0.0..=5.0, subtype: Distance },
        /// Push the inner region along its normal.
        pub depth: f64 = 0.0 => { id: 2, soft: -5.0..=5.0, subtype: Distance },
        /// Inset every face on its own instead of the region as one.
        pub individual: bool = false => { id: 3 },
    }
}

/// Inset the selection and select the inner faces. `None` if nothing is selected.
fn run(m: &mut Mesh, p: &InsetProps) -> OpResult<Option<InsetResult>> {
    let faces = select::selected_faces(m);
    if faces.is_empty() {
        return Ok(None);
    }
    let r = m.inset_faces(&faces, p.thickness, p.depth, p.individual)?;
    select::select_faces(m, &r.faces);
    Ok(Some(r))
}

#[derive(Default)]
pub struct InsetModal {
    mesh: Id,
    verts: Vec<InsetVert>,
    faces: usize,
    start: Vec2,
    /// Mesh units per pixel of pointer travel.
    units_per_pixel: f64,
    release_confirm: bool,
}

pub struct Inset;
impl Operator for Inset {
    const ID: &'static str = "mesh.inset";
    const LABEL: &'static str = "Inset Faces";
    const FLAGS: OpFlags = INTERACTIVE;
    type Props = InsetProps;
    type Modal = InsetModal;
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &InsetProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let Some(r) = run(&mut block.mesh, p)? else {
            return Ok(Outcome::Cancelled);
        };
        ctx.report(format!("Inset {} face(s)", r.faces.len()));
        Ok(Outcome::Finished)
    }
    fn invoke(ctx: &mut Ctx, p: &mut InsetProps, event: &Event, m: &mut InsetModal) -> OpResult<Flow> {
        let Some(view) = ctx.view else {
            return Self::exec(ctx, p).map(Flow::from);
        };
        let id = ctx.doc.active_object_id();
        let to_world = ctx.doc.object_matrix(id);
        let Some(mesh) = ctx.doc.objects.get(id).map(|o| o.data) else {
            return Ok(Flow::Cancelled);
        };
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Flow::Cancelled);
        };
        // Build the geometry at zero thickness; the drag places it.
        let zero = InsetProps { thickness: 0.0, depth: 0.0, ..p.clone() };
        let Some(r) = run(&mut block.mesh, &zero)? else {
            ctx.report("Inset: nothing selected");
            return Ok(Flow::Cancelled);
        };
        let n = r.verts.len().max(1) as f64;
        let centre = r.verts.iter().fold(prism_math::Vec3::ZERO, |s, v| s + to_world.transform_point(v.base)) / n;
        let scale = to_world.transform_vector(prism_math::Vec3::X).length().max(1e-9);
        m.units_per_pixel = view.units_per_pixel(centre) / scale;
        m.mesh = mesh;
        m.faces = r.faces.len();
        m.verts = r.verts;
        m.start = pointer_of(event).unwrap_or(ctx.pointer);
        m.release_confirm = starts_drag(event);
        p.thickness = 0.0;
        p.depth = 0.0;
        ctx.report("Inset  0.000");
        Ok(Flow::Running)
    }
    fn modal(m: &mut InsetModal, ctx: &mut Ctx, p: &mut InsetProps, event: &Event) -> OpResult<Flow> {
        match control(event, m.release_confirm) {
            Control::Move(pixel) => {
                p.thickness = pixel.distance(m.start) * m.units_per_pixel;
                if let Some(block) = ctx.doc.meshes.get_mut(m.mesh) {
                    block.mesh.place_inset(&m.verts, p.thickness, p.depth);
                }
                ctx.report(format!("Inset  {:.3}", p.thickness));
                Ok(Flow::Running)
            }
            Control::Confirm => {
                ctx.report(format!("Inset {} face(s)", m.faces));
                Ok(Flow::Finished)
            }
            Control::Cancel => Ok(Flow::Cancelled),
            Control::Axis(_) | Control::Ignore => Ok(Flow::Running),
        }
    }
}

pub fn register(r: &mut Registry) {
    r.register::<Inset>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::transform::tests::{click, ctx, escape, key, moved, selected_cube};
    use prism_doc::Doc;
    use prism_math::Vec3;
    use prism_props::Value;

    fn inner_half_width(doc: &Doc, cube: prism_core::Id) -> f64 {
        // The selected (inner) face after an inset of the top: its |x| extent.
        let m = &doc.object_mesh(cube).unwrap().mesh;
        let f = select::selected_faces(m)[0];
        m.verts_of_face(f).map(|v| m.position(v).x.abs()).fold(0.0, f64::max)
    }

    #[test]
    fn interactive_inset_follows_pointer_travel() {
        let (mut doc, mut ex, cube) = selected_cube();
        ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
        {
            let block = doc.object_mesh_mut(cube).unwrap();
            let top = block.mesh.faces().find(|&f| block.mesh.face_normal(f).approx_eq(Vec3::Y, 1e-9)).unwrap();
            select::select_faces(&mut block.mesh, &[top]);
        }
        let faces = |doc: &Doc| doc.object_mesh(cube).unwrap().mesh.face_count();
        let steps = ex.history.len();

        // 80 px is one unit in the test view; 40 px of travel is a 0.5 rim.
        let mut c = ctx(&mut doc, Vec2::new(400.0, 400.0));
        assert_eq!(ex.invoke_with("mesh.inset", &[], &mut c, &key('i')).unwrap(), Flow::Running);
        assert_eq!(faces(c.doc), 10, "rim exists from the start");
        ex.modal_event(&mut c, &moved(Vec2::new(400.0, 440.0)));
        assert!((inner_half_width(c.doc, cube) - 0.5).abs() < 1e-9, "{}", inner_half_width(c.doc, cube));
        assert_eq!(ex.modal_event(&mut c, &escape()), Some(Ok(Flow::Cancelled)));
        assert_eq!(faces(&doc), 6);
        assert_eq!(ex.history.len(), steps);

        let mut c = ctx(&mut doc, Vec2::new(400.0, 400.0));
        ex.invoke_with("mesh.inset", &[], &mut c, &key('i')).unwrap();
        ex.modal_event(&mut c, &moved(Vec2::new(424.0, 432.0))); // 40 px diagonal
        assert_eq!(ex.modal_event(&mut c, &click()), Some(Ok(Flow::Finished)));
        assert_eq!(faces(&doc), 10);
        assert!((inner_half_width(&doc, cube) - 0.5).abs() < 1e-9);
        assert_eq!(ex.history.len(), steps + 1);
        // Adjust: a thinner rim and some depth.
        let (id, props) = ex.last_step_props().unwrap();
        assert_eq!(id, "mesh.inset");
        props.set_by_name("thickness", Value::F64(0.25)).unwrap();
        props.set_by_name("depth", Value::F64(0.5)).unwrap();
        ex.adjust_last(&mut Ctx::new(&mut doc)).unwrap();
        assert!((inner_half_width(&doc, cube) - 0.75).abs() < 1e-9);
        let m = &doc.object_mesh(cube).unwrap().mesh;
        let f = select::selected_faces(m)[0];
        assert!(m.verts_of_face(f).all(|v| (m.position(v).y - 1.5).abs() < 1e-9), "depth raises the inner face");
    }

    #[test]
    fn exec_insets_every_face_individually() {
        let (mut doc, mut ex, cube) = selected_cube();
        ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
        ex.run_with("mesh.select_all", &[("action", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
        ex.run_with("mesh.inset", &[("thickness", Value::F64(0.2)), ("individual", Value::Bool(true))], &mut Ctx::new(&mut doc)).unwrap();
        let m = &doc.object_mesh(cube).unwrap().mesh;
        assert_eq!(m.face_count(), 30);
        assert_eq!(select::selected_faces(m).len(), 6, "the inner faces stay selected");
        assert!(ex.last_report.as_deref().unwrap().starts_with("Inset 6 face"));
    }
}
