//! Loop cut (Phase 7, D028): cut a new edge loop across the ring of quads
//! through the active edge, then slide it. Right-click an edge, choose Loop
//! Cut, move the pointer along the edge to slide, click to confirm.

use prism_core::Id;
use prism_doc::{Elem, SelectMode};
use prism_math::Vec2;
use prism_mesh::ops::{CutVert, LoopCutResult};
use prism_mesh::EdgeH;
use prism_props::props;

use crate::builtin::mesh::{edit_mesh, in_edit_mode};
use crate::builtin::select;
use crate::builtin::transform::{Control, INTERACTIVE, control, pointer_of, starts_drag};
use crate::context::{Ctx, Flow, Outcome, ViewInfo};
use crate::input::Event;
use crate::operator::{OpError, OpFlags, OpResult, Operator};
use crate::registry::Registry;

props! {
    pub struct LoopCutProps {
        /// Cuts per ring edge; more than one spaces evenly and does not slide.
        pub cuts: i64 = 1 => { id: 1, hard: 1..=32, soft: 1..=10 },
        /// Where a single cut sits along its edges.
        pub factor: f64 = 0.5 => { id: 2, subtype: Factor },
    }
}

/// The edge to cut through: the active edge, else the first selected edge.
fn seed_edge(block: &prism_doc::MeshBlock) -> Option<EdgeH> {
    match block.edit.active {
        Some(Elem::Edge(e)) if block.mesh.edge_live(e) => Some(e),
        _ => select::selected_edges(&block.mesh).into_iter().next(),
    }
}

/// Cut and select the new loop. `None` when there is no seed edge.
fn run(block: &mut prism_doc::MeshBlock, p: &LoopCutProps) -> OpResult<Option<LoopCutResult>> {
    let Some(seed) = seed_edge(block) else {
        return Ok(None);
    };
    let m = &mut block.mesh;
    let r = m.loop_cut(seed, p.cuts.max(1) as usize, p.factor).map_err(|_| OpError::Failed("no ring of quads through that edge".into()))?;
    select::set_all(m, false);
    let verts: Vec<_> = r.verts.iter().map(|c| c.vert).collect();
    select::select_verts(m, &verts, true);
    select::flush_mode(m, SelectMode::Edge);
    block.edit.active = r.edges.first().map(|&e| Elem::Edge(e));
    block.edit.history.clear();
    Ok(Some(r))
}

fn select_mode_edge(ctx: &mut Ctx) {
    if let Some(s) = ctx.doc.scene_mut() {
        s.tool.select_mode = SelectMode::Edge;
    }
}

#[derive(Default)]
pub struct LoopCutModal {
    mesh: Id,
    verts: Vec<CutVert>,
    edges: usize,
    /// Screen ends of the seed edge, `a` and `b`, for the slide.
    a: Vec2,
    b: Vec2,
    release_confirm: bool,
}

impl LoopCutModal {
    /// Where the pointer falls along the seed edge on screen, clamped to it.
    fn factor_at(&self, p: Vec2) -> f64 {
        let ab = self.b - self.a;
        let l2 = ab.dot(ab);
        if l2 < 1e-9 { 0.5 } else { ((p - self.a).dot(ab) / l2).clamp(0.0, 1.0) }
    }
}

fn screen_ends(view: &ViewInfo, to_world: &prism_math::Mat4, c: &CutVert) -> Option<(Vec2, Vec2)> {
    Some((view.project(to_world.transform_point(c.a))?, view.project(to_world.transform_point(c.b))?))
}

pub struct LoopCut;
impl Operator for LoopCut {
    const ID: &'static str = "mesh.loop_cut";
    const LABEL: &'static str = "Loop Cut";
    const FLAGS: OpFlags = INTERACTIVE;
    type Props = LoopCutProps;
    type Modal = LoopCutModal;
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &LoopCutProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let Some(r) = run(block, p)? else {
            ctx.report("Loop Cut: select an edge to cut through");
            return Ok(Outcome::Cancelled);
        };
        select_mode_edge(ctx);
        ctx.report(format!("Loop cut: {} edge(s)", r.edges.len()));
        Ok(Outcome::Finished)
    }
    fn invoke(ctx: &mut Ctx, p: &mut LoopCutProps, event: &Event, m: &mut LoopCutModal) -> OpResult<Flow> {
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
        let Some(r) = run(block, p)? else {
            ctx.report("Loop Cut: select an edge to cut through");
            return Ok(Flow::Cancelled);
        };
        select_mode_edge(ctx);
        // The seed edge's cut is the one whose ends the pointer slides between.
        let Some((a, b)) = r.verts.first().and_then(|c| screen_ends(&view, &to_world, c)) else {
            ctx.report(format!("Loop cut: {} edge(s)", r.edges.len()));
            return Ok(Flow::Finished);
        };
        m.mesh = mesh;
        m.edges = r.edges.len();
        m.verts = r.verts;
        m.a = a;
        m.b = b;
        m.release_confirm = starts_drag(event);
        if p.cuts == 1 {
            let start = pointer_of(event).unwrap_or(ctx.pointer);
            p.factor = m.factor_at(start);
            if let Some(block) = ctx.doc.meshes.get_mut(mesh) {
                block.mesh.place_loop_cut(&m.verts, p.factor);
            }
        }
        ctx.report(format!("Loop Cut  {:.3}  ({} edges)", p.factor, m.edges));
        Ok(Flow::Running)
    }
    fn modal(m: &mut LoopCutModal, ctx: &mut Ctx, p: &mut LoopCutProps, event: &Event) -> OpResult<Flow> {
        match control(event, m.release_confirm) {
            Control::Move(pixel) => {
                if p.cuts == 1 {
                    p.factor = m.factor_at(pixel);
                    if let Some(block) = ctx.doc.meshes.get_mut(m.mesh) {
                        block.mesh.place_loop_cut(&m.verts, p.factor);
                    }
                }
                ctx.report(format!("Loop Cut  {:.3}  ({} edges)", p.factor, m.edges));
                Ok(Flow::Running)
            }
            Control::Confirm => {
                ctx.report(format!("Loop cut: {} edge(s)", m.edges));
                Ok(Flow::Finished)
            }
            Control::Cancel => Ok(Flow::Cancelled),
            Control::Axis(_) | Control::Ignore => Ok(Flow::Running),
        }
    }
}

