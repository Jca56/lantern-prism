//! `Mesh`: the four tables plus every read-only query. Mutation goes through
//! [`crate::euler`] and the compound ops built on it.

use prism_math::Vec3;

use crate::attr::AttributeSet;
use crate::handle::{EdgeH, FaceH, LoopH, VertH};
use crate::tables::{EdgeTable, FaceTable, LoopTable, V_POSITION, VertTable};

#[derive(Clone, Debug)]
pub struct Mesh {
    pub(crate) verts: VertTable,
    pub(crate) edges: EdgeTable,
    pub(crate) loops: LoopTable,
    pub(crate) faces: FaceTable,
    /// Run `validate()` after every euler operator (tests and fuzzing).
    pub paranoid: bool,
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            verts: VertTable::new(),
            edges: EdgeTable::new(),
            loops: LoopTable::new(),
            faces: FaceTable::new(),
            paranoid: false,
        }
    }

    // ---- counts and iteration --------------------------------------------

    pub fn vert_count(&self) -> usize {
        self.verts.slots.len()
    }
    pub fn edge_count(&self) -> usize {
        self.edges.slots.len()
    }
    pub fn face_count(&self) -> usize {
        self.faces.slots.len()
    }
    pub fn loop_count(&self) -> usize {
        self.loops.slots.len()
    }
    pub fn is_empty(&self) -> bool {
        self.vert_count() == 0
    }

    pub fn verts(&self) -> impl Iterator<Item = VertH> + '_ {
        self.verts.slots.iter()
    }
    pub fn edges(&self) -> impl Iterator<Item = EdgeH> + '_ {
        self.edges.slots.iter()
    }
    pub fn faces(&self) -> impl Iterator<Item = FaceH> + '_ {
        self.faces.slots.iter()
    }
    pub fn loops(&self) -> impl Iterator<Item = LoopH> + '_ {
        self.loops.slots.iter()
    }

    pub fn vert_live(&self, v: VertH) -> bool {
        self.verts.slots.is_live(v)
    }
    pub fn edge_live(&self, e: EdgeH) -> bool {
        self.edges.slots.is_live(e)
    }
    pub fn face_live(&self, f: FaceH) -> bool {
        self.faces.slots.is_live(f)
    }
    pub fn loop_live(&self, l: LoopH) -> bool {
        self.loops.slots.is_live(l)
    }

    // ---- attributes -------------------------------------------------------

    pub fn vert_attrs(&self) -> &AttributeSet {
        &self.verts.attrs
    }
    pub fn vert_attrs_mut(&mut self) -> &mut AttributeSet {
        &mut self.verts.attrs
    }
    pub fn edge_attrs(&self) -> &AttributeSet {
        &self.edges.attrs
    }
    pub fn edge_attrs_mut(&mut self) -> &mut AttributeSet {
        &mut self.edges.attrs
    }
    pub fn face_attrs(&self) -> &AttributeSet {
        &self.faces.attrs
    }
    pub fn face_attrs_mut(&mut self) -> &mut AttributeSet {
        &mut self.faces.attrs
    }
    pub fn loop_attrs(&self) -> &AttributeSet {
        &self.loops.attrs
    }
    pub fn loop_attrs_mut(&mut self) -> &mut AttributeSet {
        &mut self.loops.attrs
    }

    #[inline]
    pub fn position(&self, v: VertH) -> Vec3 {
        self.verts.attrs.vec3s(V_POSITION)[v.idx()]
    }

    #[inline]
    pub fn set_position(&mut self, v: VertH, p: Vec3) {
        self.verts.attrs.vec3s_mut(V_POSITION).set(v.idx(), p);
    }

    /// The position column (rows for dead slots hold stale values).
    pub fn positions(&self) -> &prism_core::ChunkedVec<Vec3> {
        self.verts.attrs.vec3s(V_POSITION)
    }

    /// Changes whenever positions, topology, or the smooth/sharp flags
    /// change: the key for evaluated geometry caches.
    pub fn geometry_version(&self) -> u64 {
        use crate::tables::{E_SHARP, F_SMOOTH};
        [
            self.verts.attrs.layer(V_POSITION).data.version(),
            self.verts.edge.version(),
            self.edges.v.version(),
            self.edges.loop_.version(),
            self.edges.attrs.layer(E_SHARP).data.version(),
            self.loops.vert.version(),
            self.loops.edge.version(),
            self.loops.face.version(),
            self.loops.next.version(),
            self.faces.loop_.version(),
            self.faces.len.version(),
            self.faces.attrs.layer(F_SMOOTH).data.version(),
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    /// Changes whenever selection or hiding changes on any domain.
    pub fn selection_version(&self) -> u64 {
        use crate::tables::{E_HIDE, E_SELECT, F_HIDE, F_SELECT, V_HIDE, V_SELECT};
        [
            self.verts.attrs.layer(V_SELECT).data.version(),
            self.verts.attrs.layer(V_HIDE).data.version(),
            self.edges.attrs.layer(E_SELECT).data.version(),
            self.edges.attrs.layer(E_HIDE).data.version(),
            self.faces.attrs.layer(F_SELECT).data.version(),
            self.faces.attrs.layer(F_HIDE).data.version(),
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    /// Addresses of every storage chunk (topology and attributes), for
    /// memory accounting across snapshots that share structure.
    pub fn chunk_ptrs(&self, out: &mut std::collections::HashSet<usize>) {
        fn add<T>(out: &mut std::collections::HashSet<usize>, v: &prism_core::ChunkedVec<T>) {
            for c in v.chunks() {
                out.insert(std::sync::Arc::as_ptr(c) as *const u8 as usize);
            }
        }
        fn attrs(out: &mut std::collections::HashSet<usize>, a: &AttributeSet) {
            for l in a.layers() {
                match &l.data {
                    crate::attr::AttrData::Bool(v) => add(out, v),
                    crate::attr::AttrData::F64(v) => add(out, v),
                    crate::attr::AttrData::I32(v) => add(out, v),
                    crate::attr::AttrData::U32(v) => add(out, v),
                    crate::attr::AttrData::Vec2(v) => add(out, v),
                    crate::attr::AttrData::Vec3(v) => add(out, v),
                    crate::attr::AttrData::Vec4(v) => add(out, v),
                    crate::attr::AttrData::Color(v) => add(out, v),
                }
            }
        }
        add(out, &self.verts.edge);
        attrs(out, &self.verts.attrs);
        add(out, &self.edges.v);
        add(out, &self.edges.disk);
        add(out, &self.edges.loop_);
        attrs(out, &self.edges.attrs);
        add(out, &self.loops.vert);
        add(out, &self.loops.edge);
        add(out, &self.loops.face);
        add(out, &self.loops.next);
        add(out, &self.loops.prev);
        add(out, &self.loops.radial_next);
        add(out, &self.loops.radial_prev);
        attrs(out, &self.loops.attrs);
        add(out, &self.faces.loop_);
        add(out, &self.faces.len);
        attrs(out, &self.faces.attrs);
    }

    // ---- vertex ----------------------------------------------------------

    /// Any one edge at `v`, or `None` for a loose vertex.
    #[inline]
    pub fn vert_edge(&self, v: VertH) -> Option<EdgeH> {
        self.verts.edge[v.idx()]
    }

    /// Edges around `v` (disk cycle), starting anywhere.
    pub fn edges_of(&self, v: VertH) -> DiskIter<'_> {
        DiskIter { mesh: self, v, first: self.vert_edge(v), cur: self.vert_edge(v), remaining: self.edge_count() + 1 }
    }

    pub fn vert_edge_count(&self, v: VertH) -> usize {
        self.edges_of(v).count()
    }

    /// Faces touching `v`, each once.
    pub fn faces_of_vert(&self, v: VertH) -> Vec<FaceH> {
        let mut out: Vec<FaceH> = Vec::new();
        for e in self.edges_of(v) {
            for l in self.loops_of_edge(e) {
                let f = self.loop_face(l);
                if !out.contains(&f) {
                    out.push(f);
                }
            }
        }
        out
    }

    /// Corners at `v`: the loops whose vertex is `v`, each once.
    pub fn loops_of_vert(&self, v: VertH) -> Vec<LoopH> {
        let mut out: Vec<LoopH> = Vec::new();
        for e in self.edges_of(v) {
            for l in self.loops_of_edge(e) {
                let at = if self.loop_vert(l) == v { l } else { self.loop_next(l) };
                if self.loop_vert(at) == v && !out.contains(&at) {
                    out.push(at);
                }
            }
        }
        out
    }

    // ---- edge -------------------------------------------------------------

    #[inline]
    pub fn edge_verts(&self, e: EdgeH) -> [VertH; 2] {
        self.edges.v[e.idx()]
    }

    /// The endpoint of `e` that is not `v`.
    #[inline]
    pub fn other_vert(&self, e: EdgeH, v: VertH) -> VertH {
        let [a, b] = self.edge_verts(e);
        if a == v { b } else { a }
    }

    #[inline]
    pub fn edge_has_vert(&self, e: EdgeH, v: VertH) -> bool {
        let [a, b] = self.edge_verts(e);
        a == v || b == v
    }

    /// Any one loop on `e`, or `None` for a wire edge.
    #[inline]
    pub fn edge_loop(&self, e: EdgeH) -> Option<LoopH> {
        self.edges.loop_[e.idx()]
    }

    #[inline]
    pub fn disk_next(&self, e: EdgeH, v: VertH) -> EdgeH {
        self.edges.disk[e.idx()][self.disk_side(e, v)].next
    }

    #[inline]
    pub fn disk_prev(&self, e: EdgeH, v: VertH) -> EdgeH {
        self.edges.disk[e.idx()][self.disk_side(e, v)].prev
    }

    /// Loops around `e` (radial cycle).
    pub fn loops_of_edge(&self, e: EdgeH) -> RadialIter<'_> {
        RadialIter { mesh: self, first: self.edge_loop(e), cur: self.edge_loop(e), remaining: self.loop_count() + 1 }
    }

    pub fn faces_of_edge(&self, e: EdgeH) -> impl Iterator<Item = FaceH> + '_ {
        self.loops_of_edge(e).map(move |l| self.loop_face(l))
    }

    pub fn edge_face_count(&self, e: EdgeH) -> usize {
        self.loops_of_edge(e).count()
    }

    pub fn is_wire_edge(&self, e: EdgeH) -> bool {
        self.edge_loop(e).is_none()
    }

    pub fn is_boundary_edge(&self, e: EdgeH) -> bool {
        self.edge_face_count(e) == 1
    }

    pub fn is_manifold_edge(&self, e: EdgeH) -> bool {
        self.edge_face_count(e) == 2
    }

    /// The edge joining `a` and `b`, if any.
    pub fn edge_between(&self, a: VertH, b: VertH) -> Option<EdgeH> {
        if a == b {
            return None;
        }
        self.edges_of(a).find(|&e| self.other_vert(e, a) == b)
    }

    // ---- loop -------------------------------------------------------------

    #[inline]
    pub fn loop_vert(&self, l: LoopH) -> VertH {
        self.loops.vert[l.idx()]
    }
    #[inline]
    pub fn loop_edge(&self, l: LoopH) -> EdgeH {
        self.loops.edge[l.idx()]
    }
    #[inline]
    pub fn loop_face(&self, l: LoopH) -> FaceH {
        self.loops.face[l.idx()]
    }
    #[inline]
    pub fn loop_next(&self, l: LoopH) -> LoopH {
        self.loops.next[l.idx()]
    }
    #[inline]
    pub fn loop_prev(&self, l: LoopH) -> LoopH {
        self.loops.prev[l.idx()]
    }
    #[inline]
    pub fn radial_next(&self, l: LoopH) -> LoopH {
        self.loops.radial_next[l.idx()]
    }
    #[inline]
    pub fn radial_prev(&self, l: LoopH) -> LoopH {
        self.loops.radial_prev[l.idx()]
    }

    // ---- face -------------------------------------------------------------

    #[inline]
    pub fn face_loop(&self, f: FaceH) -> LoopH {
        self.faces.loop_[f.idx()]
    }

    #[inline]
    pub fn face_len(&self, f: FaceH) -> usize {
        self.faces.len[f.idx()] as usize
    }

    /// Corners of `f` in winding order.
    pub fn loops_of_face(&self, f: FaceH) -> LoopIter<'_> {
        let first = self.face_loop(f);
        LoopIter { mesh: self, first, cur: first, remaining: self.face_len(f) }
    }

    pub fn verts_of_face(&self, f: FaceH) -> impl Iterator<Item = VertH> + '_ {
        self.loops_of_face(f).map(move |l| self.loop_vert(l))
    }

    pub fn edges_of_face(&self, f: FaceH) -> impl Iterator<Item = EdgeH> + '_ {
        self.loops_of_face(f).map(move |l| self.loop_edge(l))
    }

    pub fn face_positions(&self, f: FaceH) -> Vec<Vec3> {
        self.verts_of_face(f).map(|v| self.position(v)).collect()
    }

    pub fn face_normal(&self, f: FaceH) -> Vec3 {
        prism_geom::normal::polygon_normal(&self.face_positions(f))
    }

    pub fn face_center(&self, f: FaceH) -> Vec3 {
        prism_geom::normal::centroid(&self.face_positions(f))
    }

    /// The loop of `f` whose vertex is `v`.
    pub fn face_loop_at(&self, f: FaceH, v: VertH) -> Option<LoopH> {
        self.loops_of_face(f).find(|&l| self.loop_vert(l) == v)
    }

    /// The loop of `f` on edge `e`.
    pub fn face_loop_on(&self, f: FaceH, e: EdgeH) -> Option<LoopH> {
        self.loops_of_face(f).find(|&l| self.loop_edge(l) == e)
    }

    /// Does `f` use edge `e`?
    pub fn face_has_edge(&self, f: FaceH, e: EdgeH) -> bool {
        self.loops_of_edge(e).any(|l| self.loop_face(l) == f)
    }
}

