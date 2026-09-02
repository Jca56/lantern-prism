//! Prism mesh kernel: radial-edge topology (BMesh lineage) stored as flat,
//! persistent columns. See `docs/design/mesh-kernel.md`.
//!
//! Four element domains — vertex, edge, face, loop — each a table of
//! `ChunkedVec` columns sharing one slot allocator, so the whole mesh clones
//! in O(chunks) and edits copy only what they touch. Topology links are
//! typed columns (`edge.v`, `loop.next`, …); user data lives in named
//! attribute layers (`position`, `select`, `uv`, …). Both are the same
//! storage, so undo, save and evaluation treat them alike (D003, D020).
//!
//! Only the [euler operators](euler) touch topology columns. Everything else
//! is composed from them, which is what keeps compound tools from ever
//! corrupting a mesh. [`Mesh::validate`] checks every invariant.

pub mod attr;
mod cycles;
pub mod euler;
pub mod fuzz;
pub mod handle;
pub mod mesh;
pub mod ops;
pub mod primitives;
pub mod slots;
pub mod tables;
pub mod validate;

pub use attr::{AttrData, AttrFlags, AttrKind, AttrValue, Attribute, AttributeSet, names};
pub use euler::{EulerError, EulerResult};
pub use handle::{Edge, EdgeH, Face, FaceH, Loop, LoopH, Vert, VertH};
pub use mesh::Mesh;
pub use slots::Slots;
pub use validate::ValidateError;
