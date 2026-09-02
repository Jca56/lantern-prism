//! `MeshBuffers`: corners for solid shading, compact vertices for wireframe,
//! and maps from every output element back to the edit mesh.

use prism_geom::normal::corner_angle;
use prism_geom::triangulate::triangulate;
use prism_math::{Aabb, Vec2, Vec3};
use prism_mesh::tables::{E_SHARP, F_SMOOTH};
use prism_mesh::{EdgeH, FaceH, LoopH, Mesh, VertH, names};

#[derive(Clone, Debug, Default)]
pub struct MeshBuffers {
    /// One entry per face corner (loop), in face order.
    pub corner_positions: Vec<Vec3>,
    pub corner_normals: Vec<Vec3>,
    /// Empty when the mesh has no `uv` loop layer.
    pub corner_uvs: Vec<Vec2>,
    /// Three corner indices per triangle.
    pub tri_indices: Vec<u32>,
    pub tri_to_face: Vec<FaceH>,
    pub corner_to_loop: Vec<LoopH>,
    /// Compact live faces, in face order; `corner_face` indexes this.
    pub face_handles: Vec<FaceH>,
    /// Compact face index per corner (flat shading data and pick ids).
    pub corner_face: Vec<u32>,
    /// Compact live vertices, for wireframe and vertex overlays.
    pub vert_positions: Vec<Vec3>,
    pub vert_to_vert: Vec<VertH>,
    /// Two compact vertex indices per edge (wire edges included).
    pub edge_indices: Vec<u32>,
    pub edge_to_edge: Vec<EdgeH>,
    /// Compact indices of vertices with no edges.
    pub loose_verts: Vec<u32>,
    pub bounds: Aabb,
}

impl MeshBuffers {
    pub fn tri_count(&self) -> usize {
        self.tri_indices.len() / 3
    }
}

/// Build buffers from `mesh`.
pub fn evaluate(mesh: &Mesh) -> MeshBuffers {
    let mut out = MeshBuffers::default();

    // Compact vertices.
    let cap = mesh.positions().len();
    let mut vert_index = vec![u32::MAX; cap];
    for v in mesh.verts() {
        vert_index[v.idx()] = out.vert_positions.len() as u32;
        let p = mesh.position(v);
        out.vert_positions.push(p);
        out.vert_to_vert.push(v);
        out.bounds.include(p);
        if mesh.vert_edge(v).is_none() {
            out.loose_verts.push(vert_index[v.idx()]);
        }
    }
    for e in mesh.edges() {
        let [a, b] = mesh.edge_verts(e);
        out.edge_indices.push(vert_index[a.idx()]);
        out.edge_indices.push(vert_index[b.idx()]);
        out.edge_to_edge.push(e);
    }

    // Face normals once.
    let face_cap = mesh.faces().map(|f| f.idx() + 1).max().unwrap_or(0);
    let mut face_normal = vec![Vec3::ZERO; face_cap];
    for f in mesh.faces() {
        face_normal[f.idx()] = mesh.face_normal(f);
    }
    let smooth = mesh.face_attrs().bools(F_SMOOTH);
    let sharp = mesh.edge_attrs().bools(E_SHARP);
    let uv_layer = mesh.loop_attrs().index(names::UV).map(|i| mesh.loop_attrs().vec2s(i));

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut positions: Vec<Vec3> = Vec::new();
    for f in mesh.faces() {
        let base = out.corner_positions.len() as u32;
        let face_index = out.face_handles.len() as u32;
        out.face_handles.push(f);
        positions.clear();
        for l in mesh.loops_of_face(f) {
            out.corner_face.push(face_index);
            let v = mesh.loop_vert(l);
            let p = mesh.position(v);
            positions.push(p);
            out.corner_positions.push(p);
            out.corner_to_loop.push(l);
            out.corner_normals.push(corner_normal(mesh, l, f, &face_normal, smooth, sharp));
            if let Some(uv) = uv_layer {
                out.corner_uvs.push(uv[l.idx()]);
            }
        }
        tris.clear();
        triangulate(&positions, &mut tris);
        for t in &tris {
            out.tri_indices.extend_from_slice(&[base + t[0], base + t[1], base + t[2]]);
            out.tri_to_face.push(f);
        }
    }
    out
}

