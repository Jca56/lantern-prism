//! Named, typed attribute layers per element domain. Position, selection,
//! UVs, creases and anything a tool wants to add all live here.

use core::fmt;
use std::sync::Arc;

use prism_core::ChunkedVec;
use prism_math::{Color, Vec2, Vec3, Vec4};

/// Built-in layer names.
pub mod names {
    pub const POSITION: &str = "position";
    pub const SELECT: &str = "select";
    pub const HIDE: &str = "hide";
    pub const SEAM: &str = "seam";
    pub const SHARP: &str = "sharp";
    pub const CREASE: &str = "crease";
    pub const BEVEL_WEIGHT: &str = "bevel_weight";
    pub const SMOOTH: &str = "smooth";
    pub const MATERIAL_INDEX: &str = "material_index";
    pub const UV: &str = "uv";
    pub const COLOR: &str = "color";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttrKind {
    Bool,
    F64,
    I32,
    U32,
    Vec2,
    Vec3,
    Vec4,
    Color,
}

/// A single element's value, for generic access.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttrValue {
    Bool(bool),
    F64(f64),
    I32(i32),
    U32(u32),
    Vec2(Vec2),
    Vec3(Vec3),
    Vec4(Vec4),
    Color(Color),
}

#[derive(Clone, Debug)]
pub enum AttrData {
    Bool(ChunkedVec<bool>),
    F64(ChunkedVec<f64>),
    I32(ChunkedVec<i32>),
    U32(ChunkedVec<u32>),
    Vec2(ChunkedVec<Vec2>),
    Vec3(ChunkedVec<Vec3>),
    Vec4(ChunkedVec<Vec4>),
    Color(ChunkedVec<Color>),
}

macro_rules! each_data {
    ($self:expr, $v:ident => $e:expr) => {
        match $self {
            AttrData::Bool($v) => $e,
            AttrData::F64($v) => $e,
            AttrData::I32($v) => $e,
            AttrData::U32($v) => $e,
            AttrData::Vec2($v) => $e,
            AttrData::Vec3($v) => $e,
            AttrData::Vec4($v) => $e,
            AttrData::Color($v) => $e,
        }
    };
}

impl AttrData {
    pub fn new(kind: AttrKind) -> Self {
        match kind {
            AttrKind::Bool => AttrData::Bool(ChunkedVec::new()),
            AttrKind::F64 => AttrData::F64(ChunkedVec::new()),
            AttrKind::I32 => AttrData::I32(ChunkedVec::new()),
            AttrKind::U32 => AttrData::U32(ChunkedVec::new()),
            AttrKind::Vec2 => AttrData::Vec2(ChunkedVec::new()),
            AttrKind::Vec3 => AttrData::Vec3(ChunkedVec::new()),
            AttrKind::Vec4 => AttrData::Vec4(ChunkedVec::new()),
            AttrKind::Color => AttrData::Color(ChunkedVec::new()),
        }
    }

    pub fn kind(&self) -> AttrKind {
        match self {
            AttrData::Bool(_) => AttrKind::Bool,
            AttrData::F64(_) => AttrKind::F64,
            AttrData::I32(_) => AttrKind::I32,
            AttrData::U32(_) => AttrKind::U32,
            AttrData::Vec2(_) => AttrKind::Vec2,
            AttrData::Vec3(_) => AttrKind::Vec3,
            AttrData::Vec4(_) => AttrKind::Vec4,
            AttrData::Color(_) => AttrKind::Color,
        }
    }

    pub fn len(&self) -> usize {
        each_data!(self, v => v.len())
    }

