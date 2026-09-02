//! Mesh selection helpers: selection is the `select` attribute per domain.
//! Vertices are primary; edges and faces follow from their vertices.

use prism_core::Id;
use prism_doc::{Doc, Elem, MeshBlock, SelectMode};
use prism_mesh::tables::{E_SELECT, F_SELECT, V_SELECT};
use prism_mesh::{EdgeH, FaceH, Mesh, VertH};

pub fn selected_verts(m: &Mesh) -> Vec<VertH> {
    let sel = m.vert_attrs().bools(V_SELECT);
    m.verts().filter(|v| sel[v.idx()]).collect()
}

pub fn selected_edges(m: &Mesh) -> Vec<EdgeH> {
    let sel = m.edge_attrs().bools(E_SELECT);
    m.edges().filter(|e| sel[e.idx()]).collect()
}

pub fn selected_faces(m: &Mesh) -> Vec<FaceH> {
    let sel = m.face_attrs().bools(F_SELECT);
    m.faces().filter(|f| sel[f.idx()]).collect()
}

pub fn any_selected(m: &Mesh) -> bool {
    let sel = m.vert_attrs().bools(V_SELECT);
    m.verts().any(|v| sel[v.idx()])
}

/// Select or deselect every element.
pub fn set_all(m: &mut Mesh, on: bool) {
    let verts: Vec<VertH> = m.verts().collect();
    let edges: Vec<EdgeH> = m.edges().collect();
    let faces: Vec<FaceH> = m.faces().collect();
    let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
    for v in verts {
        vs.set(v.idx(), on);
    }
    let es = m.edge_attrs_mut().bools_mut(E_SELECT);
    for e in edges {
        es.set(e.idx(), on);
    }
    let fs = m.face_attrs_mut().bools_mut(F_SELECT);
    for f in faces {
        fs.set(f.idx(), on);
    }
}

pub fn invert(m: &mut Mesh) {
    let verts: Vec<VertH> = m.verts().collect();
    let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
    for v in verts {
        let cur = vs[v.idx()];
        vs.set(v.idx(), !cur);
    }
    flush(m);
}

/// Derive edge and face selection from vertex selection.
pub fn flush(m: &mut Mesh) {
    let edges: Vec<(EdgeH, bool)> = m
        .edges()
        .map(|e| {
            let [a, b] = m.edge_verts(e);
            let vs = m.vert_attrs().bools(V_SELECT);
            (e, vs[a.idx()] && vs[b.idx()])
        })
        .collect();
    let faces: Vec<(FaceH, bool)> = m
        .faces()
        .map(|f| {
            let vs = m.vert_attrs().bools(V_SELECT);
            (f, m.verts_of_face(f).all(|v| vs[v.idx()]))
        })
        .collect();
    let es = m.edge_attrs_mut().bools_mut(E_SELECT);
    for (e, on) in edges {
        es.set(e.idx(), on);
    }
    let fs = m.face_attrs_mut().bools_mut(F_SELECT);
    for (f, on) in faces {
        fs.set(f.idx(), on);
    }
}

/// Select exactly these faces (their vertices, then flush).
pub fn select_faces(m: &mut Mesh, faces: &[FaceH]) {
    set_all(m, false);
    let verts: Vec<VertH> = faces.iter().flat_map(|&f| m.verts_of_face(f).collect::<Vec<_>>()).collect();
    let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
    for v in verts {
        vs.set(v.idx(), true);
    }
    flush(m);
}

pub fn select_verts(m: &mut Mesh, verts: &[VertH], on: bool) {
    let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
    for &v in verts {
        if m_live(v, vs.len()) {
            vs.set(v.idx(), on);
        }
    }
    flush(m);
}

fn m_live(v: VertH, len: usize) -> bool {
    v.idx() < len
}

/// Make the three domains agree, treating `mode`'s domain as primary.
pub fn flush_mode(m: &mut Mesh, mode: prism_doc::SelectMode) {
    match mode {
        prism_doc::SelectMode::Vertex => flush(m),
        prism_doc::SelectMode::Edge => {
            let edges = selected_edges(m);
            let verts: Vec<VertH> = m.verts().collect();
            let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
            for v in verts {
                vs.set(v.idx(), false);
            }
            for &e in &edges {
                let [a, b] = m.edge_verts(e);
                let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
                vs.set(a.idx(), true);
                vs.set(b.idx(), true);
            }
            let faces: Vec<(FaceH, bool)> = m
                .faces()
                .map(|f| {
                    let es = m.edge_attrs().bools(E_SELECT);
                    (f, m.edges_of_face(f).all(|e| es[e.idx()]))
                })
                .collect();
            let fs = m.face_attrs_mut().bools_mut(F_SELECT);
            for (f, on) in faces {
                fs.set(f.idx(), on);
            }
        }
        prism_doc::SelectMode::Face => {
            let faces = selected_faces(m);
            let verts: Vec<VertH> = m.verts().collect();
            let edges: Vec<EdgeH> = m.edges().collect();
            let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
            for v in verts {
                vs.set(v.idx(), false);
            }
            let es = m.edge_attrs_mut().bools_mut(E_SELECT);
            for e in edges {
                es.set(e.idx(), false);
            }
            for &f in &faces {
                let vs_of: Vec<VertH> = m.verts_of_face(f).collect();
                let es_of: Vec<EdgeH> = m.edges_of_face(f).collect();
                let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
                for v in vs_of {
                    vs.set(v.idx(), true);
                }
                let es = m.edge_attrs_mut().bools_mut(E_SELECT);
                for e in es_of {
                    es.set(e.idx(), true);
                }
            }
        }
    }
}