/// Normal at corner `l` of face `f`: the face normal for flat faces, else the
/// angle-weighted average over the fan of smooth faces reachable from `f`
/// around the corner's vertex without crossing a sharp or non-manifold edge.
fn corner_normal(
    mesh: &Mesh,
    l: LoopH,
    f: FaceH,
    face_normal: &[Vec3],
    smooth: &prism_core::ChunkedVec<bool>,
    sharp: &prism_core::ChunkedVec<bool>,
) -> Vec3 {
    if !smooth[f.idx()] {
        return face_normal[f.idx()];
    }
    let v = mesh.loop_vert(l);
    let mut fan: Vec<FaceH> = vec![f];
    let mut stack = vec![f];
    while let Some(g) = stack.pop() {
        // The two edges of `g` at `v`.
        let Some(lg) = mesh.face_loop_at(g, v) else {
            continue;
        };
        for e in [mesh.loop_edge(lg), mesh.loop_edge(mesh.loop_prev(lg))] {
            if sharp[e.idx()] || !mesh.is_manifold_edge(e) {
                continue;
            }
            for h in mesh.faces_of_edge(e) {
                if h != g && smooth[h.idx()] && !fan.contains(&h) {
                    fan.push(h);
                    stack.push(h);
                }
            }
        }
    }
    let mut n = Vec3::ZERO;
    for g in fan {
        let Some(lg) = mesh.face_loop_at(g, v) else {
            continue;
        };
        let prev = mesh.position(mesh.loop_vert(mesh.loop_prev(lg)));
        let next = mesh.position(mesh.loop_vert(mesh.loop_next(lg)));
        n += face_normal[g.idx()] * corner_angle(prev, mesh.position(v), next);
    }
    n.normalize_or(face_normal[f.idx()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_mesh::primitives;

    #[test]
    fn cube_buffers() {
        let m = primitives::cube(2.0);
        let b = evaluate(&m);
        assert_eq!(b.vert_positions.len(), 8);
        assert_eq!(b.edge_indices.len(), 24);
        assert_eq!(b.corner_positions.len(), 24);
        assert_eq!(b.tri_count(), 12);
        assert_eq!(b.tri_to_face.len(), 12);
        assert_eq!(b.face_handles.len(), 6);
        assert_eq!(b.corner_face.len(), 24);
        assert_eq!(b.corner_face[23], 5);
        assert!(b.loose_verts.is_empty());
        assert!(b.corner_uvs.is_empty());
        assert_eq!(b.bounds, Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0)));
        // Flat shading: each corner normal is its face's axis normal.
        for (i, n) in b.corner_normals.iter().enumerate() {
            let f = m.loop_face(b.corner_to_loop[i]);
            assert!(n.approx_eq(m.face_normal(f), 1e-12));
        }
        // Every triangle winds with its face.
        for (t, &f) in b.tri_to_face.iter().enumerate() {
            let [a, bb, c] = [b.tri_indices[t * 3], b.tri_indices[t * 3 + 1], b.tri_indices[t * 3 + 2]];
            let (pa, pb, pc) = (b.corner_positions[a as usize], b.corner_positions[bb as usize], b.corner_positions[c as usize]);
            assert!((pb - pa).cross(pc - pa).dot(m.face_normal(f)) > 0.0);
        }
    }

    #[test]
    fn smooth_sphere_normals_point_out() {
        let mut m = primitives::uv_sphere(1.0, 12, 8);
        let n = m.face_count();
        {
            let smooth = m.face_attrs_mut().bools_mut(F_SMOOTH);
            for i in 0..smooth.len() {
                smooth.set(i, true);
            }
        }
        assert_eq!(m.face_count(), n);
        let b = evaluate(&m);
        for (i, nrm) in b.corner_normals.iter().enumerate() {
            let p = b.corner_positions[i];
            // A smooth sphere's corner normals approach the radial direction.
            assert!(nrm.dot(p.normalize()) > 0.9, "corner {i}: {nrm:?} vs {p:?}");
        }
    }

    #[test]
    fn sharp_edges_split_normals() {
        let mut m = primitives::cube(2.0);
        {
            let smooth = m.face_attrs_mut().bools_mut(F_SMOOTH);
            for i in 0..smooth.len() {
                smooth.set(i, true);
            }
        }
        let b = evaluate(&m);
        // Fully smooth cube: corners at the same vertex share one normal.
        let v0 = m.verts().next().unwrap();
        let normals: Vec<Vec3> = b
            .corner_to_loop
            .iter()
            .enumerate()
            .filter(|(_, l)| m.loop_vert(**l) == v0)
            .map(|(i, _)| b.corner_normals[i])
            .collect();
        assert_eq!(normals.len(), 3);
        assert!(normals.iter().all(|n| n.approx_eq(normals[0], 1e-9)));
        // Mark every edge sharp: back to face normals.
        {
            let sharp = m.edge_attrs_mut().bools_mut(E_SHARP);
            for i in 0..sharp.len() {
                sharp.set(i, true);
            }
        }
        let b = evaluate(&m);
        for (i, n) in b.corner_normals.iter().enumerate() {
            let f = m.loop_face(b.corner_to_loop[i]);
            assert!(n.approx_eq(m.face_normal(f), 1e-12));
        }
    }

    #[test]
    fn loose_and_wire() {
        let mut m = primitives::circle(1.0, 5, false);
        m.make_vert(Vec3::Y * 3.0);
        let b = evaluate(&m);
        assert_eq!(b.tri_count(), 0);
        assert_eq!(b.edge_indices.len(), 10);
        assert_eq!(b.loose_verts, vec![5]);
    }
}
