//! Deletion in the three flavours a modeler expects.

use crate::euler::EulerResult;
use crate::handle::{EdgeH, FaceH, VertH};
use crate::mesh::Mesh;

impl Mesh {
    /// Remove vertices and everything that uses them.
    pub fn delete_verts(&mut self, verts: &[VertH]) -> EulerResult<()> {
        for &v in verts {
            if !self.vert_live(v) {
                continue;
            }
            for e in self.edges_of(v).collect::<Vec<_>>() {
                self.kill_edge(e)?;
            }
            self.kill_vert(v)?;
        }
        Ok(())
    }

    /// Remove edges (and their faces). With `remove_loose_verts`, vertices
    /// left with no edges go too.
    pub fn delete_edges(&mut self, edges: &[EdgeH], remove_loose_verts: bool) -> EulerResult<()> {
        let mut touched: Vec<VertH> = Vec::new();
        for &e in edges {
            if !self.edge_live(e) {
                continue;
            }
            touched.extend(self.edge_verts(e));
            self.kill_edge(e)?;
        }
        if remove_loose_verts {
            for v in touched {
                if self.vert_live(v) && self.vert_edge(v).is_none() {
                    self.kill_vert(v)?;
                }
            }
        }
        Ok(())
    }

    /// Remove faces. With `only_faces`, their edges and vertices stay;
    /// otherwise edges and vertices used by nothing else go too.
    pub fn delete_faces(&mut self, faces: &[FaceH], only_faces: bool) -> EulerResult<()> {
        let mut edges: Vec<EdgeH> = Vec::new();
        let mut verts: Vec<VertH> = Vec::new();
        for &f in faces {
            if !self.face_live(f) {
                continue;
            }
            if !only_faces {
                for l in self.loops_of_face(f).collect::<Vec<_>>() {
                    edges.push(self.loop_edge(l));
                    verts.push(self.loop_vert(l));
                }
            }
            self.kill_face(f)?;
        }
        for e in edges {
            if self.edge_live(e) && self.is_wire_edge(e) {
                self.kill_edge(e)?;
            }
        }
        for v in verts {
            if self.vert_live(v) && self.vert_edge(v).is_none() {
                self.kill_vert(v)?;
            }
        }
        Ok(())
    }
}
