//! The four element tables. Each owns a slot allocator, its topology
//! columns and an attribute set, all kept at the same row count.

use prism_core::ChunkedVec;
use prism_math::Vec3;

use crate::attr::{AttrFlags, AttrKind, AttributeSet, names};
use crate::handle::{Edge, EdgeH, Face, FaceH, Loop, LoopH, Vert, VertH, invalid};
use crate::slots::Slots;

/// One edge's links inside one vertex's disk cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiskLink {
    pub next: EdgeH,
    pub prev: EdgeH,
}

impl DiskLink {
    fn unset() -> Self {
        Self { next: invalid(), prev: invalid() }
    }
}

// Fixed indices of the built-in layers.
pub const V_POSITION: usize = 0;
pub const V_SELECT: usize = 1;
pub const V_HIDE: usize = 2;
pub const E_SELECT: usize = 0;
pub const E_HIDE: usize = 1;
pub const E_SEAM: usize = 2;
pub const E_SHARP: usize = 3;
pub const E_CREASE: usize = 4;
pub const E_BEVEL_WEIGHT: usize = 5;
pub const F_SELECT: usize = 0;
pub const F_HIDE: usize = 1;
pub const F_SMOOTH: usize = 2;
pub const F_MATERIAL: usize = 3;

#[derive(Clone, Debug)]
pub struct VertTable {
    pub slots: Slots<Vert>,
    /// Any one edge of the disk cycle; `None` for a loose vertex.
    pub edge: ChunkedVec<Option<EdgeH>>,
    pub attrs: AttributeSet,
}

#[derive(Clone, Debug)]
pub struct EdgeTable {
    pub slots: Slots<Edge>,
    pub v: ChunkedVec<[VertH; 2]>,
    /// Disk links, one per endpoint (`disk[i]` belongs to `v[i]`'s cycle).
    pub disk: ChunkedVec<[DiskLink; 2]>,
    /// Any one loop of the radial cycle; `None` for a wire edge.
    pub loop_: ChunkedVec<Option<LoopH>>,
    pub attrs: AttributeSet,
}

#[derive(Clone, Debug)]
pub struct LoopTable {
    pub slots: Slots<Loop>,
    pub vert: ChunkedVec<VertH>,
    /// Edge from this corner to the next corner's vertex.
    pub edge: ChunkedVec<EdgeH>,
    pub face: ChunkedVec<FaceH>,
    pub next: ChunkedVec<LoopH>,
    pub prev: ChunkedVec<LoopH>,
    pub radial_next: ChunkedVec<LoopH>,
    pub radial_prev: ChunkedVec<LoopH>,
    pub attrs: AttributeSet,
}

#[derive(Clone, Debug)]
pub struct FaceTable {
    pub slots: Slots<Face>,
    pub loop_: ChunkedVec<LoopH>,
    pub len: ChunkedVec<u32>,
    pub attrs: AttributeSet,
}

fn add(attrs: &mut AttributeSet, name: &str, kind: AttrKind, flags: AttrFlags) {
    attrs.add(name, kind, flags).expect("fresh attribute set");
}

impl VertTable {
    pub fn new() -> Self {
        let mut attrs = AttributeSet::new();
        add(&mut attrs, names::POSITION, AttrKind::Vec3, AttrFlags::REQUIRED | AttrFlags::INTERPOLATE);
        add(&mut attrs, names::SELECT, AttrKind::Bool, AttrFlags::NONE);
        add(&mut attrs, names::HIDE, AttrKind::Bool, AttrFlags::NONE);
        Self { slots: Slots::new(), edge: ChunkedVec::new(), attrs }
    }

    pub fn alloc(&mut self, position: Vec3) -> VertH {
        let (h, new) = self.slots.alloc();
        if new {
            self.edge.push(None);
            self.attrs.push_default();
        } else {
            self.edge.set(h.idx(), None);
            self.attrs.reset(h.idx());
        }
        self.attrs.vec3s_mut(V_POSITION).set(h.idx(), position);
        h
    }
}

