//! Loop cut (Phase 7): walk the ring of quads through an edge, cut every
//! edge of the ring and connect the cuts across each quad, making a new edge
//! loop. The cuts can then slide along their edges as one.

use prism_math::Vec3;

use crate::euler::{EulerError, EulerResult};
use crate::handle::{EdgeH, FaceH, LoopH, VertH};
use crate::mesh::Mesh;

/// One cut vertex and the ends of the edge it slides along, oriented so
/// every cut in the ring slides the same way.
#[derive(Clone, Copy, Debug)]
pub struct CutVert {
    pub vert: VertH,
    pub a: Vec3,
    pub b: Vec3,
}

#[derive(Clone, Debug, Default)]
pub struct LoopCutResult {
    /// The cut vertices, ring order then cut order along each edge.
    pub verts: Vec<CutVert>,
    /// The new edges forming the loop(s).
    pub edges: Vec<EdgeH>,
    /// The ring closed on itself (a belt round the mesh).
    pub closed: bool,
}

/// The edge ring through `seed`: consecutive edges of a strip of quads, with
/// each edge's vertices ordered so `a`s are on one side of the strip and
/// `b`s on the other. `closed` when the strip meets itself.
pub struct Ring {
    pub edges: Vec<(EdgeH, VertH, VertH)>,
    /// The quad after each edge (between it and the next); one fewer than
    /// edges unless closed.
    pub quads: Vec<FaceH>,
    pub closed: bool,
}

impl Mesh {
    /// The loop of `e` in `f`.
    fn loop_of_edge_in_face(&self, e: EdgeH, f: FaceH) -> Option<LoopH> {
        self.loops_of_edge(e).find(|&l| self.loop_face(l) == f)
    }

    /// Step from `e` across quad `f` to the opposite edge, orienting it so
    /// its `a` is joined to `a` and its `b` to `b`.
    fn across_quad(&self, e: EdgeH, a: VertH, f: FaceH) -> Option<(EdgeH, VertH, VertH)> {
        if self.face_len(f) != 4 {
            return None;
        }
        let l0 = self.loop_of_edge_in_face(e, f)?;
        let l2 = self.loop_next(self.loop_next(l0));
        let (v2, v3) = (self.loop_vert(l2), self.loop_vert(self.loop_next(l2)));
        // l0 runs v0→v1 (or b→a); v3 neighbours v0 along the side, v2 neighbours v1.
        let opposite = self.loop_edge(l2);
        Some(if self.loop_vert(l0) == a { (opposite, v3, v2) } else { (opposite, v2, v3) })
    }

    /// Walk from `seed` in one direction, through `first` face.
    fn walk_ring(&self, seed: (EdgeH, VertH, VertH), first: FaceH, out: &mut Vec<(EdgeH, VertH, VertH)>, quads: &mut Vec<FaceH>) -> bool {
        let (mut e, mut a, _) = seed;
        let mut f = first;
        loop {
            let Some(next) = self.across_quad(e, a, f) else {
                return false;
            };
            quads.push(f);
            if next.0 == seed.0 {
                return true; // closed
            }
            if out.iter().any(|(x, _, _)| *x == next.0) {
                return false; // a ring that crosses itself: stop rather than loop
            }
            out.push(next);
            let Some(g) = self.faces_of_edge(next.0).find(|&g| g != f) else {
                return false;
            };
            (e, a) = (next.0, next.1);
            f = g;
        }
    }

    /// The ring of quads through `seed`, walked both ways.
    pub fn edge_ring(&self, seed: EdgeH) -> Ring {
        let [a, b] = self.edge_verts(seed);
        let faces: Vec<FaceH> = self.faces_of_edge(seed).collect();
        let mut forward = Vec::new();
        let mut quads_f = Vec::new();
        let closed = faces.first().is_some_and(|&f| self.walk_ring((seed, a, b), f, &mut forward, &mut quads_f));
        if closed {
            let mut edges = vec![(seed, a, b)];
            edges.extend(forward);
            return Ring { edges, quads: quads_f, closed: true };
        }
        let mut backward = Vec::new();
        let mut quads_b = Vec::new();
        if let Some(&f) = faces.get(1) {
            self.walk_ring((seed, a, b), f, &mut backward, &mut quads_b);
        }
        // Stitch: backward (reversed) … seed … forward, with quads in between.
        let mut edges: Vec<(EdgeH, VertH, VertH)> = backward.iter().rev().copied().collect();
        edges.push((seed, a, b));
        edges.extend(forward);
        let mut quads: Vec<FaceH> = quads_b.iter().rev().copied().collect();
        quads.extend(quads_f);
        Ring { edges, quads, closed: false }
    }

