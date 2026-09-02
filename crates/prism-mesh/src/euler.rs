//! The euler operators: the only code allowed to touch topology columns.
//! Each preserves every invariant `validate()` checks. Names follow the
//! classic Split / Join × Make / Kill × Vert / Edge / Face scheme.

use core::fmt;

use prism_math::Vec3;

use crate::handle::{EdgeH, FaceH, LoopH, VertH};
use crate::mesh::Mesh;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EulerError {
    DeadVert(VertH),
    DeadEdge(EdgeH),
    DeadFace(FaceH),
    DeadLoop(LoopH),
    /// `kill_vert` on a vertex that still has edges.
    VertHasEdges(VertH),
    /// An edge from a vertex to itself.
    SameVert(VertH),
    /// An edge already joins these vertices.
    EdgeExists(EdgeH),
    TooFewVerts(usize),
    RepeatedVert(VertH),
    /// Consecutive face vertices with no edge between them.
    MissingEdge(VertH, VertH),
    NotEndpoint { edge: EdgeH, vert: VertH },
    /// `join_edge_kill_vert` needs a vertex with exactly two edges.
    VertNotValence2(VertH),
    /// The faces around a vertex or edge do not pair up as required.
    FacesMismatch,
    /// The loops given to `split_face_make_edge` are not on the face.
    LoopsNotOnFace,
    LoopsAdjacent,
    SameLoop,
    /// `join_face_kill_edge`: faces share more than the one edge.
    FacesShareMore,
    /// Both faces run the shared edge the same way.
    WindingMismatch,
    /// The result would contain a duplicate or degenerate element.
    Degenerate,
}

impl fmt::Display for EulerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EulerError {}

pub type EulerResult<T> = Result<T, EulerError>;

impl Mesh {
    fn check_v(&self, v: VertH) -> EulerResult<()> {
        if self.vert_live(v) { Ok(()) } else { Err(EulerError::DeadVert(v)) }
    }
    fn check_e(&self, e: EdgeH) -> EulerResult<()> {
        if self.edge_live(e) { Ok(()) } else { Err(EulerError::DeadEdge(e)) }
    }
    fn check_f(&self, f: FaceH) -> EulerResult<()> {
        if self.face_live(f) { Ok(()) } else { Err(EulerError::DeadFace(f)) }
    }
    fn check_l(&self, l: LoopH) -> EulerResult<()> {
        if self.loop_live(l) { Ok(()) } else { Err(EulerError::DeadLoop(l)) }
    }

    fn after_op(&self, name: &str) {
        if self.paranoid
            && let Err(errs) = self.validate()
        {
            panic!("mesh invalid after {name}: {errs:#?}");
        }
    }

    /// An isolated vertex.
    pub fn make_vert(&mut self, position: Vec3) -> VertH {
        let v = self.verts.alloc(position);
        self.after_op("make_vert");
        v
    }

    /// Remove a vertex that has no edges.
    pub fn kill_vert(&mut self, v: VertH) -> EulerResult<()> {
        self.check_v(v)?;
        if self.vert_edge(v).is_some() {
            return Err(EulerError::VertHasEdges(v));
        }
        self.verts.slots.free(v);
        self.after_op("kill_vert");
        Ok(())
    }

    /// A wire edge between two vertices.
    pub fn make_edge(&mut self, a: VertH, b: VertH) -> EulerResult<EdgeH> {
        self.check_v(a)?;
        self.check_v(b)?;
        if a == b {
            return Err(EulerError::SameVert(a));
        }
        if let Some(e) = self.edge_between(a, b) {
            return Err(EulerError::EdgeExists(e));
        }
        let e = self.edges.alloc([a, b]);
        self.disk_insert(e, a);
        self.disk_insert(e, b);
        self.after_op("make_edge");
        Ok(e)
    }

    /// Remove an edge and every face using it.
    pub fn kill_edge(&mut self, e: EdgeH) -> EulerResult<()> {
        self.check_e(e)?;
        let mut faces: Vec<FaceH> = Vec::new();
        for l in self.loops_of_edge(e) {
            let f = self.loop_face(l);
            if !faces.contains(&f) {
                faces.push(f);
            }
        }
        for f in faces {
            self.kill_face(f)?;
        }
        let [a, b] = self.edge_verts(e);
        self.disk_remove(e, a);
        self.disk_remove(e, b);
        self.edges.slots.free(e);
        self.after_op("kill_edge");
        Ok(())
    }

