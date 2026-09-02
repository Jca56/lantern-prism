//! The `props!` macro and its helpers. `macro_rules!` only — no `syn`.

/// Define a reflected struct or enum. See the crate docs for the grammar.
///
/// Struct fields: `vis name: Type = default => { meta… }` where meta keys are
/// `id`, `label`, `hard`, `soft`, `step`, `subtype`, `flags`. Doc comments on
/// the struct and its fields are captured. Enum variants: `Name = value =>
/// { label: "…" }`.
#[macro_export]
macro_rules! props {
    // ---------------------------------------------------------------- struct
    (
        $(#[$($smeta:tt)*])*
        $vis:vis struct $name:ident {
            $(
                $(#[$($fmeta:tt)*])*
                $fvis:vis $field:ident : $ty:ty = $default:expr $(=> { $($meta:tt)* })?
            ),* $(,)?
        }
    ) => {
        $(#[$($smeta)*])*
        #[derive(Clone, Debug, PartialEq)]
        $vis struct $name {
            $( $(#[$($fmeta)*])* $fvis $field: $ty, )*
        }

        impl ::core::default::Default for $name {
            fn default() -> Self {
                Self { $( $field: $default, )* }
            }
        }

        const _: () = {
            #[allow(non_camel_case_types, dead_code, clippy::enum_variant_names)]
            enum __Field { $($field,)* }

            impl $crate::ReflectStatic for $name {
                fn info() -> &'static $crate::TypeInfo {
                    static INFO: ::std::sync::OnceLock<$crate::TypeInfo> = ::std::sync::OnceLock::new();
                    INFO.get_or_init(|| {
                        #[allow(unused_mut)]
                        let mut t = $crate::TypeInfo::builder(stringify!($name));
                        $( t = $crate::__props_doc!(t; $($smeta)*); )*
                        $(
                            #[allow(unused_mut)]
                            let mut f = $crate::FieldInfo::builder::<$ty>(
                                stringify!($field),
                                <$ty as $crate::Prop>::to_value(&($default)),
                            );
                            $( f = $crate::__props_doc!(f; $($fmeta)*); )*
                            $( $crate::__props_meta!(f; $($meta)*); )?
                            t = t.field(f);
                        )*
                        t.build()
                    })
                }
            }

            impl $crate::Reflect for $name {
                fn type_info(&self) -> &'static $crate::TypeInfo {
                    <Self as $crate::ReflectStatic>::info()
                }
                #[allow(unused_variables)]
                fn get(&self, field: usize) -> $crate::Value {
                    $( if field == __Field::$field as usize {
                        return <$ty as $crate::Prop>::to_value(&self.$field);
                    } )*
                    $crate::Value::None
                }
                #[allow(unused_variables)]
                fn set(&mut self, field: usize, value: $crate::Value) -> Result<(), $crate::PropError> {
                    $( if field == __Field::$field as usize {
                        return match <$ty as $crate::Prop>::from_value(&value) {
                            Some(v) => { self.$field = v; Ok(()) }
                            None => Err($crate::PropError::TypeMismatch {
                                field: stringify!($field),
                                expected: <$ty as $crate::Prop>::kind(),
                                got: value,
                            }),
                        };
                    } )*
                    Err($crate::PropError::NoSuchField(field))
                }
                #[allow(unused_variables)]
                fn get_struct(&self, field: usize) -> Option<&dyn $crate::Reflect> {
                    $( if field == __Field::$field as usize {
                        return <$ty as $crate::Prop>::as_reflect(&self.$field);
                    } )*
                    None
                }
                #[allow(unused_variables)]
                fn get_struct_mut(&mut self, field: usize) -> Option<&mut dyn $crate::Reflect> {
                    $( if field == __Field::$field as usize {
                        return <$ty as $crate::Prop>::as_reflect_mut(&mut self.$field);
                    } )*
                    None
                }
                #[allow(unused_variables)]
                fn get_list(&self, field: usize) -> Option<&dyn $crate::ReflectList> {
                    $( if field == __Field::$field as usize {
                        return <$ty as $crate::Prop>::as_list(&self.$field);
                    } )*
                    None
                }
                #[allow(unused_variables)]
                fn get_list_mut(&mut self, field: usize) -> Option<&mut dyn $crate::ReflectList> {
                    $( if field == __Field::$field as usize {
                        return <$ty as $crate::Prop>::as_list_mut(&mut self.$field);
                    } )*
                    None
                }
                fn as_any(&self) -> &dyn ::core::any::Any { self }
                fn as_any_mut(&mut self) -> &mut dyn ::core::any::Any { self }
                fn box_clone(&self) -> Box<dyn $crate::Reflect> { Box::new(self.clone()) }
            }
        };
    };

    // ------------------------------------------------------------------ enum
    (
        $(#[$($emeta:tt)*])*
        $vis:vis enum $name:ident {
            $(
                $(#[$($vmeta:tt)*])*
                $variant:ident = $value:expr $(=> { $($meta:tt)* })?
            ),+ $(,)?
        }
    ) => {
        $(#[$($emeta)*])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(i64)]
        $vis enum $name {
            $( $(#[$($vmeta)*])* $variant = $value, )+
        }

        impl ::core::default::Default for $name {
            fn default() -> Self {
                $crate::__props_first!($( Self::$variant ),+)
            }
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[ $( Self::$variant ),+ ];

            pub fn info() -> &'static $crate::EnumInfo {
                static INFO: $crate::EnumInfo = $crate::EnumInfo {
                    name: stringify!($name),
                    variants: &[ $(
                        $crate::VariantInfo {
                            value: $value,
                            name: stringify!($variant),
                            label: $crate::__props_variant_label!($variant; $($($meta)*)?),
                        },
                    )+ ],
                };
                &INFO
            }

            pub fn from_i64(v: i64) -> Option<Self> {
                $( if v == $value { return Some(Self::$variant); } )+
                None
            }

            pub fn label(self) -> &'static str {
                Self::info().variant(self as i64).map_or("?", |v| v.label)
            }
        }

        impl $crate::Prop for $name {
            fn kind() -> $crate::Kind {
                $crate::Kind::Enum(Self::info())
            }
            fn to_value(&self) -> $crate::Value {
                $crate::Value::Enum(*self as i64)
            }
            fn from_value(v: &$crate::Value) -> Option<Self> {
                match v {
                    $crate::Value::Enum(x) | $crate::Value::I64(x) => Self::from_i64(*x),
                    _ => None,
                }
            }
        }
    };
}

/// Fold one attribute into a builder: doc comments are captured, anything
/// else is ignored (it is passed through to the struct verbatim).
#[doc(hidden)]
#[macro_export]
macro_rules! __props_doc {
    ($b:ident; doc = $s:literal) => { $b.doc_line($s) };
    ($b:ident; $($other:tt)*) => { $b };
}

/// Apply `key: value` metadata to a `FieldInfoBuilder`.
#[doc(hidden)]
#[macro_export]
macro_rules! __props_meta {
    ($b:ident;) => {};
    ($b:ident; id: $v:expr $(, $($rest:tt)*)?) => {
        $b = $b.id($v);
        $crate::__props_meta!($b; $($($rest)*)?);
    };
    ($b:ident; label: $v:expr $(, $($rest:tt)*)?) => {
        $b = $b.label($v);
        $crate::__props_meta!($b; $($($rest)*)?);
    };
    ($b:ident; hard: $v:expr $(, $($rest:tt)*)?) => {
        $b = $b.hard($v);
        $crate::__props_meta!($b; $($($rest)*)?);
    };
    ($b:ident; soft: $v:expr $(, $($rest:tt)*)?) => {
        $b = $b.soft($v);
        $crate::__props_meta!($b; $($($rest)*)?);
    };
    ($b:ident; step: $v:expr $(, $($rest:tt)*)?) => {
        $b = $b.step($v);
        $crate::__props_meta!($b; $($($rest)*)?);
    };
    ($b:ident; subtype: $v:ident $(, $($rest:tt)*)?) => {
        $b = $b.subtype($crate::Subtype::$v);
        $crate::__props_meta!($b; $($($rest)*)?);
    };
    ($b:ident; flags: $($f:ident)|+ $(, $($rest:tt)*)?) => {
        $b = $b.flags($( $crate::flags::$f )|+);
        $crate::__props_meta!($b; $($($rest)*)?);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __props_variant_label {
    ($name:ident; label: $l:literal $(,)?) => { $l };
    ($name:ident;) => { stringify!($name) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __props_first {
    ($first:expr $(, $rest:expr)*) => { $first };
}
