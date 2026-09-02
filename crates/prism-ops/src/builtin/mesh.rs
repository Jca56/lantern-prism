//! Edit-mode operators on the active object's mesh.

use prism_doc::{MeshBlock, ObjectMode};
use prism_math::Vec3;
use prism_mesh::tables::{E_SELECT, F_SELECT, V_SELECT};
use prism_props::props;

use crate::builtin::select;
use crate::context::{Ctx, Outcome};
use crate::operator::{OpFlags, OpResult, Operator};
use crate::registry::Registry;

/// The mesh being edited, if the active object is a mesh in edit mode.
fn edit_mesh<'c>(ctx: &'c mut Ctx<'_>) -> Option<&'c mut MeshBlock> {
    let id = ctx.doc.active_object_id();
    let o = ctx.doc.objects.get(id)?;
    if o.mode != ObjectMode::Edit {
        return None;
    }
    ctx.doc.object_mesh_mut(id)
}

fn in_edit_mode(ctx: &Ctx) -> bool {
    let id = ctx.doc.active_object_id();
    ctx.doc.objects.get(id).is_some_and(|o| o.mode == ObjectMode::Edit) && ctx.doc.object_mesh(id).is_some()
}

props! {
    pub enum SelectAction {
        Toggle = 0,
        Select = 1,
        Deselect = 2,
        Invert = 3,
    }
}

props! {
    pub struct SelectAllProps {
        pub action: SelectAction = SelectAction::Toggle => { id: 1 },
    }
}

pub struct SelectAll;
impl Operator for SelectAll {
    const ID: &'static str = "mesh.select_all";
    const LABEL: &'static str = "Select All (Mesh)";
    type Props = SelectAllProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &SelectAllProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let m = &mut block.mesh;
        match p.action {
            SelectAction::Toggle => {
                let any = select::any_selected(m);
                select::set_all(m, !any);
            }
            SelectAction::Select => select::set_all(m, true),
            SelectAction::Deselect => select::set_all(m, false),
            SelectAction::Invert => select::invert(m),
        }
        Ok(Outcome::Finished)
    }
}

props! {
    pub enum DeleteKind {
        Verts = 0 => { label: "Vertices" },
        Edges = 1,
        Faces = 2,
        OnlyFaces = 3 => { label: "Only Faces" },
    }
}

props! {
    pub struct DeleteProps {
        pub kind: DeleteKind = DeleteKind::Verts => { id: 1 },
    }
}

pub struct Delete;
impl Operator for Delete {
    const ID: &'static str = "mesh.delete";
    const LABEL: &'static str = "Delete (Mesh)";
    type Props = DeleteProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &DeleteProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let m = &mut block.mesh;
        let n = match p.kind {
            DeleteKind::Verts => {
                let v = select::selected_verts(m);
                m.delete_verts(&v)?;
                v.len()
            }
            DeleteKind::Edges => {
                let e = select::selected_edges(m);
                m.delete_edges(&e, true)?;
                e.len()
            }
            DeleteKind::Faces => {
                let f = select::selected_faces(m);
                m.delete_faces(&f, false)?;
                f.len()
            }
            DeleteKind::OnlyFaces => {
                let f = select::selected_faces(m);
                m.delete_faces(&f, true)?;
                f.len()
            }
        };
        if n == 0 {
            return Ok(Outcome::Cancelled);
        }
        block.edit.active = None;
        block.edit.history.clear();
        ctx.report(format!("Deleted {n}"));
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct ExtrudeProps {
        /// Distance along the average normal of the selected faces.
        pub offset: f64 = 1.0 => { id: 1, soft: -10.0..=10.0, subtype: Distance },
    }
}

pub struct Extrude;
impl Operator for Extrude {
    const ID: &'static str = "mesh.extrude";
    const LABEL: &'static str = "Extrude Faces";
    type Props = ExtrudeProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &ExtrudeProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let m = &mut block.mesh;
        let faces = select::selected_faces(m);
        if faces.is_empty() {
            return Ok(Outcome::Cancelled);
        }
        let r = m.extrude_faces(&faces)?;
        // Each new vertex moves along the average normal of the new faces
        // around it, so a flat region moves as a slab and a closed shell grows.
        let normals: Vec<Vec3> = r.faces.iter().map(|&f| m.face_normal(f)).collect();
        for &v in &r.verts {
            let mut n = Vec3::ZERO;
            for (i, &f) in r.faces.iter().enumerate() {
                if m.verts_of_face(f).any(|x| x == v) {
                    n += normals[i];
                }
            }
            let p0 = m.position(v);
            m.set_position(v, p0 + n.normalize_or_zero() * p.offset);
        }
        select::select_faces(m, &r.faces);
        ctx.report(format!("Extruded {} face(s)", r.faces.len()));
        Ok(Outcome::Finished)
    }
}

