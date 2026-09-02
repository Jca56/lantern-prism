//! Prism eval: turn an editable mesh into flat, triangulated, GPU-ready
//! buffers with origin maps back to the editable elements. Later this crate
//! grows the dependency graph; the contract "pure function of a snapshot"
//! never changes.

pub mod mesh_buffers;
pub mod modifiers;

pub use mesh_buffers::{MeshBuffers, evaluate};
pub use modifiers::{EvalMesh, apply_modifiers};
