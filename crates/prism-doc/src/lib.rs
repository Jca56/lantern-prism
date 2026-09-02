//! Prism document: the truth (D002). Datablocks are `props!` structs held in
//! copy-on-write [`Store`]s; `Doc: Clone` is a handful of `Arc` bumps, so
//! undo history is a list of old documents ([`History`]). The file format
//! ([`file`]) is a chunked container of field-id-tagged datablocks (D012).

pub mod blocks;
pub mod doc;
pub mod file;
pub mod history;
pub mod mesh_io;
pub mod modifiers;
pub mod obj;
pub mod store;

pub use blocks::{
    Camera, Collection, DataKind, EditState, Elem, Light, LightKind, Material, MeshBlock, MeshProps, Object,
    ObjectMode, Projection, Scene, SelectMode, ToolSettings,
};
pub use doc::{Doc, DocProps};
pub use file::{FileError, load, load_file, save, save_file};
pub use history::{History, HistoryStats, UndoStep};
pub use mesh_io::MeshIoError;
pub use modifiers::{MirrorProps, Modifier, ModifierKind, SubsurfProps};
pub use obj::{ObjError, ObjMesh};
pub use store::Store;