    /// A face over `verts` in winding order. Every consecutive pair (and the
    /// last→first pair) must already be joined by an edge.
    pub fn make_face(&mut self, verts: &[VertH]) -> EulerResult<FaceH> {
        let n = verts.len();
        if n < 3 {
            return Err(EulerError::TooFewVerts(n));
        }
        for (i, &v) in verts.iter().enumerate() {
            self.check_v(v)?;
            if verts[..i].contains(&v) {
                return Err(EulerError::RepeatedVert(v));
            }
        }
        let mut edges = Vec::with_capacity(n);
        for i in 0..n {
            let (a, b) = (verts[i], verts[(i + 1) % n]);
            edges.push(self.edge_between(a, b).ok_or(EulerError::MissingEdge(a, b))?);
        }
        let f = self.faces.alloc();
        let loops: Vec<LoopH> = (0..n).map(|i| self.loops.alloc(verts[i], edges[i], f)).collect();
        for i in 0..n {
            self.loop_link(loops[i], loops[(i + 1) % n]);
        }
        for &l in &loops {
            self.radial_insert(l);
        }
        self.faces.loop_.set(f.idx(), loops[0]);
        self.faces.len.set(f.idx(), n as u32);
        self.after_op("make_face");
        Ok(f)
    }

    /// Remove a face; its edges and vertices stay.
    pub fn kill_face(&mut self, f: FaceH) -> EulerResult<()> {
        self.check_f(f)?;
        let loops: Vec<LoopH> = self.loops_of_face(f).collect();
        for l in loops {
            self.radial_remove(l);
            self.loops.slots.free(l);
        }
        self.faces.slots.free(f);
        self.after_op("kill_face");
        Ok(())
    }

    /// SEMV: insert a vertex into `e` on `v`'s side. `e` keeps the far
    /// endpoint; the new edge joins `v` to the new vertex. Every face using
    /// `e` gains a corner. Returns `(new_vert, new_edge)`.
    pub fn split_edge_make_vert(&mut self, e: EdgeH, v: VertH) -> EulerResult<(VertH, EdgeH)> {
        self.check_e(e)?;
        self.check_v(v)?;
        if !self.edge_has_vert(e, v) {
            return Err(EulerError::NotEndpoint { edge: e, vert: v });
        }
        let w = self.other_vert(e, v);
        let mid = (self.position(v) + self.position(w)) * 0.5;
        let nv = self.verts.alloc(mid);
        self.verts.attrs.interpolate(nv.idx(), &[(v.idx(), 0.5), (w.idx(), 0.5)]);

        let loops: Vec<LoopH> = self.loops_of_edge(e).collect();

        // Move e's `v` end to the new vertex.
        self.disk_remove(e, v);
        let mut ends = self.edges.v[e.idx()];
        if ends[0] == v {
            ends[0] = nv;
        } else {
            ends[1] = nv;
        }
        self.edges.v.set(e.idx(), ends);
        self.disk_insert(e, nv);

        let ne = self.edges.alloc([v, nv]);
        self.edges.attrs.copy(ne.idx(), e.idx());
        self.disk_insert(ne, v);
        self.disk_insert(ne, nv);

        for l in loops {
            let f = self.loop_face(l);
            let lnext = self.loop_next(l);
            let nl = if self.loop_vert(l) == v {
                // l ran v→w on e; now l runs v→nv on ne and nl runs nv→w on e.
                self.radial_remove(l);
                self.loops.edge.set(l.idx(), ne);
                self.radial_insert(l);
                self.loops.alloc(nv, e, f)
            } else {
                // l ran w→v on e; now l runs w→nv on e and nl runs nv→v on ne.
                self.loops.alloc(nv, ne, f)
            };
            self.loop_link(l, nl);
            self.loop_link(nl, lnext);
            self.radial_insert(nl);
            self.loops.attrs.interpolate(nl.idx(), &[(l.idx(), 0.5), (lnext.idx(), 0.5)]);
            let len = self.faces.len[f.idx()];
            self.faces.len.set(f.idx(), len + 1);
        }
        self.after_op("split_edge_make_vert");
        Ok((nv, ne))
    }

