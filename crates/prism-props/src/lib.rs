//! Prism props: describe a struct once, get its UI, file format, undo and
//! (later) animation channels for free. Blender calls this RNA.
//!
//! ```ignore
//! props! {
//!     /// Settings for Extrude Region.
//!     pub struct ExtrudeProps {
//!         /// How far to push the new faces.
//!         pub offset: f64 = 0.0 => { id: 1, hard: 0.0.., soft: 0.0..=10.0, subtype: Distance },
//!         pub along_normals: bool = true => { id: 2, label: "Along Normals" },
//!     }
//! }
//! ```
//!
//! That emits the struct, `Default`, and [`Reflect`]: a runtime description
//! of every field (name, label, kind, default, ranges, subtype, flags, doc)
//! plus `get`/`set` by field index into a small [`Value`] enum. Nested
//! reflected structs and `Vec<T>` of reflected types are supported. Field ids
//! are stable and used by [`serial`] so files survive renames and reorders.

pub mod field;
pub mod info;
mod macros;
pub mod reflect;
pub mod serial;
pub mod value;
pub mod walk;

pub use field::{FieldInfo, FieldInfoBuilder, Flags, Range, Subtype, flags};
pub use info::{TypeInfo, TypeInfoBuilder};
pub use reflect::{Prop, PropError, Reflect, ReflectList, ReflectStatic};
pub use value::{EnumInfo, Kind, Value, VariantInfo};