props! {
    pub enum DissolveKind {
        Verts = 0 => { label: "Vertices" },
        Edges = 1,
        Faces = 2,
    }
}

props! {
    pub struct DissolveProps {
        pub kind: DissolveKind = DissolveKind::Edges => { id: 1 },
    }
}

pub struct Dissolve;
impl Operator for Dissolve {
    const ID: &'static str = "mesh.dissolve";
    const LABEL: &'static str = "Dissolve";
    type Props = DissolveProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &DissolveProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let m = &mut block.mesh;
        let n = match p.kind {
            DissolveKind::Verts => m.dissolve_verts(&select::selected_verts(m)),
            DissolveKind::Edges => m.dissolve_edges(&select::selected_edges(m)),
            DissolveKind::Faces => {
                let faces = select::selected_faces(m);
                let before = m.face_count();
                let left = m.dissolve_faces(&faces)?;
                select::select_faces(m, &left);
                before - m.face_count()
            }
        };
        if n == 0 {
            return Ok(Outcome::Cancelled);
        }
        select::flush(m);
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct SubdivideProps {
        pub cuts: i64 = 1 => { id: 1, hard: 1..=100, soft: 1..=10 },
    }
}

pub struct Subdivide;
impl Operator for Subdivide {
    const ID: &'static str = "mesh.subdivide";
    const LABEL: &'static str = "Subdivide Edges";
    type Props = SubdivideProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &SubdivideProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let m = &mut block.mesh;
        let edges = select::selected_edges(m);
        if edges.is_empty() {
            return Ok(Outcome::Cancelled);
        }
        let new = m.subdivide_edges(&edges, p.cuts.max(1) as usize)?;
        select::select_verts(m, &new, true);
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct MergeProps {
        pub threshold: f64 = 0.0001 => { id: 1, hard: 0.0.., soft: 0.0..=1.0, subtype: Distance },
    }
}

pub struct MergeByDistance;
impl Operator for MergeByDistance {
    const ID: &'static str = "mesh.merge_by_distance";
    const LABEL: &'static str = "Merge by Distance";
    type Props = MergeProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &MergeProps) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let n = block.mesh.merge_by_distance(p.threshold);
        select::flush(&mut block.mesh);
        ctx.report(format!("Removed {n} vertices"));
        Ok(if n > 0 { Outcome::Finished } else { Outcome::Cancelled })
    }
}

props! {
    pub struct Empty {}
}

pub struct FlipNormals;
impl Operator for FlipNormals {
    const ID: &'static str = "mesh.flip_normals";
    const LABEL: &'static str = "Flip Normals";
    type Props = Empty;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let faces = select::selected_faces(&block.mesh);
        if faces.is_empty() {
            return Ok(Outcome::Cancelled);
        }
        block.mesh.flip_faces(&faces)?;
        Ok(Outcome::Finished)
    }
}

pub struct NormalsMakeConsistent;
impl Operator for NormalsMakeConsistent {
    const ID: &'static str = "mesh.normals_make_consistent";
    const LABEL: &'static str = "Recalculate Normals";
    type Props = Empty;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, _: &Empty) -> OpResult<Outcome> {
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let mut faces = select::selected_faces(&block.mesh);
        if faces.is_empty() {
            faces = block.mesh.faces().collect();
        }
        let n = block.mesh.make_normals_consistent(&faces)?;
        ctx.report(format!("Flipped {n} face(s)"));
        Ok(Outcome::Finished)
    }
}

props! {
    pub enum ElemKind {
        Vert = 0,
        Edge = 1,
        Face = 2,
    }
}

props! {
    /// Select one element, as picked in a viewport.
    pub struct SelectElemProps {
        pub kind: ElemKind = ElemKind::Vert => { id: 1 },
        /// Raw handle bits (`Handle::to_raw`).
        pub handle: i64 = 0 => { id: 2, flags: HIDDEN },
        pub extend: bool = false => { id: 3 },
        pub toggle: bool = false => { id: 4 },
    }
}