pub fn is_selected(m: &Mesh, e: Elem) -> bool {
    match e {
        Elem::Vert(v) => m.vert_attrs().bools(V_SELECT)[v.idx()],
        Elem::Edge(ed) => m.edge_attrs().bools(E_SELECT)[ed.idx()],
        Elem::Face(f) => m.face_attrs().bools(F_SELECT)[f.idx()],
    }
}

/// Box-select objects (D025). With neither flag `ids` replaces the selection;
/// `extend` adds them, `subtract` removes them. The active object follows a
/// replaced selection. Returns whether anything changed.
pub fn select_objects(doc: &mut Doc, ids: &[Id], extend: bool, subtract: bool) -> bool {
    let mut changed = false;
    if !extend && !subtract {
        for id in doc.scene_objects() {
            if let Some(o) = doc.objects.get_mut(id)
                && o.selected
                && !ids.contains(&id)
            {
                o.selected = false;
                changed = true;
            }
        }
    }
    for &id in ids {
        if let Some(o) = doc.objects.get_mut(id)
            && o.selected == subtract
        {
            o.selected = !subtract;
            changed = true;
        }
    }
    // The active object must stay selected; otherwise hand it to the first
    // picked object that is, else to any selected object, else nothing.
    let active_selected = doc.objects.get(doc.active_object_id()).is_some_and(|o| o.selected);
    if !active_selected {
        let next = ids.iter().copied().find(|&id| doc.objects.get(id).is_some_and(|o| o.selected)).or_else(|| doc.selected_objects().first().copied());
        if let Some(s) = doc.scene_mut() {
            s.active_object = next.unwrap_or(Id::NONE);
        }
    }
    changed
}

/// Box-select elements of one domain (D025), same flags as
/// [`select_objects`]. Vertices of chosen edges and faces follow, then the
/// domains are flushed for `mode`. The active element is dropped if it ends
/// up deselected. Returns whether anything changed.
pub fn select_elems(block: &mut MeshBlock, mode: SelectMode, elems: &[Elem], extend: bool, subtract: bool) -> bool {
    let m = &mut block.mesh;
    let snapshot = |m: &Mesh| (selected_verts(m), selected_edges(m), selected_faces(m));
    let before = snapshot(m);
    if !extend && !subtract {
        set_all(m, false);
    }
    let on = !subtract;
    for &e in elems {
        match e {
            Elem::Vert(v) => m.vert_attrs_mut().bools_mut(V_SELECT).set(v.idx(), on),
            Elem::Edge(ed) => {
                let [a, b] = m.edge_verts(ed);
                m.edge_attrs_mut().bools_mut(E_SELECT).set(ed.idx(), on);
                let vs = m.vert_attrs_mut().bools_mut(V_SELECT);
                vs.set(a.idx(), on);
                vs.set(b.idx(), on);
            }
            Elem::Face(f) => {
                let verts: Vec<_> = m.verts_of_face(f).collect();
                let edges: Vec<_> = m.edges_of_face(f).collect();
                m.face_attrs_mut().bools_mut(F_SELECT).set(f.idx(), on);
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
    flush_mode(m, mode);
    block.edit.history.retain(|&e| is_selected(m, e));
    if block.edit.active.is_some_and(|a| !is_selected(m, a)) {
        block.edit.active = block.edit.history.last().copied();
    }
    snapshot(m) != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_select_objects_replaces_extends_subtracts() {
        let mut doc = Doc::starter();
        let objs = doc.scene_objects();
        let (cube, light) = (objs[0], objs[1]);
        assert!(select_objects(&mut doc, &[cube], false, false));
        assert!(doc.objects.get(cube).unwrap().selected && doc.active_object_id() == cube);
        assert!(!select_objects(&mut doc, &[cube], false, false), "nothing changed");
        assert!(select_objects(&mut doc, &[light], true, false));
        assert_eq!(doc.selected_objects().len(), 2);
        assert!(select_objects(&mut doc, &[cube], false, true));
        assert_eq!(doc.selected_objects(), vec![light]);
        assert_eq!(doc.active_object_id(), light, "active follows when the old one goes");
        assert!(select_objects(&mut doc, &[], false, false), "an empty box clears");
        assert!(doc.selected_objects().is_empty());
    }

    #[test]
    fn box_select_faces_flushes_and_prunes_active() {
        let mut doc = Doc::starter();
        let cube = doc.scene_objects()[0];
        let block = doc.object_mesh_mut(cube).unwrap();
        let faces: Vec<FaceH> = block.mesh.faces().collect();
        block.edit.active = Some(Elem::Face(faces[0]));
        block.edit.history.push(Elem::Face(faces[0]));
        assert!(select_elems(block, SelectMode::Face, &[Elem::Face(faces[1])], false, false));
        assert_eq!(selected_faces(&block.mesh), vec![faces[1]]);
        assert_eq!(selected_verts(&block.mesh).len(), 4);
        assert_eq!(block.edit.active, None, "the old active face is no longer selected");
        assert!(select_elems(block, SelectMode::Face, &[Elem::Face(faces[0])], true, false));
        assert_eq!(selected_faces(&block.mesh).len(), 2);
        assert!(select_elems(block, SelectMode::Face, &[Elem::Face(faces[1])], false, true));
        assert_eq!(selected_faces(&block.mesh), vec![faces[0]]);
        assert!(!select_elems(block, SelectMode::Face, &[], true, false), "extend with nothing: no change");
    }
}
