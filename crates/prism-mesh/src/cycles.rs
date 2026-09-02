//! Link and unlink primitives for the three ring structures. Private: only
//! the euler operators call these.

use crate::handle::{EdgeH, LoopH, VertH};
use crate::mesh::Mesh;
use crate::tables::DiskLink;

impl Mesh {
    /// Which of `e`'s two disk links belongs to `v`'s cycle.
    #[inline]
    pub(crate) fn disk_side(&self, e: EdgeH, v: VertH) -> usize {
        let [a, _] = self.edges.v[e.idx()];
        if a == v { 0 } else { 1 }
    }

    fn set_disk(&mut self, e: EdgeH, side: usize, link: DiskLink) {
        let mut d = self.edges.disk[e.idx()];
        d[side] = link;
        self.edges.disk.set(e.idx(), d);
    }

    /// Add `e` to `v`'s disk cycle (`v` must be an endpoint of `e`).
    pub(crate) fn disk_insert(&mut self, e: EdgeH, v: VertH) {
        let side = self.disk_side(e, v);
        match self.vert_edge(v) {
            None => {
                self.set_disk(e, side, DiskLink { next: e, prev: e });
                self.verts.edge.set(v.idx(), Some(e));
            }
            Some(first) => {
                // Insert after `first`.
                let fside = self.disk_side(first, v);
                let after = self.edges.disk[first.idx()][fside].next;
                let aside = self.disk_side(after, v);
                self.set_disk(e, side, DiskLink { next: after, prev: first });
                let mut fl = self.edges.disk[first.idx()];
                fl[fside].next = e;
                self.edges.disk.set(first.idx(), fl);
                let mut al = self.edges.disk[after.idx()];
                al[aside].prev = e;
                self.edges.disk.set(after.idx(), al);
            }
        }
    }

    /// Remove `e` from `v`'s disk cycle.
    pub(crate) fn disk_remove(&mut self, e: EdgeH, v: VertH) {
        let side = self.disk_side(e, v);
        let DiskLink { next, prev } = self.edges.disk[e.idx()][side];
        if next == e {
            self.verts.edge.set(v.idx(), None);
        } else {
            let nside = self.disk_side(next, v);
            let pside = self.disk_side(prev, v);
            let mut nl = self.edges.disk[next.idx()];
            nl[nside].prev = prev;
            self.edges.disk.set(next.idx(), nl);
            let mut pl = self.edges.disk[prev.idx()];
            pl[pside].next = next;
            self.edges.disk.set(prev.idx(), pl);
            if self.vert_edge(v) == Some(e) {
                self.verts.edge.set(v.idx(), Some(next));
            }
        }
        self.set_disk(e, side, DiskLink { next: crate::handle::invalid(), prev: crate::handle::invalid() });
    }

    /// Add loop `l` to the radial cycle of its edge.
    pub(crate) fn radial_insert(&mut self, l: LoopH) {
        let e = self.loop_edge(l);
        match self.edge_loop(e) {
            None => {
                self.loops.radial_next.set(l.idx(), l);
                self.loops.radial_prev.set(l.idx(), l);
                self.edges.loop_.set(e.idx(), Some(l));
            }
            Some(first) => {
                let after = self.radial_next(first);
                self.loops.radial_next.set(l.idx(), after);
                self.loops.radial_prev.set(l.idx(), first);
                self.loops.radial_next.set(first.idx(), l);
                self.loops.radial_prev.set(after.idx(), l);
            }
        }
    }

    /// Remove loop `l` from the radial cycle of its edge.
    pub(crate) fn radial_remove(&mut self, l: LoopH) {
        let e = self.loop_edge(l);
        let next = self.radial_next(l);
        let prev = self.radial_prev(l);
        if next == l {
            self.edges.loop_.set(e.idx(), None);
        } else {
            self.loops.radial_prev.set(next.idx(), prev);
            self.loops.radial_next.set(prev.idx(), next);
            if self.edge_loop(e) == Some(l) {
                self.edges.loop_.set(e.idx(), Some(next));
            }
        }
        self.loops.radial_next.set(l.idx(), crate::handle::invalid());
        self.loops.radial_prev.set(l.idx(), crate::handle::invalid());
    }

    /// Link `a → b` in a loop cycle.
    #[inline]
    pub(crate) fn loop_link(&mut self, a: LoopH, b: LoopH) {
        self.loops.next.set(a.idx(), b);
        self.loops.prev.set(b.idx(), a);
    }
}
