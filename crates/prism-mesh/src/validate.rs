//! Every invariant of the kernel, checked exhaustively. Runs after each euler
//! operator when `Mesh::paranoid` is set (tests, fuzzing) and on demand.

use std::collections::HashSet;

use crate::handle::{EdgeH, FaceH, LoopH, VertH, is_invalid};
use crate::mesh::Mesh;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidateError {
    pub rule: &'static str,
    pub detail: String,
}

const MAX_ERRORS: usize = 50;

struct Report {
    errors: Vec<ValidateError>,
}

impl Report {
    fn err(&mut self, rule: &'static str, detail: String) {
        if self.errors.len() < MAX_ERRORS {
            self.errors.push(ValidateError { rule, detail });
        }
    }
}

impl Mesh {
    /// Check all invariants. `Ok` means the topology is fully consistent.
    pub fn validate(&self) -> Result<(), Vec<ValidateError>> {
        let mut r = Report { errors: Vec::new() };
        self.validate_storage(&mut r);
        self.validate_verts(&mut r);
        self.validate_edges(&mut r);
        self.validate_loops(&mut r);
        self.validate_faces(&mut r);
        if r.errors.is_empty() { Ok(()) } else { Err(r.errors) }
    }

    fn validate_storage(&self, r: &mut Report) {
        for (name, res) in [
            ("verts", self.verts.slots.check()),
            ("edges", self.edges.slots.check()),
            ("loops", self.loops.slots.check()),
            ("faces", self.faces.slots.check()),
        ] {
            if let Err(e) = res {
                r.err("free-list", format!("{name}: {e}"));
            }
        }
        let checks = [
            ("verts", self.verts.slots.capacity(), self.verts.attrs.len(), self.verts.attrs.check_lengths()),
            ("edges", self.edges.slots.capacity(), self.edges.attrs.len(), self.edges.attrs.check_lengths()),
            ("loops", self.loops.slots.capacity(), self.loops.attrs.len(), self.loops.attrs.check_lengths()),
            ("faces", self.faces.slots.capacity(), self.faces.attrs.len(), self.faces.attrs.check_lengths()),
        ];
        for (name, cap, alen, ok) in checks {
            if cap != alen || !ok {
                r.err("layer-length", format!("{name}: {cap} slots but attribute rows {alen} (consistent: {ok})"));
            }
        }
        let cols = [
            ("vert.edge", self.verts.edge.len(), self.verts.slots.capacity()),
            ("edge.v", self.edges.v.len(), self.edges.slots.capacity()),
            ("edge.disk", self.edges.disk.len(), self.edges.slots.capacity()),
            ("edge.loop", self.edges.loop_.len(), self.edges.slots.capacity()),
            ("loop.vert", self.loops.vert.len(), self.loops.slots.capacity()),
            ("loop.next", self.loops.next.len(), self.loops.slots.capacity()),
            ("face.loop", self.faces.loop_.len(), self.faces.slots.capacity()),
            ("face.len", self.faces.len.len(), self.faces.slots.capacity()),
        ];
        for (name, len, cap) in cols {
            if len != cap {
                r.err("column-length", format!("{name}: {len} rows for {cap} slots"));
            }
        }
    }

    fn validate_verts(&self, r: &mut Report) {
        let edge_count = self.edge_count();
        for v in self.verts() {
            let Some(first) = self.vert_edge(v) else {
                continue;
            };
            if !self.edge_live(first) {
                r.err("vert.edge-live", format!("{v} points at dead edge {first}"));
                continue;
            }
            if !self.edge_has_vert(first, v) {
                r.err("vert.edge-endpoint", format!("{v} points at {first} which does not touch it"));
                continue;
            }
            // Walk the disk cycle.
            let mut e = first;
            let mut steps = 0;
            loop {
                if !self.edge_live(e) || !self.edge_has_vert(e, v) {
                    r.err("disk.member", format!("disk of {v} reaches {e}"));
                    break;
                }
                let n = self.disk_next(e, v);
                if is_invalid(n) || !self.edge_live(n) {
                    r.err("disk.next-live", format!("disk of {v}: {e}.next = {n}"));
                    break;
                }
                if self.disk_prev(n, v) != e {
                    r.err("disk.prev-mirror", format!("disk of {v}: {e}.next = {n} but {n}.prev = {}", self.disk_prev(n, v)));
                    break;
                }
                steps += 1;
                if steps > edge_count {
                    r.err("disk.closed", format!("disk of {v} does not return to {first}"));
                    break;
                }
                e = n;
                if e == first {
                    break;
                }
            }
        }
    }

