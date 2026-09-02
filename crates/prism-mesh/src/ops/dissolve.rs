//! Dissolve (remove geometry while keeping the surface), subdivide edges,
//! connect vertices.
//!
//! The workhorse is [`Mesh::join_faces`]: it replaces a connected region of
//! faces by one n-gon over the region's boundary loop, dropping everything
//! inside. Dissolving an edge, a vertex or a face selection all reduce to it.

use std::collections::HashMap;

use crate::euler::{EulerError, EulerResult};
use crate::handle::{EdgeH, FaceH, VertH};
use crate::mesh::Mesh;

impl Mesh {
    /// Replace `region` (connected, no holes) by a single face over its
    /// boundary. Interior edges and vertices are removed. Fails, leaving the
    /// mesh untouched, if the boundary is not one simple closed loop.
    pub fn join_faces(&mut self, region: &[FaceH]) -> EulerResult<FaceH> {
        let region: Vec<FaceH> = {
            let mut r: Vec<FaceH> = Vec::new();
            for &f in region {
                if self.face_live(f) && !r.contains(&f) {
                    r.push(f);
                }
            }
            r
        };
        let Some(&first) = region.first() else {
            return Err(EulerError::TooFewVerts(0));
        };
        if region.len() == 1 {
            return Ok(first);
        }
        // Directed boundary edges (in the region faces' winding) and the
        // interior edges/vertices.
        let mut next_of: HashMap<VertH, (VertH, EdgeH)> = HashMap::new();
        let mut boundary_count = 0usize;
        let mut region_edges: Vec<EdgeH> = Vec::new();
        let mut region_verts: Vec<VertH> = Vec::new();
        for &f in &region {
            for l in self.loops_of_face(f).collect::<Vec<_>>() {
                let e = self.loop_edge(l);
                let v = self.loop_vert(l);
                if !region_verts.contains(&v) {
                    region_verts.push(v);
                }
                if region_edges.contains(&e) {
                    continue;
                }
                region_edges.push(e);
                let inside = self.faces_of_edge(e).filter(|g| region.contains(g)).count();
                let total = self.edge_face_count(e);
                if inside == 1 || inside < total {
                    let to = self.loop_vert(self.loop_next(l));
                    if next_of.insert(v, (to, e)).is_some() {
                        return Err(EulerError::Degenerate); // boundary pinches at v
                    }
                    boundary_count += 1;
                }
            }
        }
        // Walk the boundary into one closed ring.
        let start = *next_of.keys().min_by_key(|v| v.index()).ok_or(EulerError::Degenerate)?;
        let mut ring = vec![start];
        let mut cur = start;
        loop {
            let &(to, _) = next_of.get(&cur).ok_or(EulerError::Degenerate)?;
            if to == start {
                break;
            }
            if ring.contains(&to) || ring.len() > boundary_count {
                return Err(EulerError::Degenerate);
            }
            ring.push(to);
            cur = to;
        }
        if ring.len() != boundary_count || ring.len() < 3 {
            return Err(EulerError::Degenerate); // holes or several loops
        }

        // Tear down the region, keep its boundary, build the n-gon.
        for &f in &region {
            self.kill_face(f)?;
        }
        for e in region_edges {
            if self.edge_live(e) && self.is_wire_edge(e) && !next_of.values().any(|&(_, be)| be == e) {
                self.kill_edge(e)?;
            }
        }
        for v in region_verts {
            if self.vert_live(v) && self.vert_edge(v).is_none() {
                self.kill_vert(v)?;
            }
        }
        let nf = self.make_face(&ring)?;
        self.faces.attrs.copy(nf.idx(), first.idx());
        Ok(nf)
    }

    /// Merge the two faces of a manifold edge.
    pub fn dissolve_edge(&mut self, e: EdgeH) -> EulerResult<FaceH> {
        if !self.edge_live(e) {
            return Err(EulerError::DeadEdge(e));
        }
        let faces: Vec<FaceH> = self.faces_of_edge(e).collect();
        if faces.len() != 2 || faces[0] == faces[1] {
            return Err(EulerError::FacesMismatch);
        }
        self.join_faces(&faces)
    }

    /// Dissolve every edge that can be. Returns how many were.
    pub fn dissolve_edges(&mut self, edges: &[EdgeH]) -> usize {
        edges.iter().filter(|&&e| self.dissolve_edge(e).is_ok()).count()
    }

