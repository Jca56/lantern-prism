//! Generic binary serialization of any `Reflect` value, tagged by stable
//! field id. Old files load in new builds (missing fields keep defaults);
//! new files load in old builds (unknown fields are skipped); a field whose
//! type changed under the same id is skipped too.
//!
//! Layout (all little-endian):
//! ```text
//! struct  := u32 count, count × ( u32 field_id, u8 tag, payload )
//! payload := fixed bytes for scalars/vectors
//!          | STR:    u32 len, bytes
//!          | STRUCT: u32 len, struct
//!          | LIST:   u32 len, u32 count, u8 item_tag, count × item
//! item    := payload (leaf)  |  u32 len, struct (STRUCT)
//! ```

use core::fmt;

use prism_core::Id;
use prism_math::{Color, Vec2, Vec3, Vec4};

use crate::reflect::{Reflect, ReflectList};
use crate::value::{Kind, Value};

const T_BOOL: u8 = 1;
const T_I64: u8 = 2;
const T_F64: u8 = 3;
const T_VEC2: u8 = 4;
const T_VEC3: u8 = 5;
const T_VEC4: u8 = 6;
const T_COLOR: u8 = 7;
const T_STR: u8 = 8;
const T_ENUM: u8 = 9;
const T_ID: u8 = 10;
const T_STRUCT: u8 = 11;
const T_LIST: u8 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialError {
    UnexpectedEof,
    BadUtf8,
    BadTag(u8),
}

impl fmt::Display for SerialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerialError::UnexpectedEof => write!(f, "unexpected end of data"),
            SerialError::BadUtf8 => write!(f, "invalid UTF-8 in string"),
            SerialError::BadTag(t) => write!(f, "unknown type tag {t}"),
        }
    }
}

impl std::error::Error for SerialError {}

fn tag_for(kind: &Kind) -> u8 {
    match kind {
        Kind::Bool => T_BOOL,
        Kind::I64 => T_I64,
        Kind::F64 => T_F64,
        Kind::Vec2 => T_VEC2,
        Kind::Vec3 => T_VEC3,
        Kind::Vec4 => T_VEC4,
        Kind::Color => T_COLOR,
        Kind::Str => T_STR,
        Kind::Enum(_) => T_ENUM,
        Kind::Id => T_ID,
        Kind::Struct(_) => T_STRUCT,
        Kind::List(_) => T_LIST,
    }
}

// ------------------------------------------------------------------ writing

struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    /// Write a length-prefixed block produced by `f`.
    fn block(&mut self, f: impl FnOnce(&mut Writer)) {
        let at = self.out.len();
        self.u32(0);
        f(self);
        let len = (self.out.len() - at - 4) as u32;
        self.out[at..at + 4].copy_from_slice(&len.to_le_bytes());
    }

    fn value(&mut self, v: &Value) {
        match v {
            Value::None => {}
            Value::Bool(b) => self.u8(*b as u8),
            Value::I64(x) | Value::Enum(x) => self.i64(*x),
            Value::F64(x) => self.f64(*x),
            Value::Vec2(v) => {
                self.f64(v.x);
                self.f64(v.y);
            }
            Value::Vec3(v) => {
                self.f64(v.x);
                self.f64(v.y);
                self.f64(v.z);
            }
            Value::Vec4(v) => {
                self.f64(v.x);
                self.f64(v.y);
                self.f64(v.z);
                self.f64(v.w);
            }
            Value::Color(c) => {
                self.f64(c.r);
                self.f64(c.g);
                self.f64(c.b);
                self.f64(c.a);
            }
            Value::Str(s) => {
                self.u32(s.len() as u32);
                self.out.extend_from_slice(s.as_bytes());
            }
            Value::Id(id) => self.u64(id.raw()),
        }
    }

    fn list(&mut self, l: &dyn ReflectList) {
        let item_kind = l.item_kind();
        self.u32(l.len() as u32);
        self.u8(tag_for(&item_kind));
        for i in 0..l.len() {
            match item_kind {
                Kind::Struct(_) => {
                    if let Some(s) = l.get_struct(i) {
                        self.block(|w| w.structure(s));
                    } else {
                        self.u32(0);
                    }
                }
                Kind::List(_) => self.u32(0), // nested lists are not supported
                _ => self.value(&l.get(i)),
            }
        }
    }

    fn structure(&mut self, r: &dyn Reflect) {
        let info = r.type_info();
        let saved: Vec<usize> = (0..info.fields.len()).filter(|&i| info.fields[i].is_saved()).collect();
        self.u32(saved.len() as u32);
        for i in saved {
            let field = &info.fields[i];
            self.u32(field.id);
            self.u8(tag_for(&field.kind));
            match &field.kind {
                Kind::Struct(_) => match r.get_struct(i) {
                    Some(s) => self.block(|w| w.structure(s)),
                    None => self.u32(0),
                },
                Kind::List(_) => match r.get_list(i) {
                    Some(l) => self.block(|w| w.list(l)),
                    None => self.block(|w| {
                        w.u32(0);
                        w.u8(0);
                    }),
                },
                _ => self.value(&r.get(i)),
            }
        }
    }
}