impl EdgeTable {
    pub fn new() -> Self {
        let mut attrs = AttributeSet::new();
        add(&mut attrs, names::SELECT, AttrKind::Bool, AttrFlags::NONE);
        add(&mut attrs, names::HIDE, AttrKind::Bool, AttrFlags::NONE);
        add(&mut attrs, names::SEAM, AttrKind::Bool, AttrFlags::NONE);
        add(&mut attrs, names::SHARP, AttrKind::Bool, AttrFlags::NONE);
        add(&mut attrs, names::CREASE, AttrKind::F64, AttrFlags::INTERPOLATE);
        add(&mut attrs, names::BEVEL_WEIGHT, AttrKind::F64, AttrFlags::INTERPOLATE);
        Self { slots: Slots::new(), v: ChunkedVec::new(), disk: ChunkedVec::new(), loop_: ChunkedVec::new(), attrs }
    }

    pub fn alloc(&mut self, v: [VertH; 2]) -> EdgeH {
        let (h, new) = self.slots.alloc();
        let i = h.idx();
        if new {
            self.v.push(v);
            self.disk.push([DiskLink::unset(), DiskLink::unset()]);
            self.loop_.push(None);
            self.attrs.push_default();
        } else {
            self.v.set(i, v);
            self.disk.set(i, [DiskLink::unset(), DiskLink::unset()]);
            self.loop_.set(i, None);
            self.attrs.reset(i);
        }
        h
    }
}

impl LoopTable {
    pub fn new() -> Self {
        Self {
            slots: Slots::new(),
            vert: ChunkedVec::new(),
            edge: ChunkedVec::new(),
            face: ChunkedVec::new(),
            next: ChunkedVec::new(),
            prev: ChunkedVec::new(),
            radial_next: ChunkedVec::new(),
            radial_prev: ChunkedVec::new(),
            attrs: AttributeSet::new(),
        }
    }

    pub fn alloc(&mut self, vert: VertH, edge: EdgeH, face: FaceH) -> LoopH {
        let (h, new) = self.slots.alloc();
        let i = h.idx();
        if new {
            self.vert.push(vert);
            self.edge.push(edge);
            self.face.push(face);
            self.next.push(invalid());
            self.prev.push(invalid());
            self.radial_next.push(invalid());
            self.radial_prev.push(invalid());
            self.attrs.push_default();
        } else {
            self.vert.set(i, vert);
            self.edge.set(i, edge);
            self.face.set(i, face);
            self.next.set(i, invalid());
            self.prev.set(i, invalid());
            self.radial_next.set(i, invalid());
            self.radial_prev.set(i, invalid());
            self.attrs.reset(i);
        }
        h
    }
}

impl FaceTable {
    pub fn new() -> Self {
        let mut attrs = AttributeSet::new();
        add(&mut attrs, names::SELECT, AttrKind::Bool, AttrFlags::NONE);
        add(&mut attrs, names::HIDE, AttrKind::Bool, AttrFlags::NONE);
        add(&mut attrs, names::SMOOTH, AttrKind::Bool, AttrFlags::NONE);
        add(&mut attrs, names::MATERIAL_INDEX, AttrKind::U32, AttrFlags::NONE);
        Self { slots: Slots::new(), loop_: ChunkedVec::new(), len: ChunkedVec::new(), attrs }
    }

    pub fn alloc(&mut self) -> FaceH {
        let (h, new) = self.slots.alloc();
        let i = h.idx();
        if new {
            self.loop_.push(invalid());
            self.len.push(0);
            self.attrs.push_default();
        } else {
            self.loop_.set(i, invalid());
            self.len.set(i, 0);
            self.attrs.reset(i);
        }
        h
    }
}

impl Default for VertTable {
    fn default() -> Self {
        Self::new()
    }
}
impl Default for EdgeTable {
    fn default() -> Self {
        Self::new()
    }
}
impl Default for LoopTable {
    fn default() -> Self {
        Self::new()
    }
}
impl Default for FaceTable {
    fn default() -> Self {
        Self::new()
    }
}