pub fn register(r: &mut Registry) {
    r.register::<LoopCut>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::transform::tests::{click, ctx, escape, key, moved, selected_cube};
    use prism_doc::Doc;
    use prism_math::Vec3;
    use prism_props::Value;

    /// Edit mode with one of the top face's edges active, as a right-click leaves it.
    fn with_top_edge_active() -> (Doc, crate::executor::Executor, prism_core::Id) {
        let (mut doc, mut ex, cube) = selected_cube();
        ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
        let block = doc.object_mesh_mut(cube).unwrap();
        let top = block.mesh.faces().find(|&f| block.mesh.face_normal(f).approx_eq(Vec3::Y, 1e-9)).unwrap();
        // The test view looks along Z, so pick a top edge that runs along X:
        // on screen it is a horizontal segment the pointer can slide along,
        // and its ring (top, back, bottom, front) cuts a loop of constant X.
        let e = block.mesh.edges_of_face(top).find(|&e| {
            let [a, b] = block.mesh.edge_verts(e);
            (block.mesh.position(a).z - block.mesh.position(b).z).abs() < 1e-9
        }).unwrap();
        block.edit.active = Some(Elem::Edge(e));
        (doc, ex, cube)
    }

    fn counts(doc: &Doc, cube: prism_core::Id) -> (usize, usize, usize) {
        let m = &doc.object_mesh(cube).unwrap().mesh;
        (m.vert_count(), m.edge_count(), m.face_count())
    }

    #[test]
    fn loop_cut_slides_with_the_pointer_and_cancels_clean() {
        let (mut doc, mut ex, cube) = with_top_edge_active();
        let steps = ex.history.len();
        assert_eq!(counts(&doc, cube), (8, 12, 6));
        // The seed edge spans x = -1..1 at y = 1: on screen, 320..480 px.
        let mut c = ctx(&mut doc, Vec2::new(400.0, 400.0));
        assert_eq!(ex.invoke_with("mesh.loop_cut", &[], &mut c, &key('l')).unwrap(), Flow::Running);
        assert_eq!(counts(c.doc, cube), (12, 20, 10));
        ex.modal_event(&mut c, &moved(Vec2::new(360.0, 400.0)));
        let m = &c.doc.object_mesh(cube).unwrap().mesh;
        let xs: Vec<f64> = select::selected_verts(m).iter().map(|&v| m.position(v).x).collect();
        assert_eq!(xs.len(), 4);
        assert!(xs.iter().all(|x| (x - xs[0]).abs() < 1e-9), "the loop stays in one plane: {xs:?}");
        assert!((xs[0].abs() - 0.5).abs() < 1e-9, "40 px of 160 along the edge is a quarter: {xs:?}");
        assert_eq!(ex.modal_event(&mut c, &escape()), Some(Ok(Flow::Cancelled)));
        assert_eq!(counts(&doc, cube), (8, 12, 6));
        assert_eq!(ex.history.len(), steps);

        let mut c = ctx(&mut doc, Vec2::new(400.0, 400.0));
        ex.invoke_with("mesh.loop_cut", &[], &mut c, &key('l')).unwrap();
        assert_eq!(ex.modal_event(&mut c, &click()), Some(Ok(Flow::Finished)));
        assert_eq!(counts(&doc, cube), (12, 20, 10));
        assert_eq!(ex.history.len(), steps + 1);
        assert_eq!(doc.scene().unwrap().tool.select_mode, SelectMode::Edge, "the new loop is selected as edges");
        // Adjust: two evenly spaced cuts instead.
        ex.last_step_props().unwrap().1.set_by_name("cuts", Value::I64(2)).unwrap();
        ex.adjust_last(&mut Ctx::new(&mut doc)).unwrap();
        assert_eq!(counts(&doc, cube), (16, 28, 14));
        ex.undo(&mut doc);
        assert_eq!(counts(&doc, cube), (8, 12, 6));
    }

    #[test]
    fn needs_a_seed_edge() {
        let (mut doc, mut ex, _) = selected_cube();
        ex.run_with("object.mode_set", &[("mode", Value::Enum(1))], &mut Ctx::new(&mut doc)).unwrap();
        let mut c = ctx(&mut doc, Vec2::ZERO);
        assert_eq!(ex.invoke_with("mesh.loop_cut", &[], &mut c, &key('l')).unwrap(), Flow::Cancelled);
        assert!(ex.last_report.as_deref().unwrap().contains("select an edge"));
    }
}
