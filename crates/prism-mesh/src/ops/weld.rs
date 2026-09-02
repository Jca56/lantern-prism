//! Vertex welding: the one topology change the euler set cannot express.
//! `weld_verts` re-points every edge and corner from one vertex to another,
//! merging edges that would become duplicates and dropping corners that
//! would become degenerate. Edge collapse and merge-by-distance build on it.

use prism_geom::KdTree;

use crate::euler::{EulerError, EulerResult};
use crate::handle::{EdgeH, FaceH, LoopH, VertH};
use crate::mesh::Mesh;

impl Mesh {
    /// Merge `remove` into `keep`. Fails if a face contains both vertices
    /// without the edge between them (the face would pinch).
    pub fn weld_verts(&mut self, keep: VertH, remove: VertH) -> EulerResult<()> {
        if !self.vert_live(keep) {
            return Err(EulerError::DeadVert(keep));
        }
        if !self.vert_live(remove) {
            return Err(EulerError::DeadVert(remove));
        }
        if keep == remove {
            return Err(EulerError::SameVert(keep));
        }
        let bridge = self.edge_between(keep, remove);
        for f in self.faces_of_vert(remove) {
            let has_keep = self.verts_of_face(f).any(|v| v == keep);
            if has_keep && !bridge.is_some_and(|e| self.face_has_edge(f, e)) {
                return Err(EulerError::Degenerate);
            }
        }

        let mut shrunk: Vec<FaceH> = Vec::new();
        for e in self.edges_of(remove).collect::<Vec<EdgeH>>() {
            let other = self.other_vert(e, remove);
            if other == keep {
                // Degenerate edge: every face using it drops its corner at `remove`.
                for l in self.loops_of_edge(e).collect::<Vec<LoopH>>() {
                    let lr = if self.loop_vert(l) == remove { l } else { self.loop_next(l) };
                    let f = self.loop_face(lr);
                    let prev = self.loop_prev(lr);
                    let next = self.loop_next(lr);
                    if self.loop_edge(prev) == e {
                        // prev ran keep→remove on e; it now runs keep→y on lr's edge.
                        let ye = self.loop_edge(lr);
                        self.radial_remove(prev);
                        self.loops.edge.set(prev.idx(), ye);
                        self.radial_insert(prev);
                    }
                    self.radial_remove(lr);
                    self.loop_link(prev, next);
                    if self.face_loop(f) == lr {
                        self.faces.loop_.set(f.idx(), next);
                    }
                    let len = self.faces.len[f.idx()] - 1;
                    self.faces.len.set(f.idx(), len);
                    self.loops.slots.free(lr);
                    if len < 3 && !shrunk.contains(&f) {
                        shrunk.push(f);
                    }
                }
                debug_assert!(self.edge_loop(e).is_none());
                self.disk_remove(e, remove);
                self.disk_remove(e, keep);
                self.edges.slots.free(e);
            } else if let Some(ke) = self.edge_between(keep, other) {
                // Would duplicate `ke`: move the loops over and drop `e`.
                for l in self.loops_of_edge(e).collect::<Vec<LoopH>>() {
                    self.radial_remove(l);
                    self.loops.edge.set(l.idx(), ke);
                    self.radial_insert(l);
                }
                self.disk_remove(e, remove);
                self.disk_remove(e, other);
                self.edges.slots.free(e);
            } else {
                self.disk_remove(e, remove);
                let mut ends = self.edges.v[e.idx()];
                if ends[0] == remove {
                    ends[0] = keep;
                } else {
                    ends[1] = keep;
                }
                self.edges.v.set(e.idx(), ends);
                self.disk_insert(e, keep);
            }
        }
        // Corners that still name `remove` now belong to `keep`. Every such
        // corner sits on an edge that is now in `keep`'s disk.
        for e in self.edges_of(keep).collect::<Vec<EdgeH>>() {
            for l in self.loops_of_edge(e).collect::<Vec<LoopH>>() {
                for c in [l, self.loop_next(l)] {
                    if self.loop_vert(c) == remove {
                        self.loops.vert.set(c.idx(), keep);
                    }
                }
            }
        }
        for f in shrunk {
            if self.face_live(f) {
                self.kill_face_any_len(f);
            }
        }
        debug_assert!(self.vert_edge(remove).is_none());
        self.verts.slots.free(remove);
        if self.paranoid
            && let Err(errs) = self.validate()
        {
            panic!("mesh invalid after weld_verts: {errs:#?}");
        }
        Ok(())
    }

    /// Kill a face whose length may have dropped below three.
    fn kill_face_any_len(&mut self, f: FaceH) {
        let first = self.face_loop(f);
        let mut loops = vec![first];
        let mut cur = self.loop_next(first);
        while cur != first && loops.len() < 8 {
            loops.push(cur);
            cur = self.loop_next(cur);
        }
        for l in loops {
            self.radial_remove(l);
            self.loops.slots.free(l);
        }
        self.faces.slots.free(f);
    }

    /// Collapse an edge to its midpoint. Returns the surviving vertex.
    pub fn collapse_edge(&mut self, e: EdgeH) -> EulerResult<VertH> {
        if !self.edge_live(e) {
            return Err(EulerError::DeadEdge(e));
        }
        let [a, b] = self.edge_verts(e);
        self.verts.attrs.interpolate(a.idx(), &[(a.idx(), 0.5), (b.idx(), 0.5)]);
        self.weld_verts(a, b)?;
        Ok(a)
    }

    /// Weld every group of vertices closer than `threshold`. Returns how many
    /// vertices were removed.
    pub fn merge_by_distance(&mut self, threshold: f64) -> usize {
        let verts: Vec<VertH> = self.verts().collect();
        let points: Vec<_> = verts.iter().map(|&v| self.position(v)).collect();
        let tree = KdTree::build(&points);
        // Union-find over close pairs.
        let mut parent: Vec<usize> = (0..verts.len()).collect();
        fn find(p: &mut [usize], i: usize) -> usize {
            let mut r = i;
            while p[r] != r {
                r = p[r];
            }
            let mut c = i;
            while p[c] != r {
                let n = p[c];
                p[c] = r;
                c = n;
            }
            r
        }
        for (i, j) in tree.pairs_within(threshold) {
            let (ri, rj) = (find(&mut parent, i as usize), find(&mut parent, j as usize));
            if ri != rj {
                parent[ri.max(rj)] = ri.min(rj);
            }
        }
        let mut removed = 0;
        for i in 0..verts.len() {
            let r = find(&mut parent, i);
            if r != i && self.vert_live(verts[r]) && self.vert_live(verts[i]) && self.weld_verts(verts[r], verts[i]).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}
