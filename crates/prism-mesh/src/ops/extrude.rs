//! Region extrude: the selected faces move onto duplicated vertices and the
//! region boundary grows side quads. Interior geometry that nothing uses any
//! more is removed. The caller then translates `verts`.

use std::collections::{HashMap, HashSet};

use crate::euler::EulerResult;
use crate::handle::{EdgeH, FaceH, LoopH, VertH};
use crate::mesh::Mesh;

#[derive(Clone, Debug, Default)]
pub struct ExtrudeResult {
    /// The moved faces (replacements for the input faces, in the same order).
    pub faces: Vec<FaceH>,
    /// One quad per boundary edge of the region.
    pub side_faces: Vec<FaceH>,
    /// Every new vertex; translate these to actually extrude.
    pub verts: Vec<VertH>,
    /// Old → new vertex for every vertex of the region.
    pub vert_map: Vec<(VertH, VertH)>,
}

impl Mesh {
    pub fn extrude_faces(&mut self, faces: &[FaceH]) -> EulerResult<ExtrudeResult> {
        let region: HashSet<FaceH> = faces.iter().copied().filter(|&f| self.face_live(f)).collect();
        if region.is_empty() {
            return Ok(ExtrudeResult::default());
        }
        // Region edges and their boundary-ness, decided before anything moves.
        let mut region_edges: Vec<EdgeH> = Vec::new();
        let mut boundary: Vec<(EdgeH, VertH, VertH)> = Vec::new(); // edge, a, b in the region face's direction
        for &f in faces {
            if !region.contains(&f) {
                continue;
            }
            for l in self.loops_of_face(f).collect::<Vec<LoopH>>() {
                let e = self.loop_edge(l);
                if region_edges.contains(&e) {
                    continue;
                }
                region_edges.push(e);
                let total = self.edge_face_count(e);
                let inside = self.faces_of_edge(e).filter(|g| region.contains(g)).count();
                if inside < total || inside == 1 {
                    boundary.push((e, self.loop_vert(l), self.loop_vert(self.loop_next(l))));
                }
            }
        }

        // Duplicate every region vertex.
        let mut map: HashMap<VertH, VertH> = HashMap::new();
        let mut order: Vec<VertH> = Vec::new();
        for &f in faces {
            if !region.contains(&f) {
                continue;
            }
            for v in self.verts_of_face(f).collect::<Vec<_>>() {
                if let std::collections::hash_map::Entry::Vacant(slot) = map.entry(v) {
                    let nv = self.verts.alloc(self.position(v));
                    self.verts.attrs.copy(nv.idx(), v.idx());
                    slot.insert(nv);
                    order.push(v);
                }
            }
        }

        // Rebuild every region face on the new vertices, carrying attributes.
        let mut new_faces = Vec::with_capacity(region.len());
        let mut old_faces = Vec::with_capacity(region.len());
        for &f in faces {
            if !region.contains(&f) || old_faces.contains(&f) {
                continue;
            }
            let old_loops: Vec<LoopH> = self.loops_of_face(f).collect();
            let ring: Vec<VertH> = old_loops.iter().map(|&l| map[&self.loop_vert(l)]).collect();
            let nf = self.add_face(&ring);
            self.faces.attrs.copy(nf.idx(), f.idx());
            for (nl, &ol) in self.loops_of_face(nf).collect::<Vec<_>>().into_iter().zip(&old_loops) {
                self.loops.attrs.copy(nl.idx(), ol.idx());
            }
            new_faces.push(nf);
            old_faces.push(f);
        }
        for f in &old_faces {
            self.kill_face(*f)?;
        }

        // Side quads along the boundary, wound to face outward.
        let mut side_faces = Vec::with_capacity(boundary.len());
        for (_, a, b) in &boundary {
            side_faces.push(self.add_face(&[*a, *b, map[b], map[a]]));
        }

        // Interior geometry nothing uses any more.
        for e in region_edges {
            if self.edge_live(e) && self.is_wire_edge(e) {
                self.kill_edge(e)?;
            }
        }
        for v in &order {
            if self.vert_live(*v) && self.vert_edge(*v).is_none() {
                self.kill_vert(*v)?;
            }
        }

        Ok(ExtrudeResult {
            faces: new_faces,
            side_faces,
            verts: order.iter().map(|v| map[v]).collect(),
            vert_map: order.iter().map(|&v| (v, map[&v])).collect(),
        })
    }

    /// Move vertices by `delta`.
    pub fn translate_verts(&mut self, verts: &[VertH], delta: prism_math::Vec3) {
        for &v in verts {
            if self.vert_live(v) {
                let p = self.position(v);
                self.set_position(v, p + delta);
            }
        }
    }
}