/// Edges around a vertex.
pub struct DiskIter<'a> {
    mesh: &'a Mesh,
    v: VertH,
    first: Option<EdgeH>,
    cur: Option<EdgeH>,
    remaining: usize,
}

impl Iterator for DiskIter<'_> {
    type Item = EdgeH;
    fn next(&mut self) -> Option<EdgeH> {
        let e = self.cur?;
        if self.remaining == 0 {
            return None; // corrupt cycle guard
        }
        self.remaining -= 1;
        let n = self.mesh.disk_next(e, self.v);
        self.cur = if Some(n) == self.first { None } else { Some(n) };
        Some(e)
    }
}

/// Loops around an edge.
pub struct RadialIter<'a> {
    mesh: &'a Mesh,
    first: Option<LoopH>,
    cur: Option<LoopH>,
    remaining: usize,
}

impl Iterator for RadialIter<'_> {
    type Item = LoopH;
    fn next(&mut self) -> Option<LoopH> {
        let l = self.cur?;
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let n = self.mesh.radial_next(l);
        self.cur = if Some(n) == self.first { None } else { Some(n) };
        Some(l)
    }
}

/// Corners around a face.
pub struct LoopIter<'a> {
    mesh: &'a Mesh,
    first: LoopH,
    cur: LoopH,
    remaining: usize,
}

impl Iterator for LoopIter<'_> {
    type Item = LoopH;
    fn next(&mut self) -> Option<LoopH> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let l = self.cur;
        let n = self.mesh.loop_next(l);
        self.cur = n;
        if n == self.first {
            self.remaining = 0;
        }
        Some(l)
    }
}
