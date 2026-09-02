//! Runtime values and the kinds that describe them.

use core::fmt;

use prism_core::Id;
use prism_math::{Color, Vec2, Vec3, Vec4};

use crate::info::TypeInfo;

/// A dynamically typed property value. Struct and list fields have no
/// `Value`; they are reached through `Reflect::get_struct` / `get_list`.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    I64(i64),
    F64(f64),
    Vec2(Vec2),
    Vec3(Vec3),
    Vec4(Vec4),
    Color(Color),
    Str(String),
    /// The discriminant value of an enum variant.
    Enum(i64),
    Id(Id),
}

impl Value {
    /// The leaf kind this value carries, or `None` for [`Value::None`].
    /// Enum values report `None` here as well because the value alone does
    /// not know which enum it belongs to.
    pub fn kind(&self) -> Option<Kind> {
        Some(match self {
            Value::None | Value::Enum(_) => return None,
            Value::Bool(_) => Kind::Bool,
            Value::I64(_) => Kind::I64,
            Value::F64(_) => Kind::F64,
            Value::Vec2(_) => Kind::Vec2,
            Value::Vec3(_) => Kind::Vec3,
            Value::Vec4(_) => Kind::Vec4,
            Value::Color(_) => Kind::Color,
            Value::Str(_) => Kind::Str,
            Value::Id(_) => Kind::Id,
        })
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(v) = self { Some(*v) } else { None }
    }
    pub fn as_i64(&self) -> Option<i64> {
        if let Value::I64(v) = self { Some(*v) } else { None }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::F64(v) => Some(*v),
            Value::I64(v) => Some(*v as f64),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(v) = self { Some(v) } else { None }
    }
    pub fn as_enum(&self) -> Option<i64> {
        if let Value::Enum(v) = self { Some(*v) } else { None }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::None => write!(f, "-"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::I64(v) => write!(f, "{v}"),
            Value::F64(v) => write!(f, "{v}"),
            Value::Vec2(v) => write!(f, "({}, {})", v.x, v.y),
            Value::Vec3(v) => write!(f, "({}, {}, {})", v.x, v.y, v.z),
            Value::Vec4(v) => write!(f, "({}, {}, {}, {})", v.x, v.y, v.z, v.w),
            Value::Color(c) => write!(f, "rgba({}, {}, {}, {})", c.r, c.g, c.b, c.a),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Enum(v) => write!(f, "enum:{v}"),
            Value::Id(id) => write!(f, "{id}"),
        }
    }
}

/// Static description of one enum type.
#[derive(Debug)]
pub struct EnumInfo {
    pub name: &'static str,
    pub variants: &'static [VariantInfo],
}

#[derive(Debug)]
pub struct VariantInfo {
    pub value: i64,
    pub name: &'static str,
    pub label: &'static str,
}

impl EnumInfo {
    pub fn variant(&self, value: i64) -> Option<&VariantInfo> {
        self.variants.iter().find(|v| v.value == value)
    }

    pub fn variant_by_name(&self, name: &str) -> Option<&VariantInfo> {
        self.variants.iter().find(|v| v.name == name)
    }
}

/// The type of a field.
#[derive(Clone)]
pub enum Kind {
    Bool,
    I64,
    F64,
    Vec2,
    Vec3,
    Vec4,
    Color,
    Str,
    Id,
    Enum(&'static EnumInfo),
    Struct(&'static TypeInfo),
    List(Box<Kind>),
}

impl Kind {
    pub fn is_leaf(&self) -> bool {
        !matches!(self, Kind::Struct(_) | Kind::List(_))
    }

    /// Does `v` fit a field of this kind?
    pub fn accepts(&self, v: &Value) -> bool {
        match (self, v) {
            (Kind::Bool, Value::Bool(_))
            | (Kind::I64, Value::I64(_))
            | (Kind::F64, Value::F64(_))
            | (Kind::Vec2, Value::Vec2(_))
            | (Kind::Vec3, Value::Vec3(_))
            | (Kind::Vec4, Value::Vec4(_))
            | (Kind::Color, Value::Color(_))
            | (Kind::Str, Value::Str(_))
            | (Kind::Id, Value::Id(_)) => true,
            (Kind::Enum(e), Value::Enum(x)) => e.variant(*x).is_some(),
            _ => false,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Kind::Bool => "bool".into(),
            Kind::I64 => "int".into(),
            Kind::F64 => "float".into(),
            Kind::Vec2 => "vec2".into(),
            Kind::Vec3 => "vec3".into(),
            Kind::Vec4 => "vec4".into(),
            Kind::Color => "color".into(),
            Kind::Str => "string".into(),
            Kind::Id => "id".into(),
            Kind::Enum(e) => format!("enum {}", e.name),
            Kind::Struct(t) => format!("struct {}", t.name),
            Kind::List(k) => format!("list<{}>", k.name()),
        }
    }
}

impl PartialEq for Kind {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) {
            (Kind::Enum(a), Kind::Enum(b)) => core::ptr::eq(*a, *b),
            (Kind::Struct(a), Kind::Struct(b)) => core::ptr::eq(*a, *b),
            (Kind::List(a), Kind::List(b)) => a == b,
            _ => core::mem::discriminant(self) == core::mem::discriminant(o),
        }
    }
}

impl fmt::Debug for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_accept_matching_values() {
        assert!(Kind::F64.accepts(&Value::F64(1.0)));
        assert!(!Kind::F64.accepts(&Value::I64(1)));
        assert!(Kind::Str.accepts(&Value::Str("x".into())));
        static E: EnumInfo = EnumInfo {
            name: "E",
            variants: &[VariantInfo { value: 0, name: "A", label: "A" }, VariantInfo { value: 5, name: "B", label: "Bee" }],
        };
        assert!(Kind::Enum(&E).accepts(&Value::Enum(5)));
        assert!(!Kind::Enum(&E).accepts(&Value::Enum(1)));
        assert_eq!(E.variant_by_name("B").unwrap().label, "Bee");
        assert_eq!(Kind::List(Box::new(Kind::Enum(&E))).name(), "list<enum E>");
        assert_eq!(Kind::List(Box::new(Kind::I64)), Kind::List(Box::new(Kind::I64)));
        assert_ne!(Kind::I64, Kind::F64);
        assert_eq!(Value::I64(3).as_f64(), Some(3.0));
        assert_eq!(Value::Str("a".into()).kind(), Some(Kind::Str));
        assert_eq!(Value::Enum(1).kind(), None);
        assert_eq!(format!("{}", Value::Vec2(Vec2::new(1.0, 2.0))), "(1, 2)");
    }
}