    /// Storage version of the layer (see `ChunkedVec::version`).
    pub fn version(&self) -> u64 {
        each_data!(self, v => v.version())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push_default(&mut self) {
        each_data!(self, v => v.push(Default::default()))
    }

    pub fn reset(&mut self, i: usize) {
        each_data!(self, v => v.set(i, Default::default()))
    }

    pub fn copy(&mut self, dst: usize, src: usize) {
        if dst != src {
            each_data!(self, v => { let x = v[src]; v.set(dst, x); })
        }
    }

    pub fn get(&self, i: usize) -> AttrValue {
        match self {
            AttrData::Bool(v) => AttrValue::Bool(v[i]),
            AttrData::F64(v) => AttrValue::F64(v[i]),
            AttrData::I32(v) => AttrValue::I32(v[i]),
            AttrData::U32(v) => AttrValue::U32(v[i]),
            AttrData::Vec2(v) => AttrValue::Vec2(v[i]),
            AttrData::Vec3(v) => AttrValue::Vec3(v[i]),
            AttrData::Vec4(v) => AttrValue::Vec4(v[i]),
            AttrData::Color(v) => AttrValue::Color(v[i]),
        }
    }

    /// `false` if the value's type does not match the layer.
    pub fn set(&mut self, i: usize, value: AttrValue) -> bool {
        match (self, value) {
            (AttrData::Bool(v), AttrValue::Bool(x)) => v.set(i, x),
            (AttrData::F64(v), AttrValue::F64(x)) => v.set(i, x),
            (AttrData::I32(v), AttrValue::I32(x)) => v.set(i, x),
            (AttrData::U32(v), AttrValue::U32(x)) => v.set(i, x),
            (AttrData::Vec2(v), AttrValue::Vec2(x)) => v.set(i, x),
            (AttrData::Vec3(v), AttrValue::Vec3(x)) => v.set(i, x),
            (AttrData::Vec4(v), AttrValue::Vec4(x)) => v.set(i, x),
            (AttrData::Color(v), AttrValue::Color(x)) => v.set(i, x),
            _ => return false,
        }
        true
    }

    /// Weighted blend of `sources` into `dst`. Numeric layers interpolate;
    /// bool/int layers take the heaviest source.
    pub fn interpolate(&mut self, dst: usize, sources: &[(usize, f64)]) {
        if sources.is_empty() {
            return;
        }
        let total: f64 = sources.iter().map(|s| s.1).sum();
        let norm = if total > 0.0 { 1.0 / total } else { 1.0 / sources.len() as f64 };
        let heaviest = sources.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map_or(sources[0].0, |s| s.0);
        match self {
            AttrData::Bool(_) | AttrData::I32(_) | AttrData::U32(_) => self.copy(dst, heaviest),
            AttrData::F64(v) => {
                let x = sources.iter().map(|&(i, w)| v[i] * w * norm).sum();
                v.set(dst, x);
            }
            AttrData::Vec2(v) => {
                let x = sources.iter().map(|&(i, w)| v[i] * (w * norm)).sum();
                v.set(dst, x);
            }
            AttrData::Vec3(v) => {
                let x = sources.iter().map(|&(i, w)| v[i] * (w * norm)).sum();
                v.set(dst, x);
            }
            AttrData::Vec4(v) => {
                let x = sources.iter().map(|&(i, w)| v[i] * (w * norm)).sum();
                v.set(dst, x);
            }
            AttrData::Color(v) => {
                let mut acc = Color::TRANSPARENT;
                for &(i, w) in sources {
                    let c = v[i];
                    let w = w * norm;
                    acc = Color::rgba(acc.r + c.r * w, acc.g + c.g * w, acc.b + c.b * w, acc.a + c.a * w);
                }
                v.set(dst, acc);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct AttrFlags(pub u32);

impl AttrFlags {
    pub const NONE: AttrFlags = AttrFlags(0);
    /// Bookkeeping, never shown to the user.
    pub const INTERNAL: AttrFlags = AttrFlags(1);
    /// Cannot be removed (position).
    pub const REQUIRED: AttrFlags = AttrFlags(2);
    /// Not saved.
    pub const TEMPORARY: AttrFlags = AttrFlags(4);
    /// Blend on split/subdivide (UVs yes, selection no).
    pub const INTERPOLATE: AttrFlags = AttrFlags(8);

    pub const fn contains(self, o: AttrFlags) -> bool {
        self.0 & o.0 == o.0
    }
}

impl core::ops::BitOr for AttrFlags {
    type Output = AttrFlags;
    fn bitor(self, o: AttrFlags) -> AttrFlags {
        AttrFlags(self.0 | o.0)
    }
}

#[derive(Clone, Debug)]
pub struct Attribute {
    pub name: Arc<str>,
    pub data: AttrData,
    pub flags: AttrFlags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrError {
    Exists(String),
    Missing(String),
    Required(String),
}

impl fmt::Display for AttrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttrError::Exists(n) => write!(f, "attribute `{n}` already exists"),
            AttrError::Missing(n) => write!(f, "attribute `{n}` does not exist"),
            AttrError::Required(n) => write!(f, "attribute `{n}` cannot be removed"),
        }
    }
}

impl std::error::Error for AttrError {}

/// All layers of one domain. Every layer always has `len()` rows.
#[derive(Clone, Debug, Default)]
pub struct AttributeSet {
    layers: Vec<Attribute>,
    len: usize,
}

impl AttributeSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn layers(&self) -> &[Attribute] {
        &self.layers
    }

    pub fn index(&self, name: &str) -> Option<usize> {
        self.layers.iter().position(|l| &*l.name == name)
    }

    pub fn layer(&self, i: usize) -> &Attribute {
        &self.layers[i]
    }

    pub fn layer_mut(&mut self, i: usize) -> &mut Attribute {
        &mut self.layers[i]
    }

    pub fn by_name(&self, name: &str) -> Option<&Attribute> {
        self.index(name).map(|i| &self.layers[i])
    }

    pub fn by_name_mut(&mut self, name: &str) -> Option<&mut Attribute> {
        self.index(name).map(|i| &mut self.layers[i])
    }

    /// Add a layer filled with defaults. Returns its index.
    pub fn add(&mut self, name: &str, kind: AttrKind, flags: AttrFlags) -> Result<usize, AttrError> {
        if self.index(name).is_some() {
            return Err(AttrError::Exists(name.to_owned()));
        }
        let mut data = AttrData::new(kind);
        for _ in 0..self.len {
            data.push_default();
        }
        self.layers.push(Attribute { name: Arc::from(name), data, flags });
        Ok(self.layers.len() - 1)
    }

    pub fn remove(&mut self, name: &str) -> Result<(), AttrError> {
        let i = self.index(name).ok_or_else(|| AttrError::Missing(name.to_owned()))?;
        if self.layers[i].flags.contains(AttrFlags::REQUIRED) {
            return Err(AttrError::Required(name.to_owned()));
        }
        self.layers.remove(i);
        Ok(())
    }

    /// One new row (defaults) on every layer.
    pub fn push_default(&mut self) {
        for l in &mut self.layers {
            l.data.push_default();
        }
        self.len += 1;
    }

    /// Reset row `i` to defaults on every layer (slot reuse).
    pub fn reset(&mut self, i: usize) {
        for l in &mut self.layers {
            l.data.reset(i);
        }
    }

    pub fn copy(&mut self, dst: usize, src: usize) {
        for l in &mut self.layers {
            l.data.copy(dst, src);
        }
    }

    /// Blend `sources` into `dst` on layers flagged `INTERPOLATE`; the
    /// others copy from the heaviest source.
    pub fn interpolate(&mut self, dst: usize, sources: &[(usize, f64)]) {
        let Some(heaviest) = sources.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|s| s.0) else {
            return;
        };
        for l in &mut self.layers {
            if l.flags.contains(AttrFlags::INTERPOLATE) {
                l.data.interpolate(dst, sources);
            } else {
                l.data.copy(dst, heaviest);
            }
        }
    }

    pub fn check_lengths(&self) -> bool {
        self.layers.iter().all(|l| l.data.len() == self.len)
    }
}

macro_rules! typed_access {
    ($get:ident, $get_mut:ident, $variant:ident, $t:ty) => {
        impl AttributeSet {
            /// Typed view of layer `i`. Panics if the kind differs.
            pub fn $get(&self, i: usize) -> &ChunkedVec<$t> {
                match &self.layers[i].data {
                    AttrData::$variant(v) => v,
                    other => panic!("layer `{}` is {:?}, not {}", self.layers[i].name, other.kind(), stringify!($variant)),
                }
            }
            pub fn $get_mut(&mut self, i: usize) -> &mut ChunkedVec<$t> {
                match &mut self.layers[i].data {
                    AttrData::$variant(v) => v,
                    other => panic!("layer is {:?}, not {}", other.kind(), stringify!($variant)),
                }
            }
        }
    };
}
typed_access!(bools, bools_mut, Bool, bool);
typed_access!(f64s, f64s_mut, F64, f64);
typed_access!(i32s, i32s_mut, I32, i32);
typed_access!(u32s, u32s_mut, U32, u32);
typed_access!(vec2s, vec2s_mut, Vec2, Vec2);
typed_access!(vec3s, vec3s_mut, Vec3, Vec3);
typed_access!(vec4s, vec4s_mut, Vec4, Vec4);
typed_access!(colors, colors_mut, Color, Color);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_stay_in_step() {
        let mut a = AttributeSet::new();
        let p = a.add(names::POSITION, AttrKind::Vec3, AttrFlags::REQUIRED | AttrFlags::INTERPOLATE).unwrap();
        let s = a.add(names::SELECT, AttrKind::Bool, AttrFlags::NONE).unwrap();
        assert!(a.add(names::SELECT, AttrKind::Bool, AttrFlags::NONE).is_err());
        for _ in 0..3 {
            a.push_default();
        }
        assert_eq!(a.len(), 3);
        let w = a.add("weight", AttrKind::F64, AttrFlags::INTERPOLATE).unwrap();
        assert_eq!(a.f64s(w).len(), 3, "late layer is back-filled");
        a.vec3s_mut(p).set(0, Vec3::X);
        a.vec3s_mut(p).set(1, Vec3::Z);
        a.bools_mut(s).set(0, true);
        a.f64s_mut(w).set(1, 4.0);
        a.interpolate(2, &[(0, 1.0), (1, 3.0)]);
        assert!(a.vec3s(p)[2].approx_eq(Vec3::new(0.25, 0.0, 0.75), 1e-12));
        assert_eq!(a.f64s(w)[2], 3.0);
        assert!(!a.bools(s)[2], "non-interpolating layers copy the heaviest source (1)");
        a.copy(2, 0);
        assert!(a.bools(s)[2]);
        a.reset(2);
        assert!(!a.bools(s)[2]);
        assert_eq!(a.vec3s(p)[2], Vec3::ZERO);
        assert_eq!(a.remove(names::POSITION), Err(AttrError::Required(names::POSITION.into())));
        assert!(a.remove("weight").is_ok());
        assert_eq!(a.remove("weight"), Err(AttrError::Missing("weight".into())));
        assert!(a.check_lengths());
        assert_eq!(a.layer(s).data.get(0), AttrValue::Bool(true));
        assert!(!a.layer_mut(s).data.set(0, AttrValue::F64(1.0)), "type mismatch");
    }
}
