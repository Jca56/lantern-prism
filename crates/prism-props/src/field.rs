//! Per-field metadata: what the UI, the file format and the animation system
//! need to know about one property.

use crate::reflect::Prop;
use crate::value::{Kind, Value};

/// Numeric bounds. Open ends are `±INFINITY`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    pub const fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }
    pub fn clamp(&self, v: f64) -> f64 {
        v.clamp(self.min, self.max)
    }
    pub fn contains(&self, v: f64) -> bool {
        v >= self.min && v <= self.max
    }
}

impl From<core::ops::RangeInclusive<f64>> for Range {
    fn from(r: core::ops::RangeInclusive<f64>) -> Self {
        Self::new(*r.start(), *r.end())
    }
}
impl From<core::ops::RangeFrom<f64>> for Range {
    fn from(r: core::ops::RangeFrom<f64>) -> Self {
        Self::new(r.start, f64::INFINITY)
    }
}
impl From<core::ops::RangeToInclusive<f64>> for Range {
    fn from(r: core::ops::RangeToInclusive<f64>) -> Self {
        Self::new(f64::NEG_INFINITY, r.end)
    }
}
impl From<core::ops::RangeInclusive<i64>> for Range {
    fn from(r: core::ops::RangeInclusive<i64>) -> Self {
        Self::new(*r.start() as f64, *r.end() as f64)
    }
}
impl From<core::ops::RangeFrom<i64>> for Range {
    fn from(r: core::ops::RangeFrom<i64>) -> Self {
        Self::new(r.start as f64, f64::INFINITY)
    }
}
impl From<core::ops::RangeToInclusive<i64>> for Range {
    fn from(r: core::ops::RangeToInclusive<i64>) -> Self {
        Self::new(f64::NEG_INFINITY, r.end as f64)
    }
}

/// How the UI should present a value. Storage is unaffected: angles are
/// always radians in memory and degrees on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Subtype {
    #[default]
    None,
    /// Scene units.
    Distance,
    /// Radians in memory, degrees in the UI.
    Angle,
    /// A `Vec3` of angles.
    Euler,
    /// `0..=1`, shown as a slider.
    Factor,
    /// `0..=1`, shown as `0..=100 %`.
    Percentage,
    /// Screen pixels.
    Pixels,
    /// Seconds.
    Time,
    /// A translation vector.
    Translation,
    /// A scale vector.
    Scale,
    /// A unit direction.
    Direction,
    FilePath,
    DirPath,
}

/// Bit flags on a field. Combine with `|` (see [`flags`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Flags(pub u32);

/// Flag constants, importable with `use prism_props::flags::*`.
pub mod flags {
    use super::Flags;
    pub const NONE: Flags = Flags(0);
    /// May be keyframed.
    pub const ANIMATABLE: Flags = Flags(1 << 0);
    /// Never shown in auto panels.
    pub const HIDDEN: Flags = Flags(1 << 1);
    /// Not written to files (runtime cache, UI scratch).
    pub const SKIP_SAVE: Flags = Flags(1 << 2);
    /// Shown, but not editable.
    pub const READ_ONLY: Flags = Flags(1 << 3);
    /// Editing this field requires re-evaluation of the object.
    pub const UPDATE_EVAL: Flags = Flags(1 << 4);
}

impl Flags {
    pub const fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for Flags {
    type Output = Flags;
    fn bitor(self, o: Flags) -> Flags {
        Flags(self.0 | o.0)
    }
}
impl core::ops::BitOrAssign for Flags {
    fn bitor_assign(&mut self, o: Flags) {
        self.0 |= o.0;
    }
}

/// Everything known about one field of a reflected struct.
#[derive(Clone, Debug)]
pub struct FieldInfo {
    /// Stable id for serialization. Implicitly `index + 1` unless declared.
    pub id: u32,
    /// Rust field name.
    pub name: &'static str,
    /// Human label; defaults to the humanized name.
    pub label: String,
    /// Doc comment, lines joined with newlines.
    pub doc: String,
    pub kind: Kind,
    /// Default for leaf fields; `Value::None` for structs and lists.
    pub default: Value,
    /// Hard limits the value can never leave.
    pub hard: Option<Range>,
    /// Soft limits for sliders; dragging stops here, typing does not.
    pub soft: Option<Range>,
    /// Increment for drag / arrow keys.
    pub step: Option<f64>,
    pub subtype: Subtype,
    pub flags: Flags,
}

impl FieldInfo {
    /// Start describing a field whose Rust type is `T`.
    pub fn builder<T: Prop>(name: &'static str, default: Value) -> FieldInfoBuilder {
        FieldInfoBuilder {
            info: FieldInfo {
                id: 0,
                name,
                label: String::new(),
                doc: String::new(),
                kind: T::kind(),
                default,
                hard: None,
                soft: None,
                step: None,
                subtype: Subtype::None,
                flags: Flags::default(),
            },
        }
    }