    /// JEKV: remove a vertex with exactly two edges, merging them into `e`.
    /// Inverse of SEMV. Every face around `v` loses a corner.
    pub fn join_edge_kill_vert(&mut self, e: EdgeH, v: VertH) -> EulerResult<EdgeH> {
        self.check_e(e)?;
        self.check_v(v)?;
        if !self.edge_has_vert(e, v) {
            return Err(EulerError::NotEndpoint { edge: e, vert: v });
        }
        let edges: Vec<EdgeH> = self.edges_of(v).collect();
        if edges.len() != 2 {
            return Err(EulerError::VertNotValence2(v));
        }
        let oe = if edges[0] == e { edges[1] } else { edges[0] };
        let x = self.other_vert(e, v);
        let w = self.other_vert(oe, v);
        if x == w || self.edge_between(x, w).is_some() {
            return Err(EulerError::Degenerate);
        }
        // The faces around v must use both edges, and stay at least triangles.
        let faces_e: Vec<FaceH> = self.faces_of_edge(e).collect();
        let faces_oe: Vec<FaceH> = self.faces_of_edge(oe).collect();
        if faces_e.len() != faces_oe.len() || !faces_e.iter().all(|f| faces_oe.contains(f)) {
            return Err(EulerError::FacesMismatch);
        }
        if faces_e.iter().any(|&f| self.face_len(f) <= 3) {
            return Err(EulerError::Degenerate);
        }

        for &f in &faces_e {
            let lv = self.face_loop_at(f, v).ok_or(EulerError::FacesMismatch)?;
            let prev = self.loop_prev(lv);
            let next = self.loop_next(lv);
            if self.loop_edge(prev) != e {
                self.radial_remove(prev);
                self.loops.edge.set(prev.idx(), e);
                self.radial_insert(prev);
            }
            self.radial_remove(lv);
            self.loop_link(prev, next);
            if self.face_loop(f) == lv {
                self.faces.loop_.set(f.idx(), next);
            }
            let len = self.faces.len[f.idx()];
            self.faces.len.set(f.idx(), len - 1);
            self.loops.slots.free(lv);
        }

        // Re-point e from v to w, drop oe and v.
        self.disk_remove(e, v);
        self.disk_remove(oe, v);
        self.disk_remove(oe, w);
        let mut ends = self.edges.v[e.idx()];
        if ends[0] == v {
            ends[0] = w;
        } else {
            ends[1] = w;
        }
        self.edges.v.set(e.idx(), ends);
        self.disk_insert(e, w);
        debug_assert!(self.edge_loop(oe).is_none());
        self.edges.slots.free(oe);
        self.verts.slots.free(v);
        self.after_op("join_edge_kill_vert");
        Ok(e)
    }

    /// SFME: cut face `f` between two of its corners. `f` keeps the loops
    /// from `l1` up to (not including) `l2`; the new face gets the rest. An
    /// existing edge between the two vertices is reused. Returns
    /// `(new_face, edge)`.
    pub fn split_face_make_edge(&mut self, f: FaceH, l1: LoopH, l2: LoopH) -> EulerResult<(FaceH, EdgeH)> {
        self.check_f(f)?;
        self.check_l(l1)?;
        self.check_l(l2)?;
        if self.loop_face(l1) != f || self.loop_face(l2) != f {
            return Err(EulerError::LoopsNotOnFace);
        }
        if l1 == l2 {
            return Err(EulerError::SameLoop);
        }
        if self.loop_next(l1) == l2 || self.loop_next(l2) == l1 {
            return Err(EulerError::LoopsAdjacent);
        }
        let v1 = self.loop_vert(l1);
        let v2 = self.loop_vert(l2);
        let ne = match self.edge_between(v1, v2) {
            Some(e) => e,
            None => {
                let e = self.edges.alloc([v1, v2]);
                self.disk_insert(e, v1);
                self.disk_insert(e, v2);
                e
            }
        };
        let nf = self.faces.alloc();
        self.faces.attrs.copy(nf.idx(), f.idx());

        let l1_prev = self.loop_prev(l1);
        let l2_prev = self.loop_prev(l2);
        // Loops from l2 through l1_prev move to nf.
        let mut count_b = 0u32;
        let mut cur = l2;
        loop {
            self.loops.face.set(cur.idx(), nf);
            count_b += 1;
            if cur == l1_prev {
                break;
            }
            cur = self.loop_next(cur);
        }
        let old_len = self.faces.len[f.idx()];

        let nla = self.loops.alloc(v2, ne, f);
        let nlb = self.loops.alloc(v1, ne, nf);
        self.loops.attrs.copy(nla.idx(), l2.idx());
        self.loops.attrs.copy(nlb.idx(), l1.idx());
        self.loop_link(l2_prev, nla);
        self.loop_link(nla, l1);
        self.loop_link(l1_prev, nlb);
        self.loop_link(nlb, l2);
        self.radial_insert(nla);
        self.radial_insert(nlb);

        self.faces.loop_.set(f.idx(), l1);
        self.faces.len.set(f.idx(), old_len - count_b + 1);
        self.faces.loop_.set(nf.idx(), l2);
        self.faces.len.set(nf.idx(), count_b + 1);
        self.after_op("split_face_make_edge");
        Ok((nf, ne))
    }