pub struct SelectElem;
impl Operator for SelectElem {
    const ID: &'static str = "mesh.select";
    const LABEL: &'static str = "Select Element";
    const FLAGS: OpFlags = OpFlags::UNDO;
    type Props = SelectElemProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &SelectElemProps) -> OpResult<Outcome> {
        let mode = ctx.doc.scene().map_or(prism_doc::SelectMode::Vertex, |s| s.tool.select_mode);
        let Some(block) = edit_mesh(ctx) else {
            return Ok(Outcome::Cancelled);
        };
        let m = &mut block.mesh;
        let raw = p.handle as u64;
        let elem = match p.kind {
            ElemKind::Vert => prism_mesh::VertH::from_raw(raw).filter(|&v| m.vert_live(v)).map(prism_doc::Elem::Vert),
            ElemKind::Edge => prism_mesh::EdgeH::from_raw(raw).filter(|&e| m.edge_live(e)).map(prism_doc::Elem::Edge),
            ElemKind::Face => prism_mesh::FaceH::from_raw(raw).filter(|&f| m.face_live(f)).map(prism_doc::Elem::Face),
        };
        let Some(elem) = elem else {
            return Err(crate::operator::OpError::Failed("element is gone".into()));
        };
        let currently = match elem {
            prism_doc::Elem::Vert(v) => m.vert_attrs().bools(V_SELECT)[v.idx()],
            prism_doc::Elem::Edge(e) => m.edge_attrs().bools(E_SELECT)[e.idx()],
            prism_doc::Elem::Face(f) => m.face_attrs().bools(F_SELECT)[f.idx()],
        };
        let on = if p.toggle { !currently } else { true };
        if !p.extend && !p.toggle {
            select::set_all(m, false);
        }
        match elem {
            prism_doc::Elem::Vert(v) => m.vert_attrs_mut().bools_mut(V_SELECT).set(v.idx(), on),
            prism_doc::Elem::Edge(e) => {
                let [a, b] = m.edge_verts(e);
                m.edge_attrs_mut().bools_mut(E_SELECT).set(e.idx(), on);
                if on || !p.extend {
                    let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
                    vs.set(a.idx(), on);
                    vs.set(b.idx(), on);
                }
            }
            prism_doc::Elem::Face(f) => {
                let verts: Vec<_> = m.verts_of_face(f).collect();
                let edges: Vec<_> = m.edges_of_face(f).collect();
                m.face_attrs_mut().bools_mut(F_SELECT).set(f.idx(), on);
                if on || !p.extend {
                    let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
                    for v in verts {
                        vs.set(v.idx(), on);
                    }
                    let es = m.edge_attrs_mut().bools_mut(E_SELECT);
                    for e in edges {
                        es.set(e.idx(), on);
                    }
                }
            }
        }
        select::flush_mode(m, mode);
        block.edit.history.retain(|e| *e != elem);
        if on {
            block.edit.history.push(elem);
            block.edit.active = Some(elem);
        } else if block.edit.active == Some(elem) {
            block.edit.active = block.edit.history.last().copied();
        }
        Ok(Outcome::Finished)
    }
}

props! {
    pub struct SelectModeProps {
        pub mode: prism_doc::SelectMode = prism_doc::SelectMode::Vertex => { id: 1 },
    }
}

pub struct SelectMode;
impl Operator for SelectMode {
    const ID: &'static str = "mesh.select_mode";
    const LABEL: &'static str = "Select Mode";
    type Props = SelectModeProps;
    type Modal = ();
    fn poll(ctx: &Ctx) -> bool {
        in_edit_mode(ctx)
    }
    fn exec(ctx: &mut Ctx, p: &SelectModeProps) -> OpResult<Outcome> {
        if let Some(s) = ctx.doc.scene_mut() {
            if s.tool.select_mode == p.mode {
                return Ok(Outcome::Cancelled);
            }
            s.tool.select_mode = p.mode;
        }
        if let Some(block) = edit_mesh(ctx) {
            select::flush_mode(&mut block.mesh, p.mode);
        }
        Ok(Outcome::Finished)
    }
}

pub fn register(r: &mut Registry) {
    r.register::<SelectElem>();
    r.register::<SelectMode>();
    r.register::<SelectAll>();
    r.register::<Delete>();
    r.register::<Extrude>();
    r.register::<Dissolve>();
    r.register::<Subdivide>();
    r.register::<MergeByDistance>();
    r.register::<FlipNormals>();
    r.register::<NormalsMakeConsistent>();
}