    /// Remove a vertex, merging the faces around it into one.
    pub fn dissolve_vert(&mut self, v: VertH) -> EulerResult<()> {
        if !self.vert_live(v) {
            return Err(EulerError::DeadVert(v));
        }
        let faces = self.faces_of_vert(v);
        if faces.len() >= 2 {
            self.join_faces(&faces)?;
        }
        if !self.vert_live(v) {
            return Ok(());
        }
        // Still here: v sits on the boundary of the merged face (or a lone one).
        let edges: Vec<EdgeH> = self.edges_of(v).collect();
        match edges.len() {
            0 => self.kill_vert(v),
            2 => self.join_edge_kill_vert(edges[0], v).map(|_| ()),
            _ => Err(EulerError::VertNotValence2(v)),
        }
    }

    pub fn dissolve_verts(&mut self, verts: &[VertH]) -> usize {
        verts.iter().filter(|&&v| self.dissolve_vert(v).is_ok()).count()
    }

    /// Merge a connected region of faces into one face, then dissolve the
    /// boundary vertices that became collinear (a straight edge no longer
    /// needs its midpoints).
    pub fn dissolve_faces(&mut self, faces: &[FaceH]) -> EulerResult<Vec<FaceH>> {
        let mut region: Vec<FaceH> = faces.iter().copied().filter(|&f| self.face_live(f)).collect();
        if region.is_empty() {
            return Ok(Vec::new());
        }
        let mut verts: Vec<VertH> = Vec::new();
        for &f in &region {
            for v in self.verts_of_face(f) {
                if !verts.contains(&v) {
                    verts.push(v);
                }
            }
        }
        // Grow connected groups and join each; a group that will not join
        // (holes, pinches) is left as it is.
        let mut result = Vec::new();
        while let Some(seed) = region.pop() {
            let mut group = vec![seed];
            let mut i = 0;
            while i < group.len() {
                let f = group[i];
                for e in self.edges_of_face(f).collect::<Vec<_>>() {
                    for g in self.faces_of_edge(e).collect::<Vec<_>>() {
                        if let Some(pos) = region.iter().position(|&r| r == g) {
                            region.remove(pos);
                            group.push(g);
                        }
                    }
                }
                i += 1;
            }
            match self.join_faces(&group) {
                Ok(nf) => result.push(nf),
                Err(_) => result.extend(group),
            }
        }
        for v in verts {
            if self.vert_live(v) && self.vert_edge_count(v) == 2 && self.vert_is_collinear(v) {
                let fs = self.faces_of_vert(v);
                if fs.len() == 1 && result.contains(&fs[0]) {
                    let _ = self.dissolve_vert(v);
                }
            }
        }
        Ok(result.into_iter().filter(|&f| self.face_live(f)).collect())
    }

    /// A valence-two vertex whose edges continue straight through it.
    pub fn vert_is_collinear(&self, v: VertH) -> bool {
        let edges: Vec<EdgeH> = self.edges_of(v).collect();
        if edges.len() != 2 {
            return false;
        }
        let p = self.position(v);
        let a = self.position(self.other_vert(edges[0], v)) - p;
        let b = self.position(self.other_vert(edges[1], v)) - p;
        let (la, lb) = (a.length(), b.length());
        la > 0.0 && lb > 0.0 && a.dot(b) < 0.0 && a.cross(b).length() <= 1e-9 * la * lb
    }

    /// Cut every edge `cuts` times at even spacing. Returns the new vertices.
    pub fn subdivide_edges(&mut self, edges: &[EdgeH], cuts: usize) -> EulerResult<Vec<VertH>> {
        let mut out = Vec::new();
        for &e in edges {
            if !self.edge_live(e) || cuts == 0 {
                continue;
            }
            let [a, b] = self.edge_verts(e);
            let (pa, pb) = (self.position(a), self.position(b));
            let mut cur = e;
            let mut side = a;
            for k in 1..=cuts {
                let (nv, _) = self.split_edge_make_vert(cur, side)?;
                self.set_position(nv, pa.lerp(pb, k as f64 / (cuts + 1) as f64));
                out.push(nv);
                side = nv;
                cur = self.edge_between(nv, b).expect("remaining edge to b");
            }
        }
        Ok(out)
    }

    /// Split face `f` with a new edge between two of its vertices.
    pub fn connect_verts(&mut self, f: FaceH, a: VertH, b: VertH) -> EulerResult<(FaceH, EdgeH)> {
        let la = self.face_loop_at(f, a).ok_or(EulerError::LoopsNotOnFace)?;
        let lb = self.face_loop_at(f, b).ok_or(EulerError::LoopsNotOnFace)?;
        self.split_face_make_edge(f, la, lb)
    }
}