    fn validate_edges(&self, r: &mut Report) {
        let loop_count = self.loop_count();
        let mut pairs: HashSet<(u32, u32)> = HashSet::new();
        let mut radial_members = 0usize;
        for e in self.edges() {
            let [a, b] = self.edge_verts(e);
            if a == b {
                r.err("edge.distinct", format!("{e} joins {a} to itself"));
            }
            for v in [a, b] {
                if !self.vert_live(v) {
                    r.err("edge.vert-live", format!("{e} uses dead vertex {v}"));
                } else if !self.edges_of(v).any(|x| x == e) {
                    r.err("edge.in-disk", format!("{e} is missing from the disk of {v}"));
                }
            }
            let key = (a.index().min(b.index()), a.index().max(b.index()));
            if !pairs.insert(key) {
                r.err("edge.unique", format!("{e} duplicates another edge between {a} and {b}"));
            }
            let Some(first) = self.edge_loop(e) else {
                continue;
            };
            let mut l = first;
            let mut steps = 0;
            loop {
                if !self.loop_live(l) {
                    r.err("radial.live", format!("radial of {e} reaches dead loop {l}"));
                    break;
                }
                if self.loop_edge(l) != e {
                    r.err("radial.edge", format!("radial of {e} contains {l} whose edge is {}", self.loop_edge(l)));
                    break;
                }
                let n = self.radial_next(l);
                if is_invalid(n) || !self.loop_live(n) {
                    r.err("radial.next-live", format!("radial of {e}: {l}.next = {n}"));
                    break;
                }
                if self.radial_prev(n) != l {
                    r.err("radial.prev-mirror", format!("radial of {e}: {l}.next = {n} but {n}.prev = {}", self.radial_prev(n)));
                    break;
                }
                steps += 1;
                radial_members += 1;
                if steps > loop_count {
                    r.err("radial.closed", format!("radial of {e} does not return"));
                    break;
                }
                l = n;
                if l == first {
                    break;
                }
            }
        }
        if radial_members != loop_count {
            r.err("radial.coverage", format!("{radial_members} radial memberships for {loop_count} loops"));
        }
    }

    fn validate_loops(&self, r: &mut Report) {
        for l in self.loops() {
            let v = self.loop_vert(l);
            let e = self.loop_edge(l);
            let f = self.loop_face(l);
            if !self.vert_live(v) {
                r.err("loop.vert-live", format!("{l} at dead vertex {v}"));
                continue;
            }
            if !self.edge_live(e) {
                r.err("loop.edge-live", format!("{l} on dead edge {e}"));
                continue;
            }
            if !self.face_live(f) {
                r.err("loop.face-live", format!("{l} in dead face {f}"));
                continue;
            }
            if !self.edge_has_vert(e, v) {
                r.err("loop.vert-on-edge", format!("{l}: vertex {v} is not on edge {e}"));
            }
            let n = self.loop_next(l);
            let p = self.loop_prev(l);
            if is_invalid(n) || is_invalid(p) || !self.loop_live(n) || !self.loop_live(p) {
                r.err("loop.links-live", format!("{l}: next {n} / prev {p}"));
                continue;
            }
            if self.loop_prev(n) != l {
                r.err("loop.prev-mirror", format!("{l}.next = {n} but {n}.prev = {}", self.loop_prev(n)));
            }
            if self.loop_face(n) != f {
                r.err("loop.same-face", format!("{l} in {f} but next {n} in {}", self.loop_face(n)));
            }
            let nv = self.loop_vert(n);
            if !(self.edge_has_vert(e, nv) && nv != v) {
                r.err("loop.edge-connects", format!("{l}: edge {e} does not join {v} to next vertex {nv}"));
            }
            if is_invalid(self.radial_next(l)) || is_invalid(self.radial_prev(l)) {
                r.err("loop.radial-set", format!("{l} has no radial links"));
            } else if !self.loops_of_edge(e).any(|x| x == l) {
                r.err("loop.in-radial", format!("{l} is missing from the radial cycle of {e}"));
            }
        }
    }

    fn validate_faces(&self, r: &mut Report) {
        let mut covered = 0usize;
        for f in self.faces() {
            let len = self.face_len(f);
            if len < 3 {
                r.err("face.len", format!("{f} has {len} corners"));
                continue;
            }
            let first = self.face_loop(f);
            if is_invalid(first) || !self.loop_live(first) {
                r.err("face.loop-live", format!("{f} starts at {first}"));
                continue;
            }
            let mut l = first;
            let mut seen_verts: HashSet<VertH> = HashSet::new();
            for step in 0..len {
                if !self.loop_live(l) {
                    r.err("face.loop-live", format!("{f} reaches dead loop {l}"));
                    break;
                }
                if self.loop_face(l) != f {
                    r.err("face.loop-face", format!("{f} contains {l} which belongs to {}", self.loop_face(l)));
                    break;
                }
                if !seen_verts.insert(self.loop_vert(l)) {
                    r.err("face.repeated-vert", format!("{f} visits {} twice", self.loop_vert(l)));
                }
                l = self.loop_next(l);
                if l == first && step + 1 != len {
                    r.err("face.len-matches", format!("{f} closes after {} loops, len says {len}", step + 1));
                    break;
                }
            }
            if l != first {
                r.err("face.len-matches", format!("{f} does not close after {len} loops"));
            } else {
                covered += len;
            }
        }
        if covered != self.loop_count() {
            r.err("face.coverage", format!("faces cover {covered} loops of {}", self.loop_count()));
        }
    }
}

// Keep the unused-import lint quiet for handle types used only in messages.
#[allow(dead_code)]
fn _types(_: EdgeH, _: FaceH, _: LoopH) {}
