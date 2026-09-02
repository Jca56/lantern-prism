//! The reflection traits and the `Prop` implementations for leaf types,
//! nested structs and `Vec<T>`.

use core::any::Any;
use core::fmt;

use prism_core::Id;
use prism_math::{Color, Vec2, Vec3, Vec4};

use crate::info::TypeInfo;
use crate::value::{Kind, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum PropError {
    NoSuchField(usize),
    TypeMismatch { field: &'static str, expected: Kind, got: Value },
    IndexOutOfRange(usize),
    BadPath(String),
}

impl fmt::Display for PropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropError::NoSuchField(i) => write!(f, "no field at index {i}"),
            PropError::TypeMismatch { field, expected, got } => {
                write!(f, "field `{field}` expects {expected:?}, got {got}")
            }
            PropError::IndexOutOfRange(i) => write!(f, "list index {i} out of range"),
            PropError::BadPath(p) => write!(f, "bad property path `{p}`"),
        }
    }
}

impl std::error::Error for PropError {}

/// A struct described by `props!`. Object-safe: panels, serialization and
/// undo all work on `&dyn Reflect`.
pub trait Reflect: Any + Send + Sync {
    fn type_info(&self) -> &'static TypeInfo;
    /// Leaf value of field `i`; `Value::None` for struct/list fields.
    fn get(&self, field: usize) -> Value;
    fn set(&mut self, field: usize, value: Value) -> Result<(), PropError>;
    fn get_struct(&self, field: usize) -> Option<&dyn Reflect>;
    fn get_struct_mut(&mut self, field: usize) -> Option<&mut dyn Reflect>;
    fn get_list(&self, field: usize) -> Option<&dyn ReflectList>;
    fn get_list_mut(&mut self, field: usize) -> Option<&mut dyn ReflectList>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn box_clone(&self) -> Box<dyn Reflect>;
}

impl dyn Reflect {
    pub fn get_by_name(&self, name: &str) -> Option<Value> {
        self.type_info().field_index(name).map(|i| self.get(i))
    }

    pub fn set_by_name(&mut self, name: &str, value: Value) -> Result<(), PropError> {
        match self.type_info().field_index(name) {
            Some(i) => self.set(i, value),
            None => Err(PropError::BadPath(name.to_owned())),
        }
    }

    pub fn downcast_ref<T: Reflect>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }

    pub fn downcast_mut<T: Reflect>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut::<T>()
    }
}

impl Clone for Box<dyn Reflect> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

impl fmt::Debug for dyn Reflect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let info = self.type_info();
        let mut s = f.debug_struct(info.name);
        for (i, field) in info.fields.iter().enumerate() {
            match &field.kind {
                Kind::Struct(_) => {
                    if let Some(sub) = self.get_struct(i) {
                        s.field(field.name, &sub);
                    }
                }
                Kind::List(_) => {
                    s.field(field.name, &format_args!("[{} items]", self.get_list(i).map_or(0, |l| l.len())));
                }
                _ => {
                    s.field(field.name, &format_args!("{}", self.get(i)));
                }
            }
        }
        s.finish()
    }
}

/// Reflected types also know their description without an instance.
pub trait ReflectStatic: Reflect + Sized {
    fn info() -> &'static TypeInfo;
}

/// A homogeneous list reachable through reflection.
pub trait ReflectList: Send + Sync {
    fn item_kind(&self) -> Kind;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn get(&self, i: usize) -> Value;
    fn set(&mut self, i: usize, value: Value) -> Result<(), PropError>;
    fn get_struct(&self, i: usize) -> Option<&dyn Reflect>;
    fn get_struct_mut(&mut self, i: usize) -> Option<&mut dyn Reflect>;
    fn push_default(&mut self);
    fn remove(&mut self, i: usize);
    fn clear(&mut self);
}

/// Anything that can be a field of a `props!` struct.
pub trait Prop: Send + Sync + 'static {
    fn kind() -> Kind
    where
        Self: Sized;
    /// Leaf value; `Value::None` for structs and lists.
    fn to_value(&self) -> Value;
    fn from_value(v: &Value) -> Option<Self>
    where
        Self: Sized;
    fn as_reflect(&self) -> Option<&dyn Reflect> {
        None
    }
    fn as_reflect_mut(&mut self) -> Option<&mut dyn Reflect> {
        None
    }
    fn as_list(&self) -> Option<&dyn ReflectList> {
        None
    }
    fn as_list_mut(&mut self) -> Option<&mut dyn ReflectList> {
        None
    }
}

macro_rules! leaf_prop {
    ($t:ty, $kind:ident, |$v:ident| $to:expr, |$from:ident| $back:expr) => {
        impl Prop for $t {
            fn kind() -> Kind {
                Kind::$kind
            }
            fn to_value(&self) -> Value {
                let $v = self;
                $to
            }
            fn from_value($from: &Value) -> Option<Self> {
                $back
            }
        }
    };
}