/// Serialize a reflected value.
pub fn to_bytes(r: &dyn Reflect) -> Vec<u8> {
    let mut w = Writer { out: Vec::new() };
    w.structure(r);
    w.out
}

// ------------------------------------------------------------------ reading

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SerialError> {
        if self.pos + n > self.data.len() {
            return Err(SerialError::UnexpectedEof);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, SerialError> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, SerialError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, SerialError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, SerialError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, SerialError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn block(&mut self) -> Result<Reader<'a>, SerialError> {
        let len = self.u32()? as usize;
        Ok(Reader { data: self.take(len)?, pos: 0 })
    }

    fn value(&mut self, tag: u8) -> Result<Value, SerialError> {
        Ok(match tag {
            T_BOOL => Value::Bool(self.u8()? != 0),
            T_I64 => Value::I64(self.i64()?),
            T_F64 => Value::F64(self.f64()?),
            T_VEC2 => Value::Vec2(Vec2::new(self.f64()?, self.f64()?)),
            T_VEC3 => Value::Vec3(Vec3::new(self.f64()?, self.f64()?, self.f64()?)),
            T_VEC4 => Value::Vec4(Vec4::new(self.f64()?, self.f64()?, self.f64()?, self.f64()?)),
            T_COLOR => Value::Color(Color::rgba(self.f64()?, self.f64()?, self.f64()?, self.f64()?)),
            T_STR => {
                let len = self.u32()? as usize;
                let bytes = self.take(len)?;
                Value::Str(core::str::from_utf8(bytes).map_err(|_| SerialError::BadUtf8)?.to_owned())
            }
            T_ENUM => Value::Enum(self.i64()?),
            T_ID => Value::Id(Id(self.u64()?)),
            other => return Err(SerialError::BadTag(other)),
        })
    }

    /// Consume one payload of `tag` without interpreting it.
    fn skip(&mut self, tag: u8) -> Result<(), SerialError> {
        match tag {
            T_STR | T_STRUCT | T_LIST => {
                self.block()?;
            }
            _ => {
                self.value(tag)?;
            }
        }
        Ok(())
    }

    fn list(&mut self, l: &mut dyn ReflectList) -> Result<(), SerialError> {
        let count = self.u32()? as usize;
        let item_tag = self.u8()?;
        let expected = tag_for(&l.item_kind());
        l.clear();
        if item_tag != expected {
            return Ok(()); // element type changed: leave the list empty
        }
        for _ in 0..count {
            l.push_default();
            let i = l.len() - 1;
            match item_tag {
                T_STRUCT => {
                    let mut sub = self.block()?;
                    if let Some(s) = l.get_struct_mut(i) {
                        sub.structure(s)?;
                    }
                }
                T_LIST => {
                    self.block()?;
                }
                _ => {
                    let v = self.value(item_tag)?;
                    let _ = l.set(i, v);
                }
            }
        }
        Ok(())
    }

    fn structure(&mut self, r: &mut dyn Reflect) -> Result<(), SerialError> {
        let info = r.type_info();
        let count = self.u32()?;
        for _ in 0..count {
            let id = self.u32()?;
            let tag = self.u8()?;
            let Some(fi) = info.field_index_by_id(id) else {
                self.skip(tag)?;
                continue;
            };
            let field = &info.fields[fi];
            if tag != tag_for(&field.kind) || !field.is_saved() {
                self.skip(tag)?;
                continue;
            }
            match &field.kind {
                Kind::Struct(_) => {
                    let mut sub = self.block()?;
                    if let Some(s) = r.get_struct_mut(fi) {
                        sub.structure(s)?;
                    }
                }
                Kind::List(_) => {
                    let mut sub = self.block()?;
                    if let Some(l) = r.get_list_mut(fi) {
                        sub.list(l)?;
                    }
                }
                _ => {
                    let v = self.value(tag)?;
                    // A rejected value (e.g. a removed enum variant) keeps the default.
                    let _ = r.set(fi, v);
                }
            }
        }
        Ok(())
    }
}

/// Load `bytes` into `r`, which should start as its default.
pub fn from_bytes(r: &mut dyn Reflect, bytes: &[u8]) -> Result<(), SerialError> {
    Reader { data: bytes, pos: 0 }.structure(r)
}