    /// JFKE: merge `f2` into `f1` across their shared edge `e`, which is
    /// removed. Inverse of SFME. The faces must share exactly that edge and
    /// its two vertices, with opposite winding along it.
    pub fn join_face_kill_edge(&mut self, f1: FaceH, f2: FaceH, e: EdgeH) -> EulerResult<FaceH> {
        self.check_f(f1)?;
        self.check_f(f2)?;
        self.check_e(e)?;
        if f1 == f2 {
            return Err(EulerError::FacesMismatch);
        }
        let radial: Vec<LoopH> = self.loops_of_edge(e).collect();
        if radial.len() != 2 {
            return Err(EulerError::FacesMismatch);
        }
        let l1 = *radial.iter().find(|&&l| self.loop_face(l) == f1).ok_or(EulerError::FacesMismatch)?;
        let l2 = *radial.iter().find(|&&l| self.loop_face(l) == f2).ok_or(EulerError::FacesMismatch)?;
        let [a, b] = [self.loop_vert(l1), self.loop_vert(self.loop_next(l1))];
        if self.loop_vert(l2) != b || self.loop_vert(self.loop_next(l2)) != a {
            return Err(EulerError::WindingMismatch);
        }
        // Only e and its two vertices may be shared.
        for oe in self.edges_of_face(f1).collect::<Vec<_>>() {
            if oe != e && self.face_has_edge(f2, oe) {
                return Err(EulerError::FacesShareMore);
            }
        }
        let verts2: Vec<VertH> = self.verts_of_face(f2).collect();
        for v in self.verts_of_face(f1).collect::<Vec<_>>() {
            if v != a && v != b && verts2.contains(&v) {
                return Err(EulerError::FacesShareMore);
            }
        }

        let l1_prev = self.loop_prev(l1);
        let l1_next = self.loop_next(l1);
        let l2_prev = self.loop_prev(l2);
        let l2_next = self.loop_next(l2);
        let mut cur = l2_next;
        while cur != l2 {
            self.loops.face.set(cur.idx(), f1);
            cur = self.loop_next(cur);
        }
        self.loop_link(l1_prev, l2_next);
        self.loop_link(l2_prev, l1_next);
        self.radial_remove(l1);
        self.radial_remove(l2);
        let n1 = self.faces.len[f1.idx()];
        let n2 = self.faces.len[f2.idx()];
        self.loops.slots.free(l1);
        self.loops.slots.free(l2);
        self.faces.loop_.set(f1.idx(), l1_next);
        self.faces.len.set(f1.idx(), n1 + n2 - 2);
        self.faces.slots.free(f2);

        debug_assert!(self.edge_loop(e).is_none());
        self.disk_remove(e, a);
        self.disk_remove(e, b);
        self.edges.slots.free(e);
        self.after_op("join_face_kill_edge");
        Ok(f1)
    }

