//! Mesh selection helpers: selection is the `select` attribute per domain.
//! Vertices are primary; edges and faces follow from their vertices.

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