    /// Cut the ring through `seed` `cuts` times per edge; with one cut, place
    /// it at `factor` along each edge (0 = the `a` side).
    pub fn loop_cut(&mut self, seed: EdgeH, cuts: usize, factor: f64) -> EulerResult<LoopCutResult> {
        let ring = self.edge_ring(seed);
        let cuts = cuts.max(1);
        if ring.quads.is_empty() {
            return Err(EulerError::LoopsNotOnFace);
        }
        // Cut every ring edge; order the new vertices from `a` to `b`.
        let mut per_edge: Vec<Vec<VertH>> = Vec::with_capacity(ring.edges.len());
        let mut verts = Vec::new();
        for &(e, a, b) in &ring.edges {
            let (pa, pb) = (self.position(a), self.position(b));
            let from_first = self.edge_verts(e)[0] == a;
            let mut new = self.subdivide_edges(&[e], cuts)?;
            if !from_first {
                new.reverse();
            }
            for (k, &v) in new.iter().enumerate() {
                let t = if cuts == 1 { factor } else { (k + 1) as f64 / (cuts + 1) as f64 };
                self.set_position(v, pa.lerp(pb, t));
                verts.push(CutVert { vert: v, a: pa, b: pb });
            }
            per_edge.push(new);
        }
        // Connect matching cuts across each quad.
        let mut edges = Vec::new();
        let n = ring.edges.len();
        for (i, &q) in ring.quads.iter().enumerate() {
            let j = (i + 1) % n;
            let pairs: Vec<(VertH, VertH)> = per_edge[i].iter().copied().zip(per_edge[j].iter().copied()).collect();
            for (va, vb) in pairs {
                let (_, e) = self.connect_verts(q, va, vb)?;
                edges.push(e);
            }
        }
        Ok(LoopCutResult { verts, edges, closed: ring.closed })
    }

    /// Slide a single-cut loop: every cut at `factor` along its edge.
    pub fn place_loop_cut(&mut self, verts: &[CutVert], factor: f64) {
        for c in verts {
            if self.vert_live(c.vert) {
                self.set_position(c.vert, c.a.lerp(c.b, factor.clamp(0.0, 1.0)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{cube, grid};

    #[test]
    fn ring_round_a_cube_is_a_closed_belt() {
        let mut m = cube(2.0);
        let top = m.faces().find(|&f| m.face_normal(f).approx_eq(Vec3::Y, 1e-9)).unwrap();
        let e = m.edges_of_face(top).next().unwrap();
        let ring = m.edge_ring(e);
        assert!(ring.closed);
        assert_eq!((ring.edges.len(), ring.quads.len()), (4, 4));
        let r = m.loop_cut(e, 1, 0.5).unwrap();
        assert_eq!((r.verts.len(), r.edges.len()), (4, 4));
        assert!(r.closed);
        assert_eq!((m.vert_count(), m.edge_count(), m.face_count()), (12, 20, 10));
        m.validate().unwrap();
        // Slide: every cut moves the same way along its edge.
        m.place_loop_cut(&r.verts, 0.25);
        for c in &r.verts {
            let p = m.position(c.vert);
            assert!((p - c.a.lerp(c.b, 0.25)).length() < 1e-9);
        }
        // On a cube the ring edges are parallel, so the new loop lies in one
        // plane across them: every cut sits at the same coordinate along the
        // seed edge's direction, a quarter of the way in.
        let d = (r.verts[0].b - r.verts[0].a).normalize();
        let along: Vec<f64> = r.verts.iter().map(|c| m.position(c.vert).dot(d)).collect();
        assert!(along.iter().all(|t| (t - along[0]).abs() < 1e-9), "the loop stays in one plane: {along:?}");
        assert!((along[0].abs() - 0.5).abs() < 1e-9, "{along:?}");
    }

    #[test]
    fn open_strip_stops_at_the_boundary_and_multiple_cuts_space_evenly() {
        // A 1×3 strip of quads: the ring across it has 4 edges and 3 quads.
        let mut m = grid(3.0, 1.0, 3, 1);
        assert_eq!(m.face_count(), 3);
        let mid = m.edges().find(|&e| m.edge_face_count(e) == 2).unwrap();
        let ring = m.edge_ring(mid);
        assert!(!ring.closed);
        assert_eq!((ring.edges.len(), ring.quads.len()), (4, 3));
        let r = m.loop_cut(mid, 2, 0.5).unwrap();
        assert_eq!((r.verts.len(), r.edges.len()), (8, 6));
        assert_eq!(m.face_count(), 9);
        m.validate().unwrap();
        let ts: Vec<f64> = r.verts.iter().take(2).map(|c| (m.position(c.vert) - c.a).length() / (c.b - c.a).length()).collect();
        assert!((ts[0] - 1.0 / 3.0).abs() < 1e-9 && (ts[1] - 2.0 / 3.0).abs() < 1e-9, "{ts:?}");
    }
}