    /// Flip a face's winding (and therefore its normal).
    pub fn reverse_face(&mut self, f: FaceH) -> EulerResult<()> {
        self.check_f(f)?;
        let loops: Vec<LoopH> = self.loops_of_face(f).collect();
        let n = loops.len();
        let old_edges: Vec<EdgeH> = loops.iter().map(|&l| self.loop_edge(l)).collect();
        for &l in &loops {
            self.radial_remove(l);
        }
        for i in 0..n {
            let l = loops[i];
            // The corner now walks to the previous vertex, along that edge.
            self.loops.edge.set(l.idx(), old_edges[(i + n - 1) % n]);
            self.loop_link(l, loops[(i + n - 1) % n]);
        }
        for &l in &loops {
            self.radial_insert(l);
        }
        self.after_op("reverse_face");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> (Mesh, [VertH; 4], FaceH) {
        let mut m = Mesh::new();
        m.paranoid = true;
        let v = [
            m.make_vert(Vec3::new(0.0, 0.0, 0.0)),
            m.make_vert(Vec3::new(1.0, 0.0, 0.0)),
            m.make_vert(Vec3::new(1.0, 0.0, -1.0)),
            m.make_vert(Vec3::new(0.0, 0.0, -1.0)),
        ];
        for i in 0..4 {
            m.make_edge(v[i], v[(i + 1) % 4]).unwrap();
        }
        let f = m.make_face(&v).unwrap();
        (m, v, f)
    }

    #[test]
    fn build_and_tear_down() {
        let (mut m, v, f) = quad();
        assert_eq!((m.vert_count(), m.edge_count(), m.face_count(), m.loop_count()), (4, 4, 1, 4));
        assert_eq!(m.face_len(f), 4);
        assert!(m.face_normal(f).approx_eq(Vec3::Y, 1e-12));
        assert_eq!(m.verts_of_face(f).collect::<Vec<_>>(), v.to_vec());
        assert_eq!(m.edge_face_count(m.edge_between(v[0], v[1]).unwrap()), 1);
        assert_eq!(m.kill_vert(v[0]), Err(EulerError::VertHasEdges(v[0])));
        assert_eq!(m.make_edge(v[0], v[0]), Err(EulerError::SameVert(v[0])));
        assert!(matches!(m.make_edge(v[0], v[1]), Err(EulerError::EdgeExists(_))));
        assert_eq!(m.make_face(&[v[0], v[2], v[1]]), Err(EulerError::MissingEdge(v[0], v[2])));
        m.kill_face(f).unwrap();
        assert_eq!(m.loop_count(), 0);
        assert!(m.is_wire_edge(m.edge_between(v[0], v[1]).unwrap()));
        for i in 0..4 {
            m.kill_edge(m.edge_between(v[i], v[(i + 1) % 4]).unwrap()).unwrap();
        }
        for &x in &v {
            m.kill_vert(x).unwrap();
        }
        assert!(m.is_empty());
        assert!(!m.vert_live(v[0]));
    }

    #[test]
    fn kill_edge_removes_faces() {
        let (mut m, v, _) = quad();
        m.kill_edge(m.edge_between(v[0], v[1]).unwrap()).unwrap();
        assert_eq!((m.edge_count(), m.face_count(), m.loop_count()), (3, 0, 0));
    }

    #[test]
    fn semv_then_jekv_roundtrip() {
        let (mut m, v, f) = quad();
        let e = m.edge_between(v[0], v[1]).unwrap();
        let (nv, ne) = m.split_edge_make_vert(e, v[0]).unwrap();
        assert_eq!((m.vert_count(), m.edge_count(), m.loop_count()), (5, 5, 5));
        assert_eq!(m.face_len(f), 5);
        assert!(m.position(nv).approx_eq(Vec3::new(0.5, 0.0, 0.0), 1e-12));
        assert_eq!(m.edge_verts(ne), [v[0], nv]);
        assert!(m.edge_has_vert(e, nv) && m.edge_has_vert(e, v[1]));
        assert_eq!(m.verts_of_face(f).collect::<Vec<_>>(), vec![v[0], nv, v[1], v[2], v[3]]);
        // Split from the other side too, then undo both.
        let e2 = m.edge_between(v[2], v[3]).unwrap();
        let (nv2, _) = m.split_edge_make_vert(e2, v[3]).unwrap();
        assert_eq!(m.face_len(f), 6);
        m.join_edge_kill_vert(e2, nv2).unwrap();
        let merged = m.join_edge_kill_vert(ne, nv).unwrap();
        assert_eq!((m.vert_count(), m.edge_count(), m.loop_count()), (4, 4, 4));
        assert_eq!(m.face_len(f), 4);
        assert!(m.edge_has_vert(merged, v[0]) && m.edge_has_vert(merged, v[1]));
        // A quad corner can still go: the face becomes a triangle…
        let e = m.join_edge_kill_vert(merged, v[0]).unwrap();
        assert_eq!((m.vert_count(), m.edge_count(), m.face_len(f)), (3, 3, 3));
        assert!(m.edge_has_vert(e, v[3]) && m.edge_has_vert(e, v[1]));
        // …but a triangle corner cannot.
        assert_eq!(m.join_edge_kill_vert(e, v[1]), Err(EulerError::Degenerate));
    }

    #[test]
    fn sfme_then_jfke_roundtrip() {
        let (mut m, v, f) = quad();
        let l0 = m.face_loop_at(f, v[0]).unwrap();
        let l2 = m.face_loop_at(f, v[2]).unwrap();
        let l1 = m.face_loop_at(f, v[1]).unwrap();
        assert_eq!(m.split_face_make_edge(f, l0, l1), Err(EulerError::LoopsAdjacent));
        let (nf, ne) = m.split_face_make_edge(f, l0, l2).unwrap();
        assert_eq!((m.face_count(), m.edge_count(), m.loop_count()), (2, 5, 6));
        assert_eq!(m.face_len(f), 3);
        assert_eq!(m.face_len(nf), 3);
        assert_eq!(m.edge_face_count(ne), 2);
        assert_eq!(m.verts_of_face(f).collect::<Vec<_>>(), vec![v[0], v[1], v[2]]);
        assert_eq!(m.verts_of_face(nf).collect::<Vec<_>>(), vec![v[2], v[3], v[0]]);
        assert!(m.face_normal(f).approx_eq(Vec3::Y, 1e-12) && m.face_normal(nf).approx_eq(Vec3::Y, 1e-12));
        let joined = m.join_face_kill_edge(f, nf, ne).unwrap();
        assert_eq!(joined, f);
        assert_eq!((m.face_count(), m.edge_count(), m.loop_count()), (1, 4, 4));
        assert_eq!(m.face_len(f), 4);
        let mut ring: Vec<VertH> = m.verts_of_face(f).collect();
        let start = ring.iter().position(|&x| x == v[0]).unwrap();
        ring.rotate_left(start);
        assert_eq!(ring, v.to_vec());
    }

    #[test]
    fn jfke_rejects_same_winding() {
        let (mut m, v, f) = quad();
        let l0 = m.face_loop_at(f, v[0]).unwrap();
        let l2 = m.face_loop_at(f, v[2]).unwrap();
        let (nf, ne) = m.split_face_make_edge(f, l0, l2).unwrap();
        m.reverse_face(nf).unwrap();
        assert!(m.face_normal(nf).approx_eq(Vec3::NEG_Y, 1e-12));
        assert_eq!(m.join_face_kill_edge(f, nf, ne), Err(EulerError::WindingMismatch));
        m.reverse_face(nf).unwrap();
        m.join_face_kill_edge(f, nf, ne).unwrap();
    }

    #[test]
    fn reverse_twice_is_identity() {
        let (mut m, v, f) = quad();
        let before: Vec<(VertH, EdgeH)> = m.loops_of_face(f).map(|l| (m.loop_vert(l), m.loop_edge(l))).collect();
        m.reverse_face(f).unwrap();
        assert!(m.face_normal(f).approx_eq(Vec3::NEG_Y, 1e-12));
        assert_eq!(m.verts_of_face(f).collect::<Vec<_>>(), vec![v[0], v[3], v[2], v[1]]);
        m.reverse_face(f).unwrap();
        let after: Vec<(VertH, EdgeH)> = m.loops_of_face(f).map(|l| (m.loop_vert(l), m.loop_edge(l))).collect();
        assert_eq!(before, after);
    }
}