leaf_prop!(bool, Bool, |v| Value::Bool(*v), |v| v.as_bool());
leaf_prop!(f64, F64, |v| Value::F64(*v), |v| v.as_f64());
leaf_prop!(String, Str, |v| Value::Str(v.clone()), |v| v.as_str().map(str::to_owned));
leaf_prop!(Vec2, Vec2, |v| Value::Vec2(*v), |v| if let Value::Vec2(x) = v { Some(*x) } else { None });
leaf_prop!(Vec3, Vec3, |v| Value::Vec3(*v), |v| if let Value::Vec3(x) = v { Some(*x) } else { None });
leaf_prop!(Vec4, Vec4, |v| Value::Vec4(*v), |v| if let Value::Vec4(x) = v { Some(*x) } else { None });
leaf_prop!(Color, Color, |v| Value::Color(*v), |v| if let Value::Color(x) = v { Some(*x) } else { None });
leaf_prop!(Id, Id, |v| Value::Id(*v), |v| if let Value::Id(x) = v { Some(*x) } else { None });

macro_rules! int_prop {
    ($($t:ty),*) => {$(
        impl Prop for $t {
            fn kind() -> Kind {
                Kind::I64
            }
            fn to_value(&self) -> Value {
                Value::I64(*self as i64)
            }
            fn from_value(v: &Value) -> Option<Self> {
                match v {
                    Value::I64(x) => (*x).try_into().ok(),
                    Value::F64(x) if x.fract() == 0.0 => (*x as i64).try_into().ok(),
                    _ => None,
                }
            }
        }
    )*};
}
int_prop!(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

/// Every reflected struct is a valid field type.
impl<T: ReflectStatic + Send + Sync + 'static> Prop for T {
    fn kind() -> Kind {
        Kind::Struct(T::info())
    }
    fn to_value(&self) -> Value {
        Value::None
    }
    fn from_value(_: &Value) -> Option<Self> {
        None
    }
    fn as_reflect(&self) -> Option<&dyn Reflect> {
        Some(self)
    }
    fn as_reflect_mut(&mut self) -> Option<&mut dyn Reflect> {
        Some(self)
    }
}

impl<T: Prop + Default + Clone> Prop for Vec<T> {
    fn kind() -> Kind {
        Kind::List(Box::new(T::kind()))
    }
    fn to_value(&self) -> Value {
        Value::None
    }
    fn from_value(_: &Value) -> Option<Self> {
        None
    }
    fn as_list(&self) -> Option<&dyn ReflectList> {
        Some(self)
    }
    fn as_list_mut(&mut self) -> Option<&mut dyn ReflectList> {
        Some(self)
    }
}

impl<T: Prop + Default + Clone> ReflectList for Vec<T> {
    fn item_kind(&self) -> Kind {
        T::kind()
    }
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn get(&self, i: usize) -> Value {
        self.as_slice().get(i).map_or(Value::None, Prop::to_value)
    }
    fn set(&mut self, i: usize, value: Value) -> Result<(), PropError> {
        match self.as_mut_slice().get_mut(i) {
            None => Err(PropError::IndexOutOfRange(i)),
            Some(slot) => match T::from_value(&value) {
                Some(v) => {
                    *slot = v;
                    Ok(())
                }
                None => Err(PropError::TypeMismatch { field: "<item>", expected: T::kind(), got: value }),
            },
        }
    }
    fn get_struct(&self, i: usize) -> Option<&dyn Reflect> {
        self.as_slice().get(i)?.as_reflect()
    }
    fn get_struct_mut(&mut self, i: usize) -> Option<&mut dyn Reflect> {
        self.as_mut_slice().get_mut(i)?.as_reflect_mut()
    }
    fn push_default(&mut self) {
        Vec::push(self, T::default());
    }
    fn remove(&mut self, i: usize) {
        if i < Vec::len(self) {
            Vec::remove(self, i);
        }
    }
    fn clear(&mut self) {
        Vec::clear(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_conversions() {
        assert_eq!(<u8 as Prop>::from_value(&Value::I64(300)), None);
        assert_eq!(<u8 as Prop>::from_value(&Value::I64(200)), Some(200));
        assert_eq!(<i32 as Prop>::from_value(&Value::F64(4.0)), Some(4));
        assert_eq!(<i32 as Prop>::from_value(&Value::F64(4.5)), None);
        assert_eq!(<f64 as Prop>::from_value(&Value::I64(2)), Some(2.0));
        assert_eq!(<String as Prop>::to_value(&"hi".to_owned()), Value::Str("hi".into()));
        assert_eq!(<usize as Prop>::kind(), Kind::I64);
        assert_eq!(<Vec<Vec3> as Prop>::kind(), Kind::List(Box::new(Kind::Vec3)));
    }

    #[test]
    fn vec_as_list() {
        let mut v: Vec<f64> = vec![1.0, 2.0];
        let l: &mut dyn ReflectList = &mut v;
        assert_eq!(l.len(), 2);
        assert_eq!(l.get(1), Value::F64(2.0));
        l.push_default();
        assert_eq!(l.len(), 3);
        l.set(2, Value::F64(9.0)).unwrap();
        assert_eq!(l.set(2, Value::Str("x".into())), Err(PropError::TypeMismatch { field: "<item>", expected: Kind::F64, got: Value::Str("x".into()) }));
        assert_eq!(l.set(7, Value::F64(0.0)), Err(PropError::IndexOutOfRange(7)));
        l.remove(0);
        assert_eq!(v, vec![2.0, 9.0]);
        assert!(!v.is_empty());
        let l: &mut dyn ReflectList = &mut v;
        l.clear();
        assert!(l.is_empty());
        assert!(l.get_struct(0).is_none());
    }
}