    pub fn is_hidden(&self) -> bool {
        self.flags.contains(flags::HIDDEN)
    }

    pub fn is_saved(&self) -> bool {
        !self.flags.contains(flags::SKIP_SAVE)
    }

    /// The range a slider should use: soft if given, else hard, else `None`.
    pub fn slider_range(&self) -> Option<Range> {
        self.soft.or(self.hard)
    }

    /// Clamp to the hard range, if any.
    pub fn clamp(&self, v: f64) -> f64 {
        self.hard.map_or(v, |r| r.clamp(v))
    }
}

/// `use_normals` → `Use Normals`.
pub fn humanize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = true;
    for ch in name.chars() {
        if ch == '_' {
            out.push(' ');
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

pub struct FieldInfoBuilder {
    info: FieldInfo,
}

impl FieldInfoBuilder {
    pub fn id(mut self, id: u32) -> Self {
        self.info.id = id;
        self
    }
    pub fn label(mut self, label: &str) -> Self {
        self.info.label = label.to_owned();
        self
    }
    /// Append one line of documentation (leading space from `///` trimmed).
    pub fn doc_line(mut self, line: &str) -> Self {
        if !self.info.doc.is_empty() {
            self.info.doc.push('\n');
        }
        self.info.doc.push_str(line.strip_prefix(' ').unwrap_or(line));
        self
    }
    pub fn hard(mut self, r: impl Into<Range>) -> Self {
        self.info.hard = Some(r.into());
        self
    }
    pub fn soft(mut self, r: impl Into<Range>) -> Self {
        self.info.soft = Some(r.into());
        self
    }
    pub fn step(mut self, step: f64) -> Self {
        self.info.step = Some(step);
        self
    }
    pub fn subtype(mut self, s: Subtype) -> Self {
        self.info.subtype = s;
        self
    }
    pub fn flags(mut self, f: Flags) -> Self {
        self.info.flags |= f;
        self
    }
    pub fn build(mut self) -> FieldInfo {
        if self.info.label.is_empty() {
            self.info.label = humanize(self.info.name);
        }
        if let Some(Range { min, max }) = self.info.hard
            && let Some(soft) = self.info.soft
        {
            // Soft limits never exceed hard limits.
            self.info.soft = Some(Range::new(soft.min.max(min), soft.max.min(max)));
        }
        self.info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_names() {
        assert_eq!(humanize("use_normals"), "Use Normals");
        assert_eq!(humanize("x"), "X");
        assert_eq!(humanize("ui_scale_percent"), "Ui Scale Percent");
    }

    #[test]
    fn ranges_and_flags() {
        assert_eq!(Range::from(0.0..=1.0), Range::new(0.0, 1.0));
        assert_eq!(Range::from(2.0..), Range::new(2.0, f64::INFINITY));
        assert_eq!(Range::from(..=3.0), Range::new(f64::NEG_INFINITY, 3.0));
        assert_eq!(Range::from(1..=5), Range::new(1.0, 5.0));
        let f = flags::ANIMATABLE | flags::HIDDEN;
        assert!(f.contains(flags::HIDDEN));
        assert!(!f.contains(flags::SKIP_SAVE));
        assert!(flags::NONE.is_empty());
    }

    #[test]
    fn builder_defaults_and_soft_clamping() {
        let f = FieldInfo::builder::<f64>("bevel_width", Value::F64(0.1))
            .hard(0.0..=1.0)
            .soft(-5.0..=10.0)
            .doc_line(" First line.")
            .doc_line(" Second.")
            .flags(flags::ANIMATABLE)
            .build();
        assert_eq!(f.label, "Bevel Width");
        assert_eq!(f.doc, "First line.\nSecond.");
        assert_eq!(f.soft, Some(Range::new(0.0, 1.0)));
        assert_eq!(f.slider_range(), Some(Range::new(0.0, 1.0)));
        assert_eq!(f.clamp(7.0), 1.0);
        assert!(f.is_saved() && !f.is_hidden());
        assert_eq!(f.kind, Kind::F64);
        assert_eq!(f.id, 0, "implicit until TypeInfo::build assigns it");
    }
}
